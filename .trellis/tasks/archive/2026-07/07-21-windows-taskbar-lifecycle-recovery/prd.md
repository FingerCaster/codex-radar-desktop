# 修复 Windows 任务栏伴侣生命周期与恢复一致性

## Goal

让 Windows 任务栏伴侣在 Explorer 重建、低级鼠标钩子失效、原生窗口状态与偏好漂移等情况下仍能自愈，并保证托盘第一次点击始终尝试恢复不可见的主窗口。

## Background

- `src-tauri/src/desktop/windows.rs:133` 安装进程级 `WH_MOUSE_LL` 后只用布尔值记录“已安装”；代码没有显式卸载、超时摘钩后的恢复或安装线程消息循环保证。
- `src-tauri/src/desktop/windows.rs:71` 对 detached taskbar 调用异步 `close()` 后立即复用 `taskbar` label；`src-tauri/src/lib.rs:50` 又对所有 `CloseRequested` 无条件 `prevent_close()`，因此旧窗口不会按预期销毁。
- `src-tauri/src/desktop.rs:1186` 在原生主窗口恢复前提交 `showMainWindow=true`；恢复失败后，`src-tauri/src/desktop.rs:752` 的偏好驱动 toggle 会先走隐藏分支。
- `src-tauri/src/desktop.rs:625` 持有 preference mutex 调用 `apply_option`；显示路径会执行 Wry getter/setter，并可能在 `src-tauri/src/desktop.rs:1463` 移动窗口。
- 稳定几何已经在 `src-tauri/src/desktop/windows.rs:309` 短路，不需要再缓存 `Shell_TrayWnd` 或取消动态 blocker 检查。
- `src-tauri/src/desktop/windows.rs:265` 使用递归父链语义的 `IsWindowVisible` 作为健康条件，无法区分 child 自身隐藏与 taskbar 父窗口的显示策略。
- 现有 `runtime_taskbar_failure_disables_projection_and_restores_main` 只测试偏好转换；没有覆盖 taskbar 输入分发、异步重建、显示前置条件或原生/偏好漂移。

## Requirements

### R1. 低级鼠标钩子生命周期

- `WH_MOUSE_LL` 必须由具有 Win32 消息循环的专用线程拥有。
- hook callback 只能做有界、非阻塞的命中计算和消息投递；不得获取应用 mutex、执行 Tauri 操作或直接调度异步任务。
- 关闭 taskbar 偏好和应用退出时必须显式 `UnhookWindowsHookEx` 并结束专用线程。
- Windows 无法查询静默摘钩状态，因此使用 30 秒 hook 租约轮换限制无声失效时间；monitor 每秒比较鼠标位置和 hook 事件序号，位置变化但序号不变时提前轮换。
- enable、disable、rearm 和 shutdown 必须有显式完成确认；安装或重新安装失败时把 taskbar projection 判为失败。

### R2. Detached taskbar 异步重建

- detached companion 必须使用不触发 close-to-hide 业务逻辑的强制销毁 API。
- 销毁和同 label 重建不得发生在同一个同步调用中；destroy 只发起一次，Manager 注销前返回“重建中”，由后续 monitor tick 重试。
- Explorer host 暂时缺失、旧 label 等待注销、build 期间 host generation 改变都属于瞬态恢复；不得在第一次观察时永久关闭偏好。
- 重建等待使用 10 秒单调时钟宽限期。等待期间先原生显示主窗口；超时或确定性创建/放置失败后执行既有安全降级。
- 成功重建后才允许按 `showMainWindow=false` 再次隐藏临时恢复的主窗口。

### R3. 原生可见性与偏好一致性

- 非 macOS 托盘左键必须优先参考主窗口的原生可见性；原生不可见或查询失败时偏向 force-show，不能因 `showMainWindow=true` 而先隐藏。
- taskbar monitor 发生永久失败时，先尝试原生恢复，再提交安全偏好；即使恢复失败，下一次托盘点击也必须再次尝试显示。
- 原生恢复、偏好持久化、菜单同步和事件失败继续聚合诊断，不得回滚到已知失效的 taskbar projection。

### R4. Preference 锁边界

- preference mutex 只保护内存快照/提交，不得跨 Wry/Win32 getter、setter、窗口创建、放置或显示隐藏操作。
- 使用独立的 preference transition gate 串行化 option、opacity、radar source、emergency recovery 和 monitor demotion 等所有偏好写路径及回滚，保留快速连续点击不会基于同一旧值提交的语义。
- monitor 的普通 tick 仍不得持有 transition gate 或 preference mutex 执行 taskbar placement。

### R5. 健康检查与稳定 tick

- taskbar 健康检查必须验证 HWND 存活、挂接到当前主 taskbar，并检查 child 自身的可见样式；不能因父 taskbar 的自动隐藏策略误判 child 已被业务隐藏。
- 保留每秒 blocker/geometry 重算以及 Explorer 重启发现能力；禁止缓存长期 `Shell_TrayWnd` 句柄。
- 几何不变时继续保证不调用 `SetWindowPos`。taskbar 偏好关闭后不得保留全局鼠标 hook。

### R6. 回归测试与规格

- 覆盖鼠标消息到 left/right action 的纯决策和 hit-rect 边界。
- 覆盖 hook 启停/轮换状态决策、taskbar rebuild pending/timeout/complete 状态。
- 覆盖 `apply_main_window_visibility` 的“确保 taskbar 失败 -> 恢复 main -> 返回错误且不隐藏”调用顺序。
- 覆盖偏好显示但原生隐藏时，第一次托盘 toggle 选择 force-show。
- 覆盖用户 close-to-hide 与内部 destroy 的策略边界；程序化 rebuild 不得经过 `CloseRequested`。
- 更新 desktop companion contract 与 backend quality test inventory。

### R7. 固定任务栏客户区适配

- `parent_raw` 重挂到 `Shell_TrayWnd` 后，Tauri scale 可能低于子 WebView
  HWND 的实际 DPI。原生尺寸必须使用
  `max(window.scale_factor(), GetDpiForWindow(child) / 96)`；子 DPI 为 0 时
  回退 Tauri scale，禁止读取 Explorer/taskbar HWND 的 DPI。
- Windows 嵌入和 DPI 换算可能让 WebView 实际客户区短暂小于名义
  `168 x 30` CSS viewport；任务栏根节点必须受父客户区约束，不能继续按
  168px 溢出后裁掉右侧状态。
- 首行模型名使用可收缩轨道，图标、effort 和三字状态保持稳定；第二行
  IQ 值使用可收缩轨道，tie 标记靠右。长文本省略但不得互相遮挡或改变
  原生窗口尺寸。

## Acceptance Criteria

- [x] AC1: taskbar preference 从关到开时，无论调用来自 setup、菜单、IPC 或 monitor，hook 都只在专用消息线程安装并能够接收 left/right action。
- [x] AC2: taskbar preference 关闭或应用退出后，hook handle 清零且线程可回收；30 秒租约或心跳疑点触发的重装失败会在 monitor 中产生安全降级，而不是保持虚假的健康标记。
- [x] AC3: detached taskbar 第一次 tick 只发起 destroy 并报告 rebuilding，不尝试同 label build；Manager 注销后的 tick 创建新 companion。
- [x] AC4: Explorer/label 的瞬态 rebuilding 在 10 秒内保持偏好且主窗口可见；成功后才恢复 taskbar-only 投影，超时则持久化 `showTaskbarWindow=false`、`showMainWindow=true`。
- [x] AC5: 原生主窗口隐藏但偏好为 true 时，第一次托盘左键执行显示而不是隐藏。
- [x] AC6: `commit_option`、显示 apply 和 monitor recovery 不持有 preference mutex 跨 Wry/Win32 调用，同时用户 option transitions 仍串行。
- [x] AC7: 自动隐藏父 taskbar 不会仅因祖先 `WS_VISIBLE` 状态导致 child 健康误判；稳定 tick 不重复 `SetWindowPos`。
- [x] AC8: 新增单元测试覆盖 R6，全部 Rust tests、fmt 和 clippy `-D warnings` 通过。
- [x] AC9: 在名义 `168 x 30` 以及更窄父客户区中，模型、effort、状态、
  IQ 和 tie 均留在两行固定布局内；右侧状态不再被相邻任务栏组件截断，
  子 HWND 150% DPI 能得到 `252 x 45` 完整物理客户区，前端 lint、
  typecheck、tests 和 build 通过。

## Out Of Scope

- 替换 taskbar companion 的 WebView2/`Shell_TrayWnd` 嵌入方案。
- 用 Raw Input 或 UI Automation 全面替换 `WH_MOUSE_LL`。
- 缓存 Explorer/taskbar HWND，支持副任务栏或经典任务栏。
- 修改 macOS、Linux、前端 DTO、整体视觉风格或任务栏几何选槽算法。
