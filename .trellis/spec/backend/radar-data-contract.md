# Radar Data Contract

## Scenario: Selectable Radar Sources to Desktop Radar

### 1. Scope / Trigger

- Trigger: `src-tauri/src/radar/` consumes either supported remote JSON schema
  and publishes source-tagged app-owned DTOs across the Rust/Tauri/TypeScript
  boundary.
- Apply this contract when changing a source URL, request policy, parser,
  ranking/leader rules, source selection, refresh state, command/event
  signature, or serializable radar field.
- The fixed sources are the primary public summary at
  `https://codex-reset-radar.pages.dev/current.json` and the distributed live
  table at `https://api.codexradar.com/api/v1/table`. The backend does not
  scrape HTML, consume hourly `iq-history`, or call the authorization-only full
  API.

### 2. Signatures

Remote and parser signatures:

```text
GET https://codex-reset-radar.pages.dev/current.json
If-None-Match: <last successful response ETag>   # omitted when absent

GET https://api.codexradar.com/api/v1/table
If-Modified-Since: <last successful Last-Modified> # omitted when absent

parse_snapshot(bytes: &[u8], checked_at: DateTime<Utc>)
  -> Result<RadarSnapshot, SourceError>

parse_distributed_snapshot(bytes: &[u8], checked_at: DateTime<Utc>)
  -> Result<RadarSnapshot, SourceError>

parse_source_snapshot(source: RadarSource, bytes: &[u8], checked_at: DateTime<Utc>)
  -> Result<RadarSnapshot, SourceError>
```

`parse_snapshot` remains the primary-only compatibility entry point.

Registered Tauri commands:

```rust
get_radar_snapshot(state: State<'_, RadarService>)
    -> Result<Option<RadarSnapshot>, String>

refresh_radar(app: AppHandle, state: State<'_, RadarService>)
    -> Result<RefreshOutcome, RefreshFailure>

set_window_expanded(window: WebviewWindow, expanded: bool)
    -> Result<(), String>

hide_window(window: WebviewWindow)
    -> Result<(), String>
```

Events emitted application-wide to the main and passive taskbar renderers:

| Event | Payload |
| --- | --- |
| `radar://snapshot-updated` | `RadarSnapshot` |
| `radar://refresh-failed` | `RefreshFailure` |
| `radar://refresh-requested` | unit/empty payload; tray asks the frontend to invoke `refresh_radar` |

There is no database signature and there are no environment keys.

### 3. Contracts

#### Remote request and response

`RadarService::new(initial_source)` creates one HTTPS-only client with redirects
disabled, a 20-second timeout, and user agent
`ModelRadar/0.1 (+https://codexradar.com)`. Only `2xx` and `304 Not Modified`
have success paths. The primary body limit remains 512 KiB; the distributed
table limit is 4 MiB.

The partial remote payload accepted by `source.rs` is:

```text
schema_version: "2.0"                         required, exact
type: "public_summary"                        required, exact
model_iq.updated_at: RFC 3339 string           required
model_iq.latest: RemoteScore                   optional candidate
model_iq.comparisons.*.latest: RemoteScore     optional candidates
api_access.requirements.attribution_text       optional
```

Unknown JSON fields are ignored. For comparison candidates, `latest.model` and `latest.reasoning_effort` take precedence; missing values may fall back to the comparison-level fields. Both identity parts are trimmed and must be non-empty. `score` must exist and be finite. Candidate identity is exactly `{model}:{reasoning_effort}`.

Candidates with the same identity are deduplicated. The higher score wins; an equal-score candidate encountered later replaces the prior candidate and may therefore replace its metadata. Rankings sort by score descending and then identity ascending. `leaderIds` contains every candidate whose score is exactly equal to the first ranking score, sorted by identity.

Labels use comparison-level `label`, then score-level `label`, then a generated model/effort label. Blank optional strings become `null`. Non-finite optional cost/time numbers become `null`; count fields deserialize as optional `u64`.

Attribution text is trimmed from the remote field or defaults to the exact Chinese attribution held in `DEFAULT_ATTRIBUTION_TEXT`. Its URL is always `https://codexradar.com`; a remote site URL is not trusted. `sourceUrl` is always the fixed public-summary URL.

The partial distributed payload is:

```text
schema: 1                                      required, exact integer
baseline_generated_at: RFC 3339 string         optional timestamp candidate
combos[]: { model, effort }                     advertised identities
tasks[]: { id }                                 task key components
cells["task|model|effort"].last_graded_at       optional timestamp candidate
cells[...].ran_by[0]                            newest effective result only
```

For each combo, each task contributes at most its first `ran_by` entry. A
present latest run increments `n`; truthy `passed` increments `p`; integer IQ
is `(p * 150 + n / 2) / n`, equivalent to JavaScript `Math.round(p / n *
150)` for non-negative counts. `passed/tasks/validTasks` serialize as `p/n/n`.
Finite non-negative duration values have their own average denominator. Finite
non-negative costs are averaged for ordinary efforts; `ultra` costs additionally
require `cost_complete: true`. `updatedAt` is the newest valid timestamp among
the baseline, cell, and latest-run grading timestamps.

Distributed rankings use the same score-descending/identity-ascending ordering,
deduplication, exact-score tie, label fallback, and normalized DTO rules as the
primary parser. Its attribution URL is always
`https://deng.codexradar.com`, and `sourceUrl` is always the fixed table URL.

#### Tauri DTOs

Rust uses `#[serde(rename_all = "camelCase")]`; TypeScript mirrors these fields in `src/types/radar.ts`.

| DTO | Required serialized fields |
| --- | --- |
| `ModelScore` | `id`, `label`, `model`, `reasoningEffort`, finite `score`, and nullable `status`, `passed`, `tasks`, `validTasks`, `averageCostUsd`, `averageTaskSeconds`, `averageTaskTimeHuman`, `wallTimeHuman` |
| `Attribution` | `text`, `url` |
| `RadarSource` | exact lowercase `main` or `distributed` |
| `RadarSnapshot` | `source`, `schemaVersion`, RFC 3339 `updatedAt`, UTC `checkedAt`, `leaderIds`, non-empty `rankings`, `attribution`, `sourceUrl` |
| `RefreshOutcome` | `snapshot`, `notModified`, `leaderChanged` |
| `RefreshFailure` | `source`, `kind`, human-readable `message`, UTC `occurredAt` |

#### Refresh state and publication

- `activeSource` is initialized from the persisted desktop preference. Each
  source owns an independent `SourceState { snapshot, validator }` behind one
  lock, plus its stale comparison and `SingleFlight`. Snapshot and validator
  therefore commit as one cache entity. No source may read the other runtime's
  state.
- The active selection is a `RefreshToken { source, generation }`, not only an
  enum. Every real source change increments generation. Therefore A -> B -> A
  produces a new A activation and an A request captured before B can never
  regain publication authority merely because the enum matches again.
- Active selection also records the latest generation ever activated for each
  source. An A0 request is permanently unable to commit after A2 exists, even
  if it finishes while B3 is active. Only the latest generation for an inactive
  source may refresh that source's private cache.
- A main request sends only its ETag as `If-None-Match`. A distributed request
  sends only its Last-Modified value as `If-Modified-Since`.
- `SingleFlight<Result<RefreshOutcome, RefreshFailure>>` stores a small
  generation-to-`Arc<OnceCell<_>>` map per source. Callers from the same
  activation await and clone one result. Different sources or generations do
  not join. Keeping overlapping generations as separate entries also prevents
  a delayed old caller from displacing the current generation's join cell.
- Every caller rechecks its captured full token after awaiting the shared
  result. A joined waiter that resumes after a source switch returns
  `superseded`, even when the initializer completed successfully while its
  activation was current.
- Event emission, error logging, and leader notification run inside the `OnceCell` initializer. One flight therefore publishes side effects exactly once even when several command/background callers await it.
- After a flight resolves, its caller clears it only when `Arc::ptr_eq` confirms it is still the active cell. A later refresh can then start a new request without an old waiter clearing the new flight.
- A `304` requires a cached snapshot from that source, updates its `checkedAt`, optionally replaces the matching validator if supplied, and returns `notModified: true`.
- A parsed payload with `updatedAt` earlier than that source's cached value is rejected. An equal timestamp is accepted; timestamps from the other source are irrelevant.
- Every successful flight that is still active, including a `304`, emits
  `radar://snapshot-updated` once. All joined callers receive the same
  initializer result while their captured token remains current; a caller that
  resumes after a transition receives `superseded`. Joined callers never emit
  additional side effects.
- The first successful refresh of every activation establishes its notification
  baseline and never reports a leader change, including when that source has an
  older private cache. Later same-activation snapshots compare sorted,
  deduplicated identity sets. Score or order changes with the same identities do
  not notify.
- A changed leader identity set sets `leaderChanged: true` and attempts one native notification. Notification failure does not fail the refresh.
- `refresh_and_publish` captures the full token before entering its flight and
  rechecks it while holding the active-selection write guard before side
  effects. A superseded activation may update a different source's private
  runtime only while it remains that source's latest generation; an older
  epoch is discarded permanently. It returns an internal source-tagged
  `superseded` failure to command callers and does not emit, log, or notify.
- Source transition holds the active-selection write guard while the native
  preference/menu commit runs. A failed commit leaves the old token unchanged;
  a successful commit increments generation before the guard is released.
  Natural polling and in-flight publication therefore cannot observe an
  uncommitted transient source. Poll wake and immediate refresh happen only
  after commit.
- Background polling refreshes immediately. It waits five minutes for main or
  one minute for distributed, and a stored `Notify` wake permit interrupts the
  previous cadence after a successful source change.

### 4. Validation & Error Matrix

| Condition | Failure kind | State/publication behavior |
| --- | --- | --- |
| Request/send/body stream error or timeout | `network` | Preserve that source's snapshot/validator; emit only if still active |
| Non-success status other than `304` | `http` | Preserve source runtime; message includes status code |
| Main body over 512 KiB or distributed body over 4 MiB | `response_too_large` | Stop reading; preserve source runtime |
| Malformed JSON | `json` | Preserve source runtime |
| Main schema not `2.0` or distributed schema not integer `1` | `schema` | Preserve source runtime |
| Missing or non-`public_summary` main type | `type` | Preserve main runtime |
| Main update timestamp invalid, or distributed timestamp set has no valid value | `timestamp` | Preserve source runtime |
| No candidate with non-empty identity and finite score | `no_candidates` | Preserve source runtime |
| Valid payload older than same-source cached `updatedAt` | `stale_payload` | Preserve same-source snapshot/validator |
| `304` before that source has a snapshot | `no_cache` | Do not borrow cross-source cache or commit the response validator |
| Flight completes while another source is active | internal `superseded` | Different-source private cache may update; no event/log/notification |
| Flight source matches but generation is old | internal `superseded` | Do not mutate reactivated runtime; frontend ignores command rejection without state change |
| Old source generation finishes after a newer same-source activation and another switch | internal `superseded` | Per-source generation watermark forbids private cache/validator mutation |
| Joined caller resumes after its initializer returned | internal `superseded` when token changed | Do not return an old success/failure across the command boundary |
| Caller joins an active refresh | same success or failure as initializer | Clone the full `Result`; no second request, event, log, or notification |
| Window sizing/hide API fails | command rejection string | No radar state mutation |
| Native notification fails | no refresh failure | Log to stderr; snapshot remains published |

### 5. Good/Base/Bad Cases

- **Good:** Distributed cells produce `79/112`; the normalized combo score is
  IQ 106, carries `source: distributed`, and uses only latest runs. If that
  source remains active, its snapshot is emitted once.
- **Base:** Main returns 304 to an ETag request or distributed returns 304 to a
  Last-Modified request. Only the matching cached snapshot gets a new
  `checkedAt`; `notModified` is true and `leaderChanged` is false.
- **Bad:** Main is refreshing when the user selects distributed. The main
  response can update the private main cache but cannot replace the distributed
  renderer state, emit a failure, or send a notification.
- **Bad A -> B -> A:** Old A and new A use distinct generation flights. The old
  A result cannot mutate the reactivated A runtime, publish, notify, or make its
  command caller mark new A stale.
- **Bad A0 -> B1 -> A2 -> B3:** A0 is older than A's recorded generation 2, so
  it cannot update A's private snapshot or validator while B3 is active.

### 6. Tests Required

- Parser/ranking tests in `radar/source.rs` assert both source discriminators,
  primary compatibility, distributed latest-run semantics, IQ half-up rounding,
  stable ties, p/n/n, ordinary/Ultra cost policy, duration filtering, maximum
  timestamp selection, schema/time/no-candidate failures, and fixed attribution.
- Service tests assert endpoint/header/limit/interval selection, validator and
  304 isolation, per-source stale and leader baselines, private snapshots,
  independent flights, active-source publication guards, shared same-source
  failure, generation detachment, A-B-A-B permanent supersession,
  delayed-old/current join isolation, late joined-caller rejection, atomic
  snapshot/validator pairing, transactional preference failure,
  per-activation notification baseline, and next-flight cleanup.
- HTTP/AppHandle tests remain required when a harness is added: assert actual
  request headers, response streaming/timeout, exactly-once event/notification
  effects, and inactive-source side-effect suppression.
- Cross-layer tests in `src/lib/radar.test.ts` and `src/hooks/useRadar*.test.ts`
  assert the source discriminator, v2 cache isolation, fixed-main v1 migration,
  source transitions, one-frame masking, and rejection of late cross-source
  successes and failures.
- Window tests in `desktop.rs`: assert compact/expanded logical dimensions remain capped to physical work area at the active scale factor.

### 7. Wrong vs Correct

#### Wrong

Sharing one global cache/validator lets source timestamps and 304 responses
contaminate each other:

```rust
let previous = self.inner.snapshot.read().await.clone();
let etag = self.inner.etag.read().await.clone();
```

#### Correct

Select one source runtime, normalize into a source-tagged DTO, and publish only
if the full captured activation token is still active:

```rust
let refresh_token = self.refresh_token().await;
let runtime = self.runtime(refresh_token.source);
let next = parse_source_snapshot(refresh_token.source, &bytes, Utc::now())?;
let previous = runtime.snapshot.read().await.clone();
if previous
    .as_ref()
    .is_some_and(|current| next.updated_at < current.updated_at)
{
    return Err(RadarError::StalePayload);
}
*runtime.snapshot.write().await = Some(next.clone());
let active = self.inner.active_source.read().await;
if active.token == refresh_token {
    let _ = app.emit(SNAPSHOT_UPDATED_EVENT, &next);
}
```
