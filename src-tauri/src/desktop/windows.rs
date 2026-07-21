use std::{
    collections::VecDeque,
    ffi::c_void,
    ptr,
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager, WebviewWindow, WebviewWindowBuilder};
use windows::Win32::Foundation::HWND;

use super::DesktopController;

const TASKBAR_WINDOW_LABEL: &str = "taskbar";
const TASKBAR_GAP_LOGICAL: f64 = 2.0;
const SWP_NOACTIVATE: u32 = 0x0010;
const SW_HIDE: i32 = 0;
const SW_SHOW: i32 = 5;
const HWND_TOP: isize = 0;
const WH_MOUSE_LL: i32 = 14;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_QUIT: u32 = 0x0012;
const WM_APP: u32 = 0x8000;
const WM_TASKBAR_HOOK_CONTROL: u32 = WM_APP + 0x41;
const WM_TASKBAR_HOOK_ACTION: u32 = WM_APP + 0x42;
const HC_ACTION: i32 = 0;
const PM_NOREMOVE: u32 = 0;
const GWL_STYLE: i32 = -16;
const WS_VISIBLE: isize = 0x1000_0000;
const ERROR_INVALID_HOOK_HANDLE: u32 = 1404;
const HOOK_CONTROL_QUEUE_CAPACITY: usize = 8;
const HOOK_COMMAND_TIMEOUT: Duration = Duration::from_millis(500);
const HOOK_LEASE_DURATION: Duration = Duration::from_secs(30);

static TASKBAR_HOOK_BRIDGE: HookBridge = HookBridge::new();

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl WinRect {
    fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }

    fn intersects(self, other: Self) -> bool {
        self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }

    fn contains(self, point: PointI32) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }
}

struct AtomicHitRect {
    sequence: AtomicU64,
    present: AtomicBool,
    left: AtomicI32,
    top: AtomicI32,
    right: AtomicI32,
    bottom: AtomicI32,
}

impl AtomicHitRect {
    const fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            present: AtomicBool::new(false),
            left: AtomicI32::new(0),
            top: AtomicI32::new(0),
            right: AtomicI32::new(0),
            bottom: AtomicI32::new(0),
        }
    }

    fn store(&self, rect: Option<WinRect>) {
        let write_sequence = loop {
            let current = self.sequence.load(Ordering::Acquire);
            if current & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            if self
                .sequence
                .compare_exchange_weak(
                    current,
                    current.wrapping_add(1),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                break current;
            }
        };

        if let Some(rect) = rect {
            self.left.store(rect.left, Ordering::Relaxed);
            self.top.store(rect.top, Ordering::Relaxed);
            self.right.store(rect.right, Ordering::Relaxed);
            self.bottom.store(rect.bottom, Ordering::Relaxed);
            self.present.store(true, Ordering::Relaxed);
        } else {
            self.present.store(false, Ordering::Relaxed);
        }
        self.sequence
            .store(write_sequence.wrapping_add(2), Ordering::Release);
    }

    /// One-shot seqlock read for the hook callback. Contention skips this event.
    fn load(&self) -> Option<WinRect> {
        let before = self.sequence.load(Ordering::Acquire);
        if before & 1 != 0 || !self.present.load(Ordering::Relaxed) {
            return None;
        }
        let rect = WinRect {
            left: self.left.load(Ordering::Relaxed),
            top: self.top.load(Ordering::Relaxed),
            right: self.right.load(Ordering::Relaxed),
            bottom: self.bottom.load(Ordering::Relaxed),
        };
        (self.sequence.load(Ordering::Acquire) == before).then_some(rect)
    }
}

struct HookBridge {
    hit_rect: AtomicHitRect,
    desired_enabled: AtomicBool,
    worker_thread_id: AtomicU32,
    event_sequence: AtomicU64,
    dropped_actions: AtomicU64,
}

impl HookBridge {
    const fn new() -> Self {
        Self {
            hit_rect: AtomicHitRect::new(),
            desired_enabled: AtomicBool::new(false),
            worker_thread_id: AtomicU32::new(0),
            event_sequence: AtomicU64::new(0),
            dropped_actions: AtomicU64::new(0),
        }
    }

    fn claim_worker(&self, thread_id: u32) -> bool {
        thread_id != 0
            && self
                .worker_thread_id
                .compare_exchange(0, thread_id, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }

    fn owns_worker(&self, thread_id: u32) -> bool {
        thread_id != 0 && self.worker_thread_id.load(Ordering::Acquire) == thread_id
    }

    fn release_worker(&self, thread_id: u32) -> bool {
        if !self.owns_worker(thread_id) {
            return false;
        }
        self.desired_enabled.store(false, Ordering::Release);
        self.hit_rect.store(None);
        self.worker_thread_id
            .compare_exchange(thread_id, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskbarInputAction {
    LeftClick = 1,
    RightClick = 2,
}

impl TaskbarInputAction {
    fn from_message_value(value: usize) -> Option<Self> {
        match value {
            value if value == Self::LeftClick as usize => Some(Self::LeftClick),
            value if value == Self::RightClick as usize => Some(Self::RightClick),
            _ => None,
        }
    }
}

fn classify_mouse_event(
    code: i32,
    message: usize,
    point: PointI32,
    rect: Option<WinRect>,
) -> Option<TaskbarInputAction> {
    if code != HC_ACTION || !rect.is_some_and(|rect| rect.contains(point)) {
        return None;
    }
    match message as u32 {
        WM_LBUTTONUP => Some(TaskbarInputAction::LeftClick),
        WM_RBUTTONUP => Some(TaskbarInputAction::RightClick),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EmbedGeometry {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

struct BlockerSearch {
    taskbar: WinRect,
    taskbar_process_id: u32,
    current_process_id: u32,
    companion: isize,
    blockers: Vec<WinRect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookControl {
    Enable,
    Disable,
    Rearm,
    Shutdown,
}

type HookControlResult = Result<(), String>;
type HookAcknowledgement = SyncSender<HookControlResult>;
type PendingTerminalAcknowledgement = (HookAcknowledgement, HookControlResult);

struct QueuedHookControl {
    id: u64,
    command: HookControl,
    acknowledgement: HookAcknowledgement,
}

struct HookControlQueue {
    next_id: AtomicU64,
    entries: Mutex<VecDeque<QueuedHookControl>>,
}

impl HookControlQueue {
    fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            entries: Mutex::new(VecDeque::with_capacity(HOOK_CONTROL_QUEUE_CAPACITY)),
        }
    }

    fn enqueue(&self, command: HookControl) -> Result<(u64, Receiver<Result<(), String>>), String> {
        let (acknowledgement, receiver) = mpsc::sync_channel(1);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "taskbar hook control queue is poisoned".to_owned())?;
        if entries.len() >= HOOK_CONTROL_QUEUE_CAPACITY {
            return Err("taskbar hook control queue is full".to_owned());
        }
        entries.push_back(QueuedHookControl {
            id,
            command,
            acknowledgement,
        });
        Ok((id, receiver))
    }

    fn cancel(&self, id: u64) -> bool {
        let Ok(mut entries) = self.entries.lock() else {
            return false;
        };
        let Some(index) = entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        entries.remove(index);
        true
    }

    fn drain(&self) -> Result<Vec<QueuedHookControl>, String> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "taskbar hook control queue is poisoned".to_owned())?;
        Ok(entries.drain(..).collect())
    }

    fn fail_pending(&self, error: &str) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        for entry in entries.drain(..) {
            let _ = entry.acknowledgement.send(Err(error.to_owned()));
        }
    }
}

struct HookWorker {
    thread_id: u32,
    queue: Arc<HookControlQueue>,
    join: Option<JoinHandle<()>>,
}

impl HookWorker {
    fn start(app: AppHandle) -> Result<Self, String> {
        let queue = Arc::new(HookControlQueue::new());
        let worker_queue = Arc::clone(&queue);
        let startup_cancelled = Arc::new(AtomicBool::new(false));
        let worker_startup_cancelled = Arc::clone(&startup_cancelled);
        let startup_thread_id = Arc::new(AtomicU32::new(0));
        let worker_startup_thread_id = Arc::clone(&startup_thread_id);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("model-radar-taskbar-hook".to_owned())
            .spawn(move || {
                run_taskbar_hook_worker(
                    app,
                    worker_queue,
                    worker_startup_cancelled,
                    worker_startup_thread_id,
                    ready_sender,
                )
            })
            .map_err(|error| format!("start taskbar hook worker: {error}"))?;

        match ready_receiver.recv_timeout(HOOK_COMMAND_TIMEOUT) {
            Ok(Ok(thread_id)) => Ok(Self {
                thread_id,
                queue,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                Self::cleanup_failed_start(startup_cancelled, startup_thread_id, join);
                Err(error)
            }
            Err(error) => {
                Self::cleanup_failed_start(startup_cancelled, startup_thread_id, join);
                Err(format!(
                    "taskbar hook worker ready acknowledgement failed: {error}"
                ))
            }
        }
    }

    fn cleanup_failed_start(
        startup_cancelled: Arc<AtomicBool>,
        startup_thread_id: Arc<AtomicU32>,
        join: JoinHandle<()>,
    ) {
        startup_cancelled.store(true, Ordering::Release);
        let thread_id = startup_thread_id.load(Ordering::Acquire);
        if TASKBAR_HOOK_BRIDGE.owns_worker(thread_id) {
            unsafe {
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            }
        }
        if join.is_finished() {
            let _ = join.join();
        }
    }

    fn send(&self, command: HookControl) -> Result<(), String> {
        let (id, acknowledgement) = self.queue.enqueue(command)?;
        if unsafe { PostThreadMessageW(self.thread_id, WM_TASKBAR_HOOK_CONTROL, 0, id as isize) }
            == 0
        {
            let error = last_error("PostThreadMessageW(taskbar hook control)");
            if self.queue.cancel(id) {
                return Err(error);
            }
        }
        acknowledgement
            .recv_timeout(HOOK_COMMAND_TIMEOUT)
            .map_err(|error| format!("taskbar hook command acknowledgement failed: {error}"))?
    }

    fn is_finished(&self) -> bool {
        self.join.as_ref().is_none_or(JoinHandle::is_finished)
    }

    fn join_if_finished(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        if join.is_finished() {
            let _ = join.join();
        }
        // Dropping an unfinished handle deliberately detaches it. Exit must not
        // block the Tauri event loop beyond the bounded command acknowledgement.
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HookSample {
    cursor: Option<PointI32>,
    event_sequence: u64,
}

#[derive(Debug, Default)]
struct HookLeaseState {
    enabled: bool,
    installed_at: Option<Instant>,
    last_sample: Option<HookSample>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookMaintenance {
    None,
    Enable,
    Rearm,
}

fn hook_maintenance(state: &HookLeaseState, now: Instant, sample: HookSample) -> HookMaintenance {
    if !state.enabled {
        return HookMaintenance::Enable;
    }
    if state.installed_at.is_none_or(|installed_at| {
        now.saturating_duration_since(installed_at) >= HOOK_LEASE_DURATION
    }) {
        return HookMaintenance::Rearm;
    }
    if state.last_sample.is_some_and(|previous| {
        previous.cursor.is_some()
            && sample.cursor.is_some()
            && previous.cursor != sample.cursor
            && previous.event_sequence == sample.event_sequence
    }) {
        return HookMaintenance::Rearm;
    }
    HookMaintenance::None
}

/// Owns the process-wide low-level mouse hook and its dedicated Win32 message thread.
pub struct TaskbarInputController {
    lifecycle: Mutex<()>,
    runtime: Mutex<Option<HookWorker>>,
    lease: Mutex<HookLeaseState>,
}

impl TaskbarInputController {
    pub fn new(_app: &AppHandle) -> Result<Self, String> {
        Ok(Self::idle())
    }

    fn idle() -> Self {
        Self {
            lifecycle: Mutex::new(()),
            runtime: Mutex::new(None),
            lease: Mutex::new(HookLeaseState::default()),
        }
    }

    /// Enable input and refresh the 30-second lease / cursor heartbeat when needed.
    pub fn ensure_enabled(&self, window: &WebviewWindow) -> Result<(), String> {
        let _lifecycle = self.lock_lifecycle()?;
        let hwnd = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
        if unsafe { IsWindow(hwnd) } == 0 {
            return Err("taskbar companion handle is invalid".to_owned());
        }
        let rect = window_rect(hwnd)?;
        TASKBAR_HOOK_BRIDGE.hit_rect.store(Some(rect));

        let now = Instant::now();
        let sample = current_hook_sample();
        let mut lease = self
            .lease
            .lock()
            .map_err(|_| "taskbar hook lease state is poisoned".to_owned())?;
        let maintenance = hook_maintenance(&lease, now, sample);
        let command = match maintenance {
            HookMaintenance::None => {
                lease.last_sample = Some(sample);
                return Ok(());
            }
            HookMaintenance::Enable => HookControl::Enable,
            HookMaintenance::Rearm => HookControl::Rearm,
        };
        let result = self.send_or_start(window.app_handle(), command);
        if result.is_ok() {
            lease.enabled = true;
            lease.installed_at = Some(now);
            lease.last_sample = Some(sample);
        } else {
            lease.enabled = false;
            lease.installed_at = None;
            TASKBAR_HOOK_BRIDGE
                .desired_enabled
                .store(false, Ordering::Release);
            TASKBAR_HOOK_BRIDGE.hit_rect.store(None);
        }
        result
    }

    /// Stop global input synchronously. Clearing the bridge happens before the ack wait.
    pub fn disable(&self) -> Result<(), String> {
        let _lifecycle = self.lock_lifecycle()?;
        TASKBAR_HOOK_BRIDGE
            .desired_enabled
            .store(false, Ordering::Release);
        TASKBAR_HOOK_BRIDGE.hit_rect.store(None);
        let result = self.stop_worker(HookControl::Disable, true);
        let mut lease = self
            .lease
            .lock()
            .map_err(|_| "taskbar hook lease state is poisoned".to_owned())?;
        lease.enabled = false;
        lease.installed_at = None;
        lease.last_sample = None;
        result
    }

    /// Explicit process-exit teardown. Waiting and joining are both bounded.
    pub fn shutdown(&self) -> Result<(), String> {
        let _lifecycle = self.lock_lifecycle()?;
        TASKBAR_HOOK_BRIDGE
            .desired_enabled
            .store(false, Ordering::Release);
        TASKBAR_HOOK_BRIDGE.hit_rect.store(None);
        if let Ok(mut lease) = self.lease.lock() {
            *lease = HookLeaseState::default();
        }

        self.stop_worker(HookControl::Shutdown, false)
    }

    fn lock_lifecycle(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.lifecycle
            .lock()
            .map_err(|_| "taskbar input lifecycle lock is poisoned".to_owned())
    }

    fn send_or_start(&self, app: &AppHandle, command: HookControl) -> Result<(), String> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| "taskbar hook runtime is poisoned".to_owned())?;
        if runtime.as_ref().is_some_and(|worker| worker.is_finished()) {
            if let Some(mut worker) = runtime.take() {
                worker.join_if_finished();
            }
        }
        if runtime.is_none() {
            *runtime = Some(HookWorker::start(app.clone())?);
        }
        runtime
            .as_ref()
            .expect("taskbar hook worker must exist after lazy start")
            .send(command)
    }

    fn stop_worker(
        &self,
        command: HookControl,
        retain_live_on_failure: bool,
    ) -> Result<(), String> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| "taskbar hook runtime is poisoned".to_owned())?;
        let Some(mut worker) = runtime.take() else {
            return Ok(());
        };
        let result = worker.send(command);
        if result.is_err() && retain_live_on_failure && !worker.is_finished() {
            *runtime = Some(worker);
        } else {
            worker.join_if_finished();
        }
        result
    }
}

impl Drop for TaskbarInputController {
    fn drop(&mut self) {
        TASKBAR_HOOK_BRIDGE
            .desired_enabled
            .store(false, Ordering::Release);
        TASKBAR_HOOK_BRIDGE.hit_rect.store(None);
        let runtime = self
            .runtime
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(mut worker) = runtime.take() {
            // Best-effort fallback for setup failures. Normal application exit
            // calls shutdown explicitly and leaves this slot empty.
            let _ = worker.send(HookControl::Shutdown);
            worker.join_if_finished();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarRecoveryReason {
    DestroyRequested,
    WaitingForLabelRemoval,
    WaitingForTaskbar,
    BuildInProgress,
    HostChanged,
}

pub enum TaskbarWindowOutcome {
    Ready(Box<WebviewWindow>),
    Recovering(TaskbarRecoveryReason),
    Fatal(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskbarLifecyclePhase {
    Stable,
    DestroyRequested { window_id: isize },
    WaitingForTaskbar,
    Building { token: u64, host: isize },
}

#[derive(Debug)]
struct TaskbarLifecycleState {
    phase: TaskbarLifecyclePhase,
    next_token: u64,
}

impl Default for TaskbarLifecycleState {
    fn default() -> Self {
        Self {
            phase: TaskbarLifecyclePhase::Stable,
            next_token: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TaskbarLifecycleObservation {
    window_id: Option<isize>,
    attached_to_host: bool,
    host: Option<isize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskbarLifecyclePlan {
    Ready,
    Destroy { window_id: isize },
    Wait(TaskbarRecoveryReason),
    Build { token: u64, host: isize },
}

impl TaskbarLifecycleState {
    fn plan(&mut self, observation: TaskbarLifecycleObservation) -> TaskbarLifecyclePlan {
        if matches!(self.phase, TaskbarLifecyclePhase::Building { .. }) {
            return TaskbarLifecyclePlan::Wait(TaskbarRecoveryReason::BuildInProgress);
        }

        if let Some(window_id) = observation.window_id {
            if matches!(
                self.phase,
                TaskbarLifecyclePhase::DestroyRequested {
                    window_id: pending
                } if pending == window_id
            ) {
                return TaskbarLifecyclePlan::Wait(TaskbarRecoveryReason::WaitingForLabelRemoval);
            }
            if observation.attached_to_host {
                self.phase = TaskbarLifecyclePhase::Stable;
                return TaskbarLifecyclePlan::Ready;
            }
            self.phase = TaskbarLifecyclePhase::DestroyRequested { window_id };
            return TaskbarLifecyclePlan::Destroy { window_id };
        }

        let Some(host) = observation.host else {
            self.phase = TaskbarLifecyclePhase::WaitingForTaskbar;
            return TaskbarLifecyclePlan::Wait(TaskbarRecoveryReason::WaitingForTaskbar);
        };
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        self.phase = TaskbarLifecyclePhase::Building { token, host };
        TaskbarLifecyclePlan::Build { token, host }
    }

    fn finish_build(&mut self, token: u64, host: isize, window_id: isize, valid: bool) -> bool {
        if self.phase != (TaskbarLifecyclePhase::Building { token, host }) {
            return false;
        }
        if valid {
            self.phase = TaskbarLifecyclePhase::Stable;
            true
        } else {
            self.phase = TaskbarLifecyclePhase::DestroyRequested { window_id };
            false
        }
    }

    fn cancel_build(&mut self, token: u64, host: isize) {
        if self.phase == (TaskbarLifecyclePhase::Building { token, host }) {
            self.phase = TaskbarLifecyclePhase::Stable;
        }
    }

    fn cancel_destroy(&mut self, window_id: isize) {
        if self.phase == (TaskbarLifecyclePhase::DestroyRequested { window_id }) {
            self.phase = TaskbarLifecyclePhase::Stable;
        }
    }
}

#[derive(Default)]
pub struct TaskbarWindowLifecycle {
    state: Mutex<TaskbarLifecycleState>,
}

impl TaskbarWindowLifecycle {
    pub fn ensure(&self, app: &AppHandle) -> TaskbarWindowOutcome {
        let window = app.get_webview_window(TASKBAR_WINDOW_LABEL);
        let host = find_win11_taskbar().ok();
        let window_id = window.as_ref().map(taskbar_window_identity);
        let attached_to_host = window
            .as_ref()
            .zip(host)
            .is_some_and(|(window, host)| taskbar_companion_is_attached_to(window, host));
        let observation = TaskbarLifecycleObservation {
            window_id,
            attached_to_host,
            host,
        };
        let plan = match self.state.lock() {
            Ok(mut state) => state.plan(observation),
            Err(_) => {
                return TaskbarWindowOutcome::Fatal(
                    "taskbar window lifecycle state is poisoned".to_owned(),
                )
            }
        };

        match plan {
            TaskbarLifecyclePlan::Ready => TaskbarWindowOutcome::Ready(Box::new(
                window.expect("ready taskbar lifecycle plan requires a window"),
            )),
            TaskbarLifecyclePlan::Wait(reason) => TaskbarWindowOutcome::Recovering(reason),
            TaskbarLifecyclePlan::Destroy { window_id } => {
                let Some(window) = window else {
                    return TaskbarWindowOutcome::Recovering(
                        TaskbarRecoveryReason::WaitingForLabelRemoval,
                    );
                };
                debug_assert_eq!(taskbar_window_identity(&window), window_id);
                clear_taskbar_hit_rect();
                let _ = hide_taskbar_window(&window);
                match window.destroy() {
                    Ok(()) => {
                        TaskbarWindowOutcome::Recovering(TaskbarRecoveryReason::DestroyRequested)
                    }
                    Err(error) => {
                        self.cancel_destroy(window_id);
                        TaskbarWindowOutcome::Fatal(format!(
                            "destroy detached taskbar companion: {error}"
                        ))
                    }
                }
            }
            TaskbarLifecyclePlan::Build { token, host } => self.build_and_verify(app, token, host),
        }
    }

    fn build_and_verify(&self, app: &AppHandle, token: u64, host: isize) -> TaskbarWindowOutcome {
        let Some(config) = app
            .config()
            .app
            .windows
            .iter()
            .find(|window| window.label == TASKBAR_WINDOW_LABEL)
        else {
            self.cancel_build(token, host);
            return TaskbarWindowOutcome::Fatal(
                "taskbar window configuration is missing".to_owned(),
            );
        };
        let parent = HWND(host as *mut c_void);
        let builder = match WebviewWindowBuilder::from_config(app, config) {
            Ok(builder) => builder,
            Err(error) => {
                self.cancel_build(token, host);
                return TaskbarWindowOutcome::Fatal(error.to_string());
            }
        };
        let window = match builder.parent_raw(parent).build() {
            Ok(window) => window,
            Err(error) => {
                self.cancel_build(token, host);
                let current_host = find_win11_taskbar().ok();
                if current_host != Some(host) {
                    return TaskbarWindowOutcome::Recovering(if current_host.is_some() {
                        TaskbarRecoveryReason::HostChanged
                    } else {
                        TaskbarRecoveryReason::WaitingForTaskbar
                    });
                }
                if app.get_webview_window(TASKBAR_WINDOW_LABEL).is_some() {
                    return TaskbarWindowOutcome::Recovering(
                        TaskbarRecoveryReason::WaitingForLabelRemoval,
                    );
                }
                return TaskbarWindowOutcome::Fatal(error.to_string());
            }
        };

        let window_id = taskbar_window_identity(&window);
        let current_host = find_win11_taskbar().ok();
        let valid = current_host == Some(host) && taskbar_companion_is_attached_to(&window, host);
        let accepted = match self.state.lock() {
            Ok(mut state) => state.finish_build(token, host, window_id, valid),
            Err(_) => false,
        };
        if accepted {
            return TaskbarWindowOutcome::Ready(Box::new(window));
        }

        clear_taskbar_hit_rect();
        let _ = hide_taskbar_window(&window);
        match window.destroy() {
            Ok(()) => TaskbarWindowOutcome::Recovering(TaskbarRecoveryReason::HostChanged),
            Err(error) => {
                self.cancel_destroy(window_id);
                TaskbarWindowOutcome::Fatal(format!(
                    "destroy taskbar companion built for stale host: {error}"
                ))
            }
        }
    }

    fn cancel_build(&self, token: u64, host: isize) {
        if let Ok(mut state) = self.state.lock() {
            state.cancel_build(token, host);
        }
    }

    fn cancel_destroy(&self, window_id: isize) {
        if let Ok(mut state) = self.state.lock() {
            state.cancel_destroy(window_id);
        }
    }
}

fn taskbar_window_identity(window: &WebviewWindow) -> isize {
    window.hwnd().map_or(0, |hwnd| hwnd.0 as isize)
}

pub fn clear_taskbar_hit_rect() {
    TASKBAR_HOOK_BRIDGE.hit_rect.store(None);
}

fn update_taskbar_hit_rect(rect: Option<WinRect>) {
    TASKBAR_HOOK_BRIDGE.hit_rect.store(rect);
}

fn current_hook_sample() -> HookSample {
    let mut cursor = PointI32::default();
    let cursor = (unsafe { GetCursorPos(&mut cursor) } != 0).then_some(cursor);
    HookSample {
        cursor,
        event_sequence: TASKBAR_HOOK_BRIDGE.event_sequence.load(Ordering::Acquire),
    }
}

fn install_taskbar_mouse_hook(hook: &mut isize) -> Result<(), String> {
    if *hook != 0 {
        uninstall_taskbar_mouse_hook(hook)?;
    }
    let new_hook = unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(taskbar_mouse_hook_proc),
            GetModuleHandleW(ptr::null()),
            0,
        )
    };
    if new_hook == 0 {
        return Err(last_error("SetWindowsHookExW(WH_MOUSE_LL)"));
    }
    *hook = new_hook;
    TASKBAR_HOOK_BRIDGE
        .desired_enabled
        .store(true, Ordering::Release);
    Ok(())
}

fn uninstall_taskbar_mouse_hook(hook: &mut isize) -> Result<(), String> {
    TASKBAR_HOOK_BRIDGE
        .desired_enabled
        .store(false, Ordering::Release);
    if *hook == 0 {
        return Ok(());
    }
    if unsafe { UnhookWindowsHookEx(*hook) } != 0 {
        *hook = 0;
        return Ok(());
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_INVALID_HOOK_HANDLE {
        *hook = 0;
        return Ok(());
    }
    Err(format!(
        "UnhookWindowsHookEx(WH_MOUSE_LL) failed with Win32 error {error}"
    ))
}

fn run_taskbar_hook_worker(
    app: AppHandle,
    queue: Arc<HookControlQueue>,
    startup_cancelled: Arc<AtomicBool>,
    startup_thread_id: Arc<AtomicU32>,
    ready: SyncSender<Result<u32, String>>,
) {
    let mut message = ThreadMessage::default();
    unsafe {
        PeekMessageW(&mut message, 0, 0, 0, PM_NOREMOVE);
    }
    let thread_id = unsafe { GetCurrentThreadId() };
    if thread_id == 0 {
        let _ = ready.send(Err("taskbar hook worker has no thread id".to_owned()));
        return;
    }
    startup_thread_id.store(thread_id, Ordering::Release);
    if startup_cancelled.load(Ordering::Acquire) {
        return;
    }
    if !TASKBAR_HOOK_BRIDGE.claim_worker(thread_id) {
        let _ = ready.send(Err(
            "taskbar hook bridge is already owned by another worker".to_owned(),
        ));
        return;
    }
    if startup_cancelled.load(Ordering::Acquire) {
        TASKBAR_HOOK_BRIDGE.release_worker(thread_id);
        return;
    }
    if ready.send(Ok(thread_id)).is_err() || startup_cancelled.load(Ordering::Acquire) {
        TASKBAR_HOOK_BRIDGE.release_worker(thread_id);
        return;
    }

    let mut hook = 0;
    let mut terminal_error = None;
    let mut terminal_ack: Option<PendingTerminalAcknowledgement> = None;
    'messages: loop {
        let status = unsafe { GetMessageW(&mut message, 0, 0, 0) };
        if status == -1 {
            terminal_error = Some(last_error("GetMessageW(taskbar hook worker)"));
            break;
        }
        if status == 0 {
            break;
        }
        match message.message {
            WM_TASKBAR_HOOK_CONTROL => {
                let entries = match queue.drain() {
                    Ok(entries) => entries,
                    Err(error) => {
                        terminal_error = Some(error);
                        break 'messages;
                    }
                };
                let mut shutting_down = false;
                for entry in entries {
                    if shutting_down {
                        let _ = entry
                            .acknowledgement
                            .send(Err("taskbar hook worker is shutting down".to_owned()));
                        continue;
                    }
                    let (result, shutdown) = execute_hook_control(entry.command, &mut hook);
                    if shutdown {
                        terminal_ack = Some((entry.acknowledgement, result));
                        shutting_down = true;
                    } else {
                        let _ = entry.acknowledgement.send(result);
                    }
                }
                if shutting_down {
                    break;
                }
            }
            WM_TASKBAR_HOOK_ACTION
                if TASKBAR_HOOK_BRIDGE.desired_enabled.load(Ordering::Acquire) =>
            {
                if let Some(action) = TaskbarInputAction::from_message_value(message.wparam) {
                    dispatch_taskbar_action(&app, action);
                }
            }
            _ => {}
        }
    }

    match uninstall_taskbar_mouse_hook(&mut hook) {
        Ok(()) => {
            TASKBAR_HOOK_BRIDGE.release_worker(thread_id);
        }
        Err(error) => {
            // Retain ownership when User32 cannot confirm unhook. A later
            // worker must not install a second global hook in that state.
            eprintln!("[model-radar] taskbar hook worker cleanup failed: {error}");
        }
    }
    let failure = terminal_error
        .as_deref()
        .unwrap_or("taskbar hook worker stopped");
    queue.fail_pending(failure);
    if let Some(error) = terminal_error {
        eprintln!("[model-radar] taskbar hook worker stopped: {error}");
    }
    if let Some((acknowledgement, result)) = terminal_ack {
        let _ = acknowledgement.send(result);
    }
}

fn execute_hook_control(command: HookControl, hook: &mut isize) -> (Result<(), String>, bool) {
    match command {
        HookControl::Enable | HookControl::Rearm => (install_taskbar_mouse_hook(hook), false),
        HookControl::Disable => (uninstall_taskbar_mouse_hook(hook), true),
        HookControl::Shutdown => (uninstall_taskbar_mouse_hook(hook), true),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PointI32 {
    x: i32,
    y: i32,
}

#[repr(C)]
struct MsllHookStruct {
    pt: PointI32,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[repr(C)]
#[derive(Default)]
struct ThreadMessage {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    point: PointI32,
    private: u32,
}

unsafe extern "system" fn taskbar_mouse_hook_proc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    // SAFETY: WH_MOUSE_LL supplies MSLLHOOKSTRUCT for HC_ACTION. This hot path
    // performs only atomics, a bounded classification and PostThreadMessageW.
    unsafe {
        if code == HC_ACTION {
            TASKBAR_HOOK_BRIDGE
                .event_sequence
                .fetch_add(1, Ordering::Relaxed);
        }
        if code == HC_ACTION
            && lparam != 0
            && TASKBAR_HOOK_BRIDGE.desired_enabled.load(Ordering::Relaxed)
        {
            let info = &*(lparam as *const MsllHookStruct);
            if let Some(action) =
                classify_mouse_event(code, wparam, info.pt, TASKBAR_HOOK_BRIDGE.hit_rect.load())
            {
                let thread_id = TASKBAR_HOOK_BRIDGE.worker_thread_id.load(Ordering::Relaxed);
                if thread_id == 0
                    || PostThreadMessageW(thread_id, WM_TASKBAR_HOOK_ACTION, action as usize, 0)
                        == 0
                {
                    TASKBAR_HOOK_BRIDGE
                        .dropped_actions
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        CallNextHookEx(0, code, wparam, lparam)
    }
}

fn dispatch_taskbar_action(app: &AppHandle, action: TaskbarInputAction) {
    dispatch_taskbar_action_with(
        action,
        || dispatch_taskbar_left_click(app),
        || dispatch_taskbar_right_click(app),
    );
}

fn dispatch_taskbar_action_with<Left, Right>(
    action: TaskbarInputAction,
    mut left: Left,
    mut right: Right,
) where
    Left: FnMut(),
    Right: FnMut(),
{
    match action {
        TaskbarInputAction::LeftClick => left(),
        TaskbarInputAction::RightClick => right(),
    }
}

fn dispatch_taskbar_left_click(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let controller = app.state::<DesktopController>();
        if let Err(error) = controller.show_main_details(&app) {
            eprintln!("[model-radar] native taskbar left-click failed: {error}");
            if let Err(recovery) = controller.force_show_main_window(&app) {
                eprintln!("[model-radar] native taskbar force-show failed: {recovery}");
            }
        }
    });
}

fn dispatch_taskbar_right_click(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let controller = app.state::<DesktopController>();
        if let Some(window) = app.get_webview_window(TASKBAR_WINDOW_LABEL) {
            if let Err(error) = controller.show_context_menu(&window) {
                eprintln!("[model-radar] native taskbar right-click menu failed: {error}");
            }
        }
    });
}

/// True when the companion HWND still exists and is parented to the primary taskbar.
pub fn taskbar_companion_is_attached(window: &WebviewWindow) -> bool {
    find_window("Shell_TrayWnd")
        .is_some_and(|taskbar| taskbar_companion_is_attached_to(window, taskbar))
}

fn taskbar_companion_is_attached_to(window: &WebviewWindow, taskbar: isize) -> bool {
    let Ok(hwnd) = window.hwnd() else {
        return false;
    };
    let child = hwnd.0 as isize;
    if unsafe { IsWindow(child) } == 0 {
        return false;
    }
    let parent = unsafe { GetParent(child) };
    if parent == 0 || unsafe { IsWindow(parent) } == 0 {
        return false;
    }
    parent == taskbar
}

/// Attached, still a live HWND, and carrying its own WS_VISIBLE style.
///
/// `IsWindowVisible` recursively inspects ancestors, so it incorrectly couples
/// companion health to Explorer's auto-hide policy.
pub fn taskbar_companion_is_healthy(window: &WebviewWindow) -> bool {
    if !taskbar_companion_is_attached(window) {
        return false;
    }
    let Ok(hwnd) = window.hwnd() else {
        return false;
    };
    let child = hwnd.0 as isize;
    child_has_visible_style(unsafe { GetWindowLongPtrW(child, GWL_STYLE) })
}

fn child_has_visible_style(style: isize) -> bool {
    style & WS_VISIBLE != 0
}

pub fn place_taskbar_window(
    window: &WebviewWindow,
    logical_size: (f64, f64),
) -> Result<(), String> {
    let child = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    let taskbar = unsafe { GetParent(child) };
    if taskbar == 0 || unsafe { IsWindow(taskbar) } == 0 {
        return Err("taskbar companion is no longer attached to Explorer".to_owned());
    }

    let notification = find_child(taskbar, "TrayNotifyWnd")
        .ok_or_else(|| "Windows notification-area window was not found".to_owned())?;
    let start = find_child(taskbar, "Start")
        .ok_or_else(|| "classic Windows taskbars are not embedded in this release".to_owned())?;
    let task_band = find_task_band(taskbar)
        .ok_or_else(|| "Windows task-list window was not found".to_owned())?;
    let taskbar_rect = window_rect(taskbar)?;
    let notification_rect = window_rect(notification)?;
    let start_rect = window_rect(start)?;
    let task_band_rect = window_rect(task_band)?;
    let blockers = external_taskbar_blockers(taskbar, child, taskbar_rect)?;
    let tauri_scale = window.scale_factor().map_err(|error| error.to_string())?;
    let child_dpi = unsafe { GetDpiForWindow(child) };
    let scale = effective_taskbar_scale(tauri_scale, child_dpi)
        .ok_or_else(|| "taskbar companion scale factor is invalid".to_owned())?;
    let geometry = win11_geometry(
        taskbar_rect,
        notification_rect,
        task_band_rect,
        start_rect,
        &blockers,
        logical_size,
        scale,
    )
    .ok_or_else(|| "Windows taskbar has no room for the companion".to_owned())?;

    let current_rect = window_rect(child)?;
    if geometry_matches_current_window(current_rect, taskbar_rect, geometry) {
        return Ok(());
    }

    if unsafe {
        SetWindowPos(
            child,
            HWND_TOP,
            geometry.x,
            geometry.y,
            geometry.width,
            geometry.height,
            SWP_NOACTIVATE,
        )
    } == 0
    {
        return Err(last_error("SetWindowPos"));
    }

    // Keep LL-mouse hit testing in sync with the placed screen rect.
    if let Ok(screen) = window_rect(child) {
        update_taskbar_hit_rect(Some(screen));
    }
    Ok(())
}

fn effective_taskbar_scale(tauri_scale: f64, child_dpi: u32) -> Option<f64> {
    if !tauri_scale.is_finite() || tauri_scale <= 0.0 {
        return None;
    }
    if child_dpi == 0 {
        return Some(tauri_scale);
    }
    let child_dpi_scale = f64::from(child_dpi) / 96.0;
    let scale = tauri_scale.max(child_dpi_scale);
    (scale.is_finite() && scale > 0.0).then_some(scale)
}

pub fn hide_taskbar_window(window: &WebviewWindow) -> Result<(), String> {
    clear_taskbar_hit_rect();
    let child = match window.hwnd() {
        Ok(hwnd) => hwnd.0 as isize,
        Err(_) => return Ok(()),
    };
    if unsafe { IsWindow(child) } == 0 {
        return Ok(());
    }

    if window.hide().is_ok() {
        return Ok(());
    }

    unsafe {
        ShowWindow(child, SW_HIDE);
    }
    if unsafe { IsWindow(child) } == 0
        || !child_has_visible_style(unsafe { GetWindowLongPtrW(child, GWL_STYLE) })
    {
        Ok(())
    } else {
        Err("taskbar companion remained visible after native hide".to_owned())
    }
}

pub fn show_recovery_window(window: &WebviewWindow) -> Result<(), String> {
    if window.show().is_ok() {
        return Ok(());
    }

    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    if unsafe { IsWindow(hwnd) } == 0 {
        return Err("main recovery window handle is invalid".to_owned());
    }
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
    }
    if unsafe { IsWindowVisible(hwnd) } != 0 {
        Ok(())
    } else {
        Err("main recovery window remained hidden after native show".to_owned())
    }
}

fn find_win11_taskbar() -> Result<isize, String> {
    let taskbar = find_window("Shell_TrayWnd")
        .ok_or_else(|| "Windows taskbar window was not found".to_owned())?;
    find_child(taskbar, "TrayNotifyWnd")
        .ok_or_else(|| "Windows notification-area window was not found".to_owned())?;
    find_child(taskbar, "Start")
        .ok_or_else(|| "classic Windows taskbars are not embedded in this release".to_owned())?;
    find_task_band(taskbar).ok_or_else(|| "Windows task-list window was not found".to_owned())?;
    Ok(taskbar)
}

fn find_task_band(taskbar: isize) -> Option<isize> {
    let rebar = find_child(taskbar, "ReBarWindow32")?;
    find_child(rebar, "MSTaskSwWClass")
}

fn external_taskbar_blockers(
    taskbar: isize,
    companion: isize,
    taskbar_rect: WinRect,
) -> Result<Vec<WinRect>, String> {
    let mut taskbar_process_id = 0;
    if unsafe { GetWindowThreadProcessId(taskbar, &mut taskbar_process_id) } == 0
        || taskbar_process_id == 0
    {
        return Err(last_error("GetWindowThreadProcessId"));
    }

    let mut search = BlockerSearch {
        taskbar: taskbar_rect,
        taskbar_process_id,
        current_process_id: std::process::id(),
        companion,
        blockers: Vec::new(),
    };
    unsafe {
        EnumChildWindows(
            taskbar,
            Some(collect_external_taskbar_window),
            &mut search as *mut BlockerSearch as isize,
        );
    }
    Ok(search.blockers)
}

unsafe extern "system" fn collect_external_taskbar_window(hwnd: isize, data: isize) -> i32 {
    let search = unsafe { &mut *(data as *mut BlockerSearch) };
    if hwnd == search.companion
        || unsafe { IsChild(search.companion, hwnd) } != 0
        || unsafe { IsWindowVisible(hwnd) } == 0
    {
        return 1;
    }

    let mut process_id = 0;
    if unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) } == 0
        || process_id == 0
        || process_id == search.taskbar_process_id
        || process_id == search.current_process_id
    {
        return 1;
    }

    let mut rect = WinRect::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) } != 0
        && rect.width() > 0
        && rect.height() > 0
        && rect.intersects(search.taskbar)
    {
        search.blockers.push(rect);
    }
    1
}

fn win11_geometry(
    taskbar: WinRect,
    notification: WinRect,
    task_band: WinRect,
    start: WinRect,
    blockers: &[WinRect],
    logical_size: (f64, f64),
    scale: f64,
) -> Option<EmbedGeometry> {
    if !scale.is_finite()
        || scale <= 0.0
        || taskbar.width() < taskbar.height()
        || taskbar.width() <= 8
        || taskbar.height() <= 4
    {
        return None;
    }

    let requested_width = (logical_size.0 * scale).round();
    let requested_height = (logical_size.1 * scale).round();
    let gap = (TASKBAR_GAP_LOGICAL * scale).round();
    if !requested_width.is_finite()
        || !requested_height.is_finite()
        || !gap.is_finite()
        || requested_width < 1.0
        || requested_height < 1.0
        || gap < 1.0
        || requested_width > f64::from(taskbar.width())
        || requested_height > f64::from(taskbar.height())
        || gap > f64::from(i32::MAX)
    {
        return None;
    }
    let width = requested_width as i32;
    let height = requested_height as i32;
    let gap = gap as i32;

    let start_height = start.height().max(1);
    let desired_y = start
        .top
        .saturating_sub(taskbar.top)
        .saturating_add(start_height.saturating_sub(height).saturating_div(2));
    let maximum_y = taskbar.height().saturating_sub(height);
    let y = desired_y.clamp(0, maximum_y);
    let target_top = taskbar.top.saturating_add(y);
    let target_bottom = target_top.saturating_add(height);

    if !taskbar.intersects(notification) || !taskbar.intersects(task_band) {
        return None;
    }
    let safe_left = task_band.right.max(taskbar.left).saturating_add(gap);
    let safe_right = notification.left.min(taskbar.right).saturating_sub(gap);
    if safe_right.saturating_sub(safe_left) < width {
        return None;
    }

    let mut occupied = blockers
        .iter()
        .copied()
        .filter(|rect| {
            rect.width() > 0
                && rect.height() > 0
                && rect.left < safe_right
                && rect.right > safe_left
                && rect.top < target_bottom
                && rect.bottom > target_top
        })
        .map(|rect| {
            (
                rect.left.saturating_sub(gap).max(safe_left),
                rect.right.saturating_add(gap).min(safe_right),
            )
        })
        .filter(|(left, right)| left < right)
        .collect::<Vec<_>>();
    occupied.sort_unstable_by_key(|&(left, right)| (left, right));

    let mut merged = Vec::<(i32, i32)>::with_capacity(occupied.len());
    for (left, right) in occupied {
        if let Some((_, merged_right)) = merged.last_mut() {
            if left <= *merged_right {
                *merged_right = (*merged_right).max(right);
                continue;
            }
        }
        merged.push((left, right));
    }

    let mut slot_right = safe_right;
    for &(left, right) in merged.iter().rev() {
        if slot_right.saturating_sub(right) >= width {
            return Some(EmbedGeometry {
                x: slot_right
                    .saturating_sub(width)
                    .saturating_sub(taskbar.left),
                y,
                width,
                height,
            });
        }
        slot_right = slot_right.min(left);
    }
    if slot_right.saturating_sub(safe_left) < width {
        return None;
    }

    Some(EmbedGeometry {
        x: slot_right
            .saturating_sub(width)
            .saturating_sub(taskbar.left),
        y,
        width,
        height,
    })
}

fn geometry_matches_current_window(
    current: WinRect,
    taskbar: WinRect,
    geometry: EmbedGeometry,
) -> bool {
    let left = taskbar.left.saturating_add(geometry.x);
    let top = taskbar.top.saturating_add(geometry.y);
    current.left == left
        && current.top == top
        && current.right == left.saturating_add(geometry.width)
        && current.bottom == top.saturating_add(geometry.height)
}

fn find_window(class_name: &str) -> Option<isize> {
    let class_name = wide(class_name);
    let hwnd = unsafe { FindWindowW(class_name.as_ptr(), ptr::null()) };
    (hwnd != 0).then_some(hwnd)
}

fn find_child(parent: isize, class_name: &str) -> Option<isize> {
    let class_name = wide(class_name);
    let hwnd = unsafe { FindWindowExW(parent, 0, class_name.as_ptr(), ptr::null()) };
    (hwnd != 0).then_some(hwnd)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn window_rect(hwnd: isize) -> Result<WinRect, String> {
    let mut rect = WinRect::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return Err(last_error("GetWindowRect"));
    }
    Ok(rect)
}

fn last_error(operation: &str) -> String {
    let error = unsafe { GetLastError() };
    format!("{operation} failed with Win32 error {error}")
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumChildWindows(
        parent_window: isize,
        enum_function: Option<unsafe extern "system" fn(isize, isize) -> i32>,
        data: isize,
    ) -> i32;
    fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
    fn FindWindowExW(
        parent: isize,
        child_after: isize,
        class_name: *const u16,
        window_name: *const u16,
    ) -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut WinRect) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn GetWindowThreadProcessId(hwnd: isize, process_id: *mut u32) -> u32;
    fn GetParent(hwnd: isize) -> isize;
    fn IsChild(parent: isize, child: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn GetCursorPos(point: *mut PointI32) -> i32;
    fn ShowWindow(hwnd: isize, command_show: i32) -> i32;
    fn SetWindowPos(
        hwnd: isize,
        insert_after: isize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn SetWindowsHookExW(
        id_hook: i32,
        hook_proc: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        module: isize,
        thread_id: u32,
    ) -> isize;
    fn UnhookWindowsHookEx(hook: isize) -> i32;
    fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
    fn PeekMessageW(
        message: *mut ThreadMessage,
        window: isize,
        filter_minimum: u32,
        filter_maximum: u32,
        remove_message: u32,
    ) -> i32;
    fn GetMessageW(
        message: *mut ThreadMessage,
        window: isize,
        filter_minimum: u32,
        filter_maximum: u32,
    ) -> i32;
    fn PostThreadMessageW(thread_id: u32, message: u32, wparam: usize, lparam: isize) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(module_name: *const u16) -> isize;
    fn GetCurrentThreadId() -> u32;
    fn GetLastError() -> u32;
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::{atomic::Ordering, mpsc, Arc};
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{
        child_has_visible_style, classify_mouse_event, dispatch_taskbar_action_with,
        effective_taskbar_scale, geometry_matches_current_window, hook_maintenance, win11_geometry,
        AtomicHitRect, EmbedGeometry, HookBridge, HookControl, HookControlQueue, HookLeaseState,
        HookMaintenance, HookSample, PointI32, TaskbarInputAction, TaskbarInputController,
        TaskbarLifecycleObservation, TaskbarLifecyclePlan, TaskbarLifecycleState,
        TaskbarRecoveryReason, WinRect, HC_ACTION, HOOK_CONTROL_QUEUE_CAPACITY,
        HOOK_LEASE_DURATION, WM_LBUTTONUP, WM_RBUTTONUP, WS_VISIBLE,
    };

    #[test]
    fn mouse_action_mapping_honors_half_open_hit_rect_boundaries() {
        let rect = WinRect {
            left: -20,
            top: -10,
            right: 30,
            bottom: 40,
        };
        assert_eq!(
            classify_mouse_event(
                HC_ACTION,
                WM_LBUTTONUP as usize,
                PointI32 { x: -20, y: -10 },
                Some(rect),
            ),
            Some(TaskbarInputAction::LeftClick)
        );
        assert_eq!(
            classify_mouse_event(
                HC_ACTION,
                WM_RBUTTONUP as usize,
                PointI32 { x: 29, y: 39 },
                Some(rect),
            ),
            Some(TaskbarInputAction::RightClick)
        );
        for point in [
            PointI32 { x: 30, y: 0 },
            PointI32 { x: 0, y: 40 },
            PointI32 { x: -21, y: 0 },
            PointI32 { x: 0, y: -11 },
        ] {
            assert_eq!(
                classify_mouse_event(HC_ACTION, WM_LBUTTONUP as usize, point, Some(rect),),
                None
            );
        }
    }

    #[test]
    fn mouse_action_mapping_rejects_non_actions_unrelated_messages_and_missing_rects() {
        let point = PointI32 { x: 5, y: 5 };
        let rect = WinRect {
            left: 0,
            top: 0,
            right: 10,
            bottom: 10,
        };
        assert_eq!(
            classify_mouse_event(-1, WM_LBUTTONUP as usize, point, Some(rect)),
            None
        );
        assert_eq!(
            classify_mouse_event(HC_ACTION, 0x0200, point, Some(rect)),
            None
        );
        assert_eq!(
            classify_mouse_event(HC_ACTION, WM_RBUTTONUP as usize, point, None),
            None
        );
        assert_eq!(TaskbarInputAction::from_message_value(99), None);
    }

    #[test]
    fn taskbar_actions_dispatch_to_exactly_one_business_path() {
        let calls = RefCell::new(Vec::new());
        dispatch_taskbar_action_with(
            TaskbarInputAction::LeftClick,
            || calls.borrow_mut().push("show-details"),
            || calls.borrow_mut().push("context-menu"),
        );
        dispatch_taskbar_action_with(
            TaskbarInputAction::RightClick,
            || calls.borrow_mut().push("show-details"),
            || calls.borrow_mut().push("context-menu"),
        );

        assert_eq!(*calls.borrow(), ["show-details", "context-menu"]);
    }

    #[test]
    fn atomic_hit_rect_publishes_complete_snapshots_and_clear() {
        let hit_rect = AtomicHitRect::new();
        assert_eq!(hit_rect.load(), None);
        let rect = WinRect {
            left: -40,
            top: 10,
            right: 80,
            bottom: 60,
        };
        hit_rect.store(Some(rect));
        assert_eq!(hit_rect.load(), Some(rect));
        hit_rect.store(None);
        assert_eq!(hit_rect.load(), None);
    }

    #[test]
    fn hook_bridge_only_allows_its_owner_to_clear_and_release_state() {
        let bridge = HookBridge::new();
        let rect = WinRect {
            left: 1,
            top: 2,
            right: 3,
            bottom: 4,
        };
        assert!(bridge.claim_worker(11));
        bridge.desired_enabled.store(true, Ordering::Release);
        bridge.hit_rect.store(Some(rect));

        assert!(!bridge.claim_worker(22));
        assert!(!bridge.release_worker(22));
        assert!(bridge.desired_enabled.load(Ordering::Acquire));
        assert_eq!(bridge.hit_rect.load(), Some(rect));
        assert!(bridge.owns_worker(11));

        assert!(bridge.release_worker(11));
        assert!(!bridge.desired_enabled.load(Ordering::Acquire));
        assert_eq!(bridge.hit_rect.load(), None);
        assert!(bridge.claim_worker(22));
        assert!(bridge.release_worker(22));
    }

    #[test]
    fn hook_maintenance_enables_first_lease_and_rearms_when_expired() {
        let base = Instant::now();
        let sample = HookSample {
            cursor: Some(PointI32 { x: 1, y: 2 }),
            event_sequence: 7,
        };
        assert_eq!(
            hook_maintenance(&HookLeaseState::default(), base, sample),
            HookMaintenance::Enable
        );

        let active = HookLeaseState {
            enabled: true,
            installed_at: Some(base),
            last_sample: Some(sample),
        };
        assert_eq!(
            hook_maintenance(
                &active,
                base + HOOK_LEASE_DURATION - Duration::from_millis(1),
                sample
            ),
            HookMaintenance::None
        );
        assert_eq!(
            hook_maintenance(&active, base + HOOK_LEASE_DURATION, sample),
            HookMaintenance::Rearm
        );
    }

    #[test]
    fn hook_maintenance_rearms_on_cursor_movement_without_event_progress() {
        let base = Instant::now();
        let previous = HookSample {
            cursor: Some(PointI32 { x: 10, y: 10 }),
            event_sequence: 20,
        };
        let active = HookLeaseState {
            enabled: true,
            installed_at: Some(base),
            last_sample: Some(previous),
        };
        let moved = HookSample {
            cursor: Some(PointI32 { x: 11, y: 10 }),
            event_sequence: 20,
        };
        assert_eq!(
            hook_maintenance(&active, base + Duration::from_secs(1), moved),
            HookMaintenance::Rearm
        );
        assert_eq!(
            hook_maintenance(
                &active,
                base + Duration::from_secs(1),
                HookSample {
                    event_sequence: 21,
                    ..moved
                },
            ),
            HookMaintenance::None
        );
    }

    #[test]
    fn hook_control_queue_is_bounded_and_cancel_releases_capacity() {
        let queue = HookControlQueue::new();
        let mut pending = Vec::new();
        for _ in 0..HOOK_CONTROL_QUEUE_CAPACITY {
            pending.push(queue.enqueue(HookControl::Enable).expect("queue slot"));
        }
        assert!(queue.enqueue(HookControl::Enable).is_err());
        assert!(queue.cancel(pending[0].0));
        assert!(queue.enqueue(HookControl::Disable).is_ok());
    }

    #[test]
    fn taskbar_input_controller_starts_idle_and_idle_disable_is_reusable() {
        let controller = TaskbarInputController::idle();
        assert!(controller.runtime.lock().expect("runtime").is_none());
        controller.disable().expect("idle disable");
        assert!(controller.runtime.lock().expect("runtime").is_none());

        let sample = HookSample {
            cursor: None,
            event_sequence: 0,
        };
        assert_eq!(
            hook_maintenance(
                &controller.lease.lock().expect("lease"),
                Instant::now(),
                sample,
            ),
            HookMaintenance::Enable
        );
    }

    #[test]
    fn taskbar_input_lifecycle_gate_serializes_disable() {
        let controller = Arc::new(TaskbarInputController::idle());
        let lifecycle = controller.lifecycle.lock().expect("lifecycle gate");
        let worker_controller = Arc::clone(&controller);
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let (done_sender, done_receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            started_sender.send(()).expect("started");
            done_sender.send(worker_controller.disable()).expect("done");
        });

        started_receiver.recv().expect("worker start");
        assert!(done_receiver
            .recv_timeout(Duration::from_millis(50))
            .is_err());
        drop(lifecycle);
        assert!(done_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("serialized disable")
            .is_ok());
        worker.join().expect("worker join");
    }

    #[test]
    fn disable_and_shutdown_controls_both_terminate_the_worker() {
        for command in [HookControl::Disable, HookControl::Shutdown] {
            let mut hook = 0;
            let (result, terminate) = super::execute_hook_control(command, &mut hook);
            assert!(result.is_ok());
            assert!(terminate);
            assert_eq!(hook, 0);
        }
    }

    #[test]
    fn taskbar_lifecycle_destroys_detached_label_only_once() {
        let mut state = TaskbarLifecycleState::default();
        let detached = TaskbarLifecycleObservation {
            window_id: Some(11),
            attached_to_host: false,
            host: Some(22),
        };
        assert_eq!(
            state.plan(detached),
            TaskbarLifecyclePlan::Destroy { window_id: 11 }
        );
        assert_eq!(
            state.plan(detached),
            TaskbarLifecyclePlan::Wait(TaskbarRecoveryReason::WaitingForLabelRemoval)
        );
    }

    #[test]
    fn taskbar_lifecycle_waits_for_host_then_claims_one_build_token() {
        let mut state = TaskbarLifecycleState::default();
        assert_eq!(
            state.plan(TaskbarLifecycleObservation {
                window_id: None,
                attached_to_host: false,
                host: None,
            }),
            TaskbarLifecyclePlan::Wait(TaskbarRecoveryReason::WaitingForTaskbar)
        );
        let build = state.plan(TaskbarLifecycleObservation {
            window_id: None,
            attached_to_host: false,
            host: Some(22),
        });
        assert_eq!(build, TaskbarLifecyclePlan::Build { token: 1, host: 22 });
        assert_eq!(
            state.plan(TaskbarLifecycleObservation {
                window_id: None,
                attached_to_host: false,
                host: Some(22),
            }),
            TaskbarLifecyclePlan::Wait(TaskbarRecoveryReason::BuildInProgress)
        );
        assert!(!state.finish_build(2, 22, 33, true));
        assert!(state.finish_build(1, 22, 33, true));
        assert_eq!(
            state.plan(TaskbarLifecycleObservation {
                window_id: Some(33),
                attached_to_host: true,
                host: Some(22),
            }),
            TaskbarLifecyclePlan::Ready
        );
    }

    #[test]
    fn taskbar_lifecycle_rejects_a_build_after_host_change_and_waits_for_destroy() {
        let mut state = TaskbarLifecycleState::default();
        assert_eq!(
            state.plan(TaskbarLifecycleObservation {
                window_id: None,
                attached_to_host: false,
                host: Some(22),
            }),
            TaskbarLifecyclePlan::Build { token: 1, host: 22 }
        );
        assert!(!state.finish_build(1, 22, 33, false));
        assert_eq!(
            state.plan(TaskbarLifecycleObservation {
                window_id: Some(33),
                attached_to_host: false,
                host: Some(44),
            }),
            TaskbarLifecyclePlan::Wait(TaskbarRecoveryReason::WaitingForLabelRemoval)
        );
    }

    #[test]
    fn child_visibility_uses_only_the_child_style_bit() {
        assert!(child_has_visible_style(WS_VISIBLE));
        assert!(child_has_visible_style(WS_VISIBLE | 0x0008_0000));
        assert!(!child_has_visible_style(0));
        assert!(!child_has_visible_style(0x0008_0000));
    }

    #[test]
    fn effective_scale_uses_child_dpi_only_as_a_lower_bound() {
        assert_eq!(effective_taskbar_scale(1.25, 144), Some(1.5));
        assert_eq!(effective_taskbar_scale(1.5, 120), Some(1.5));
        assert_eq!(effective_taskbar_scale(1.25, 0), Some(1.25));
    }

    #[test]
    fn effective_scale_rejects_invalid_tauri_scale() {
        for scale in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            assert_eq!(effective_taskbar_scale(scale, 144), None);
        }
    }

    #[test]
    fn unchanged_child_geometry_is_detected_in_screen_coordinates() {
        let taskbar = WinRect {
            left: -1920,
            top: 1000,
            right: 0,
            bottom: 1048,
        };
        let geometry = EmbedGeometry {
            x: 1488,
            y: 9,
            width: 168,
            height: 30,
        };
        assert!(geometry_matches_current_window(
            WinRect {
                left: -432,
                top: 1009,
                right: -264,
                bottom: 1039,
            },
            taskbar,
            geometry,
        ));
        assert!(!geometry_matches_current_window(
            WinRect {
                left: -433,
                top: 1009,
                right: -265,
                bottom: 1039,
            },
            taskbar,
            geometry,
        ));
    }

    #[test]
    fn win11_geometry_scales_at_supported_display_factors() {
        for scale in [1.0_f64, 1.25, 1.5, 2.0] {
            let taskbar_height = (48.0 * scale).round() as i32;
            let taskbar = WinRect {
                left: 0,
                top: 1000,
                right: (1920.0 * scale).round() as i32,
                bottom: 1000 + taskbar_height,
            };
            let notification = WinRect {
                left: (1700.0 * scale).round() as i32,
                top: taskbar.top,
                right: taskbar.right,
                bottom: taskbar.bottom,
            };
            let task_band = WinRect {
                left: (700.0 * scale).round() as i32,
                top: taskbar.top,
                right: (1200.0 * scale).round() as i32,
                bottom: taskbar.bottom,
            };
            let start = WinRect {
                left: (900.0 * scale) as i32,
                top: taskbar.top,
                right: (948.0 * scale) as i32,
                bottom: taskbar.bottom,
            };

            let result = win11_geometry(
                taskbar,
                notification,
                task_band,
                start,
                &[],
                (168.0, 30.0),
                scale,
            )
            .expect("geometry");

            assert_eq!(result.width, (168.0 * scale).round() as i32);
            assert_eq!(result.height, (30.0 * scale).round() as i32);
            assert_eq!(
                result.x,
                notification.left - (2.0 * scale).round() as i32 - result.width - taskbar.left
            );
            assert!(result.x >= task_band.right - taskbar.left);
            assert!(result.x + result.width < notification.left - taskbar.left);
            assert!(result.y >= 0);
            assert!(result.y + result.height <= taskbar.height());
        }
    }

    #[test]
    fn geometry_preserves_the_requested_size_without_margin_shrink() {
        let geometry = win11_geometry(
            WinRect {
                left: 0,
                top: 1000,
                right: 1920,
                bottom: 1040,
            },
            WinRect {
                left: 1700,
                top: 1000,
                right: 1920,
                bottom: 1040,
            },
            WinRect {
                left: 700,
                top: 1000,
                right: 1000,
                bottom: 1040,
            },
            WinRect {
                left: 900,
                top: 1000,
                right: 940,
                bottom: 1040,
            },
            &[],
            (280.0, 30.0),
            1.0,
        );

        assert_eq!(
            geometry,
            Some(EmbedGeometry {
                x: 1418,
                y: 5,
                width: 280,
                height: 30,
            })
        );
    }

    #[test]
    fn geometry_avoids_the_observed_traffic_monitor_window() {
        let geometry = win11_geometry(
            WinRect {
                left: 0,
                top: 1392,
                right: 2560,
                bottom: 1440,
            },
            WinRect {
                left: 2186,
                top: 1392,
                right: 2560,
                bottom: 1440,
            },
            WinRect {
                left: 815,
                top: 1392,
                right: 1260,
                bottom: 1440,
            },
            WinRect {
                left: 815,
                top: 1392,
                right: 860,
                bottom: 1440,
            },
            &[WinRect {
                left: 2095,
                top: 1400,
                right: 2188,
                bottom: 1432,
            }],
            (168.0, 30.0),
            1.0,
        );

        assert_eq!(
            geometry,
            Some(EmbedGeometry {
                x: 1925,
                y: 9,
                width: 168,
                height: 30,
            })
        );
    }

    #[test]
    fn runtime_blocker_moves_an_existing_companion_left_with_a_gap() {
        let taskbar = WinRect {
            left: 0,
            top: 1392,
            right: 2560,
            bottom: 1440,
        };
        let notification = WinRect {
            left: 2186,
            top: 1392,
            right: 2560,
            bottom: 1440,
        };
        let task_band = WinRect {
            left: 815,
            top: 1392,
            right: 1260,
            bottom: 1440,
        };
        let start = WinRect {
            left: 815,
            top: 1392,
            right: 860,
            bottom: 1440,
        };
        let traffic_monitor = WinRect {
            left: 2095,
            top: 1400,
            right: 2188,
            bottom: 1432,
        };

        let before = win11_geometry(
            taskbar,
            notification,
            task_band,
            start,
            &[],
            (168.0, 30.0),
            1.0,
        )
        .expect("initial geometry");
        let after = win11_geometry(
            taskbar,
            notification,
            task_band,
            start,
            &[traffic_monitor],
            (168.0, 30.0),
            1.0,
        )
        .expect("runtime blocker geometry");

        assert!(after.x < before.x);
        assert_eq!(after.x + after.width, traffic_monitor.left - 2);
    }

    #[test]
    fn geometry_merges_unordered_blockers_and_uses_a_farther_left_slot() {
        let geometry = win11_geometry(
            WinRect {
                left: 0,
                top: 1000,
                right: 1000,
                bottom: 1040,
            },
            WinRect {
                left: 900,
                top: 1000,
                right: 1000,
                bottom: 1040,
            },
            WinRect {
                left: 0,
                top: 1000,
                right: 300,
                bottom: 1040,
            },
            WinRect {
                left: 400,
                top: 1000,
                right: 440,
                bottom: 1040,
            },
            &[
                WinRect {
                    left: 780,
                    top: 1000,
                    right: 850,
                    bottom: 1040,
                },
                WinRect {
                    left: 650,
                    top: 1000,
                    right: 720,
                    bottom: 1040,
                },
                WinRect {
                    left: 760,
                    top: 1000,
                    right: 810,
                    bottom: 1040,
                },
            ],
            (100.0, 30.0),
            1.0,
        );

        assert_eq!(geometry.expect("geometry").x, 548);
    }

    #[test]
    fn geometry_ignores_blockers_outside_the_target_vertical_band() {
        let geometry = win11_geometry(
            WinRect {
                left: 0,
                top: 1000,
                right: 1000,
                bottom: 1040,
            },
            WinRect {
                left: 900,
                top: 1000,
                right: 1000,
                bottom: 1040,
            },
            WinRect {
                left: 0,
                top: 1000,
                right: 300,
                bottom: 1040,
            },
            WinRect {
                left: 400,
                top: 1000,
                right: 440,
                bottom: 1040,
            },
            &[WinRect {
                left: 700,
                top: 950,
                right: 900,
                bottom: 1005,
            }],
            (100.0, 30.0),
            1.0,
        );

        assert_eq!(geometry.expect("geometry").x, 798);
    }

    #[test]
    fn geometry_returns_none_when_blockers_consume_the_safe_band() {
        let geometry = win11_geometry(
            WinRect {
                left: 0,
                top: 1000,
                right: 1000,
                bottom: 1040,
            },
            WinRect {
                left: 900,
                top: 1000,
                right: 1000,
                bottom: 1040,
            },
            WinRect {
                left: 0,
                top: 1000,
                right: 300,
                bottom: 1040,
            },
            WinRect {
                left: 400,
                top: 1000,
                right: 440,
                bottom: 1040,
            },
            &[WinRect {
                left: 300,
                top: 1000,
                right: 900,
                bottom: 1040,
            }],
            (100.0, 30.0),
            1.0,
        );

        assert_eq!(geometry, None);
    }

    #[test]
    fn geometry_handles_a_negative_taskbar_origin() {
        let geometry = win11_geometry(
            WinRect {
                left: -1920,
                top: 1000,
                right: 0,
                bottom: 1048,
            },
            WinRect {
                left: -220,
                top: 1000,
                right: 0,
                bottom: 1048,
            },
            WinRect {
                left: -1100,
                top: 1000,
                right: -700,
                bottom: 1048,
            },
            WinRect {
                left: -1000,
                top: 1000,
                right: -952,
                bottom: 1048,
            },
            &[],
            (210.0, 30.0),
            1.0,
        );

        assert_eq!(
            geometry,
            Some(EmbedGeometry {
                x: 1488,
                y: 9,
                width: 210,
                height: 30,
            })
        );
    }

    #[test]
    fn geometry_rejects_a_widget_taller_than_the_taskbar() {
        let geometry = win11_geometry(
            WinRect {
                left: 0,
                top: 1000,
                right: 1920,
                bottom: 1040,
            },
            WinRect {
                left: 1700,
                top: 1000,
                right: 1920,
                bottom: 1040,
            },
            WinRect {
                left: 700,
                top: 1000,
                right: 1000,
                bottom: 1040,
            },
            WinRect {
                left: 900,
                top: 1000,
                right: 940,
                bottom: 1040,
            },
            &[],
            (280.0, 41.0),
            1.0,
        );

        assert_eq!(geometry, None);
    }
}
