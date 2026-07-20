# Desktop Model Radar MVP - Technical Design

## Architecture

The app uses one Tauri window that switches between compact and expanded
dimensions. Rust owns all remote-data, polling, cache, notification, tray, and
window-lifecycle behavior. React owns presentation and short-lived UI state.

```text
Codex Radar current.json
        |
        | HTTPS + If-None-Match (5 minutes / manual)
        v
Rust source adapter -> normalized RadarSnapshot -> shared last-known-good state
        |                         |                         |
        | Tauri command           | Tauri event             | leader-set diff
        v                         v                         v
React initial/read          React live update       native notification
        |
        v
compact surface <-> expanded details
```

## Boundaries

### Remote source adapter

`src-tauri/src/radar/source.rs` defines partial serde structs for schema version
2 public summaries and maps them to app-owned domain types. Unknown remote
fields are ignored. Required fields are validated before state is replaced.

Candidate identity is `${model}:${reasoning_effort}`. The primary
`model_iq.latest` item and comparison entries pass through the same normalizer.
All finite scores are sorted descending, then by stable identity solely for
deterministic presentation. Every item with the maximum score belongs to the
leader set, so ties are not silently collapsed.

### Polling and state

`RadarService` wraps a `reqwest::Client`, last ETag, last successful snapshot,
and a refresh mutex. The background loop waits five minutes between attempts.
Manual and scheduled refreshes share the same method so they cannot race.

HTTP 304 returns the cached snapshot. A successful response older than the
cached `updated_at` is rejected. Network, status, decoding, validation, and
stale-payload errors are emitted to the frontend without replacing good data.

The first success establishes a notification baseline. Later refreshes compare
sorted leader identity sets. Only a changed set triggers a native notification.

### Tauri bridge

Commands:

- `get_radar_snapshot`: returns the current normalized snapshot, if any.
- `refresh_radar`: performs or joins a refresh and returns its outcome.
- `set_window_expanded`: switches between stable compact and detail sizes.
- `hide_window`: hides the main window.

Events:

- `radar://snapshot-updated`: a fresh normalized snapshot.
- `radar://refresh-failed`: a serializable error with failure kind and time.
- `radar://refresh-requested`: tray-originated request consumed by the
  frontend, which updates visible loading state while invoking refresh.

### Frontend

The frontend keeps a small reducer with `booting`, `ready`, `refreshing`, and
`stale` projections. It hydrates a last rendered snapshot from local storage,
then asks Rust for current state and requests a refresh when necessary.

The compact layout has fixed dimensions and stable tracks so labels, score, and
status cannot resize the window. The expanded view uses a single unframed
surface, not nested cards. Lucide icons are used for controls with tooltips and
accessible labels. CSS follows the OS color scheme and uses neutral surfaces
with green/amber/red status colors and a blue source action.

## Window And Tray

- One non-transparent, undecorated, non-resizable window.
- Compact size: 360 x 112 logical pixels.
- Expanded size: 400 x 520 logical pixels.
- Always on top and skipped from the taskbar; draggable from designated regions.
- A close request is prevented and converted to hide.
- Core Tauri tray icon with Show/Hide, Refresh, and Quit menu entries.

## Security And Permissions

- Rust only requests HTTPS from the fixed public-summary URL.
- The WebView does not receive the raw payload and needs no remote `connect-src`.
- Tauri capabilities grant only core window operations needed by the UI,
  notification permission, and opening the fixed source attribution URL.
- No secrets, API keys, telemetry, local server, or arbitrary shell execution.
- The detail view always shows source attribution. Publishing remains blocked
  on obtaining any authorization required by the source owner.

## Compatibility

- Windows: WebView2 and native notification behavior are validated locally;
  installed package behavior is the notification acceptance target.
- macOS: notification permission and accessory/tray activation need native CI
  or hardware validation before release.
- Linux: WebKitGTK/AppIndicator packages and a notification daemon are runtime
  prerequisites; exact always-on-top behavior can vary by compositor.

## Rollback

The remote adapter is isolated from UI code. If the public schema changes, the
source module and its fixtures can be updated without changing component props
or Tauri event names. The whole feature can be removed by deleting the new app
sources while leaving `.trellis/` intact.
