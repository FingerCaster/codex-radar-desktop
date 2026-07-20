# Window position persistence and presets - implementation plan

## Ordered Checklist

- [x] Increase `.taskbar-effort` to the bounded `10px/700` treatment without
  changing taskbar tracks or dimensions.
- [x] Add native preset IDs/enum and pure work-area position helpers with tests
  for all five anchors, negative origins, and oversized windows.
- [x] Add separate saved-position JSON loading/persistence plus controller state
  and tests for valid, malformed, repeated, and failed/unchanged inputs.
- [x] Restore the hidden compact main window before visibility, selecting an
  intersecting monitor or centering safely when the stored display is gone.
- [x] Add a singleton revisioned writer for moved/resized/scale events, actual-
  size compact canonical capture, explicit resize marking, and close/quit/exit
  flushing through one geometry/write gate.
- [x] Add the native `快捷设置位置` submenu and route each command through one
  controller method that moves and persists without changing visibility.
- [x] Update README and backend/frontend desktop specs with the persistence,
  monitor-selection, menu, and taskbar typography contracts.
- [x] Run all frontend and Rust quality gates and rebuild MSI/NSIS artifacts
  from final source. Native Windows/macOS verification remains assigned to the
  user.

## Validation Commands

```powershell
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo check --manifest-path src-tauri/Cargo.toml --all-targets --all-features
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
pnpm exec tauri info
pnpm tauri build
```

## Review Gates

- Before event wiring: pure tests prove preset positions and canonical compact
  mapping on positive/negative work areas.
- Before persistence acceptance: an older delayed revision cannot overwrite a
  newer preset/flush revision, and malformed/off-screen data stays recoverable.
- Before menu acceptance: all five stable IDs map exactly once and explicit
  presets remain independent of `positionLocked`.
- Before bundle build: taskbar size/constants remain `168 x 30`, the taskbar
  window has no persisted position, and `DesktopPreferences` has no coordinate
  field.

## Risks And Rollback Points

- Native moved events can arrive during programmatic resize. Coalesce dirty
  signals and recalculate from actual outer geometry rather than event payloads
  or expanded-state timing.
- Monitor layouts and DPI can change between sessions. Match by physical
  intersection and center safely when no saved rectangle remains visible.
- Direct writes can fail or be interrupted. Preserve the last in-memory value
  unless serialization/write succeeds; invalid files fall back without panic.
- Window menu actions and delayed captures can race. Serialize native geometry
  and writes under one gate, capture off-main-thread Wry geometry before taking
  that gate, then use revision checks under the save-state mutex. Marshal resize
  transactions to the main thread and never hold the save-state mutex across
  Tauri native calls.

## Verification Results

- `pnpm lint`, `pnpm typecheck`, `pnpm test` (8 files / 25 tests), and
  `pnpm build`: passed.
- Rust fmt, all-target/all-feature check, 47 tests, and clippy with denied
  warnings: passed.
- `pnpm exec tauri info`: Tauri 2.11.5 / Wry 0.55.1 on Windows x64.
- `pnpm tauri build`: passed; final Windows artifacts:
  - MSI: `4,947,968` bytes, SHA256
    `B4ADD11F6E5AEFB8751C2E3DB113FFCD74DEC1B5E51AF41FEB6DEBE88FFC9117`
  - NSIS: `3,473,914` bytes, SHA256
    `901286C03779507DFA71D5C0784FD92550B1048CB7EF34894C8EC7ABBF25B8EE`
- Native position restore, menu interaction, and macOS behavior: pending user
  verification.
