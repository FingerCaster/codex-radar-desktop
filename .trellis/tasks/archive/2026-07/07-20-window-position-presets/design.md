# Window position persistence and presets - technical design

## Architecture

Rust continues to own native window lifecycle and persistence. React owns the
fixed taskbar typography plus semantic settings controls. One renderer command
is added for explicit presets; no coordinate or desktop preference payload is
added.

```text
Moved/Resized/ScaleFactorChanged -> singleton 200ms writer -+
compact/detail resize -----------> mark geometry dirty -----+--> canonical compact position
quit/close ----------------------> synchronous flush --------+          |
                                                              JSON file
menu preset -> compute work-area anchor -> set position ------+
settings preset -> typed Tauri command -> same controller -----+

startup -> load saved position -> select visible work area -> clamp/center
        -> set native position -> apply existing visibility
```

## Native Position State

Use a separate `<app-config>/main-window-position.json` file so physical native
coordinates never enter the camel-case `DesktopPreferences` IPC/event contract.

```text
SavedMainWindowPosition {
  x: i32,
  y: i32
}
```

`DesktopController` owns the position path, a mutex-protected save state
(`revision`, `persistedRevision`, `writerActive`, readiness, and last valid
position), and a separate geometry/write gate. Window events briefly increment
the revision only after startup restore is ready; they never call native APIs
or write files. At most one delayed writer is active. Because off-main-thread
Wry getters synchronously wait for the main event loop, the writer captures
native geometry before taking the gate, then takes the gate and rechecks the
revision before writing. Preset and explicit flush operations use the same gate,
so an older delayed capture cannot overwrite them without making the main
thread wait on a worker that is itself waiting on the main event loop.

The fixed lock order is geometry/write gate, then save-state mutex. Neither
state mutex is held while calling `set_position` or querying a window, avoiding
reentrant moved-event deadlocks.

The small JSON file uses the same create-parent, serialize, and direct-write
policy as desktop preferences. Extract one generic JSON persistence helper
rather than duplicating filesystem behavior.

## Canonical Compact Position

Startup remains compact. Every capture maps the actual current position and
outer size to the compact physical size with the existing normalized
`anchored_resize_position` function and current monitor work area. A compact
window maps to itself; an expanded window maps to its equivalent compact anchor.
This avoids relying on expanded-state event timing and preserves right, bottom,
and interior anchors across restart.

Compact captures are clamped so the full current window stays inside the current
work area. Pure helpers use `i64` intermediate arithmetic for negative monitor
origins and overflow resistance.

## Restore And Monitor Selection

The hidden main window restores before `apply_visibility`:

1. Load the saved physical top-left.
2. Query available monitors, calculate the compact physical size from each
   monitor's scale factor, and choose the work area with the greatest positive
   intersection against that monitor-specific saved window rectangle.
3. Clamp the saved position into that work area.
4. If no monitor intersects, center in the primary monitor, then use the main
   window's current monitor as fallback.
5. Apply and persist the corrected visible position only after native movement
   succeeds.

This keeps same-layout restarts exact while recovering safely from disconnected
or resized displays. The taskbar WebView never enters this path.

## Movement Capture

`lib.rs` routes `WindowEvent::Moved`, `Resized`, and `ScaleFactorChanged` only
for label `main` to the controller as dirty signals; taskbar events are ignored.
One 200ms worker coalesces the stream, then queries `outer_position`,
`outer_size`, and `current_monitor` without holding the geometry gate; event
payloads are never persisted directly. It takes the gate only for the final
revision check and serialized write. A writer error retries when a newer
revision arrived during the failed attempt, avoiding a lost wakeup.
`set_main_expanded` dispatches its complete geometry transaction to the main
thread and also marks geometry dirty so a native transition that emits no move
still refreshes the compact-equivalent anchor.

Close-to-hide and `退出` synchronously flush the latest geometry before hiding
or terminating. Application `RunEvent` exit handling performs one final
synchronous flush without spawning work. A flush failure is logged but does not
block tray recovery or process exit.

## Quick Position Menu

Add a native `快捷设置位置` submenu after `锁定窗口位置`. It owns five normal
menu items with stable IDs:

```text
desktop.position.top-left     -> 上左
desktop.position.top-right    -> 上右
desktop.position.center       -> 中心
desktop.position.bottom-left  -> 下左
desktop.position.bottom-right -> 下右
```

`MainWindowPositionPreset::from_menu_id` is the single dispatcher. Placement
uses the main window's current monitor (primary fallback), current outer size,
and monitor work area. Each axis has start, centered, or end alignment. If the
current window is larger than the work area on either axis, the command fails
without moving or persisting. Position lock intentionally does not gate these
explicit native commands.

The command saves the previous native position, moves to the target, and flushes
the canonical compact result through the shared gate. It does not mutate
`showMainWindow`; a hidden main window moves for its next show. A persistence
failure rolls the native position back best-effort and does not advance the
last-saved in-memory value. Position JSON uses the project's existing direct
write policy; a truncated file is treated as invalid and falls back safely on
the next startup.

## Settings Preset Command

Expose the existing `MainWindowPositionPreset` as a small kebab-case command
enum (`top-left`, `top-right`, `center`, `bottom-left`, `bottom-right`) and
register `set_main_window_position_preset`. The command forwards directly to
`DesktopController::set_main_window_position_preset`; menu dispatch and
renderer dispatch therefore share monitor selection, oversized rejection,
movement, compact-equivalent persistence, and rollback behavior.

`src/lib/desktop.ts` owns the typed invoke wrapper. `App` routes it through the
existing single settings pending transaction with the discriminator
`positionPreset`. `SettingsView` receives only a semantic
`onSetPositionPreset` callback and renders five Lucide icon buttons in native
menu order. The component neither imports Tauri nor derives coordinates.

The controls are icon-only because these are spatial commands. Each button has
an `aria-label` and `title` (`移到上左`, `移到上右`, `移到中心`, `移到下左`,
`移到下右`) and uses a fixed five-column track. Settings remains scrollable
inside `400 x 520`; no section becomes a card.

## Taskbar Typography

Override only `.taskbar-effort` to `10px` and weight `700`. Keep its `12px`
line-height, `34px` track, ellipsis rules, row geometry, and the native
`168 x 30` viewport unchanged. `xhigh` remains within the track; longer values
truncate rather than resize the companion.

## Compatibility And Failure Handling

- Use Tauri 2 public `WindowEvent::Moved`, monitor/work-area, position, and menu
  APIs on Windows and macOS; also handle resized/scale-factor and run lifecycle
  events. Add no plugin or native platform dependency.
- Treat command enum decoding as the renderer boundary. Unknown preset strings
  reject before controller mutation; no fallback position is invented.
- Missing/malformed position JSON means no saved position and retains Tauri's
  configured centered default.
- A preset whose current native size exceeds the selected work area returns an
  error before `set_position` and leaves the last valid file untouched.
- A failed restore leaves the hidden window at its existing safe default.
- A failed drag capture preserves the previous file and logs once per attempted
  write path; it does not change desktop preferences.
- Existing Windows taskbar reflow, click-through, opacity, and visibility locks
  remain independent.

## Rollback

Remove the settings command/controls first for a UI-only rollback. A complete
rollback also removes the submenu IDs/dispatcher, moved-event routing, position
state/file helpers, and the `.taskbar-effort` override. The existing preference
file remains valid because it is not migrated.
