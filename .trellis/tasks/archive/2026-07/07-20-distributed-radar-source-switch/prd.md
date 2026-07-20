# Distributed radar source switch

## Goal

Let Windows and macOS users switch the global leaderboard shown by the main
window, taskbar companion, and detail view between the existing primary Codex
Radar summary and the distributed real-time radar. The selected source must
survive restart, refresh immediately, and never leak stale data from the other
source into the visible UI or native notifications.

## Background

- The primary source remains `https://codex-reset-radar.pages.dev/current.json`
  and uses the existing public-summary scoring contract, ETag validation, and
  five-minute polling cadence.
- The distributed site is `https://deng.codexradar.com`; its live data endpoint
  is `https://api.codexradar.com/api/v1/table`.
- The distributed table is currently about 1.54 MiB, has `schema: 1`, exposes
  `Last-Modified` rather than ETag, and has a 30-second cache lifetime.
- The distributed page computes each model/effort IQ from the newest run in
  every task cell: `round(passed / sampled * 150)`. The hourly `iq-history`
  endpoint is not real-time and must not be used.

## Requirements

### Source selection

- Add a native `Radar source` submenu to the existing right-click/tray menu
  with mutually exclusive `Primary` and `Distributed` checked items.
- Persist the selected source as `main` or `distributed` in the existing
  desktop preferences. Missing and legacy preference files default to `main`.
- A selection applies globally without showing, hiding, moving, resizing, or
  changing the opacity of either window.
- Switching sources immediately replaces the view with that source's cached
  snapshot when available, otherwise clears the old source and shows a loading
  or unavailable state. Rust initiates the refresh even when both renderers are
  hidden or only the taskbar companion is enabled.

### Source contracts

- Add a serialized `RadarSource` discriminator to every normalized snapshot
  and refresh failure. Source identity is part of the IPC, event, reducer, and
  browser-cache contracts.
- Preserve the primary parser and its 512 KiB response limit.
- Parse only the required distributed `table` fields. For every advertised
  model/effort combination, use `ran_by[0]` from each available task cell,
  count passing and sampled latest runs, and calculate integer IQ with the
  upstream page's rounding rule.
- Preserve all tied leaders and deterministic ranking. Set distributed
  `passed/tasks/validTasks` to the latest-run pass/sample counts. Average
  duration from finite latest-run durations. Average finite latest-run costs
  for every effort except `ultra`; an `ultra` cost is usable only when that run
  explicitly has `cost_complete: true`, matching the upstream page.
- Set distributed `updatedAt` to the newest valid value among
  `baseline_generated_at` and cell grading timestamps. Reject a bad schema,
  invalid timestamp set, oversized response, or payload with no usable
  candidates without replacing the last-known-good snapshot.
- Keep attribution and source-opening actions on a fixed allowlist: primary
  opens `codexradar.com`; distributed opens `deng.codexradar.com`.

### Refresh isolation and real-time behavior

- Maintain independent snapshot, validator, notification baseline, stale-data
  comparison, and single-flight state for each source.
- Send `If-None-Match` only to the primary endpoint and
  `If-Modified-Since` only to the distributed endpoint. A 304 may reuse only a
  snapshot from the same source.
- Use a distributed response ceiling of 4 MiB. Do not relax the primary
  ceiling.
- Poll the active primary source every five minutes and the active distributed
  source every minute. A source switch wakes the polling loop immediately.
- A request that completes after its source was deselected may update only that
  source's private cache. It must not emit visible snapshot/failure events or a
  native notification.
- Source identity includes an activation generation: after
  `main -> distributed -> main`, the original main request remains superseded and cannot join,
  publish into, or return a visible result to the reactivated main selection.
- Switching sources alone never sends a leader-change notification. Later
  refreshes compare leaders only against the prior snapshot of the same source.

### Compatibility

- Keep support limited to Windows and macOS. Do not add Linux-specific behavior.
- Existing preferences, cached primary snapshots, menu actions, taskbar
  placement, window position persistence, and primary-only installs remain
  backward compatible.

## Acceptance Criteria

- [ ] A legacy or fresh install starts on `Primary`; selecting either menu item
  updates both mutually exclusive checks and survives restart.
- [ ] Switching sources refreshes immediately while the main window is hidden,
  and the taskbar/main/detail views all show the selected source's leader.
- [ ] The distributed fixture produces the same IQ, pass/sample count, sorting,
  ties, timestamp, duration, and complete-cost averages as the upstream live
  algorithm.
- [ ] Primary ETag and distributed Last-Modified validators remain isolated;
  same-source 304 responses reuse cache and cross-source cache reuse is rejected.
- [ ] A delayed success or failure from the deselected source cannot replace
  state, mark the selected source stale, or send a notification.
- [ ] An `A -> B -> A` switch starts a fresh A flight; old A command results are
  ignored and the first success of the new activation establishes baseline
  without notifying.
- [ ] Source-specific local caches load only for their matching source; a valid
  legacy v1 cache migrates only to `main`.
- [ ] Source attribution opens only the matching allowlisted primary or
  distributed website.
- [ ] Existing frontend and Rust tests pass, with new parser, persistence,
  reducer race, conditional-request, and response-limit regression tests.
- [ ] Lint, TypeScript checks, frontend tests/build, Rust fmt/check/test/clippy,
  and the Windows installer build pass. Native Windows/macOS menu behavior is
  left ready for the user's requested manual validation.

## Out of Scope

- Linux support, custom source URLs, additional community endpoints, hourly IQ
  history, volunteer rankings, release signing/notarization, and publishing.
