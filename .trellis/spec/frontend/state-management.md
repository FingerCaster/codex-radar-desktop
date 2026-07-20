# State Management

> The frontend uses React local state plus one reducer. There is no external store and no client-side server-state library.

## State Ownership

| State | Owner | Persistence | Reason |
|---|---|---|---|
| Normalized snapshot | `useRadar` reducer | `localStorage` last-known-good cache | Shared by either mounted view |
| Active source projection and local activation epoch | `useDesktopPreferences` + `useRadar` reducers | Source persists natively; epoch is session-only | Rejects cross-source and A-B-A post-return asynchronous work |
| Refresh projection and failure | `useRadar` reducer | None | Must transition atomically with snapshot retention |
| Notification permission result | `useRadar` reducer | None | Informational native capability state |
| Compact vs expanded | `App` `useState` | None | One-window presentation state |
| Window resize failure | `App` `useState` | None | Native UI action error, not radar data |
| Rankings, leaders, messages, formatted metrics | Component derivation | None | Cheap projection from props |

Do not promote `expanded` into `RadarViewState`, and do not store derived leaders or formatted strings in the reducer.

## Reducer Contract

`src/hooks/useRadar.ts` defines the complete reducer state and action union:

```ts
export interface RadarViewState {
  source: RadarSource;
  activationEpoch: number;
  snapshot: RadarSnapshot | null;
  status: RadarStatus;
  error: RefreshFailure | null;
  notificationsEnabled: boolean;
}

export type RadarAction =
  | { type: "source-selected"; source: RadarSource; activationEpoch: number; cached: RadarSnapshot | null }
  | { type: "refresh-started"; source: RadarSource }
  | { type: "snapshot-received"; source: RadarSource; snapshot: RadarSnapshot }
  | { type: "refresh-failed"; source: RadarSource; failure: RefreshFailure }
  | { type: "permission-resolved"; enabled: boolean };
```

`RadarStatus` is a UI projection, not a transport status:

```ts
type RadarStatus = "booting" | "ready" | "refreshing" | "stale" | "unavailable";
```

## Transition Matrix

| Input | Previous snapshot | Next status | Snapshot effect | Error effect |
|---|---|---|---|---|
| Initial cache valid | N/A | `stale` | Use cache | `null` |
| Initial cache absent/invalid | N/A | `booting` | `null` | `null` |
| `source-selected` with matching source/epoch/cache | Either | `stale` | Replace with target activation cache | Clear |
| `source-selected` with new source or epoch and no cache | Either | `booting` | Clear old activation | Clear |
| `refresh-started` | Present | `refreshing` | Preserve | Clear |
| `refresh-started` | Absent | `booting` | Preserve null | Clear |
| Current/newer `snapshot-received` | Either | `ready` | Replace | Clear |
| Older `snapshot-received` | Present | `stale` | Preserve current | Create `stale_payload` |
| `refresh-failed` | Present | `stale` | Preserve | Store failure |
| `refresh-failed` | Absent | `unavailable` | Preserve null | Store failure |
| `refresh-failed` with `kind: superseded` | Either | Unchanged | Preserve exact object | Preserve |
| `permission-resolved` | Either | Unchanged | Unchanged | Unchanged |
| Action source differs from state, or payload source differs from action | Either | Unchanged | Preserve exact object | Preserve |

An older candidate is detected by parsed `updatedAt`, not `checkedAt`. The generated failure uses the rejected candidate's `checkedAt` as `occurredAt`.

## Last-Known-Good Invariant

Within one selected source, once `snapshot` is non-null, neither refresh start
nor refresh failure may clear or replace it. Only a valid, matching-source,
non-older `snapshot-received` action replaces it. `source-selected` is the one
intentional boundary that may replace the snapshot with another source's cache
or clear it.

```ts
case "refresh-failed":
  if (action.failure.kind === "superseded") return state;
  if (
    action.source !== state.source ||
    action.failure.source !== action.source
  ) return state;
  return {
    ...state,
    status: state.snapshot ? "stale" : "unavailable",
    error: action.failure,
  };
```

This is why `snapshot` and `status` are kept in one reducer instead of separate setters. A failure and a stale/empty projection must not become temporarily inconsistent.

## Duplicate And Out-of-Order Delivery

A manual command can return a snapshot while the backend also emits
`radar://snapshot-updated`. Source guards run before timestamp checks. Equal
same-source timestamps are accepted and settle at `ready`; an older same-source
delivery is rejected. A delayed old-source success or failure returns the exact
current state object.

`useDesktopPreferences` increments `radarActivationEpoch` for every actual
source transition. Reducer actions are processed in order, so main ->
distributed -> main reaches epoch 2 even if React batches both updates into one
render. `useRadar` stores that epoch, treats a newer epoch as a new selection,
and drops command/get-snapshot continuations whose captured activation object
no longer matches. Backend `superseded` remains a reducer no-op. The renderer
does not deduplicate leader changes or notifications; Rust owns that behavior.

Do not compare `checkedAt` to determine data freshness. `checkedAt` records the request/check time and can advance for unchanged data; `updatedAt` is the source data version.

## Derived State

Keep these calculations at render time:

- Compact leaders: filter rankings through a `Set(snapshot.leaderIds)` and fall back to the first ranking.
- Detail leaders: filter with the same ID set; use the first ranking as primary fallback.
- Visible rows: `rankings.slice(0, 5)`.
- User-facing failure copy: map `RefreshFailure.kind` in `App.userFacingError`.
- Window/data error precedence: `windowError ?? userFacingError(radar.error)`.

The normalized candidate ordering and leader identity set come from Rust. Components may select or format them, but must not resort or recompute top scores.

## Good / Base / Bad Cases

- Good: ready main snapshot A, refresh starts, main snapshot B arrives with a later `updatedAt`; state becomes ready with B.
- Good source change: distributed selection installs only a distributed cache,
  or clears main and enters booting when none exists.
- Base: a valid disk cache starts stale, then the same backend snapshot arrives; state becomes ready.
- Bad network: snapshot A remains visible and status becomes stale with the normalized failure.
- Bad first load: no placeholder model is created; state is unavailable with `snapshot === null`.
- Bad ordering: older same-source snapshot B is rejected and snapshot A remains the exact current object.
- Bad source race: a main result arrives after distributed selection; the whole
  distributed state object is returned unchanged.
- Bad A -> B -> A race: Rust returns A1 as `superseded` when it still controls
  the command, and the frontend epoch drops an A1 Promise already queued after
  Rust returned. Neither can make A2 ready or stale.

## Wrong vs Correct

### Wrong

```ts
setStatus("refreshing");
setSnapshot(null);
```

### Correct

```ts
dispatch({ type: "refresh-started", source });
```

The reducer preserves the last-known-good snapshot and produces a status consistent with whether data exists.

## Tests Required

Every new action or status must add transition tests in `src/hooks/useRadar.test.ts`. Assert:

- the exact resulting status;
- whether snapshot identity is preserved or replaced;
- whether error is cleared, retained, or synthesized;
- that unrelated fields such as `notificationsEnabled` remain unchanged.
- that cross-source action or payload mismatches preserve exact state identity.
- that a same-source `superseded` failure preserves exact state identity.
- that a same-source newer activation epoch reinitializes the state instead of
  taking the enum-equality fast path.

Runtime tests must use deferred success and failure promises across
`A0 -> B1 -> A2`, then assert the A2 cache, status, and error remain unchanged.

For stale ordering, use two valid ISO timestamps and assert both `result.snapshot === current` and `result.error?.kind === "stale_payload"`.
