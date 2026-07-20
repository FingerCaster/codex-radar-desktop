# Taskbar leader companion and window controls

## Goal

Keep the current Model IQ leader visible beside the desktop taskbar and add a
native right-click control menu modeled on the supplied reference. The
companion must remain a lightweight projection of the existing radar service,
not a second data-fetching process.

## Background

- "Show taskbar window" means a slim data window near the notification area,
  similar to the reference network-rate display. It does not mean showing a
  normal application button in the taskbar.
- Windows behavior is researched from the local `D:\UGit\TrafficMonitor`
  reference and should use a real taskbar-hosted child window where Explorer
  permits it. macOS behavior is researched from `D:\UGit\NetTool-1.2` and uses
  a menu-bar status item rather than a floating taskbar window.
- Reference repositories are evidence, not copy sources. Their licenses and
  copyright terms must be respected; implement the mechanisms independently.
- The existing `main` window remains the compact/detail experience. The new
  `taskbar` window consumes the same normalized Rust snapshot and Tauri events.
- The user delegated first-version architecture and implementation decisions,
  so no remaining product question blocks implementation.

## Requirements

### Taskbar companion

- On Windows, add one fixed-size, borderless `taskbar` WebView child that
  displays model, reasoning effort, synchronization state, IQ, and ties inside
  the taskbar. It must use the rightmost free slot between the task buttons and
  notification area without covering third-party taskbar windows. It must
  re-evaluate that slot while running as task buttons and third-party children
  change. If no complete slot remains, disable the companion and restore the
  main/tray recovery path. Explorer restart or unavailable host discovery must
  fail back to the tray/main window instead of terminating the app.
- On macOS, render the leader and IQ through the native menu-bar status item
  title. The title must remain single-line and bounded so it does not consume
  unbounded menu-bar width.
- Clicking the companion shows the main window in its expanded detail mode.
  Right-clicking the Windows companion opens the same native menu used by the
  tray icon. The macOS status item uses that menu directly.
- The Windows companion has one fixed two-row layout: model/effort/status on
  row one and IQ/value/ties on row two. It must not expose a wider mode.
- Both renderer instances read the Rust-owned snapshot and live events. They
  must not fetch the remote endpoint directly or start independent poll loops.

### Native control menu

- Provide checked menu items for always-on-top, mouse click-through, locked
  position, taskbar companion visibility, and main window visibility.
- Provide an opacity submenu with exactly one selected value from 100, 90, 80,
  70, and 60 percent, plus immediate refresh and quit commands.
- Always-on-top, click-through, position lock, and opacity apply consistently
  to both app windows. The tray remains interactive so click-through can always
  be disabled again.
- Position lock disables app-provided drag regions. It does not attempt to
  override operating-system window-management shortcuts.
- Main-window hide buttons, close-to-hide handling, tray left-click, companion
  click, and menu actions must all pass through one Rust-owned desktop state so
  native menu checkmarks cannot drift from actual state.
- Native operations commit state only after they succeed. Publish the complete
  camel-case desktop state to both renderers after each accepted change.
- Persist accepted settings in the application configuration directory and
  validate loaded opacity/menu values before applying them at startup.

### Compatibility and recovery

- Use Tauri public APIs for ordinary window behavior. Isolate target-specific
  Windows taskbar parenting and macOS status-title behavior behind narrow
  desktop adapters with no Linux compilation dependency.
- Implement real visual opacity for WebView windows with transparent surfaces
  and a CSS opacity variable. The system-rendered macOS status item keeps native
  system opacity; the setting still applies to the macOS main window.
- Mouse click-through must never remove the tray recovery path. Startup must
  show at least one of the main or taskbar windows even if persisted settings
  are corrupt or contradictory.
- Preserve source attribution in the main detail view and all existing radar
  refresh, cache, notification, and stale-data behavior.

## Acceptance Criteria

- [ ] A fresh Windows launch shows the leader inside the taskbar, and a fresh
  macOS launch shows it in the menu bar, without causing a second HTTP request.
- [ ] Companion click opens the main window directly in detail mode; companion
  right-click and tray right-click expose the same checked native controls.
- [ ] Each boolean menu action changes the corresponding real window behavior,
  updates every menu checkmark, and emits one complete desktop-state event.
- [ ] Enabling click-through allows pointer input to reach the window below and
  can always be reversed from the tray menu.
- [ ] Locked position removes all main and companion drag regions; unlocking
  restores them without forcing either window to jump.
- [ ] The fixed short companion shows model/effort/status above IQ/value/ties,
  and never overlaps the task band, notification area, or another visible
  taskbar child.
- [ ] The companion and main visibility controls stay synchronized across menu,
  close, hide, tray-left-click, and companion-click paths.
- [ ] Opacity values are exclusive, visually expose the Windows desktop at less
  than 100 percent, and survive an app restart with all other settings.
- [ ] Invalid persisted settings fall back safely and never launch with both
  display windows unavailable and no tray recovery path.
- [ ] Rust unit tests cover Windows host selection/adaptation, state validation,
  exclusive opacity selection, and transition invariants; frontend tests cover
  desktop state validation and taskbar leader projection.
- [ ] Frontend lint/typecheck/tests/build, Rust fmt/clippy/tests, Windows visual
  checks at normal and high DPI, and a fresh Tauri bundle all pass.

## Out of Scope

- Explorer shell-process DLL injection, unsupported taskbar patching, exact
  notification-area width recreation, and taskbar auto-hide animation tracking.
- Arbitrary opacity values, theme customization, font customization, custom
  taskbar metrics, network transfer rates, or unrelated system monitoring.
- Linux taskbar/panel support. The first version supports Windows and macOS
  only; Linux must fail closed without presenting the companion as supported.
- Signing, publishing, auto-update, and release claims without native builds on
  the corresponding operating system.
