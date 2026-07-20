# Windows taskbar reference research

## Source And License

- Local reference: `D:\UGit\TrafficMonitor`
- License: Anti 996 License 1.0 (`LICENSE`)
- Use: behavioral and Win32 mechanism evidence only. Do not copy C++ source
  text into Model Radar without accepting and distributing the reference
  license. The implementation in this task must be independently written.

## Relevant Files

- `TrafficMonitor/TaskBarDlg.cpp`: taskbar discovery, `SetParent`, DPI, fallback,
  transparency, and common lifecycle.
- `TrafficMonitor/Win11TaskbarDlg.cpp`: Windows 11 parent and notification-area
  positioning.
- `TrafficMonitor/ClassicalTaskbarDlg.cpp`: classic taskbar hierarchy and task-
  list space reservation/restoration.
- `TrafficMonitor/TaskbarHelper.cpp`: primary/secondary taskbar enumeration.
- `TrafficMonitor/TrafficMonitorDlg.cpp`: registered `TaskbarCreated` recovery.

## Mechanism Observed

1. Find the primary taskbar as class `Shell_TrayWnd`.
2. Windows 11 hosts the data window directly under that taskbar. It discovers
   `TrayNotifyWnd` and `Start`, converts their screen rectangles to taskbar-
   relative geometry, and positions the child immediately left of the
   notification area.
3. The classic path finds `ReBarWindow32` with `WorkerW` fallback, then
   `MSTaskSwWClass` with `MSTaskListWClass` fallback. It reserves space by
   shrinking/moving the task-list window and restores the original rectangle
   when the companion closes.
4. TrafficMonitor uses the taskbar rectangle as its MFC DPI source. A Tauri
   WebView is different: logical CSS dimensions must be converted with the
   child WebView's own `scale_factor()`, not Explorer's taskbar DPI.
5. The mature app registers the `TaskbarCreated` shell message and reattaches
   after Explorer restarts. A first version that omits this must preserve tray
   and main-window recovery and document the limitation.

## Tauri Adaptation Constraints

- TrafficMonitor reparents an MFC dialog, but Tauri 2.11.5 exposes
  `WebviewWindowBuilder::parent_raw(HWND)` on Windows. Prefer creating the
  taskbar WebView as a real child from the start so Tao tracks the child style;
  manual `SetParent` and style mutation are fallback mechanisms only.
- If a legacy fallback ever uses `SetParent`, save/restore the original style.
  `SetParent` can successfully return a null previous parent, so raw error state
  must be checked instead of treating a null return as failure.
- Keep the Win32 adapter under `cfg(target_os = "windows")` and expose only a
  small attach/detach API to the cross-platform controller.
- Do not mutate the classic task-list rectangle in the first version unless the
  exact prior rectangle is stored and restored on hide, failure, and exit.
- Current local acceptance host reports build 26200 and exposes the Windows 11
  taskbar path; this does not prove classic or secondary-taskbar support.
- A taskbar child is clipped by its parent. Keep the only layout at 168 x 30
  logical pixels. Reject geometry that cannot fit instead of silently shrinking
  the native child below its CSS viewport.
- Win11 does not reserve space for third-party taskbar children. Enumerate
  visible external descendants, treat their rectangles as blockers, and place
  the companion in the rightmost free slot between the task band and
  notification area.

## Bug Analysis: Runtime taskbar collisions

### 1. Root Cause Category

- **Category**: E - Implicit assumption, with a D - Test coverage gap.
- **Specific cause**: The first blocker-aware placement assumed taskbar child
  rectangles were stable after startup. TrafficMonitor can start later and the
  task-button band can grow while Model Radar is already running.

### 2. Why The First Fix Was Incomplete

1. Startup blocker enumeration fixed only the launch-order where the external
   window already existed.
2. Static geometry tests proved a free slot could be selected but did not prove
   that the calculation was invoked again after the blocker set changed.
3. A placement error alone left the old child at its previous rectangle, so a
   no-space result could preserve the overlap it was meant to prevent.

### 3. Prevention Mechanisms

| Priority | Mechanism | Specific action | Status |
| --- | --- | --- | --- |
| P0 | Runtime | Recompute Windows blockers once per second while the projection is enabled | DONE |
| P0 | Architecture | On placement failure, use a dedicated non-rollback demotion to main/tray recovery | DONE |
| P0 | Test | Add a two-step geometry regression where a later blocker moves the child left | DONE |
| P1 | Review | Treat embedded shell layout as mutable state, not startup configuration | DONE |

### 4. Systematic Expansion

- **Similar issues**: Task-button growth and notification-area width changes
  have the same lifecycle as a later TrafficMonitor launch.
- **Design improvement**: Keep geometry pure and repeatable; keep monitoring,
  preferences, and failure recovery in `DesktopController`.
- **Process improvement**: Native taskbar review must include both launch
  orders and a no-space transition, not only a startup screenshot.

### 5. Knowledge Capture

- [x] Update the backend desktop companion contract.
- [x] Add runtime blocker and idempotent-position tests.
- [x] Record the one-second reaction window and Explorer restart limitation.
