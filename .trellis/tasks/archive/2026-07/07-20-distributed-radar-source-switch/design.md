# Distributed radar source switch - technical design

## Architecture

`RadarSource { Main, Distributed }` is an app-owned domain enum serialized as
`main / distributed`. `DesktopPreferences` persists the active value because
the existing native menu controller already owns checked menu settings and
their transactional JSON persistence. `RadarSnapshot` and `RefreshFailure`
carry the source so every downstream boundary can reject cross-source data.

```text
native source menu -> persist/check DesktopPreferences -> select RadarService source
                                                        -> wake poller + refresh

                    +-> main runtime: snapshot / ETag / single-flight
active source ------|
                    +-> distributed runtime: snapshot / Last-Modified / single-flight
                                      |
                                      v
                        normalized source-tagged snapshot
                                      |
                         Tauri events and commands
                                      |
                  source-aware reducer + per-source local cache
```

## Remote adapters

The existing primary public-summary adapter remains intact except for adding
`source: main`. A distributed adapter deserializes `schema`, `combos`, `tasks`,
`cells`, `baseline_generated_at`, and the required latest-run metrics.

For each combo, the adapter looks up every `task|model|effort` cell and uses its
first `ran_by` entry only. A candidate exists when at least one latest run is
present. Its score is `(passed * 150 + sampled / 2) / sampled`, which implements
positive integer round-to-nearest without floating-point drift. Rankings sort
by score descending and stable identity ascending; equal scores all become
leaders. Timestamp parsing accepts upstream RFC 3339 offsets and chooses the
maximum baseline/cell timestamp. Optional duration averages use finite,
non-negative observations. Cost averages accept finite, non-negative values for
ordinary efforts; `ultra` values additionally require `cost_complete: true`.

Both adapters return the existing app schema version `2.0`; the new source
field identifies provenance while upstream schema validation remains local to
each adapter.

## Service state and requests

Each source owns a `SourceRuntime` with one atomic
`SourceState { snapshot, validator }` and a generation-keyed single-flight map.
The active selection is a `{ source, generation }` token with per-source latest
generation watermarks and a per-activation notification baseline; a poll wake
signal lives above the runtimes. Main requests have a 512 KiB cap and ETag revalidation. Distributed
requests have a 4 MiB cap and Last-Modified revalidation. Redirects remain
disabled and both endpoint URLs are compile-time constants.

`refresh_and_publish` captures the full token and joins only that activation's
flight, then rechecks the token after the request. A different-source old flight
may commit only while it is still that source's latest generation; an older
epoch is permanently discarded even after the source becomes inactive again.
Every joined caller rechecks before returning. Superseded work returns an
internal `superseded` result without events, errors, or notifications.
Stale timestamp comparisons and leader diffs occur only inside one runtime. A
304 without that runtime's snapshot is a `no_cache` failure. The first success
of each activation establishes notification baseline without notifying.

The poller refreshes, then waits for either the active source's interval or a
source-change wake signal. This preserves the five-minute primary cadence,
uses a one-minute distributed cadence, and prevents a switch from waiting on
the prior source's sleep.

## Menu transaction

`DesktopMenu` adds two check items under one submenu and synchronizes them from
`DesktopPreferences.radarSource`. Menu dispatch holds the service selection
write guard while it persists/synchronizes the desktop preference. Failure
leaves the old token and checks authoritative. Success advances generation,
emits the preference, wakes the poller, and performs an immediate refresh after
the desktop transition gate is released. Natural polling cannot observe an
uncommitted source. Network refresh failure does not roll back a successfully
selected source.

No source path calls visibility, geometry, opacity, or taskbar placement code.

## Frontend state

The frontend type guard requires `snapshot.source`. Cache v2 uses one key per
source. A valid v1 snapshot is migrated only when it points to the fixed primary
endpoint; distributed data never inherits that cache.

`useRadar` receives the hydrated desktop source. The reducer stores its active
source plus a renderer-session activation epoch, loads only the matching cache
on selection, clears a mismatched visible activation, and includes source on
refresh-started/success/failure actions. Desktop preference updates increment
the epoch for every actual transition, even when React batches A -> B -> A into
one final A render. Refresh/get-snapshot promises capture the activation object
and ignore results queued for an older epoch. It compares timestamps only for
matching sources and ignores every late action from another source. Passive
taskbar renderers listen and project updates but do not initiate duplicate
refreshes.

Desktop preferences and main expanded state register their event listener
before reading initial native state. Once an event arrives during hydration,
its value wins over any delayed initial success or failure.

The detail action maps the snapshot enum to one of two fixed website URLs; no
remote or event-provided arbitrary URL reaches the opener plugin.

## Compatibility and rollback

Serde defaults migrate old desktop preferences to `main`. The v1 browser cache
remains readable only as a primary migration source. Existing primary payloads,
commands, and UI projections retain their field behavior.

Rollback removes the submenu/source field and distributed runtime/adapter, then
returns the frontend cache key to the primary-only contract. Old preference
readers ignore the extra JSON field, so the persisted file remains recoverable.
