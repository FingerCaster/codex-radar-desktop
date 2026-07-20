# Type Safety

> TypeScript is strict at compile time and defensive at renderer boundaries. Rust normalizes the remote payload; the renderer validates anything that can come from storage or an event before it enters the reducer.

## Compile-Time Configuration

`tsconfig.json` uses bundler resolution, `jsx: "react-jsx"`, `strict: true`, `noUnusedLocals`, `noUnusedParameters`, and `noFallthroughCasesInSwitch`. Keep those checks passing; do not loosen the project compiler options to make a new type fit.

Use `import type` for type-only imports. The codebase uses explicit interfaces and discriminated unions rather than enums, schema-generation types, or `any`.

## Domain Types

The renderer-owned normalized shapes are in `src/types/radar.ts`:

```ts
export type RadarSource = "main" | "distributed";

export interface ModelScore {
  id: string;
  label: string;
  model: string;
  reasoningEffort: string;
  score: number;
  status: string | null;
  passed: number | null;
  tasks: number | null;
  validTasks: number | null;
  averageCostUsd: number | null;
  averageTaskSeconds: number | null;
  averageTaskTimeHuman: string | null;
  wallTimeHuman: string | null;
}

export interface RadarAttribution {
  text: string;
  url: string;
}

export interface RadarSnapshot {
  source: "main" | "distributed";
  schemaVersion: string;
  updatedAt: string;
  checkedAt: string;
  leaderIds: string[];
  rankings: ModelScore[];
  attribution: RadarAttribution;
  sourceUrl: string;
}
```

`schemaVersion` remains a string at the TypeScript layer for wire compatibility, but `isRadarSnapshot` currently accepts only the exact value `"2.0"`. The normalized Rust contract, including source field mapping and error kinds, is the source of truth in [the backend radar data contract](../backend/radar-data-contract.md).

## Boundary Signatures

The Tauri adapter in `src/lib/radar.ts` exposes these typed functions:

```ts
export interface RefreshOutcome {
  snapshot: RadarSnapshot;
  notModified: boolean;
  leaderChanged: boolean;
}

export interface RefreshFailure {
  source: RadarSource;
  kind: string;
  message: string;
  occurredAt: string;
}

export async function getRadarSnapshot(): Promise<RadarSnapshot | null>;
export async function refreshRadar(): Promise<RefreshOutcome>;
export async function openSourceSite(source: RadarSource): Promise<void>;
export async function setWindowExpanded(expanded: boolean): Promise<void>;
export async function hideWindow(): Promise<void>;
```

The event adapters are:

```ts
onSnapshotUpdated(handler: (snapshot: RadarSnapshot) => void): Promise<UnlistenFn>;
onRefreshFailed(handler: (failure: RefreshFailure) => void): Promise<UnlistenFn>;
onRefreshRequested(handler: () => void): Promise<UnlistenFn>;
```

Event names are fixed constants (`radar://snapshot-updated`, `radar://refresh-failed`, and `radar://refresh-requested`). The renderer never consumes raw upstream JSON.

## Runtime Snapshot Guard

`isRadarSnapshot(value: unknown): value is RadarSnapshot` in `src/lib/radar.ts` is the runtime gate for local storage and snapshot events. It requires:

- a non-null object;
- `source === "main" || source === "distributed"`;
- `schemaVersion === "2.0"`;
- parseable finite-date `updatedAt` and `checkedAt` strings;
- string `sourceUrl`;
- a `leaderIds` array containing only strings;
- a non-empty `rankings` array where every entry passes `isModelScore`;
- an attribution object with string `text` and `url`.

`isModelScore` requires string `id`, `label`, `model`, and `reasoningEffort`; a finite numeric `score`; nullable strings for `status`, `averageTaskTimeHuman`, and `wallTimeHuman`; nullable non-negative finite numbers for cost and durations; and nullable non-negative integer counts for `passed`, `tasks`, and `validTasks`.

The guard intentionally does not verify that leader IDs refer to ranking entries or that strings are non-empty. Those invariants are produced by the Rust normalizer and documented cross-layer; do not silently add a second ranking algorithm in the UI guard.

`getRadarSnapshot()` and `refreshRadar()` use Tauri generic return types because Rust is the trusted normalized producer. If a future untrusted bridge is added, validate its result before dispatching rather than relying on the generic alone.

## Cache Contract

The current keys are `model-radar:last-snapshot:v2:main` and
`model-radar:last-snapshot:v2:distributed`. `loadCachedSnapshot(source)`:

1. returns `null` outside a browser window;
2. reads only the selected source key and returns `null` when absent;
3. parses JSON as `unknown`;
4. returns the value only if `isRadarSnapshot` succeeds and its discriminator
   equals the requested source;
5. catches storage and JSON errors and returns `null`.

For `main` only, an absent valid v2 value may migrate the legacy v1 object. The
legacy object must have no `source`, must pass every other snapshot guard, and
must use the exact fixed primary summary URL. Successful migration adds
`source: main`, writes v2, then best-effort removes v1. Distributed never reads
or removes v1.

`saveCachedSnapshot(snapshot)` chooses the key from `snapshot.source` and
swallows denied/full-storage errors so cache failures cannot interrupt live
updates. Do not use a type assertion to bypass this guard.

## Failure Normalization

`normalizeRefreshFailure(value, fallbackSource)` preserves only records whose
`source` is a valid enum and whose `kind`, `message`, and `occurredAt` are
strings. A malformed command rejection uses the source captured when that
command began:

```ts
{
  source: fallbackSource,
  kind: "unknown",
  message: value instanceof Error ? value.message : String(value),
  occurredAt: new Date().toISOString(),
}
```

The function does not validate that `occurredAt` is a timestamp. Preserve that behavior unless the backend contract and tests are changed together.

## 1. Scope / Trigger

This contract is mandatory for the Tauri/Rust-to-React normalized snapshot boundary, local-storage cache, and live event stream. It prevents either raw upstream shape, malformed cache values, and untyped event payloads from reaching components.

## 2. Signatures (Command / Event)

The renderer calls `getRadarSnapshot`, `refreshRadar`, `openSourceSite`,
`setWindowExpanded`, and `hideWindow`. It listens to the three `radar://`
events listed above. Rust owns endpoint selection, source-specific polling and
validators, ranking, and notification diff; see
[the backend contract](../backend/radar-data-contract.md) instead of
reproducing those rules here.

## 3. Contracts (Payload / Cache)

| Boundary | Accepted payload | Renderer action |
|---|---|---|
| `get_radar_snapshot` | `RadarSnapshot | null` | Dispatch snapshot or request refresh |
| `refresh_radar` | `RefreshOutcome` | Dispatch `outcome.snapshot` |
| `radar://snapshot-updated` | unknown at event boundary, normalized after guard | Dispatch only validated snapshot |
| `radar://refresh-failed` | unknown at event boundary, accepted only by `isRefreshFailure` | Dispatch validated source-tagged failure |
| local storage | source-matching v2 JSON; fixed-primary v1 migration only | Use only after runtime guards |

## 4. Validation & Error Matrix

| Condition | Guard / behavior | Result |
|---|---|---|
| Valid schema 2.0 snapshot | All required fields and entries pass | Type predicate returns true |
| Missing/unknown source discriminator | `isRadarSource` fails | Cache/event value rejected |
| Snapshot source differs from requested cache | discriminator comparison fails | Return `null`; never cross-load |
| Empty rankings | `rankings.length > 0` fails | Cache/event value rejected |
| NaN, Infinity, or non-number score | `Number.isFinite` fails | Entry/snapshot rejected |
| Negative nullable metric | non-negative check fails | Entry/snapshot rejected |
| Fractional nullable count | integer check fails | Entry/snapshot rejected |
| Invalid timestamp | `Date.parse` is not finite | Snapshot rejected |
| Invalid JSON/storage exception | `try/catch` in loader | `null`, live state unaffected |
| Malformed failure event | `isRefreshFailure` fails | Ignore event; retain state |
| Malformed command rejection | `normalizeRefreshFailure` fallback | `unknown` failure tagged with requested source |
| Older valid snapshot | reducer `isSnapshotOlder` check | Current snapshot retained, `stale_payload` error |
| Valid event/result from deselected source | reducer source guard | Exact current state retained |
| Old activation returns after A -> B -> A | backend `superseded` kind | Reducer preserves exact A2 state |

## 5. Good / Base / Bad Cases

- Good: main and distributed snapshots round-trip through distinct v2 keys and
  preserve their exact source-tagged ranking values.
- Base: no cache or first-run `null` produces no fabricated leader and allows the hook to request a refresh.
- Bad: malformed JSON, missing/unknown source, cross-source cache content,
  missing rankings, non-finite score, or invalid nullable field is rejected
  without throwing into React.
- Bad ordering: a valid but older snapshot is typed correctly yet still rejected by reducer freshness logic.
- Bad bridge error: a thrown string or arbitrary object becomes a serializable `unknown` failure.

## 6. Tests Required (Assertion Points)

`src/lib/radar.test.ts` must keep asserting:

- valid `sampleSnapshot` round-trips through `saveCachedSnapshot`/`loadCachedSnapshot`;
- both source keys stay isolated, and a cross-source object in a key is rejected;
- a fixed-primary source-less v1 snapshot migrates only to main;
- empty rankings return false;
- `NaN` score returns false;
- object-valued nullable string returns false;
- invalid `updatedAt` returns false;
- invalid JSON returns `null`.

When adding a field, add at least one accepted fixture and one rejected value. When changing freshness or failure normalization, add reducer tests that assert the retained snapshot identity, status, and error kind in `src/hooks/useRadar.test.ts`.

## 7. Wrong vs Correct

### Wrong

```ts
const raw = JSON.parse(localStorage.getItem("model-radar:last-snapshot:v2:main")!);
const snapshot = raw as RadarSnapshot;
```

### Correct

```ts
const raw = window.localStorage.getItem(cacheKeyFor(source));
const value: unknown = JSON.parse(raw);
return isRadarSnapshot(value) && value.source === source ? value : null;
```

The correct path preserves the `unknown` boundary and proves the runtime shape before the reducer or a component sees it.

## Forbidden Type Patterns

- `any`, non-null assertions, and unchecked `as RadarSnapshot` at storage/event boundaries.
- Re-declaring `ModelScore` or `RadarSnapshot` inside a component.
- Parsing `model_iq` fields in TypeScript; parsing belongs to the Rust adapter.
- Treating a finite number as a valid count without checking integer/non-negative constraints.
- Broadening `RefreshFailure.kind` handling with an untyped string switch that hides unknown kinds; keep the `default` user-facing fallback.
