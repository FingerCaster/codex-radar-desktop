# Taskbar leader companion - implementation plan

## Ordered Checklist

- [x] Add the `taskbar` Tauri window, transparent surfaces, and minimum window
  capability scope.
- [x] Add validated/persisted `DesktopPreferences` and a Rust controller that
  owns native transitions plus menu check state.
- [x] Replace the tray menu with checked controls, an exclusive opacity
  submenu, refresh, and quit; reuse it for companion right-click.
- [x] Independently implement the Windows Explorer taskbar host mechanism
  evidenced by TrafficMonitor, including fixed sizing, DPI, collision-aware
  free-slot placement, runtime reflow, failure recovery, and no copied
  Anti-996-licensed source.
- [x] Implement the macOS menu-bar projection with Tauri's native status-item
  title, using NetTool only as behavioral evidence and keeping Linux disabled.
- [x] Add the TypeScript desktop-state contract, runtime validator, listener,
  commands, and focused tests.
- [x] Add TaskbarView and route by WebView label; display model/effort/status
  above IQ/value/ties with one stable short dimension.
- [x] Make all drag regions conditional on position lock and apply validated
  opacity consistently to both windows.
- [ ] Run frontend and Rust checks, then inspect both windows on Windows at
  100% and high DPI. Verify tray recovery after enabling click-through.
- [x] Rebuild the Windows installers and update task/spec documentation with
  final platform behavior and limitations.

## Current Verification Status

- Automated frontend and Rust quality gates pass.
- Windows MSI/NSIS bundles are rebuilt from the final source.
- Native visual and interaction checks remain assigned to the user. In
  particular, verify coexistence with TrafficMonitor, the fixed short two-row
  projection, runtime reflow when either app starts first, and the locked
  compact-detail-compact position round trip.
- macOS code paths are reviewed but still require a native macOS build and
  interaction check before release.

## Automated Verification Results

Passed on Windows 10.0.26200 x64 with WebView2 148.0.3967.70:

```text
pnpm lint                                      passed
pnpm typecheck                                 passed
pnpm test                                      8 files / 25 tests passed
pnpm build                                     passed
cargo fmt --all -- --check                     passed
cargo check --all-targets --all-features       passed
cargo clippy --all-targets --all-features      passed with -D warnings
cargo test                                     38 tests passed
pnpm exec tauri info                           passed
pnpm tauri build                               MSI and NSIS passed
```

Final Windows artifacts:

```text
MSI  4,915,200 bytes
SHA256 A6F19841D4F4A33149C735CBC7BC12E65E6D928BE0A23FE969987E2F74E2C8A4

NSIS 3,452,801 bytes
SHA256 3135F9A685D53AE424EFFAC7493218CD0317AAA0353637D5A74C502FCF2F93CB
```

## Validation Commands

```powershell
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
pnpm exec tauri info
pnpm tauri build
```

## Review Gates

- Before UI integration: pure Rust tests prove validated opacity, visibility
  recovery, and Windows host geometry at 100/125/150/200 percent scaling.
- Before opacity acceptance: a screenshot must show desktop content through a
  sub-100-percent window; CSS fading into an opaque WebView is not acceptable.
- Before click-through acceptance: enable it, interact with the window below,
  then disable it from the tray without restarting the app.
- Before bundle build: the companion does not parse remote JSON or trigger its
  own timer, and all menu/window mutation paths use the controller.

## Risk And Rollback Points

- Transparent WebView composition is platform-dependent. Keep 100 percent as
  the validated fallback and report unsupported compositor behavior honestly.
- Click-through can make both windows inaccessible. The tray menu must be built
  before applying persisted click-through and must never inherit that flag.
- Native menu handles and state can drift if mutations occur outside the
  controller. Route every close, hide, show, and expand path through it.
- Explorer taskbar parenting can break across Windows shell revisions. Keep the
  native adapter narrow, detect failure, and preserve tray/main recovery.
- TrafficMonitor uses the Anti-996 License. Reimplement the observed Win32 host
  mechanism independently rather than copying its C++ implementation text.
- NetTool has no repository license and marks its source as all rights reserved;
  use the NSStatusItem concept only and do not copy Swift implementation text.
