# Journal - FingerCaster (Part 1)

> AI development session journal
> Started: 2026-07-19

---

## Session 1: Taskbar companion implementation

**Date**: 2026-07-20
**Task**: Taskbar companion implementation

### Summary

Implemented Windows taskbar and macOS menu-bar leader projections, native checked controls, WebView-DPI geometry, reversible window resizing, passive renderer behavior, tests, specs, and final Windows bundles. Native visual and interaction verification remains assigned to the user; macOS still requires a native build.

### Main Changes

- Added the Windows taskbar child and macOS status-item projection.
- Added checked native controls, validated preferences, and passive taskbar data flow.
- Fixed WebView DPI cropping and reversible compact/detail window placement.
- Rebuilt Windows MSI/NSIS artifacts and documented platform limits.

### Git Commits

(No commits - the workspace root is not a Git repository.)

### Testing

- Frontend lint, typecheck, 24 tests, and production build passed.
- Rust fmt, check, clippy with denied warnings, and 28 tests passed.
- Windows MSI and NSIS bundle generation passed.
- Native visual/interaction verification was delegated to the user.

### Status

[IN PROGRESS] **Implementation complete; native verification pending**

### Next Steps

- Verify Windows taskbar pixels, context menu, click-through recovery, and locked resize round trip.
- Build and verify the menu-bar behavior on a native macOS machine.

---

## Session 2: Compact taskbar layout and runtime collision recovery

**Date**: 2026-07-20
**Task**: Taskbar companion follow-up

### Summary

Removed the wider information mode, fixed the companion at 168 x 30 with the
requested two-row model/effort/status and IQ/value/ties layout, and made Windows
placement react to taskbar children that appear after startup. A no-space or
dead-host condition now disables the companion and restores the main/tray path
instead of retaining an overlapping rectangle.

### Main Changes

- Removed the former wider-information option from the menu, preferences,
  renderer state, and docs; retained only the old-JSON migration regression.
- Added blocker-aware one-second Windows reflow, idempotent native positioning,
  serialized state handling, and non-rollback safety demotion.
- Moved the taskbar live status outside button semantics for reliable screen
  reader announcements.
- Rebuilt the final Windows MSI and NSIS bundles from the updated source.

### Git Commits

(No commits - the workspace root is not a Git repository.)

### Testing

- Frontend lint, typecheck, 25 tests, and production build passed.
- Rust fmt, all-target check, clippy with denied warnings, and 38 tests passed.
- Windows MSI and NSIS bundle generation passed.
- Native visual/interaction verification remains assigned to the user.

### Status

[IN PROGRESS] **Automated implementation complete; native verification pending**

### Next Steps

- Verify both launch orders with TrafficMonitor and the one-second runtime move.
- Verify the fixed two-row projection and locked compact/detail position round trip.
- Build and verify the menu-bar behavior on a native macOS machine.

---

## Session 3: Window position persistence and quick presets

**Date**: 2026-07-20
**Task**: Window position persistence and presets

### Summary

Increased the fixed taskbar effort label to 10px/700, added private main-window
position persistence and startup recovery, and added five native quick-position
presets shared by the tray, Windows taskbar context menu, and macOS status item.

### Main Changes

- Saved compact-equivalent main-window coordinates and restored them before the
  first show, including disconnected and mixed-DPI monitor recovery.
- Added `快捷设置位置 > 上左 / 上右 / 中心 / 下左 / 下右` without changing
  visibility or being blocked by drag locking.
- Fixed a Wry event-loop / geometry-gate ABBA deadlock and writer-error lost
  wakeup; recorded the runtime thread-affinity contract in backend specs.
- Rebuilt final Windows MSI and NSIS bundles.

### Git Commits

(No commits - the workspace root is intentionally not a Git repository.)

### Testing

- Frontend lint, typecheck, 25 tests, and production build passed.
- Rust fmt, all-target check, clippy with denied warnings, and 47 tests passed.
- Windows MSI and NSIS bundle generation passed.
- Native visual/interaction verification remains assigned to the user.

### Status

[IN PROGRESS] **Automated implementation complete; native verification pending**

### Next Steps

- Verify drag/restart restore, expanded-window restore, all five preset menu
  actions, locked presets, and hidden-window positioning on Windows.
- Build and verify the same native menu and restore behavior on macOS.

## Session 2: Desktop settings and start-at-login

**Date**: 2026-07-20
**Task**: Desktop settings and start-at-login
**Branch**: `main`

### Summary

Added in-place desktop settings, shared tray/taskbar menu start-at-login control, verified Windows autostart registration round trip, synchronized specs, and preserved native menu/macOS visual checks as residual platform validation.

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `a86d658` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete

## Session 3: Desktop visual polish and quick positions

**Date**: 2026-07-21
**Task**: Desktop visual polish and quick positions
**Branch**: `main`

### Summary

Refined all desktop projections, bundled Sol/Terra/Luna and Codex model marks, added five settings quick-position controls through the shared native transaction, verified Windows 150% DPI behavior, and rebuilt MSI/NSIS installers.

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `b57501b` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete

## Session 4: Publish Codex Radar Desktop v0.2.0

**Date**: 2026-07-21
**Task**: Publish Codex Radar Desktop v0.2.0
**Branch**: `main`

### Summary

Synchronized application metadata to 0.2.0, passed the complete frontend and Rust release gates, built and checksummed Windows MSI/NSIS installers, pushed main and annotated v0.2.0, and published a verified latest GitHub Release with three assets.

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `3120480` | (see git log) |
| `510f4ec` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete

## Session 5: Archive completed Codex Radar tasks

**Date**: 2026-07-21
**Task**: Archive completed Codex Radar tasks
**Branch**: `main`

### Summary

Closed the delivered MVP, taskbar companion, distributed radar source, and one-time guideline bootstrap records after the v0.2.0 release; preserved residual macOS/native verification notes and archived all remaining active Trellis tasks.

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `c242465` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete

---

## 2026-07-21 — Desktop recovery harden

### Goal

Prevent “main + taskbar both gone / off-screen” dead-ends after taskbar companion failure.

### Changes

- Taskbar: rebuild detached companion; health-check after place/show
- Visibility: establish taskbar before hiding main
- Recovery: clamp main into a work area when fully off-screen
- Spec: desktop-companion-contract.md
- Task: 07-21-desktop-recovery-harden

### Validation

- cargo test (76 ok)
- cargo clippy -D warnings


## Session 6: Windows 任务栏生命周期与 DPI 恢复

**Date**: 2026-07-22
**Task**: Windows 任务栏生命周期与 DPI 恢复
**Branch**: `main`

### Summary

修复 WH_MOUSE_LL 专用线程生命周期、Explorer 异步重建、偏好与原生可见性恢复，并以子 HWND DPI 下限和窄客户区 CSS 兜底解决任务栏内容裁切；完成 Rust/前端全量验证及 MSI/NSIS 构建。

### Main Changes

- Detailed change bullets were not supplied; see the summary above.

### Git Commits

| Hash | Message |
|------|---------|
| `93cbcf2` | (see git log) |

### Testing

- Validation was not recorded for this session.

### Status

[OK] **Completed**

### Next Steps

- None - task complete
