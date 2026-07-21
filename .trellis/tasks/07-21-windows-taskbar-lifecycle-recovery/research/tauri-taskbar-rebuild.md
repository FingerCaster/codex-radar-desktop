# Tauri taskbar companion rebuild research

## 结论

- 当前 `close()` 路径不能销毁 detached `taskbar`。`src-tauri/src/lib.rs`
  对所有 `CloseRequested` 无条件 `prevent_close()`，随后
  `handle_close_requested()` 还会把 `taskbar` 当成用户关闭并执行
  `showTaskbarWindow=false`。因此 `windows.rs` 中 `close()` 后立即重建不只是
  有竞态：旧窗口通常根本不会进入销毁流程。
- 改成 `destroy()` 是必要条件，但不能在同一调用栈立刻用相同 label 重建。
  Tauri/Wry 的 `destroy()` 只向事件循环投递 `WindowMessage::Destroy`；Manager
  要等后续原生 `Destroyed` 才从 window/webview map 删除 label。`destroy()`
  返回只表示投递成功，不表示 label 已释放。
- 保留固定 label `taskbar`，采用非阻塞的“两阶段重建”：请求销毁一次，后续
  monitor drive 只有在 `get_webview_window("taskbar").is_none()` 后才创建。
  不建议使用 generation label：前端 `src/App.tsx` 以 label 严格等于
  `taskbar` 决定渲染伴侣视图，而且项目契约要求恰好一个 `taskbar` WebView。
- Explorer 重启属于可恢复的瞬态，不应在第一次找不到 `Shell_TrayWnd` 或首次
  发现 detached child 时立刻永久关闭偏好。恢复期间立即清 hit rect，并在原本
  taskbar-only 时临时显示主窗口；在有界宽限期内等待新 host 和 label 释放，
  超时或确定性 placement/build 失败后才执行现有 safety demotion。

## Tauri 2.11.5 的实际生命周期

锁定版本来自 `src-tauri/Cargo.lock`：`tauri 2.11.5`、
`tauri-runtime-wry 2.11.4`、`tao 0.35.3`。

1. `WebviewWindow::close()` 委托给 `Window::close()`；官方 API 明确说明它先
   产生可拦截的 `CloseRequested`。本应用的全局 handler 总是
   `prevent_close()`，所以它只会触发 close-to-hide，不会释放 label。
2. `WebviewWindow::destroy()` 委托给 dispatcher 的 force-destroy 路径，跳过
   `CloseRequested`。在 pinned Wry runtime 中，它通过 `EventLoopProxy`
   `send_event(WindowMessage::Destroy)`，不是同步销毁。
3. runtime 收到该消息后先丢弃原生 window；Tao 随后报告 `Destroyed`。
   Tauri 的 `on_event_loop_event` 在看到 `Destroyed` 时调用
   `AppManager::on_window_close(label)`，后者才移除 window map 以及该窗口的
   webview map。
4. 新建前 `WindowManager::prepare_window` 和
   `WebviewManager::prepare_webview` 分别检查 map，旧 label 尚在就返回
   `WindowLabelAlreadyExists` / `WebviewLabelAlreadyExists`。因此
   `destroy(); build()` 仍有确定的异步时序窗口。
5. 不应仅凭收到 `Destroyed` 回调就创建。不同监听器的回调顺序不是本应用应
   依赖的契约；恢复 drive 应重新查询 Manager，以 label 实际消失为唯一建窗
   前置条件。

参考：

- [Tauri 2.11.5 `WebviewWindow::close`](https://docs.rs/tauri/2.11.5/tauri/webview/struct.WebviewWindow.html#method.close)
- [Tauri 2.11.5 `WebviewWindow::destroy`](https://docs.rs/tauri/2.11.5/tauri/webview/struct.WebviewWindow.html#method.destroy)
- [Tauri window label uniqueness check](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.5/crates/tauri/src/manager/window.rs#L66-L72)
- [Tauri `Destroyed` manager cleanup](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.5/crates/tauri/src/app.rs#L2537-L2548)
- [Tauri manager map removal](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.5/crates/tauri/src/manager/mod.rs#L653-L659)
- [Wry dispatcher close/destroy enqueue](https://github.com/tauri-apps/tauri/blob/tauri-v2.11.5/crates/tauri-runtime-wry/src/lib.rs#L2274-L2290)

## 推荐状态机

把 native observation、纯决策和副作用执行分开。建议状态至少包含：

```text
Stable
DestroyRequested { stale_hwnd, started_at, token }
WaitingForHost { started_at, token }
Building { host_hwnd, token }
```

每次 drive 重新读取两项事实：Manager 中的 canonical `taskbar` 窗口，以及当前
`Shell_TrayWnd` HWND。不要跨 tick 缓存 Explorer HWND。

```text
Stable + existing window attached to current host
  -> reuse -> place/show/verify -> Ready

Stable + existing window missing/dead/detached/from old host
  -> clear hit rect; best-effort hide; claim token; drop state lock
  -> call destroy() exactly once
  -> DestroyRequested; return Recovering

DestroyRequested + canonical label still present
  -> do not destroy again and do not build; return Recovering

DestroyRequested + label absent + host absent
  -> WaitingForHost; return Recovering

DestroyRequested/WaitingForHost + label absent + host present
  -> claim Building token; drop state lock; build canonical label
  -> re-read current host and verify parent after build
  -> matching host: place/show/health-check -> Stable/Ready
  -> host changed during build: destroy new stale window -> Recovering

Recovering exceeds grace deadline, or a deterministic non-transient error occurs
  -> Fatal -> existing safety demotion
```

`Ready / Recovering / Fatal` 应成为 `ensure_taskbar_projection` 的显式结果，而
不是把所有非 Ready 都压成 `Err`：

- monitor 收到 `Recovering`：不持久化关闭 taskbar 偏好；如果快照是
  taskbar-only，立即 best-effort 显示并校正主窗口，保证恢复期间有可见入口。
- monitor 收到 `Ready`：重新读取最新偏好；只有最新 `showMainWindow=false`
  时才隐藏临时恢复主窗口，避免覆盖恢复期间的用户操作。
- 用户正在提交 taskbar-only 偏好时收到 `Recovering`：本次事务应返回未就绪并
  回滚，主窗口保持显示；后台 monitor 可继续完成旧偏好的恢复。
- `Fatal`：沿用 `showTaskbarWindow=false, showMainWindow=true` 的持久化降级。

瞬态宽限期应是命名常量并基于单调时间；按当前 1 秒 monitor，建议从 10 秒
起步。以下属于瞬态：Explorer host 暂时不存在、旧 label 等待 `Destroyed`
清理、构建期间 host generation 改变、并发 drive 已 claim action。无可用完整
slot 等几何拒绝仍可立即视为确定性失败。宽限期内不要 sleep 或阻塞等待事件
循环。

并发保护不能持有 `std::sync::Mutex` 跨 `hwnd()`、`build()`、`scale_factor()`、
`show()` 等 Wry 调用，否则 main thread 与 worker 可能互等。状态锁只用于 claim
递增 token 和提交结果；native 副作用在锁外执行，完成后仅当 token 仍匹配才
写回。其他调用看到 `Building`/`DestroyRequested` 时返回 `Recovering`。

全局 close handler 也应显式路由：用户关闭 `main`/`taskbar` 才 prevent + hide；
内部 lifecycle teardown 永远调用 `destroy()`。即使暂时没有第三种窗口，这能防止
未来辅助窗口被无条件吞掉。Explorer 恢复开始时必须清除旧 hit rect，恢复成功后
placement 再写入新 rect。

## 不依赖脆弱 native 集成的测试方案

抽出纯函数 `plan_lifecycle(phase, observation, now) -> decision`，用简单 HWND/id
整数作为输入；副作用层只负责执行 decision。单元测试至少覆盖：

- 健康窗口复用，不 destroy/build。
- 首次 detached 只发一次 Destroy；label 仍在的多次 drive 只 Wait。
- label 消失且新 host 出现后只 Build 一次。
- host 缺失、host generation 在 build 中改变、并发 token 过期均保持
  Recovering，不误 demote，也不接受 stale completion。
- 宽限期超时才 Fatal；确定性 placement 拒绝立即 Fatal。
- Recovering 的 taskbar-only 策略显示主窗口但不改/写偏好；Ready 后基于重新
  读取的偏好决定是否隐藏；Fatal 才提交 demotion。
- close-policy 纯函数断言 `main`/`taskbar` 的用户 close-to-hide 路由；内部
  rebuild action 只能选 Destroy。

可以给副作用 executor 使用 scripted fake，断言顺序为
`clear rect -> hide -> destroy -> wait -> build -> verify -> place/show`，不需要真的
重启 Explorer。不要用 `tauri::test::mock_builder` 断言 destroy 的精确时序：mock
runtime 不复现 pinned Wry 的 EventLoopProxy/Tao `Destroyed` 链路，这类测试容易
给出错误安全感。

保留一个手工 Windows smoke test 即可：在 taskbar-only 状态重启 Explorer，
确认主窗口在恢复期出现、偏好在宽限期内不被关闭、新 `Shell_TrayWnd` 出现后只
有一个 canonical `taskbar` 并重新挂接；若恢复超过宽限期，则只发生一次 safety
demotion。CI 负责纯状态机、策略和现有 geometry 测试，不自动重启用户 Shell。
