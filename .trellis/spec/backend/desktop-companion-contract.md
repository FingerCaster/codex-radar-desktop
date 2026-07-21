# Desktop Companion Contract

## 1. Scope / Trigger

This contract applies to `src-tauri/src/desktop.rs`,
`src-tauri/src/desktop/windows.rs`, native menu/tray behavior, desktop
preferences, and renderer-facing desktop commands/events. Read it before
changing window geometry, platform routing, menu state, visibility, opacity,
or taskbar/menu-bar projection behavior.

Rust is the source of truth for accepted desktop preferences and native window
state. React projects that state but must not independently mutate native
windows or persist a second preference copy.

## 2. Signatures

The persisted and emitted camel-case state is:

```text
DesktopPreferences {
  alwaysOnTop: boolean,
  clickThrough: boolean,
  positionLocked: boolean,
  showTaskbarWindow: boolean,
  showMainWindow: boolean,
  launchAtLogin: boolean,
  opacityPercent: 100 | 90 | 80 | 70 | 60,
  radarSource: "main" | "distributed"
}
```

Commands:

```text
get_desktop_preferences() -> DesktopPreferences
get_main_expanded() -> boolean
set_desktop_option(option, enabled) -> DesktopPreferences
set_desktop_opacity(opacityPercent) -> DesktopPreferences
set_desktop_radar_source(source) -> DesktopPreferences
set_main_window_position_preset(
  preset: "top-left" | "top-right" | "center" | "bottom-left" | "bottom-right"
) -> ()
set_window_expanded(expanded) -> ()
show_main_details() -> DesktopPreferences
show_desktop_context_menu() -> ()
update_companion_projection(projection) -> ()
hide_window() -> DesktopPreferences
```

Events:

```text
desktop://preferences-updated -> complete DesktopPreferences
desktop://main-expanded -> boolean
desktop://show-main-details -> ()
```

Native-only position persistence is deliberately outside the renderer DTO:

```text
<app-config>/main-window-position.json
SavedMainWindowPosition { x: i32, y: i32 }

desktop.position.top-left     -> 上左
desktop.position.top-right    -> 上右
desktop.position.center       -> 中心
desktop.position.bottom-left  -> 下左
desktop.position.bottom-right -> 下右
```

Source menu IDs map directly to the persisted enum:

```text
desktop.radar-source.main        -> 主站
desktop.radar-source.distributed -> 分布式
```

## 3. Contracts

- Windows creates one `taskbar` WebView as a real `Shell_TrayWnd` child. Its
  only size is exactly `168 x 30` logical pixels.
- Start taskbar CSS conversion from the child WebView's
  `window.scale_factor()`. Reparenting under `Shell_TrayWnd` can leave that
  value below the DPI Windows reports for the child HWND, so placement uses
  `max(tauri_scale, GetDpiForWindow(child) / 96.0)`. A zero child-DPI result
  falls back to the Tauri value. Never use the Explorer/taskbar HWND's DPI for
  this conversion.
- `win11_geometry` either returns the full requested physical size or fails.
  It must never clamp the child below its CSS viewport because the WebView will
  then crop content.
- Treat the task-button band and notification area as hard horizontal bounds.
  Enumerate visible taskbar descendants owned by external processes, project
  their rectangles as blockers, and choose the rightmost free slot with a
  2-logical-pixel gap. If no complete slot exists, fail instead of covering an
  existing taskbar surface.
- Start exactly one Windows layout monitor after desktop initialization. While
  `showTaskbarWindow` is true, it repeats blocker-aware placement every second.
  Each tick **snapshots** preferences under the value mutex, then **drops the
  guard before** Wry/Win32 place/show work. Before accepting the native result,
  it takes the transition gate and re-reads the preference so neither an active
  nor inactive stale snapshot can overwrite a user transition. Holding the
  value guard across placement deadlocks tray/menu actions while
  `scale_factor` waits on the event loop.
- Compare the current child screen rectangle with the computed taskbar-relative
  target before `SetWindowPos`. Equal geometry is a no-op and must not raise the
  child in Z order every second.
- Runtime placement failure is a safety demotion, not an ordinary reversible
  preference transition. Show the main window, hide the taskbar child, set
  `showTaskbarWindow` false, normalize `showMainWindow` true when needed, and
  publish the complete state. Do not roll back to the known-invalid rectangle.
  Persistence/menu/event failures are aggregated into one warning; the native
  main/tray recovery path remains authoritative.
- Visibility apply on Windows establishes the taskbar companion (create or
  rebuild if detached, place, show, health-check) **before** hiding the main
  window. A failed companion setup must leave the main window visible and must
  not commit a taskbar-only preference.
- `ShowMainWindow` uses a **main-only** apply path (`apply_main_window_visibility`).
  Showing the main window must not call taskbar placement. A previous bug made
  tray left-click appear dead: toggle-show ran full visibility, taskbar ensure
  failed, and the option transaction rolled back so main never appeared.
- Windows tray left-click toggles: when main is hidden it calls
  `force_show_main_window` (not a bare preference toggle). That path shows +
  clamps + focuses main, and on preference-transaction failure still runs
  emergency native show + best-effort `showMainWindow: true` persist. A final
  tray handler fallback calls `recover_main_window_for_safety` if even that fails.
- `show_main_details` (taskbar left-click / macOS tray) force-shows the main
  window **before** expanding detail size. Expand failures are logged and must
  not leave the user with a still-hidden window.
- `tauri-plugin-single-instance` is registered first. A second process launch
  while the app is already running calls `force_show_main_window` on the live
  instance so users can recover by starting Model Radar again (Start menu /
  shortcut) even when tray clicks appear dead.
- Windows taskbar companion input uses a process-wide `WH_MOUSE_LL` hook and the
  placed companion screen rect. WebView2's `Chrome_RenderWidgetHostHWND` is
  out-of-process, so subclassing the Tauri HWND never sees real mouse clicks.
  Left-up in the rect → `show_main_details` / force-show; right-up → shared
  context menu. The hook belongs to a dedicated message-loop thread and is
  explicitly disabled whenever the projection is hidden; see section 8.
- While `showTaskbarWindow` is true, each monitor tick reuses a healthy
  companion when present. A missing Explorer host, detached label, label-removal
  wait, or host-generation change enters bounded recovery with the main window
  exposed. Rebuild uses `destroy()` and a later tick; only fatal placement/input
  failure or recovery timeout triggers safety demotion.
- Any path that surfaces the main window for recovery (taskbar demotion,
  show-main, show-details, tray force-show) must ensure the outer rect intersects
  at least one monitor work area. Fully off-screen seeds use the same multi-monitor
  restore clamp/center rules as startup and persist the recovered compact corner.
- Main-window resizing maps each axis by its normalized available travel:
  `offset / (workLength - currentLength)`. This preserves all four edges and
  makes compact-detail-compact transitions reversible, including negative
  monitor coordinates.
- Persist only a compact-equivalent physical top-left for the `main` window in
  `main-window-position.json`; taskbar geometry and native coordinates never
  enter `DesktopPreferences`. Restore before first visibility. Choose the work
  area with greatest positive intersection, calculate compact physical size
  from each candidate monitor's scale factor, clamp into that area, and center
  on the primary monitor with the window's current monitor as fallback when the
  saved display is gone.
- `Moved`, `Resized`, and `ScaleFactorChanged` for `main` increment one revision
  state and share a singleton 200ms writer. Taskbar events are ignored. A
  successful close, quit, or `ExitRequested` path synchronously captures and
  flushes the compact-equivalent position before hiding or exiting.
- Wry window getters called off the main thread send an event-loop message and
  synchronously wait for its reply. A background writer must capture native
  geometry before taking `main_position_gate`, then take the gate and recheck
  its revision before writing. Any resize transaction that may start on a Tauri
  command or async thread must be dispatched to the main thread before taking
  the gate. Never make the main event loop wait on a mutex held by a worker that
  is waiting on a Wry getter or setter.
- Writer error cleanup and the check for a newer revision happen under the same
  position-state lock. If a new revision arrived during the failed attempt, the
  existing writer remains active and retries; otherwise it clears
  `writerActive` and a later event or explicit flush may retry.
- The shared native menu contains `快捷设置位置` immediately after
  `锁定窗口位置`, ordered `上左 / 上右 / 中心 / 下左 / 下右`. Presets use the
  current monitor work area and current outer size, reject oversized windows,
  preserve `showMainWindow`, bypass drag locking, and persist only after native
  movement succeeds. Persistence failure rolls the native position back
  best-effort and does not advance the in-memory last-saved value.
- Renderer quick-position requests decode through the same kebab-case
  `MainWindowPositionPreset` enum and call the same controller transaction as
  native menu items. The renderer sends only the preset name: it never sends
  coordinates, changes `DesktopPreferences`, or creates another persistence
  path. `positionLocked` blocks drag regions, not these explicit commands.
- macOS uses the native tray/status-item title and does not create a second
  companion WebView. Left-click opens details; right-click uses the shared
  native menu. Hiding the projection calls `set_title(Some(""))`: with the
  pinned `tray-icon 0.24.1`, `None` does not clear an existing macOS title.
- Transparent macOS windows require both `app.macOSPrivateApi: true` and the
  Tauri `macos-private-api` feature.
- Menu toggles take the controller's `preference_transition` gate, snapshot the
  value mutex, then release the value mutex before apply/persist/menu/native
  work. The gate prevents rapid clicks from applying the same stale prior value
  twice without carrying the value mutex across Wry/Win32.
- The shared menu and main settings view contain one `开机自启` check backed by
  `DesktopPreferences.launchAtLogin`. Rust registers `tauri-plugin-autostart`
  before `DesktopController::new`; controller construction reads
  `autolaunch().is_enabled()` before building the menu and replaces the loaded
  JSON value when that native read succeeds. A legacy/missing field defaults
  to `false`; a native read failure logs and retains the normalized persisted
  value so tray recovery still starts.
- A start-at-login transition uses the ordinary option transaction. Apply the
  plugin enable/disable operation, verify `is_enabled()` equals the request,
  persist, synchronize the shared menu, commit memory, then emit the complete
  preference. Any apply/verification/persist/menu failure restores the prior
  native registration and preference/menu state best-effort and emits no
  proposed value. Renderers never call the plugin or persist another copy.
- Commands and event-loop callbacks that may wait for the preference transition
  gate or native window/menu work use async dispatch. A background transition
  may hold the gate while a Wry getter waits for the event loop, so the event
  loop must not synchronously wait for that same gate. The transaction itself
  remains synchronous and never carries a standard mutex guard across `await`.
- The shared menu contains `雷达数据源` with exactly two independent check
  items displayed as one exclusive choice: `主站` and `分布式`. Menu sync always
  derives both checks from `DesktopPreferences.radarSource`; clicking the
  already-selected check therefore restores it instead of leaving no source.
- Source transitions use one async desktop gate plus the service's
  active-selection write guard. While that guard blocks natural polling and
  old-flight publication, the controller persists/synchronizes the complete
  preference. Failure restores authoritative checks and leaves the old
  source/generation unchanged. Success advances the service generation, emits
  the preference, wakes polling, then releases the desktop gate before the
  immediate network refresh. Network failure does not roll back a committed
  selection.
- A source transition never calls visibility, geometry, opacity, taskbar
  placement, or main-detail APIs. It works while the main window is hidden or
  only the passive taskbar/menu-bar projection is enabled.
- An ordinary user transition updates memory, menu checkmarks, and renderer
  events only after required native calls and persistence succeed. Roll back
  best-effort on a failure. The runtime placement safety demotion above is the
  only exception because restoring an overlapping child is not a valid rollback.
- The preference file is
  `<app-config>/desktop-preferences.json`. Invalid/truncated JSON falls back to
  defaults. An unsupported opacity normalizes to 100, and contradictory hidden
  windows normalize to a recoverable visible projection. A missing legacy
  `radarSource` field defaults to `main` and is written on the next successful
  initialization persistence.
- Linux is unsupported. Platform-specific modules, constants, and dependencies
  must remain behind `cfg(windows)` or `cfg(target_os = "macos")`.

## 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Opacity outside 100/90/80/70/60 | Reject command; loaded value becomes 100 |
| Both display projections disabled | Enable the other projection before commit |
| Explorer/taskbar host unavailable | Enter `Recovering`, expose main, retry within the 10-second grace, then demote if still unavailable |
| Requested taskbar geometry does not fit | Return `None`/error; never shrink the viewport |
| Child HWND DPI exceeds the Tauri scale after reparenting | Use the child-DPI scale as the lower bound so `168 x 30` remains a complete CSS viewport |
| Child HWND DPI query returns zero | Fall back to the valid Tauri scale; do not infer scale from the Explorer/taskbar HWND |
| External blocker or task band changes with a free slot | Move to the new rightmost slot on the next monitor tick |
| Runtime placement has no complete slot or a dead host | Hide companion, show main on-screen, persist `showTaskbarWindow: false`; manual re-enable retries |
| Taskbar companion HWND missing or detached while preferred | Destroy once, wait for Manager label removal, rebuild on a later tick, and demote only on fatal error or grace timeout |
| Taskbar create/place fails while applying taskbar-only visibility | Do not hide main; return error so the option transaction rolls back |
| Hook lease expires or cursor moves without hook-event progress | Rearm on the dedicated hook thread; failure enters the ordinary safe demotion path |
| Parent taskbar auto-hides while child retains `WS_VISIBLE` | Keep the child healthy; do not demote based on recursive ancestor visibility |
| Main window fully outside every work area when shown | Clamp/center via restore rules and persist the recovered position |
| Runtime target rectangle is unchanged | Skip `SetWindowPos` and retain current Z order |
| Safety-demotion persistence/menu/event step fails | Keep the safe native/runtime projection and emit one aggregated warning; never restore the overlap |
| Existing macOS title is hidden | Set an explicit empty title |
| Native option application fails | Do not publish or retain the proposed preference |
| Native start-at-login read succeeds at startup | Replace the loaded field before menu construction and persist the reconciled complete preference during initialization |
| Native start-at-login read fails at startup | Log, retain the normalized persisted field, and keep the app/tray available |
| Start-at-login apply or verification fails | Restore the prior registration best-effort; do not persist, check, or emit the proposal |
| Preference/menu commit fails after start-at-login apply | Restore the prior native registration and persisted/menu state best-effort; keep memory/event authoritative at the prior value |
| Renderer setting competes with taskbar monitor Wry work | Run the command through Tauri async dispatch so the event loop can service the monitor; the command must eventually resolve or reject |
| Source preference/menu commit fails | Keep the old service token and restore authoritative old checks; no transient source can refresh or publish |
| Selected source network refresh fails | Keep the new persisted selection and source-specific cached projection; publish its source-tagged failure |
| Preference JSON is absent or invalid | Load normalized defaults |
| Position JSON is absent, malformed, or fully off screen | Keep the configured center or recover into an available work area; never panic |
| Saved position belongs to a differently scaled monitor | Use that monitor's compact physical size when intersecting and clamping |
| Renderer sends an unknown or non-kebab-case preset | Reject during command deserialization before controller or window mutation |
| Preset window is larger than the current work area | Reject without moving or writing the saved position |
| Preset move succeeds but position persistence fails | Roll back native position best-effort; retain the previous in-memory saved value |
| Writer fails while a newer geometry revision arrives | Keep the singleton writer active and retry after the debounce delay |
| Renderer projection contains newlines/long text | Collapse whitespace and enforce title bounds |

## 5. Good / Base / Bad Cases

- Good: a 150% Windows display converts `168 x 30` to `252 x 45`, centers it
  inside a 48px taskbar, and renders both rows without cropping.
- Good: TrafficMonitor occupies the notification area's left edge; the
  companion selects the next free slot to its left without overlap.
- Good: TrafficMonitor starts after Model Radar; the next monitor tick moves
  the companion left while preserving the 2-logical-pixel gap.
- Good: a compact window at the right-bottom work-area edge expands up/left and
  returns to the exact compact position when collapsed.
- Good: a position saved on a 150% secondary display restores with a `540 x
  168` compact footprint and clamps its right/bottom edges to that display.
- Good: selecting `快捷设置位置 > 下右` while position locking is enabled moves
  the current native window without changing its hidden/visible state.
- Good: selecting `移到中心` in settings while position locking is enabled
  invokes `center`, keeps settings mounted, and persists through the same native
  transaction as the menu command.
- Good: selecting `雷达数据源 > 分布式` while only the Windows taskbar projection
  is visible commits `distributed`, wakes polling, and refreshes without showing
  or moving the main window.
- Base: a valid preference file hydrates menu checks before the first state
  event, with both tray and at least one projection recoverable.
- Base: a legacy preference without `launchAtLogin` and no native registration
  starts unchecked and writes `launchAtLogin: false` during initialization.
- Good: enabling start-at-login creates a native registration, verification
  succeeds, and the settings view plus shared tray/taskbar menu receive one
  complete checked preference event; disabling performs the inverse.
- Base: no position file keeps Tauri's configured centered startup; the first
  successful movement creates the private position file.
- Bad: taskbar height is less than the requested physical height; companion
  placement fails and the main/tray path remains usable.
- Bad: two toggle events arrive quickly; each observes the state committed by
  the previous event rather than the same stale snapshot.
- Bad: an event-loop callback waits for the transition gate while a worker owns
  that gate and waits on `window.scale_factor()` from the same event loop;
  neither side can advance, so settings remains pending indefinitely.
- Bad: two source selections arrive quickly; without the async transition gate,
  the last service value and last persisted value can cross. The serialized
  transaction must leave both on the same final source.
- Bad: a background writer holds `main_position_gate` while calling
  `outer_position`; a main-thread preset can then wait on the gate while the
  writer waits on the same main event loop.

## 6. Tests Required

- Geometry tests at 100%, 125%, 150%, and 200% assert exact requested width and
  height, bounded coordinates, blocker avoidance, rightmost-slot selection,
  and rejection when no complete slot exists.
- Scale-selection tests assert `(1.25, 144) -> 1.5`, `(1.5, 120) -> 1.5`,
  `(1.25, 0) -> 1.25`, and rejection of invalid Tauri scale values.
- Runtime layout tests assert a later blocker moves an existing companion left,
  unchanged screen geometry is detected as a no-op, the monitor is claimed
  once, and a taskbar-only failure restores `showMainWindow`.
- Resize tests assert right-bottom and interior round trips, including a work
  area with a negative origin and out-of-bounds input clamping.
- Position tests assert all five exact preset coordinates, oversized rejection,
  compact-equivalent expanded capture, malformed/negative JSON, partial and
  disconnected-display recovery, and per-monitor mixed-DPI clamping.
- Command-boundary tests assert the five kebab-case preset values decode and
  unknown or camel-case values reject before native mutation.
- Writer state tests assert a single ready writer and that a failed attempt
  retries exactly when a newer revision arrived. Code review must also verify
  that off-main-thread Wry getters occur before `main_position_gate` and that
  resize transactions are marshalled to the main thread without holding it.
- Preference tests assert default recovery, invalid opacity normalization,
  mutually recoverable visibility, exclusive opacity/source checks, legacy
  `radarSource: main`, legacy `launchAtLogin: false`, exact camel-case boolean
  and `main/distributed` serialization, unchanged window settings across source
  and start-at-login selection, and repeated Windows file writes.
- Windows native smoke tests toggle `showTaskbarWindow` through renderer IPC
  while the monitor runs and assert both commands settle, the complete
  preference round-trips, and no control remains pending. Start-at-login smoke
  tests verify the native registration appears/disappears and restore it to the
  pre-test value.
- Platform-routing tests assert macOS tray click opens details while other
  platforms retain the tray toggle behavior.
- Run `cargo fmt`, `cargo check --all-targets --all-features`, `cargo clippy
  --all-targets --all-features -- -D warnings`, and `cargo test` after changes.
- Native macOS behavior still requires a macOS build; Windows taskbar pixels
  and click-through recovery require a Windows interaction check.

## 7. Wrong vs Correct

Wrong:

```rust
let scale = window.scale_factor()?;
let height = requested_height.clamp(1, taskbar_height - 4);
```

This trusts a scale value that may be stale after reparenting and then silently
crops content to make the resulting physical viewport fit.

Correct:

```rust
let tauri_scale = window.scale_factor()?;
let child_dpi = GetDpiForWindow(child_hwnd);
let scale = if child_dpi == 0 {
    tauri_scale
} else {
    tauri_scale.max(f64::from(child_dpi) / 96.0)
};
let height = (logical_height * scale).round();
if height > f64::from(taskbar_height) {
    return None;
}
```

Use only the rendering child HWND as the native DPI fallback and preserve the
complete viewport. Explorer's parent HWND is not the renderer scale source.

Wrong:

```rust
place_taskbar_window(&taskbar)?; // startup only
```

This assumes Explorer descendants and the task-button band never change after
launch.

Correct:

```rust
loop {
    sleep(TASKBAR_MONITOR_INTERVAL).await;
    controller.monitor_taskbar_once(&app)?;
}
```

Keep the monitor singleton and serial, make unchanged placement idempotent, and
demote to the main/tray path when a complete slot no longer exists.

Wrong:

```rust
let _gate = self.lock_main_position_gate()?;
let position = window.outer_position()?; // worker blocks on the main loop
```

If a main-thread preset or exit handler waits for the same gate first, neither
thread can advance.

Correct:

```rust
let revision = self.lock_main_position_state()?.revision;
let position = self.capture_main_window_position(app)?;
let _gate = self.lock_main_position_gate()?;
if self.lock_main_position_state()?.revision != revision {
    return Ok(false);
}
```

Capture without the gate, then serialize and revision-check the write. Native
resize transactions use `run_on_main_thread` so their getter/setter sequence is
executed by the event loop itself.

Wrong:

```typescript
invoke("set_main_window_position", { x: screen.availLeft, y: screen.availTop });
```

This makes renderer monitor data and coordinates a second, DPI-sensitive source
of truth.

Correct:

```typescript
invoke("set_main_window_position_preset", { preset: "top-left" });
```

Send one semantic preset and let the existing Rust transaction select the
monitor work area, move the native window, and persist the canonical compact
position.

Wrong:

```rust
#[tauri::command]
pub fn set_desktop_option(...) -> Result<DesktopPreferences, String> {
    state.set_option(&app, option, enabled)
}
```

This blocking invoke can occupy the event loop while waiting for a preference
guard held by taskbar monitor code that is itself waiting for a Wry event-loop
reply.

Correct:

```rust
#[tauri::command(async)]
pub fn set_desktop_option(...) -> Result<DesktopPreferences, String> {
    state.set_option(&app, option, enabled)
}
```

Async command dispatch frees the event loop while preserving the existing
synchronous transaction and lock ordering inside `set_option`.

## 8. Windows Lifecycle Addendum

This addendum records the recovery contracts that must remain true when changing
the Windows taskbar companion or its monitor. It is intentionally executable:
the named states and helper boundaries are the seams used by unit tests.

### Hook ownership and lease

- `TaskbarInputController::ensure_enabled`, `disable`, and `shutdown` are the
  only lifecycle entry points. The `WH_MOUSE_LL` handle belongs to a dedicated
  thread that has a `GetMessageW` loop; the Tauri event-loop thread never owns
  the hook. Construction is idle; the first enable lazily starts the worker.
- `taskbar_mouse_hook_proc` may only read the atomic hit rectangle, classify a
  left/right-up action, increment the event sequence, post a bounded wake
  message, and call `CallNextHookEx`. It must not lock application state, call
  Wry/Tauri APIs, or spawn an async task.
- Hook control commands use a bounded queue and an acknowledgement timeout.
  Enable, disable, rearm, and shutdown each report completion; a failed
  acknowledgement or installation is a projection failure, never a retained
  `installed` health bit.
- One controller lifecycle gate serializes enable, disable, rearm, and shutdown
  across the lease/runtime locks. A disable cannot race a stale monitor enable
  into starting a replacement worker after the projection was turned off.
- A successful hook is leased for 30 seconds. A monitor sample that sees cursor
  movement without a corresponding hook-event sequence increment requests an
  early rearm. Turning off `showTaskbarWindow` clears the hit rectangle and
  acknowledges explicit unhook/bridge cleanup and terminates the worker; a
  later enable starts a fresh worker. `RunEvent::ExitRequested` calls
  `shutdown`, with `Drop` providing only bounded best-effort cleanup.

### Detached rebuild and recovery grace

- `TaskbarWindowLifecycle::ensure` returns `Ready`, `Recovering(reason)`, or
  `Fatal(error)`. A detached/dead canonical `taskbar` label is repaired with
  `WebviewWindow::destroy()`, never close-to-hide `close()`. The destroy token
  is claimed once; no same-label build is attempted until the Manager no longer
  reports the old label.
- Missing `Shell_TrayWnd`, label removal, an in-flight build, and a host-generation
  change are transient `Recovering` reasons. A build must re-read and validate
  the host after creation; a stale result is destroyed and remains recovering.
- `TaskbarRecoveryState` uses a monotonic 10-second grace period. During grace,
  the main window is force-shown/clamped and the persisted taskbar preference is
  retained. Only a healthy `Ready` result may hide the temporarily exposed main
  window for taskbar-only mode. Timeout or deterministic create/place/health
  failure first restores the native main window, then commits the safe preference
  (`showTaskbarWindow=false`, `showMainWindow=true`).

### Preference lock and monitor ordering

- `preference_transition` serializes every full-preference writer, including
  option/opacity/source changes, emergency recovery, and monitor demotion. The
  preference-value mutex protects only snapshots and commits; it must be
  released before any Wry getter/setter, window creation/destruction, placement,
  or show/hide call.
- A monitor tick snapshots the preference, performs native work without the
  value mutex, then re-reads the latest preference before accepting the result.
  Both stale directions are guarded: an old inactive tick cannot enable a hook,
  and an old active tick cannot resurrect projection after the user disabled it.
  Normal placement also remains outside the transition gate.
- Tray toggle decisions use confirmed native main visibility when available.
  `Some(false)` or an unknown native query always chooses force-show, even when
  the stored preference says `showMainWindow=true`.

### Health and close policy

- Companion health requires a live HWND, attachment to the current
  `Shell_TrayWnd`, and the child HWND's own `WS_VISIBLE` style. Ancestor
  visibility is not a health signal because Windows auto-hide may hide the parent
  taskbar while the child remains correctly projected.
- `CloseRequested` close-to-hide handling is limited to the managed `main` and
  `taskbar` labels. Internal rebuild destruction bypasses that handler and must
  not be converted into a preference toggle or prevented close.

### Embedded DPI and viewport size

- Placement reads `window.scale_factor()` before native geometry work, then
  reads `GetDpiForWindow` from that same child HWND. The effective scale is the
  greater valid value; a zero child-DPI result uses the Tauri scale unchanged.
- Never read the DPI from `Shell_TrayWnd`, `TrayNotifyWnd`, or another Explorer
  descendant. `win11_geometry` still receives one resolved scale and either
  returns the complete requested size or fails without clamping.

### Required regression assertions

- Pure mouse mapping covers left/right action dispatch and half-open hit-rect
  boundaries; hook tests cover bounded control, lease expiry, and cursor/event
  heartbeat rearm decisions.
- Lifecycle tests assert single destroy, label-removal wait, missing/changing
  host recovery, stale build rejection, grace exposure, timeout demotion, and
  successful completion before re-hide.
- Desktop tests assert native/preference drift chooses force-show, taskbar ensure
  failure preserves a visible main window, both stale monitor directions are
  rejected, and user close-to-hide is distinct from internal destroy.

### Wrong vs correct lifecycle ordering

Wrong:

```rust
let _ = window.close();
WebviewWindowBuilder::new(app, "taskbar", url).build()?;
```

Correct:

```rust
window.destroy()?;
return TaskbarWindowOutcome::Recovering(
    TaskbarRecoveryReason::DestroyRequested,
);
// A later monitor tick builds only after Manager unregisters `taskbar`.
```
