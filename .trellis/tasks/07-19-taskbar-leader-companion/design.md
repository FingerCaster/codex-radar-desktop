# Taskbar leader companion - technical design

## Architecture

Rust remains the owner of native window state. A managed `DesktopController`
stores validated preferences, menu item handles, and the settings path. Both
the tray menu and a taskbar-window context-menu command dispatch through the
same transition functions.

```text
tray/context menu ----+
main hide/close ------+--> DesktopController --> Tauri window APIs
taskbar click --------+          |                     |
                                  | desktop://state     | show/size/position
                                  v                     v
                          main React window      taskbar React window
                                  ^                     ^
                                  +---- radar events ---+
```

The existing `RadarService` remains the only HTTP/polling service. Each WebView
uses `get_radar_snapshot` and `radar://*` events; concurrent startup refreshes
are coalesced by the service single-flight gate.

## Desktop State Contract

`DesktopPreferences` serializes in camel case with these fields:

- `alwaysOnTop: boolean`
- `clickThrough: boolean`
- `positionLocked: boolean`
- `showTaskbarWindow: boolean`
- `showMainWindow: boolean`
- `opacityPercent: 100 | 90 | 80 | 70 | 60`

Commands return or update the full validated projection:

- `get_desktop_preferences`
- `set_desktop_preference`
- `show_main_details`
- `show_desktop_context_menu`
- existing main hide and expanded-mode commands route through the controller

`desktop://preferences-updated` carries the complete state. Renderers validate
it at the IPC boundary before changing drag regions, opacity, or layout.

## Window And Menu Behavior

On Windows, the `taskbar` WebView starts hidden to avoid a centered flash. A
target-specific host adapter discovers the Explorer taskbar host, applies DPI-
correct fixed geometry, creates the WebView with Tauri's Windows-only
`parent_raw(HWND)` builder API, and then reveals it. The
companion uses one stable logical size: 168 x 30. Placement treats the task
button band and notification area as hard bounds, enumerates visible external
taskbar descendants as blockers, and selects the rightmost free slot that fits
the exact viewport plus a small gap. A singleton Windows monitor repeats that
calculation every second so later task-button or third-party-window changes
move the companion. An unchanged rectangle skips `SetWindowPos`. Explorer-host
or no-space failure performs a non-rollback safety demotion: show the main
window, hide the companion, set `showTaskbarWindow` false, and synchronize the
runtime projection. The user can retry from the menu after space returns.

On macOS, no second WebView is required for the companion. The existing Tauri
tray icon uses its native status-item title for a bounded leader/IQ projection,
matching the architectural pattern observed in NetTool without copying its
copyrighted Swift implementation. The native menu remains attached to the
status item.

The native menu uses `CheckMenuItem` for booleans and opacity entries plus a
`Submenu` for opacity. The controller updates all checkmarks only after a
native transition succeeds. Mouse click-through does not apply to the tray,
which is the recovery channel.

## Rendering

`App` branches on the current WebView label. `main` keeps CompactView and
DetailView; `taskbar` renders a dedicated TaskbarView. A small desktop-state
hook performs initial command hydration, subscribes to the full-state event,
and cleans up its listener.

The main WebView and Windows companion are transparent native surfaces. Their
root surfaces remain fully opaque at 100 percent and use a CSS custom property
for lower opacity. Layout sizes are fixed rather than content-sized, and long
model names use ellipsis. The macOS status item retains system rendering.

## Persistence And Failure Handling

Preferences are serialized as JSON in the Tauri app configuration directory.
The first version overwrites the small preference file directly. Missing,
truncated, or invalid files use defaults; unsupported opacity values normalize
to 100. A process interruption during the write can therefore reset preferences
on the next launch but cannot create an unrecoverable startup state.
Window API or persistence failures are returned without committing an
in-memory/menu state that claims success.

Runtime taskbar-placement failure is the deliberate safety exception to the
ordinary transition rollback rule. The controller must not restore a child to
an already-invalid or overlapping position. It keeps the main/tray recovery
path visible, commits the safe in-memory projection, and aggregates persistence
or menu-sync failures into one warning.

At startup, contradictory visibility settings are normalized so the taskbar
companion is visible. The tray is always created independently of WebView
visibility.

## Compatibility And Rollback

Windows is the local visual acceptance platform. macOS status-item behavior
must be compiled and visually accepted on macOS before release. Linux companion
support is intentionally absent; unsupported target behavior fails closed while
the main radar window remains buildable where practical.

The feature is isolated to desktop state, the runtime-created Windows child
projection, and configuration. Rollback removes the `taskbar` window and
controller additions without changing the radar DTO or remote-source adapter.
