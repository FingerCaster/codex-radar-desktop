use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, AtomicIsize, Ordering},
    sync::Mutex,
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
const HC_ACTION: i32 = 0;

static TASKBAR_APP: Mutex<Option<AppHandle>> = Mutex::new(None);
static TASKBAR_HIT_RECT: Mutex<Option<WinRect>> = Mutex::new(None);
static TASKBAR_MOUSE_HOOK: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

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

pub fn create_taskbar_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(TASKBAR_WINDOW_LABEL) {
        if taskbar_companion_is_attached(&window) {
            return Ok(window);
        }
        // Detached companions cannot be re-parented; close and rebuild under Shell_TrayWnd.
        let _ = window.close();
    }

    let taskbar = find_win11_taskbar()?;
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == TASKBAR_WINDOW_LABEL)
        .ok_or_else(|| "taskbar window configuration is missing".to_owned())?;

    let parent = HWND(taskbar as *mut c_void);
    let window = WebviewWindowBuilder::from_config(app, config)
        .map_err(|error| error.to_string())?
        .parent_raw(parent)
        .build()
        .map_err(|error| error.to_string())?;
    install_taskbar_input_hooks(app, &window)?;
    Ok(window)
}

/// Install recovery input for the taskbar companion.
///
/// WebView2's `Chrome_RenderWidgetHostHWND` lives in another process, so
/// subclassing our Tauri HWND never sees real mouse clicks. A low-level mouse
/// hook watches the companion screen rect instead.
pub fn install_taskbar_input_hooks(
    app: &AppHandle,
    window: &WebviewWindow,
) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?.0 as isize;
    if unsafe { IsWindow(hwnd) } == 0 {
        return Err("taskbar companion handle is invalid".to_owned());
    }

    if let Ok(mut slot) = TASKBAR_APP.lock() {
        *slot = Some(app.clone());
    }
    if let Ok(rect) = window_rect(hwnd) {
        update_taskbar_hit_rect(Some(rect));
    }
    install_taskbar_mouse_hook()?;
    Ok(())
}

pub fn clear_taskbar_hit_rect() {
    update_taskbar_hit_rect(None);
}

fn update_taskbar_hit_rect(rect: Option<WinRect>) {
    if let Ok(mut slot) = TASKBAR_HIT_RECT.lock() {
        *slot = rect;
    }
}

fn install_taskbar_mouse_hook() -> Result<(), String> {
    if TASKBAR_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let hook = unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(taskbar_mouse_hook_proc),
            GetModuleHandleW(ptr::null()),
            0,
        )
    };
    if hook == 0 {
        TASKBAR_HOOK_INSTALLED.store(false, Ordering::SeqCst);
        return Err(last_error("SetWindowsHookExW(WH_MOUSE_LL)"));
    }
    TASKBAR_MOUSE_HOOK.store(hook, Ordering::SeqCst);
    Ok(())
}

#[repr(C)]
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

unsafe extern "system" fn taskbar_mouse_hook_proc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    // SAFETY: WH_MOUSE_LL contract; lparam points to MSLLHOOKSTRUCT when code == HC_ACTION.
    unsafe {
        if code == HC_ACTION
            && (wparam == WM_LBUTTONUP as usize || wparam == WM_RBUTTONUP as usize)
            && lparam != 0
        {
            let info = &*(lparam as *const MsllHookStruct);
            if taskbar_hit_rect_contains(info.pt.x, info.pt.y) {
                if wparam == WM_LBUTTONUP as usize {
                    dispatch_taskbar_left_click();
                } else {
                    dispatch_taskbar_right_click();
                }
            }
        }
        CallNextHookEx(
            TASKBAR_MOUSE_HOOK.load(Ordering::SeqCst),
            code,
            wparam,
            lparam,
        )
    }
}

fn taskbar_hit_rect_contains(x: i32, y: i32) -> bool {
    let Ok(slot) = TASKBAR_HIT_RECT.lock() else {
        return false;
    };
    let Some(rect) = *slot else {
        return false;
    };
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

fn dispatch_taskbar_left_click() {
    let app = {
        let Ok(guard) = TASKBAR_APP.lock() else {
            return;
        };
        guard.as_ref().cloned()
    };
    let Some(app) = app else {
        return;
    };
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

fn dispatch_taskbar_right_click() {
    let app = {
        let Ok(guard) = TASKBAR_APP.lock() else {
            return;
        };
        guard.as_ref().cloned()
    };
    let Some(app) = app else {
        return;
    };
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
    find_window("Shell_TrayWnd").is_some_and(|taskbar| parent == taskbar)
}

/// Attached, still a live HWND, and currently visible to the user.
pub fn taskbar_companion_is_healthy(window: &WebviewWindow) -> bool {
    if !taskbar_companion_is_attached(window) {
        return false;
    }
    let Ok(hwnd) = window.hwnd() else {
        return false;
    };
    let child = hwnd.0 as isize;
    unsafe { IsWindowVisible(child) != 0 }
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
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
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
    if unsafe { IsWindow(child) } == 0 || unsafe { IsWindowVisible(child) } == 0 {
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
    fn GetWindowThreadProcessId(hwnd: isize, process_id: *mut u32) -> u32;
    fn GetParent(hwnd: isize) -> isize;
    fn IsChild(parent: isize, child: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
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
    fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(module_name: *const u16) -> isize;
    fn GetLastError() -> u32;
}

#[cfg(test)]
mod tests {
    use super::{geometry_matches_current_window, win11_geometry, EmbedGeometry, WinRect};

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
