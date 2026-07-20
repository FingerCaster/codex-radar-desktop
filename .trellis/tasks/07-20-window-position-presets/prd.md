# Window position persistence and presets

## Goal

Make the taskbar effort label readable at a glance, restore the main window to
its last useful location after restart, and provide fast native menu commands
for placing the main window at common screen anchors.

## Background

- The Windows taskbar companion remains a shell-managed `168 x 30` child. Its
  position is owned by the existing blocker-aware taskbar monitor and must not
  be persisted as a user window position.
- The borderless `main` window currently starts centered on every process
  launch. Compact/detail resizing already preserves a normalized screen anchor
  during the current process.
- The tray and taskbar companion share one native context menu on Windows; the
  macOS menu-bar status item uses the same menu model.
- The user explicitly authorized task creation, architecture decisions, and
  implementation without another review gate.

## Requirements

### Taskbar readability

- Increase the visible reasoning-effort text size and weight without changing
  the fixed `168 x 30` taskbar viewport, row heights, field order, or collision
  geometry.
- Long effort values must remain bounded and ellipsized inside their existing
  track.

### Last main-window position

- Persist only the `main` window's last useful position in the application
  configuration directory. Do not add native coordinates to the renderer
  `DesktopPreferences` payload.
- A user drag and a quick-position command both update the saved location.
- Compact/detail transitions must store a compact-equivalent anchor so the next
  launch restores the compact window to the same perceived screen location,
  including right and bottom edges.
- Restore the position before first showing the main window. Missing, malformed,
  stale, or off-screen coordinates must fall back to a visible position in an
  available monitor work area.
- Hiding/showing the main window, taskbar reflow, opacity, click-through, and
  position locking must not reset the saved location. Position locking blocks
  drag regions but does not disable explicit quick-position commands.

### Native quick-position menu

- Add a first-level submenu named `快捷设置位置` to the existing shared native
  context menu.
- The submenu contains exactly five commands in this order: `上左`, `上右`,
  `中心`, `下左`, `下右`.
- Commands move the main window inside the current monitor's work area using
  its current native size, so corners never extend under the taskbar or outside
  the work area.
- A preset command preserves the current main-window visibility state. When the
  window is hidden, the new location applies the next time it is shown.
- A successful preset move is persisted immediately and remains the next-launch
  restore location. A failed native move must not overwrite the last valid
  saved position.

### Compatibility

- Keep Windows and macOS behavior behind Tauri public cross-platform window and
  menu APIs. The first version still does not support Linux.
- Preserve the existing taskbar collision monitor, locked resize round trip,
  tray recovery, checked menu state, and old preference-file compatibility.

## Acceptance Criteria

- [ ] The effort label is visibly larger/stronger while the taskbar projection
  remains exactly `168 x 30` with no clipped model, status, IQ, or tie text.
- [ ] Dragging the compact main window, restarting the app, and showing it again
  restores the same visible location instead of the screen center.
- [ ] Moving the expanded window and returning/restarting restores the equivalent
  compact anchor rather than treating the expanded top-left as the compact
  location.
- [ ] Invalid or disconnected-monitor coordinates never launch the main window
  fully off screen.
- [ ] The native menu exposes `快捷设置位置 > 上左 / 上右 / 中心 / 下左 / 下右`
  from both tray and Windows taskbar context-menu entry points.
- [ ] Each preset uses the current monitor work area and persists the successful
  result without changing main-window visibility; right/bottom presets keep the
  full current window visible.
- [ ] Position lock still disables dragging but all five explicit presets remain
  usable.
- [ ] Existing taskbar avoidance, click-through recovery, visibility controls,
  opacity, and compact/detail position round trips continue to work.
- [ ] Frontend lint/typecheck/tests/build, Rust fmt/check/tests/clippy, and a fresh
  Windows bundle pass. Native visual verification remains assigned to the user.

## Out Of Scope

- Persisting the Windows taskbar companion position, arbitrary coordinates in
  the menu, keyboard shortcuts, per-monitor named profiles, window snapping,
  Linux desktop support, and Explorer restart reattachment.
- Persisting expanded/collapsed view mode across process restarts; startup stays
  compact and restores the compact-equivalent position.
