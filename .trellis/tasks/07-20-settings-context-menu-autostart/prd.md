# Settings panel, context menu, and autostart

## Goal

Give users one discoverable settings surface in the main window while keeping
the tray and Windows taskbar companion as reliable native recovery/control
paths. Start-at-login must be configurable from both the settings surface and
the shared native menu, and every surface must reflect the same accepted native
state.

## Background

- The main compact/detail renderers already project Rust-owned desktop
  preferences and use accessible Lucide icon buttons.
- The tray and the Windows `taskbar` WebView already reference the same native
  `DesktopMenu`; `TaskbarView` opens it through `show_desktop_context_menu` on
  `contextmenu` input.
- Desktop preferences, native menu checks, window application, persistence,
  and `desktop://preferences-updated` emission are centralized in
  `src-tauri/src/desktop.rs`.
- The existing desktop preferences do not include start-at-login and the app
  has no autostart dependency.
- The user explicitly delegated task creation, detailed product decisions, and
  implementation approval. No product question remains open.

## Requirements

### Main-window settings

- Add a settings icon to both compact and detail headers. It opens an in-place
  settings view in the existing `main` window; it must not create a third
  native window.
- Opening settings from compact mode expands the native main window only after
  the resize succeeds. Leaving settings returns to the view and native size
  from which settings was opened. Opening from detail returns to detail.
- Escape and an accessible back icon close settings through the same path.
- The settings view exposes the existing persistent desktop choices: radar
  source, always-on-top, mouse click-through, position lock, taskbar companion
  visibility, main-window visibility, and the five supported opacity values.
- Add a start-at-login toggle to the settings view. Binary choices use native
  checkbox/toggle semantics, radar source uses a two-value segmented control,
  and opacity uses the existing fixed value set.
- The settings layout must fit the existing expanded `400 x 520` logical
  window at normal and high display scaling without overlapping or resizing
  controls. It follows the current light/dark tokens and focus/accessibility
  behavior.
- A failed native setting update leaves the prior visible value selected and
  shows a bounded error status. Controls prevent duplicate submission while
  their update is pending.

### Shared native menu and taskbar behavior

- Add one checked `开机自启` item to the existing shared native menu. Tray
  right-click and Windows taskbar-companion right-click must expose that same
  item and all existing menu commands with synchronized checks.
- Preserve the taskbar renderer's full-surface right-click behavior: suppress
  the WebView context menu and invoke `show_desktop_context_menu` exactly once.
- This interaction applies while mouse click-through is disabled. When
  click-through is enabled, the companion intentionally receives no pointer
  input and the native tray remains the recovery/menu entry.
- Menu and settings actions must call the same Rust-owned transition path.
  Neither React nor the taskbar renderer may persist a second preference copy
  or call operating-system startup APIs directly.

### Start-at-login state

- Add `launchAtLogin: boolean` to the complete persisted/emitted
  `DesktopPreferences` contract. Missing legacy values default to `false`.
- Use the official Tauri 2 autostart plugin from Rust. Do not add its JavaScript
  guest package or grant renderer permission to its commands.
- During desktop initialization, reconcile the loaded field with the native
  registration state when that state can be read, then build/synchronize the
  menu and emit the complete reconciled preference.
- A user transition applies and verifies the native registration before
  persisting, checking the menu, committing memory, and emitting the complete
  preference. Any later failure rolls the native registration and persisted
  file back best-effort and does not publish the proposed value.
- Start-at-login launches the normal application and honors the existing
  persisted main/taskbar visibility choices; it does not introduce a second
  process, updater, or separate minimized-mode preference.

### Compatibility

- Keep Windows and macOS as the supported desktop targets. Preserve all radar
  polling/source isolation, source attribution, window-position persistence,
  taskbar placement, close-to-hide, opacity, and tray recovery behavior.
- Legacy preference JSON without `launchAtLogin` remains valid and is upgraded
  on the next successful initialization persistence.

## Acceptance Criteria

- [x] Compact and detail headers each expose an accessible settings icon; the
  in-place settings view opens/closes without a new native window and restores
  the originating compact/detail geometry.
- [x] Settings can change source, boolean desktop options, opacity, and
  start-at-login; accepted changes update the view, both renderer projections,
  and native menu checks from one complete preference event.
- [x] Tray right-click and Windows taskbar-companion right-click show the same
  native menu, including a checked `开机自启` item, and taskbar right-click does
  not show the WebView menu or trigger the primary click action while pointer
  input is enabled.
- [x] Enabling start-at-login creates a native login registration, disabling it
  removes that registration, and both menu and settings reflect the verified
  result.
- [x] A fresh or legacy install defaults start-at-login to disabled; a readable
  existing native registration is reconciled into the complete preference on
  startup.
- [x] Native apply, verification, persistence, or menu synchronization failure
  retains/publishes the previous setting and keeps the tray recovery path.
- [x] Malformed renderer preference payloads, including a missing or non-
  boolean `launchAtLogin`, are rejected at the TypeScript boundary.
- [x] Frontend component/boundary tests cover settings navigation and actions,
  control accessibility, pending/error behavior, preference validation, and
  taskbar right-click regression.
- [x] Rust tests cover the new default/legacy serialization, menu ID mapping,
  option transitions, and preservation of all other fields.
- [x] `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm build`, Rust format,
  clippy, tests, and a Windows Tauri build pass; the running Windows app is
  checked for settings layout, shared context menu, and native autostart state.

## Out of Scope

- Startup delay, launch minimized/hidden as a separate mode, per-user profiles,
  update installation, installer publishing/signing, or Linux panel support.
- New theme/font/layout customization or exposing quick-position commands as
  persistent settings.
- Adding a normal operating-system taskbar button; the existing embedded
  taskbar companion and tray/status item remain the intended native surfaces.
