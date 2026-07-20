# Desktop model radar MVP

## Goal

Build a small cross-platform desktop companion that keeps the current
highest-scoring Codex model visible at a glance and opens an in-place detail
view on click. The app should continue refreshing while its window is hidden
and notify the user only when the set of highest-scoring models changes.

## Background

- The user requested a first working version and delegated task creation,
  architecture, and implementation decisions.
- The source page is `https://codex-reset-radar.pages.dev/`. Its structured
  public summary is available at `/current.json`, with the canonical mirror at
  `https://codexradar.com/current.json`.
- The source exposes no SSE, WebSocket, or push stream. Model IQ data currently
  changes roughly every one to two hours and supports ETag revalidation.
- The public payload states that derivative integrations require authorization
  and that attribution is mandatory. This task produces a local MVP only; it
  does not publish or distribute an installer.

## Requirements

### Product behavior

- Show a compact, always-on-top, movable window with the current leader name,
  reasoning effort, IQ score, freshness state, and a clear expand affordance.
- Expand the same window on click to show the current ranking, pass/task count,
  average task cost, average task time, data timestamp, and source attribution.
- Provide icon controls for manual refresh, collapse, and hide. Closing the
  window hides it to the system tray instead of terminating the process.
- Provide tray actions to show/hide the window, refresh data, and quit.
- Poll every five minutes while the app is running, including while the window
  is hidden. Revalidate with ETag and avoid downloading an unchanged payload.
- Keep the last successful snapshot visible when refresh fails and clearly mark
  it as stale/offline. A manual refresh must remain available.
- Do not notify on the first successful fetch. Send a native notification only
  when the set of top-scoring model identities changes.
- Follow the operating-system light/dark preference and keep the interface
  usable at 100%, 125%, 150%, and 200% display scaling.

### Data contract

- Consume only the public summary endpoint. Do not call the authorization-only
  full API and do not scrape the HTML page.
- Treat `model_iq.latest` and every `model_iq.comparisons.*.latest` item as
  leaderboard candidates.
- Use `model + reasoning_effort` as stable identity. Filter candidates whose
  score is missing or non-finite, sort by score descending, and preserve all
  candidates tied for the highest score.
- Parse and validate the remote payload in one Rust adapter. The frontend must
  receive a normalized typed snapshot rather than raw remote JSON.
- Ignore a payload older than the last successful `model_iq.updated_at` value.
- Display the source-provided attribution text and link to `codexradar.com` in
  the expanded view.

### Platform scope

- Use Tauri 2 with a React/TypeScript/Vite frontend and a Rust backend.
- Keep the first version portable across Windows, macOS, and Linux. Avoid
  transparent-window and platform-private APIs.
- Produce and validate a Windows development/build artifact locally. Document
  that macOS and Linux packages must be built on their respective platforms.

## Acceptance Criteria

- [x] A fresh launch fetches the public summary and shows the current leader in
  a compact window without emitting a notification.
- [x] Clicking the compact surface expands the window without opening a second
  window and shows a ranked detail view plus mandatory attribution.
- [x] Manual refresh visibly reports loading, success, and failure states
  without shifting or overlapping controls.
- [x] Background refresh uses a five-minute interval and ETag revalidation;
  unchanged or older data does not create duplicate notifications.
- [x] A changed top-model identity set emits one native notification and updates
  the UI; a score-only change does not emit one.
- [x] Network, HTTP, schema, empty-candidate, and stale-payload failures preserve
  the last-known-good snapshot and surface an offline/stale indicator.
- [x] The tray can restore/hide the window, request refresh, and exit the app;
  the window close action hides rather than exits.
- [x] Parser/ranking unit tests cover a normal payload, ties, malformed data,
  missing candidates, and older payload rejection.
- [x] `pnpm lint`, `pnpm typecheck`, frontend tests, Rust formatting, clippy,
  Rust tests, the frontend production build, and `pnpm tauri build` pass on
  Windows.
- [x] The running Windows desktop window is visually checked in compact and
  expanded states at desktop and narrow/high-scale dimensions.

## Out of Scope

- Publishing installers, signing/notarization, auto-update, analytics, account
  login, configurable data sources, start-at-login, and global shortcuts.
- Calling the authorization-only full API or claiming authorization on behalf
  of the user.
- Reset-window alerts, quota alerts, and community rating notifications that
  are unrelated to the highest Model IQ result.
