# Windows `WH_MOUSE_LL` 生命周期研究

日期：2026-07-21
范围：`src-tauri/src/desktop/windows.rs` 的任务栏点击钩子，以及
`src-tauri/src/desktop.rs` 中启用、监控、降级和退出的调用路径。

## 结论

当前实现确实没有可靠生命周期：钩子安装在哪个调用线程取决于调用路径，
关闭任务栏偏好时只清空命中矩形，不卸钩；退出时也不卸钩；
`TASKBAR_HOOK_INSTALLED` 只能表示一次安装曾成功，不能表示钩子仍存在。

建议采用 **一个进程级、专用 Win32 消息线程 + 显式启停 + 30 秒租约轮换**：

- 专用线程创建消息队列、安装/卸载钩子并泵送 `GetMessageW`；所有
  `HHOOK` 变更只在该线程串行发生。
- 回调只读取无阻塞命中快照、递增事件心跳、用 `PostThreadMessageW`
  投递左/右键动作，然后立即 `CallNextHookEx`。不在回调里锁
  `AppHandle`、启动 Tokio 任务、写日志或执行任何 Tauri/Wry 操作。
- 偏好关闭时先使命中快照失效，再同步发送 `Disable` 并等待卸钩确认；
  偏好重新开启时，无论调用来自 Tauri 主线程还是 Tokio 工作线程，均由
  专用线程安装，因此不依赖调用方是否有 Win32 消息循环。
- 微软明确说明静默摘钩后 **没有查询 API 可以得知**。因此
  `installed=true` 不能作为健康证明。每 30 秒在专用线程上执行一次
  `UnhookWindowsHookEx(old) -> SetWindowsHookExW(new)`；若监控 tick 发现
  “系统鼠标位置已改变但钩子事件计数未前进”，则提前轮换。前者提供
  有界的最终恢复，后者让正常鼠标移动场景通常在下一个 1 秒 tick 恢复。
- 退出通过 `Shutdown` 控制消息、完成通知和有界等待同步：线程先清命中、
  卸钩、发送完成通知并退出；调用方最多等待约 500 ms，仅在
  `JoinHandle::is_finished()` 后 `join`。超时则记录一次警告并让进程退出
  负责最终系统清理，不能无限阻塞 Tauri 事件循环。

30 秒是建议初值：轮换只有每分钟约四次 User32 调用，同时正常鼠标移动
有 1 秒心跳兜底；应在 Windows 10/11 压测后再调。不要每秒无条件轮换，
也不要把 30 秒租约描述成对“钩子仍活着”的检测。

## 微软官方契约

| 契约 | 对本项目的含义 |
| --- | --- |
| [`LowLevelMouseProc`](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc)：回调在安装钩子的线程上下文执行，系统通过向该线程发送消息调用它，因此安装线程必须有消息循环。 | 不能从任意 Tauri/Tokio 调用线程直接安装。专用线程必须持续泵消息。 |
| 同页：回调超过 `LowLevelHooksTimeout` 后，Windows 7+ 会静默移除钩子，应用无从得知；Windows 10 1709+ 上限为 1000 ms。微软建议专用线程把工作交出去后立即返回。 | 回调路径必须无阻塞；不存在可信的 `IsHookAlive` 实现，恢复只能靠启发式和主动轮换。 |
| [`SetWindowsHookExW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowshookexw)：成功返回 `HHOOK`，失败返回 `NULL`；`WH_MOUSE_LL` 仅支持全局范围；退出前必须调用 `UnhookWindowsHookEx`。 | 句柄必须被真实拥有和释放，不能只存一个布尔值。全局钩子只应在任务栏投影启用时存在。 |
| [`UnhookWindowsHookEx`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-unhookwindowshookex)：返回后仍可能有其他线程正在执行回调。 | 回调桥接状态需保持进程级稳定，不能在卸钩后立刻释放其内存。对本设计而言，安装、回调和卸载都在同一专用线程串行，可进一步缩小竞态。 |
| [`PostThreadMessageW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postthreadmessagew)：目标线程尚无消息队列时调用会失败；官方做法是在目标线程先用 `PeekMessage` 强制建队列，再通知调用方 ready。线程消息不会由 `DispatchMessage` 分发，模态循环还可能丢失它们。 | 启动必须有 ready 握手；专用线程直接检查自定义消息，不进入对话框/模态循环。Rust 命令数据放进本进程队列，自定义消息只作唤醒信号。 |
| [`GetMessageW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmessagew)：普通消息返回正值，`WM_QUIT` 返回 0，错误返回 -1。`hWnd=NULL` 会同时取窗口消息和线程消息。 | 循环必须显式处理 `-1/0/>0`，不能写成简单的 `while GetMessage(...)`。 |
| [`CallNextHookEx`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-callnexthookex)：第一个 `HHOOK` 参数已忽略；通常必须继续调用钩子链。 | 回调可传空句柄，不必读取可能已经过期的全局 `TASKBAR_MOUSE_HOOK`。 |

以上页面读取于 2026-07-21；`LowLevelMouseProc` 页面最后更新
2025-07-14，其余 API 页面最后更新日期分别为 2024-01-26、
2024-02-23、2023-02-09、2023-02-09 和 2021-10-13。

## 当前实现的具体问题

1. `windows.rs:133-150` 在调用者线程直接执行 `SetWindowsHookExW`。
   首次初始化可能恰好发生在 Tauri 主线程，但运行期重开会经过
   `#[tauri::command(async)]`、菜单异步任务或监控任务；这些调用点没有
   Win32 消息循环保证，成功返回也不等于以后能收到回调。
2. `windows.rs:24-27` 的四个进程全局变量没有生命周期所有者。
   `TASKBAR_HOOK_INSTALLED` 成功后永不复位，静默摘钩后后续 ensure 永远
   早退；`TASKBAR_MOUSE_HOOK` 也从未卸载。
3. `windows.rs:197-245` 的回调路径会阻塞式获取命中矩形锁；命中后又获取
   `AppHandle` 锁并调用 `tauri::async_runtime::spawn`。任何锁竞争、分配或
   runtime 异常延迟都计入系统的低级钩子超时预算。
4. `hide_taskbar_window` 只调用 `clear_taskbar_hit_rect` 后隐藏窗口。
   投影关闭后钩子仍收到整个桌面的每次鼠标事件。
5. `desktop.rs:1606-1611` 的退出处理只刷新主窗口位置；没有 hook shutdown。
6. `ensure_taskbar_projection` 每秒调用安装函数，但布尔早退使它既不能验证
   安装线程，也不能修复静默摘钩。

## 推荐结构

### 所有权与状态

让 `DesktopController` 在 Windows 上拥有一个 `TaskbarInputController`；
不要再让几个彼此独立的静态变量冒充管理器。Win32 回调没有上下文参数，
仍需一份进程级稳定的 `HookBridge`，但它只包含：

- 命中矩形的无阻塞快照（建议序列号 + 四个 `AtomicI32`；读取冲突时本次
  直接跳过，不能自旋等待）；
- 专用线程 ID；
- `desired_enabled`、鼠标事件计数和丢弃事件计数等原子诊断值。

`HHOOK`、安装时刻和实际 Win32 状态只存在于专用线程局部变量中。
控制器可暴露“上一次命令成功/租约时间”，但字段名应避免 `healthy` 或
`installed` 这种超出系统可证明范围的表述。

建议状态机：

```text
WorkerStopped
  -> WorkerReady / HookDisabled
  -> HookLeased { installed_at, event_seq }
  -> HookDisabled                    (Disable 成功或句柄已失效)
  -> HookLeased(new generation)      (Rearm 成功)
  -> HookUnavailable { last_error }  (安装失败，交给投影监控降级)
  -> WorkerStopped                   (Shutdown)
```

### 线程与控制消息

1. `std::thread::Builder` 创建具名线程，例如
   `model-radar-taskbar-hook`。
2. 线程调用 `PeekMessageW(..., PM_NOREMOVE)` 创建消息队列，取得
   `GetCurrentThreadId()`，再通过 Rust channel 返回 ready 结果。
3. 控制方先把带一次性 ack sender 的 `Enable/Disable/Rearm/Shutdown`
   放入有界 Rust 队列，再投递一个 `WM_APP + n` 作为唤醒。若
   `PostThreadMessageW` 失败，撤回/失败该命令，不能假装成功。
4. 线程使用 `GetMessageW(NULL, 0, 0)`；收到控制消息就 drain 命令队列，
   收到点击消息才调用当前 Tauri 分发函数。分发发生在 hook 回调之外，
   并继续只把真正的业务工作提交给 async runtime。
5. 线程不得展示对话框或进入模态消息循环，否则官方文档指出线程消息
   可能丢失。

### 回调热路径

```text
if code != HC_ACTION:
    return CallNextHookEx(NULL, ...)

event_seq += 1                         // 所有鼠标事件，用于心跳
if message is not LBUTTONUP/RBUTTONUP:
    return CallNextHookEx(NULL, ...)

rect = one-shot atomic snapshot        // 失败即跳过，不锁、不自旋
if desired_enabled && rect.contains(pt):
    PostThreadMessageW(hook_thread, CLICK, left_or_right, 0)
return CallNextHookEx(NULL, ...)
```

`PostThreadMessageW` 失败时只递增原子丢弃计数；禁止在回调里格式化错误或
`eprintln!`。工作线程/监控 tick 再读取并限频记录该计数。

### 启用、关闭与轮换

- **启用**：创建/放置 companion，写入最终屏幕矩形，发送 `Enable` 并等
  安装 ack，之后才把主窗口隐藏。安装失败必须沿现有安全降级路径保留主窗。
- **位置更新**：只更新原子快照，不重装钩子。
- **关闭**：先 `desired_enabled=false` 并清空矩形，再发送 `Disable`；专用
  线程调用 `UnhookWindowsHookEx` 后 ack。即使 companion HWND 已丢失也要
  执行 disable，不能把卸钩绑在 `hide_taskbar_window(window)` 是否有窗口上。
- **轮换**：同一线程先卸旧句柄，再装新句柄。旧句柄已被系统静默摘除时，
  `UnhookWindowsHookEx` 的“无效句柄”可按“已经卸载”继续；其他卸载错误
  不应盲目安装第二个钩子，以免重复分发。新安装失败则标记 unavailable，
  让 `monitor_taskbar_once` 立即执行主窗恢复和偏好降级。
- **事务回滚**：启用失败后 rollback 的 disable 必须幂等；关闭失败后的
  rollback enable 也必须幂等。不要用 `swap(true)` 先声明成功。

### 静默摘钩恢复

微软不提供正向查询，因此只能组合两种信号：

1. **事件心跳启发式**：hook 对每个 `HC_ACTION` 鼠标事件递增计数。现有
   1 秒 taskbar monitor 同时采样 `GetCursorPos`；若位置变化而计数未变化，
   将租约标为可疑并立刻 `Rearm`。这不是证明，误判只造成一次安全轮换。
2. **固定租约**：无论心跳如何，每 30 秒主动 `Rearm`，保证即使光标一直
   静止、钩子在两个样本之间被摘除，也不会永久失效。

不要用 `SendInput` 做生产心跳：它虽然能制造可识别的注入输入并得到更强
信号，但会改变系统输入/空闲状态，并影响同桌面其他应用。可以仅用于手工
Windows smoke test。

## 方案比较

| 方案 | 优点 | 缺点 | 结论 |
| --- | --- | --- | --- |
| 调用线程直接安装 + 原子布尔 | 改动最小 | 无消息循环保证，无卸钩，静默移除永久失效 | 淘汰 |
| 专用消息线程，但只安装一次 | 正确线程模型，可显式卸钩 | 仍无法恢复 `LowLevelHooksTimeout` 静默移除 | 不足 |
| 专用线程 + 每秒无条件重装 | 恢复上界短 | 频繁扰动全局 hook chain，每次都有短点击空窗和失败面 | 不建议 |
| 专用线程 + 30 秒租约 + 1 秒心跳疑点提前轮换 | 线程契约正确，关闭零全局监听，正常移动时恢复快，静止时也有 30 秒上界 | 有极短轮换空窗；30 秒参数需实机验证 | **推荐** |
| 先装新钩子再卸旧钩子 | 理论上没有轮换空窗 | 两个钩子短暂同时调用同一静态回调，难以区分 generation，可能双击分发 | 不建议 |
| `SendInput` 定期自检 | 能获得较强端到端信号 | 注入全局输入、有系统副作用，安全软件也可能拦截 | 仅手工测试 |
| 改为 Raw Input + message-only window | 不受 `LowLevelHooksTimeout` 影响，微软一般更推荐 | 是输入架构重写；需验证后台接收、按钮事件、屏幕坐标、Explorer/WebView2 场景 | 可做后续独立任务，不作为本次最小修复 |

## 退出同步

正常退出应由 `handle_app_exit` 调用一次幂等 `shutdown(timeout)`：

1. 立即清命中快照，阻止任何新业务分发。
2. 入队 `Shutdown` 并 `PostThreadMessageW` 唤醒线程。
3. hook 线程执行 `UnhookWindowsHookEx`，清状态，发送 `done`，退出循环。
4. 调用方等待 `done` 最多 500 ms；随后只在 `JoinHandle::is_finished()` 为
   true 时调用 `join`，避免 Tauri 退出线程无界阻塞。
5. 发送消息失败或超时时记录一次错误并放弃 join；进程终止会释放剩余
   User32 资源。`Drop` 可再做 best-effort，但不能代替显式退出路径。

关闭到托盘不是进程退出，不能 shutdown worker；它只根据
`showTaskbarWindow` 执行 `Enable/Disable`。

## 测试切口

1. 抽出纯函数 `classify_mouse_event(code, message, point, rect)`：覆盖左/右
   button-up、无关消息、负 code、无矩形、负屏幕坐标，以及右/下边界排除。
2. 把 Tauri 分发放在可注入的 `TaskbarActionSink` 后：验证左键只映射
   show-details/force-show，右键只映射 context menu；这补上当前两个
   `dispatch_taskbar_*` 的缺口。
3. 用 fake `HookBackend` 测状态机：首次 enable、重复 ensure、disable
   幂等、disable 后 re-enable、租约到期轮换、可疑心跳提前轮换、旧句柄
   无效后重装、新安装失败置 unavailable、未知卸载失败不安装第二份钩子。
4. 用 fake wake/ack 测 worker 协议：`PeekMessage` ready 前不能发命令，
   `PostThreadMessage` 失败要返回错误，`GetMessage=-1` 要清理退出，shutdown
   超时不能无限 join。
5. Controller 集成测试：任务栏-only 隐藏主窗前必须收到 enable ack；
   enable/rearm 失败保持或恢复主窗；关闭偏好即使 companion HWND 不存在也
   调用 disable；monitor 降级与 app exit 都调用幂等 shutdown/disable。
6. Windows 手工 smoke：用带唯一 `dwExtraInfo` 的 `SendInput` 验证 hook
   线程收到事件、disable 后不再收到、re-enable 恢复、强制 rearm 不双发。
   该测试应标为 `ignored`/手工执行，避免 CI 注入桌面输入。

## 实现边界

- 不需要新三方依赖；现有 `windows` crate feature 已包含主要
  WindowsAndMessaging API，继续手写 FFI 时则必须补齐
  `UnhookWindowsHookEx`、`PeekMessageW`、`GetMessageW`、
  `PostThreadMessageW`、`GetCurrentThreadId` 等正确 ABI 声明。
- 需要更新 `.trellis/spec/backend/desktop-companion-contract.md`：明确专用
  消息线程、偏好关闭即卸钩、租约语义、回调禁止阻塞以及退出同步。
- Raw Input 是合理的长期替代，但不应和本次窗口重建、偏好恢复修复混在
  同一变更中。
