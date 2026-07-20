# Settings panel, context menu, and autostart implementation

## Checklist

- [x] Add the Rust autostart dependency/plugin registration and extend the
  desktop preference/menu/option transaction with verified native state.
- [x] Add focused Rust tests for default/legacy JSON, option/menu mapping, and
  preservation of unrelated preferences.
- [x] Extend the TypeScript preference contract, runtime guard, IPC adapter,
  and hook with start-at-login and radar-source setting actions.
- [x] Add the in-place `SettingsView`, settings icons in compact/detail, stable
  expanded layout, navigation/error/pending behavior, and semantic props.
- [x] Add frontend tests for preference validation, settings interactions,
  compact/detail navigation, and the existing taskbar context-menu path.
- [x] Run focused tests and formatters, then run the complete frontend/Rust
  gates and a Windows Tauri build.
- [x] Start the built/dev Windows app and verify settings, tray/taskbar shared
  menu checks, and start-at-login enable/disable against native state.
- [x] Update frontend/backend Trellis specs with the executable contract before
  commit.

## Validation commands

```powershell
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
```

## Validation record

- Frontend lint, typecheck, production build, and 58 tests passed.
- Rust format, all-target check, clippy with denied warnings, and 73 tests
  passed; the Windows Tauri build produced EXE, MSI, and NSIS artifacts.
- Windows release smoke covered the `400 x 520` settings layout, taskbar-window
  visibility transaction, and the start-at-login round trip. Enabling created
  the `Model Radar` `HKCU\\...\\Run` value and disabling removed it; the final
  preference and native registration are both disabled.
- The taskbar companion was confirmed as a visible `Tauri Window` child of
  `Shell_TrayWnd`. Its full-surface context-menu handler and single shared-menu
  IPC path are covered by frontend and Rust checks. Orca's Windows provider
  reports `surfaces.menus=false`, so the expanded native popup could not be
  inspected reliably in automation and remains a manual Windows visual check.
- macOS LaunchAgent registration and the native menu remain for macOS hardware
  verification; no Windows result is treated as evidence for those paths.

## Risk and rollback points

- `src-tauri/src/desktop.rs` contains the shared transactional path. Keep the
  preference lock ordering and apply/persist/menu/event sequence intact.
- Plugin registration must occur before `DesktopController::new` reads native
  state.
- Main-view navigation must update React only after native resize succeeds.
- Do not weaken the taskbar fixed `168 x 30` geometry or its full-surface
  context-menu handler while adding the shared menu item.
