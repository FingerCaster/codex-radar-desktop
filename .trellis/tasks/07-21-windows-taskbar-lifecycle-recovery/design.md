# Design: Windows taskbar lifecycle and recovery

## 1. Boundaries

- `src-tauri/src/desktop/windows.rs` owns raw Win32 handles, hook thread, atomic hit rect, child visibility style, taskbar create/destroy outcomes, placement and native show/hide fallbacks.
- `src-tauri/src/desktop.rs` owns preference transactions, monitor rebuild state, safe demotion, tray behavior and testable orchestration decisions.
- `src-tauri/src/lib.rs` continues routing user close requests, but application exit additionally shuts down taskbar input. Programmatic detached-window repair bypasses `CloseRequested` with `destroy()`.

## 2. Hook Runtime

Replace the process-lifetime boolean with one owned runtime:

```text
ensure taskbar input
  -> update atomic screen hit rect + AppHandle
  -> start dedicated OS thread when absent
       -> create message queue
       -> SetWindowsHookExW(WH_MOUSE_LL)
       -> GetMessageW loop
            mouse hook callback -> atomic rect read -> PostThreadMessage(action)
            action message      -> clone AppHandle -> async Tauri dispatch
            control wake        -> drain Enable/Disable/Rearm/Shutdown queue
            shutdown            -> unhook + acknowledge + exit
```

The worker calls `PeekMessageW` before publishing its thread ID so `PostThreadMessageW` cannot race message-queue creation. Commands travel through a bounded Rust queue and use a custom thread message only as the wake signal; enable/disable/rearm return an acknowledgement to the caller.

The callback never locks `TASKBAR_APP` or calls `tauri::async_runtime::spawn`. A small atomic/seqlock hit-rect snapshot avoids torn coordinates without a blocking mutex, and an event sequence increments for every `HC_ACTION`. Windows documents no query for silently removed low-level hooks, so the controller rearms after a 30-second lease. Each one-second monitor sample also compares `GetCursorPos` with the event sequence and rearms early when the cursor moved without hook progress. A failed replacement marks input unavailable and `ensure_taskbar_projection` returns an error.

The controller starts idle. The first enable lazily starts the worker. Disabling
projection clears the rect, synchronously acknowledges unhook and bridge
cleanup, and terminates the worker; a later enable creates a fresh worker.
Application exit sends `Shutdown`, waits up to 500 ms for completion, and joins
only an already-finished worker so exit cannot block indefinitely.

## 3. Taskbar Window Rebuild State Machine

The lifecycle drive returns one of:

```text
Ready
Recovering { reason }
Fatal { error }
```

- Existing + attached to the current host -> `Ready` and ordinary place/show/health work.
- Existing + detached/dead/attached to an old host -> atomically claim one destroy token, clear hit rect, call `WebviewWindow::destroy` once, then return `Recovering` while the canonical label remains registered.
- Label absent + host absent/incomplete -> `Recovering` without blocking or sleeping.
- Label absent + current host ready -> claim a build token, build canonical `taskbar`, then re-read/verify the host before accepting it. A host change destroys the stale result and stays `Recovering`.

`DesktopController` tracks a generation token, recovery phase and a 10-second monotonic deadline. State locks only claim actions and accept matching completions; no state lock crosses `hwnd`, `destroy`, `build`, `scale_factor`, `show` or placement calls. The monitor behavior is:

```text
Ready      -> clear pending state; if main was exposed for rebuilding and
              preference is taskbar-only, hide main only after health succeeds
Recovering -> force-show/clamp main; retry next tick; do not demote during grace
Timed out  -> ordinary permanent taskbar failure demotion
Fatal      -> ordinary permanent taskbar failure demotion
```

Apply paths translate `Recovering` into a recoverable error, so a taskbar-only user transaction never hides main while the label or Explorer host is being replaced. Placement geometry rejection remains deterministic and can fail immediately.

## 4. Visibility And Preference Transactions

Add a `preference_transition` mutex separate from the preference value mutex.

```text
option transaction:
  lock transition gate
  snapshot preference under value mutex; release value mutex
  apply native change
  persist + sync menu
  commit next value under value mutex; release
  emit complete preference
```

Rollback remains under the transition gate but never under the value mutex. The gate preserves serial option semantics; readers can still inspect the last committed preference while native work is in flight.

Every preference writer uses this gate, including option toggles, opacity, radar source, emergency recovery and monitor demotion. Otherwise a writer could commit a stale full `DesktopPreferences` snapshot while another native apply is in flight.

Permanent monitor failure first performs main-window native recovery outside the preference mutex. It then serializes the final taskbar disable commit and recomputes from the latest preference so unrelated concurrent changes are preserved.

Tray toggle uses a pure decision function over `native_visible: Option<bool>` and the stored preference. `Some(false)` and unknown native state both choose force-show; only a confirmed visible main chooses hide. This makes preference/native drift fail toward recovery.

## 5. Health And Tick Cost

`IsWindowVisible` is retained where recursive visibility is desired for top-level recovery/blocker enumeration. Taskbar companion health and hide verification use the child HWND's own `WS_VISIBLE` style, after validating HWND and current parent attachment.

The monitor continues recomputing blockers and target geometry every second. `geometry_matches_current_window` remains the authoritative no-op before `SetWindowPos`; taskbar HWNDs are intentionally rediscovered so Explorer restart can be detected.

Taskbar placement resolves physical size from the renderer child, not its
Explorer parent. It starts with `window.scale_factor()` and uses
`GetDpiForWindow(child) / 96` as a lower bound because `parent_raw` reparenting
can leave Tauri's reported scale stale. A zero child DPI falls back to Tauri;
`win11_geometry` still receives one scale and never clamps the requested size.

The renderer keeps a nominal `168 x 30` surface but also caps the root to the
actual parent client area. Its first row uses fixed icon/effort/status tracks
around one shrinkable model track; its second row uses a shrinkable score track
and a right-aligned tie track. This absorbs native embedding/DPI discrepancies
while the native correction converges, without changing the size contract or
allowing the rightmost status to render beneath an adjacent taskbar component.

## 6. Test Seams

- Pure mouse action mapping and an instance-level atomic hit rect.
- Pure hook enable/disable/rearm decisions, lease expiry and cursor/event-sequence suspicion; native global hook installation is not run inside unit tests.
- Pure taskbar lifecycle planning with injected observations/tokens/`Instant` values for single destroy, label wait, missing host, stale completion, grace timeout and successful completion.
- A closure-driven `apply_main_window_visibility` orchestration helper records exact ensure/recover/hide order without constructing a real `AppHandle`.
- Pure tray-toggle decision for preference/native drift.
- Pure close policy keeps user `main`/`taskbar` close-to-hide separate from internal force-destroy.
- Existing geometry tests retain the stable `SetWindowPos` no-op contract.

## 7. Compatibility And Rollback

- All new raw Win32 code remains under `cfg(windows)`; no DTO or persisted JSON migration is required.
- Failure continues to prefer a visible main window and tray over restoring a broken taskbar child.
- The hook runtime and rebuild state machine are independently revertible: if native runtime behavior regresses, taskbar projection can be disabled while main/tray recovery remains available.
