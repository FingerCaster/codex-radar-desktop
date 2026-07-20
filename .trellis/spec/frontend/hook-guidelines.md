# Hook Guidelines

> `useRadar` is the renderer's single stateful integration hook. Keep transport adapters in `src/lib/radar.ts` and deterministic transitions in the exported reducer.

## Public Shape

`src/hooks/useRadar.ts` returns the reducer state plus one stable action:

```ts
{
  source: RadarSource;
  activationEpoch: number;
  snapshot: RadarSnapshot | null;
  status: RadarStatus;
  error: RefreshFailure | null;
  notificationsEnabled: boolean;
  refresh: () => Promise<void>;
}
```

The hook accepts `{ passive?: boolean, source?: RadarSource,
activationEpoch?: number, enabled?: boolean }`. Endpoint selection and
active-source mutation remain owned by Rust. `source` and `activationEpoch`
come from the hydrated desktop preference hook: the source selects cache/data,
while the monotonic epoch distinguishes A0 from A2 after A -> B -> A.
`enabled` prevents all cache and native side effects until that preference is
known. `passive` changes side-effect ownership, not the accepted snapshot
contract.

The `main` renderer uses active mode. The Windows `taskbar` renderer uses
`useRadar({ passive: true, source, activationEpoch, enabled })`: it hydrates the matching Rust snapshot and listens for
snapshot/failure events, but does not refresh, request notification permission,
listen for tray refresh requests, or register online recovery.

Keep `createInitialRadarState` and `radarReducer` exported as pure functions. Their exports allow state behavior to be tested without rendering React or mocking Tauri.

## Active Initialization Sequence

The active main-renderer mount sequence is:

1. `useDesktopPreferences` finishes registering its update listener before it
   reads initial preferences. An update received during that read wins over the
   delayed initial result. Until hydration, `enabled` is false, no default-main
   cache is read, and no radar listener/command is started.
2. The lazy reducer initializer or selection transition calls
   `loadCachedSnapshot(source)` for only the hydrated source.
3. A valid matching cache starts as `stale`; no cache starts as `booting`.
4. Three Tauri listeners begin registering: snapshot updated, refresh failed,
   and tray refresh requested.
5. `getRadarSnapshot()` asks Rust for active-source in-memory state. A returned
   snapshot is dispatched with its own discriminator; `null` triggers
   `refresh()` in active mode.
6. Notification permission is resolved independently and only updates
   `notificationsEnabled`.
7. The browser `online` event triggers `refresh()`.

Notification permission must never gate hydration, event registration, polling, or rendering.

Passive taskbar mode performs source hydration, snapshot/failure listeners,
`getRadarSnapshot` without the null-snapshot refresh, and source-specific cache
persistence. It skips the refresh-request listener, notification permission,
initial refresh, and online event entirely.

## Listener Registration And Cleanup

Tauri `listen` returns `Promise<UnlistenFn>`, so cleanup must handle registration resolving after unmount:

```ts
let disposed = false;
const unlisteners: UnlistenFn[] = [];

const trackListener = async (registration: Promise<UnlistenFn>) => {
  const unlisten = await registration;
  if (disposed) {
    unlisten();
  } else {
    unlisteners.push(unlisten);
  }
};

return () => {
  disposed = true;
  unlisteners.forEach((unlisten) => unlisten());
  window.removeEventListener("online", handleOnline);
};
```

The production implementation also catches registration failures and dispatches a `listener` failure while mounted. Preserve both the late-resolution branch and the error branch. React StrictMode mounts effects more than once in development; missing cleanup creates duplicate refreshes and duplicate state updates.

## Refresh Flow

`refresh` captures the selected source plus an activation object. The object is
replaced from a layout effect whenever source or epoch changes. After the
command settles, both success and failure paths verify object identity before
dispatching:

```ts
const refresh = useCallback(async () => {
  const requestedActivation = activationRef.current;
  const requestedSource = requestedActivation.source;
  dispatch({ type: "refresh-started", source: requestedSource });
  try {
    const outcome = await refreshRadar();
    if (activationRef.current !== requestedActivation) return;
    dispatch({
      type: "snapshot-received",
      source: requestedSource,
      snapshot: outcome.snapshot,
    });
  } catch (error) {
    if (activationRef.current !== requestedActivation) return;
    dispatch({
      type: "refresh-failed",
      source: requestedSource,
      failure: normalizeRefreshFailure(error, requestedSource),
    });
  }
}, [enabled, source]);
```

Manual refresh, online recovery, and `radar://refresh-requested` all call this
same function. The frontend does not implement request locking or polling;
Rust serializes same-source refreshes and owns the source-specific loop.

## Source Transitions

The reducer owns a `source-selected` action containing both `source` and
`activationEpoch`. It loads only that source's cache, preserves notification
permission, and clears/reinitializes even when the enum returns to the same
value with a newer epoch. Every refresh-started/snapshot/failure action carries
a source; payload and action discriminators must both equal `state.source`
before the state can change. Timestamp comparison occurs only after those
checks.

Rust rejects old callers before returning when possible. The frontend epoch is
still mandatory because an old command success/failure can already be queued in
the WebView after Rust returned and before React observes A -> B -> A. The
captured activation check drops that post-return result without dispatching.
The reducer also returns the exact current state for backend `superseded`
failures.

React effects run after paint. Therefore the hook's returned `visibleState`
must synchronously project `snapshot: null, status: booting` whenever either
the source or activation epoch prop differs from reducer state. The later
selection effect may then install a matching stale cache. Do not return the
prior activation for one frame.

`refresh-started` is dispatched before the command. This makes the icon disabled/spinning while preserving the previous snapshot. Errors are normalized before entering the reducer.

## Event Contract

The hook subscribes only through typed adapters:

| Event | Adapter | Dispatch |
|---|---|---|
| `radar://snapshot-updated` | `onSnapshotUpdated` | source-tagged `snapshot-received` after runtime validation |
| `radar://refresh-failed` | `onRefreshFailed` | source-tagged `refresh-failed` after validation |
| `radar://refresh-requested` | `onRefreshRequested` | calls `refresh()` |

Passive mode subscribes only to snapshot-updated and refresh-failed. The
refresh-requested event belongs to the active main renderer.

Command signatures and payload ownership are in [the frontend type-safety spec](./type-safety.md) and [the backend radar data contract](../backend/radar-data-contract.md).

Do not call `listen` from a component. That would distribute lifecycle cleanup and could let compact/detail remounts multiply subscriptions.

## Cache Persistence

A separate effect calls `saveCachedSnapshot(state.snapshot)` only when the
state source, prop source, and snapshot source all match. The adapter chooses
the v2 key from the snapshot discriminator. Cache writes are best effort and
must not change UI status. Invalid or inaccessible storage is handled inside
`src/lib/radar.ts`.

The cache is only startup continuity. It is never considered fresh until a backend snapshot or refresh result is received, so initialization with cache deliberately projects `stale`.

## Common Mistakes

- Starting a `setInterval` in React duplicates the Rust polling loop and stops when renderer lifecycle changes.
- Treating notification denial as a refresh error incorrectly makes healthy data look offline.
- Omitting the `disposed` check can dispatch after unmount or leak a listener whose promise resolved late.
- Reading local storage in the component body repeats parsing on every render; use the reducer's lazy initializer.
- Directly setting `ready` after `refreshRadar()` without dispatching the snapshot bypasses older-snapshot rejection.
- Adding `state` as a `refresh` callback dependency destabilizes the callback and re-registers all listeners.
- Reading a main cache before desktop hydration can flash the wrong source for
  a user whose persisted selection is distributed.
- Relying only on a source-change `useEffect` can paint the old source once;
  synchronously mask mismatched reducer state in the returned projection.
- Comparing only the source enum after an awaited command accepts A0 again
  after A -> B -> A; capture and verify the activation object.

## Tests Required

Reducer tests belong in `src/hooks/useRadar.test.ts` and must assert both status and snapshot identity:

- Cached initialization keeps the exact cached snapshot and marks it `stale`.
- Refresh with a snapshot keeps that object visible and projects `refreshing`.
- Refresh failure with data keeps the last-known-good object and projects `stale`.
- First failure without data leaves `snapshot` null and projects `unavailable`.
- An older snapshot preserves the current object and creates a `stale_payload` failure.
- Source selection loads or clears only the target cache and keeps permission state.
- Delayed success/failure and action/payload discriminator mismatches return the exact prior state.
- A same-source `superseded` command result also returns the exact prior state.
- A same-source newer activation epoch reinitializes from cache even when the
  final source enum equals the prior enum.
- Permission resolution changes only `notificationsEnabled`.

Add effect-level tests only when changing registration/order semantics; those tests must mock all three event adapters, assert unlisten calls on cleanup, and cover a listener promise that resolves after unmount.
Use deferred command promises to prove both old success and old ordinary
failure are ignored after `A0 -> B1 -> A2`.

## Wrong vs Correct

### Wrong

```ts
useEffect(() => {
  listen("radar://snapshot-updated", ({ payload }) => setSnapshot(payload));
}, [expanded]);
```

### Correct

```ts
useEffect(() => {
  let disposed = false;
  const unlisteners: UnlistenFn[] = [];
  void trackListener(onSnapshotUpdated((snapshot) => {
    dispatch({
      type: "snapshot-received",
      source: snapshot.source,
      snapshot,
    });
  }));
  return () => {
    disposed = true;
    unlisteners.forEach((unlisten) => unlisten());
  };
}, [enabled, passive, refresh, source]);
```

The correct pattern keeps validation at the adapter, state changes in the reducer, and one subscription across compact/detail changes.
