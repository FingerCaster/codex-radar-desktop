use std::{
    fs,
    path::{Path, PathBuf},
    sync::{mpsc, Mutex, MutexGuard},
    time::Duration,
};

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri::{
    menu::{CheckMenuItem, ContextMenu, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow,
    Window, Wry,
};
use tauri_plugin_autostart::ManagerExt;

use crate::radar::{service::REFRESH_REQUESTED_EVENT, RadarService, RadarSource};

#[cfg(windows)]
mod windows;

pub const PREFERENCES_UPDATED_EVENT: &str = "desktop://preferences-updated";
pub const MAIN_EXPANDED_EVENT: &str = "desktop://main-expanded";
pub const SHOW_MAIN_DETAILS_EVENT: &str = "desktop://show-main-details";

const MAIN_WINDOW_LABEL: &str = "main";
const TASKBAR_WINDOW_LABEL: &str = "taskbar";
const TRAY_ID: &str = "model-radar-control";
const PREFERENCES_FILE: &str = "desktop-preferences.json";
const MAIN_POSITION_FILE: &str = "main-window-position.json";
const COMPACT_SIZE: (f64, f64) = (360.0, 112.0);
const EXPANDED_SIZE: (f64, f64) = (400.0, 520.0);
const MAIN_POSITION_SAVE_DELAY: Duration = Duration::from_millis(200);
#[cfg(windows)]
const TASKBAR_SIZE: (f64, f64) = (168.0, 30.0);
#[cfg(windows)]
const TASKBAR_MONITOR_INTERVAL: Duration = Duration::from_secs(1);
const VALID_OPACITY: [u8; 5] = [100, 90, 80, 70, 60];

const MENU_ALWAYS_ON_TOP: &str = "desktop.always-on-top";
const MENU_CLICK_THROUGH: &str = "desktop.click-through";
const MENU_POSITION_LOCKED: &str = "desktop.position-locked";
const MENU_POSITION_SUBMENU: &str = "desktop.position";
const MENU_POSITION_TOP_LEFT: &str = "desktop.position.top-left";
const MENU_POSITION_TOP_RIGHT: &str = "desktop.position.top-right";
const MENU_POSITION_CENTER: &str = "desktop.position.center";
const MENU_POSITION_BOTTOM_LEFT: &str = "desktop.position.bottom-left";
const MENU_POSITION_BOTTOM_RIGHT: &str = "desktop.position.bottom-right";
const MENU_SHOW_TASKBAR_WINDOW: &str = "desktop.show-taskbar-window";
const MENU_SHOW_MAIN_WINDOW: &str = "desktop.show-main-window";
const MENU_LAUNCH_AT_LOGIN: &str = "desktop.launch-at-login";
const MENU_OPACITY_PREFIX: &str = "desktop.opacity.";
const MENU_RADAR_SOURCE_SUBMENU: &str = "desktop.radar-source";
const MENU_RADAR_SOURCE_MAIN: &str = "desktop.radar-source.main";
const MENU_RADAR_SOURCE_DISTRIBUTED: &str = "desktop.radar-source.distributed";
const MENU_REFRESH: &str = "desktop.refresh";
const MENU_QUIT: &str = "desktop.quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayLeftClickBehavior {
    ToggleMain,
    ShowDetails,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MainWindowPositionPreset {
    TopLeft,
    TopRight,
    Center,
    BottomLeft,
    BottomRight,
}

impl MainWindowPositionPreset {
    fn from_menu_id(id: &str) -> Option<Self> {
        match id {
            MENU_POSITION_TOP_LEFT => Some(Self::TopLeft),
            MENU_POSITION_TOP_RIGHT => Some(Self::TopRight),
            MENU_POSITION_CENTER => Some(Self::Center),
            MENU_POSITION_BOTTOM_LEFT => Some(Self::BottomLeft),
            MENU_POSITION_BOTTOM_RIGHT => Some(Self::BottomRight),
            _ => None,
        }
    }
}

fn radar_source_from_menu_id(id: &str) -> Option<RadarSource> {
    match id {
        MENU_RADAR_SOURCE_MAIN => Some(RadarSource::Main),
        MENU_RADAR_SOURCE_DISTRIBUTED => Some(RadarSource::Distributed),
        _ => None,
    }
}

fn radar_source_checks(source: &RadarSource) -> [bool; 2] {
    [
        source == &RadarSource::Main,
        source == &RadarSource::Distributed,
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedMainWindowPosition {
    x: i32,
    y: i32,
}

impl From<PhysicalPosition<i32>> for SavedMainWindowPosition {
    fn from(position: PhysicalPosition<i32>) -> Self {
        Self {
            x: position.x,
            y: position.y,
        }
    }
}

impl From<SavedMainWindowPosition> for PhysicalPosition<i32> {
    fn from(position: SavedMainWindowPosition) -> Self {
        Self::new(position.x, position.y)
    }
}

#[derive(Debug)]
struct MainPositionSaveState {
    revision: u64,
    persisted_revision: u64,
    writer_active: bool,
    ready: bool,
    last_saved: Option<SavedMainWindowPosition>,
}

impl MainPositionSaveState {
    fn new(last_saved: Option<SavedMainWindowPosition>) -> Self {
        Self {
            revision: 0,
            persisted_revision: 0,
            writer_active: false,
            ready: false,
            last_saved,
        }
    }

    fn mark_dirty(&mut self) -> bool {
        if !self.ready {
            return false;
        }
        self.revision = self.revision.wrapping_add(1);
        if self.writer_active {
            false
        } else {
            self.writer_active = true;
            true
        }
    }

    fn finish_writer_after_error(&mut self, attempted_revision: u64) -> bool {
        let should_retry = self.revision != attempted_revision;
        self.writer_active = should_retry;
        should_retry
    }
}

fn tray_left_click_behavior(is_macos: bool) -> TrayLeftClickBehavior {
    if is_macos {
        TrayLeftClickBehavior::ShowDetails
    } else {
        TrayLeftClickBehavior::ToggleMain
    }
}

#[cfg(windows)]
#[derive(Debug, PartialEq, Eq)]
enum TaskbarMonitorOutcome {
    Inactive,
    Placed,
    Disabled {
        placement_error: String,
        recovery_errors: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DesktopPreferences {
    pub always_on_top: bool,
    pub click_through: bool,
    pub position_locked: bool,
    pub show_taskbar_window: bool,
    pub show_main_window: bool,
    pub launch_at_login: bool,
    pub opacity_percent: u8,
    pub radar_source: RadarSource,
}

impl Default for DesktopPreferences {
    fn default() -> Self {
        Self {
            always_on_top: true,
            click_through: false,
            position_locked: false,
            show_taskbar_window: true,
            show_main_window: true,
            launch_at_login: false,
            opacity_percent: 100,
            radar_source: RadarSource::Main,
        }
    }
}

impl DesktopPreferences {
    fn normalize_loaded(mut self) -> Self {
        if !VALID_OPACITY.contains(&self.opacity_percent) {
            self.opacity_percent = 100;
        }
        if !self.show_taskbar_window && !self.show_main_window {
            self.show_taskbar_window = true;
        }
        self
    }

    fn with_option(mut self, option: DesktopOption, enabled: bool) -> Self {
        match option {
            DesktopOption::AlwaysOnTop => self.always_on_top = enabled,
            DesktopOption::ClickThrough => self.click_through = enabled,
            DesktopOption::PositionLocked => self.position_locked = enabled,
            DesktopOption::ShowTaskbarWindow => self.show_taskbar_window = enabled,
            DesktopOption::ShowMainWindow => self.show_main_window = enabled,
            DesktopOption::LaunchAtLogin => self.launch_at_login = enabled,
        }

        if !self.show_taskbar_window && !self.show_main_window {
            match option {
                DesktopOption::ShowTaskbarWindow => self.show_main_window = true,
                _ => self.show_taskbar_window = true,
            }
        }
        self
    }

    fn with_radar_source(mut self, source: RadarSource) -> Self {
        self.radar_source = source;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopOption {
    AlwaysOnTop,
    ClickThrough,
    PositionLocked,
    ShowTaskbarWindow,
    ShowMainWindow,
    LaunchAtLogin,
}

impl DesktopOption {
    fn from_menu_id(id: &str) -> Option<Self> {
        match id {
            MENU_ALWAYS_ON_TOP => Some(Self::AlwaysOnTop),
            MENU_CLICK_THROUGH => Some(Self::ClickThrough),
            MENU_POSITION_LOCKED => Some(Self::PositionLocked),
            MENU_SHOW_TASKBAR_WINDOW => Some(Self::ShowTaskbarWindow),
            MENU_SHOW_MAIN_WINDOW => Some(Self::ShowMainWindow),
            MENU_LAUNCH_AT_LOGIN => Some(Self::LaunchAtLogin),
            _ => None,
        }
    }

    fn enabled(self, preferences: &DesktopPreferences) -> bool {
        match self {
            Self::AlwaysOnTop => preferences.always_on_top,
            Self::ClickThrough => preferences.click_through,
            Self::PositionLocked => preferences.position_locked,
            Self::ShowTaskbarWindow => preferences.show_taskbar_window,
            Self::ShowMainWindow => preferences.show_main_window,
            Self::LaunchAtLogin => preferences.launch_at_login,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CompanionProjection {
    pub model_name: String,
    pub reasoning_effort: String,
    pub score_text: String,
    pub tie_count: u64,
    pub status_label: String,
}

impl CompanionProjection {
    fn normalized(self) -> Self {
        Self {
            model_name: bounded_single_line(&self.model_name, 28),
            reasoning_effort: bounded_single_line(&self.reasoning_effort, 12),
            score_text: bounded_single_line(&self.score_text, 12),
            tie_count: self.tie_count.min(99),
            status_label: bounded_single_line(&self.status_label, 12),
        }
    }

    fn title(&self) -> String {
        let model = if self.model_name.is_empty() {
            "Model Radar"
        } else {
            &self.model_name
        };
        let score = if self.score_text.is_empty() {
            "--"
        } else {
            &self.score_text
        };
        let mut parts = vec![model.to_owned(), score.to_owned()];
        if self.tie_count > 0 {
            parts.push(format!("+{}", self.tie_count));
        }
        bounded_single_line(&parts.join(" | "), 64)
    }
}

struct DesktopMenu {
    menu: Menu<Wry>,
    always_on_top: CheckMenuItem<Wry>,
    click_through: CheckMenuItem<Wry>,
    position_locked: CheckMenuItem<Wry>,
    show_taskbar_window: CheckMenuItem<Wry>,
    show_main_window: CheckMenuItem<Wry>,
    launch_at_login: CheckMenuItem<Wry>,
    radar_source_main: CheckMenuItem<Wry>,
    radar_source_distributed: CheckMenuItem<Wry>,
    opacity: Vec<(u8, CheckMenuItem<Wry>)>,
}

impl DesktopMenu {
    fn new(app: &AppHandle, preferences: &DesktopPreferences) -> Result<Self, String> {
        let always_on_top = check_item(
            app,
            MENU_ALWAYS_ON_TOP,
            "总是置顶",
            preferences.always_on_top,
        )?;
        let click_through = check_item(
            app,
            MENU_CLICK_THROUGH,
            "鼠标穿透",
            preferences.click_through,
        )?;
        let position_locked = check_item(
            app,
            MENU_POSITION_LOCKED,
            "锁定窗口位置",
            preferences.position_locked,
        )?;
        let position_top_left = command_item(app, MENU_POSITION_TOP_LEFT, "上左")?;
        let position_top_right = command_item(app, MENU_POSITION_TOP_RIGHT, "上右")?;
        let position_center = command_item(app, MENU_POSITION_CENTER, "中心")?;
        let position_bottom_left = command_item(app, MENU_POSITION_BOTTOM_LEFT, "下左")?;
        let position_bottom_right = command_item(app, MENU_POSITION_BOTTOM_RIGHT, "下右")?;
        let position_menu = Submenu::with_id_and_items(
            app,
            MENU_POSITION_SUBMENU,
            "快捷设置位置",
            true,
            &[
                &position_top_left,
                &position_top_right,
                &position_center,
                &position_bottom_left,
                &position_bottom_right,
            ],
        )
        .map_err(|error| error.to_string())?;
        let show_taskbar_window = check_item(
            app,
            MENU_SHOW_TASKBAR_WINDOW,
            "显示任务栏窗口",
            preferences.show_taskbar_window,
        )?;
        let show_main_window = check_item(
            app,
            MENU_SHOW_MAIN_WINDOW,
            "显示主窗口",
            preferences.show_main_window,
        )?;
        let launch_at_login = check_item(
            app,
            MENU_LAUNCH_AT_LOGIN,
            "开机自启",
            preferences.launch_at_login,
        )?;

        let opacity_100 = opacity_item(app, 100, preferences.opacity_percent)?;
        let opacity_90 = opacity_item(app, 90, preferences.opacity_percent)?;
        let opacity_80 = opacity_item(app, 80, preferences.opacity_percent)?;
        let opacity_70 = opacity_item(app, 70, preferences.opacity_percent)?;
        let opacity_60 = opacity_item(app, 60, preferences.opacity_percent)?;
        let opacity_menu = Submenu::with_id_and_items(
            app,
            "desktop.opacity",
            "窗口不透明度",
            true,
            &[
                &opacity_100,
                &opacity_90,
                &opacity_80,
                &opacity_70,
                &opacity_60,
            ],
        )
        .map_err(|error| error.to_string())?;

        let [main_checked, distributed_checked] = radar_source_checks(&preferences.radar_source);
        let radar_source_main = check_item(app, MENU_RADAR_SOURCE_MAIN, "主站", main_checked)?;
        let radar_source_distributed = check_item(
            app,
            MENU_RADAR_SOURCE_DISTRIBUTED,
            "分布式",
            distributed_checked,
        )?;
        let radar_source_menu = Submenu::with_id_and_items(
            app,
            MENU_RADAR_SOURCE_SUBMENU,
            "雷达数据源",
            true,
            &[&radar_source_main, &radar_source_distributed],
        )
        .map_err(|error| error.to_string())?;

        let refresh = MenuItem::with_id(app, MENU_REFRESH, "立即刷新", true, None::<&str>)
            .map_err(|error| error.to_string())?;
        let quit = MenuItem::with_id(app, MENU_QUIT, "退出", true, None::<&str>)
            .map_err(|error| error.to_string())?;
        let first_separator =
            PredefinedMenuItem::separator(app).map_err(|error| error.to_string())?;
        let second_separator =
            PredefinedMenuItem::separator(app).map_err(|error| error.to_string())?;
        let menu = Menu::with_items(
            app,
            &[
                &always_on_top,
                &click_through,
                &position_locked,
                &position_menu,
                &first_separator,
                &show_taskbar_window,
                &show_main_window,
                &launch_at_login,
                &second_separator,
                &opacity_menu,
                &radar_source_menu,
                &refresh,
                &quit,
            ],
        )
        .map_err(|error| error.to_string())?;

        Ok(Self {
            menu,
            always_on_top,
            click_through,
            position_locked,
            show_taskbar_window,
            show_main_window,
            launch_at_login,
            radar_source_main,
            radar_source_distributed,
            opacity: vec![
                (100, opacity_100),
                (90, opacity_90),
                (80, opacity_80),
                (70, opacity_70),
                (60, opacity_60),
            ],
        })
    }

    fn sync(&self, preferences: &DesktopPreferences) -> Result<(), String> {
        self.always_on_top
            .set_checked(preferences.always_on_top)
            .map_err(|error| error.to_string())?;
        self.click_through
            .set_checked(preferences.click_through)
            .map_err(|error| error.to_string())?;
        self.position_locked
            .set_checked(preferences.position_locked)
            .map_err(|error| error.to_string())?;
        self.show_taskbar_window
            .set_checked(preferences.show_taskbar_window)
            .map_err(|error| error.to_string())?;
        self.show_main_window
            .set_checked(preferences.show_main_window)
            .map_err(|error| error.to_string())?;
        self.launch_at_login
            .set_checked(preferences.launch_at_login)
            .map_err(|error| error.to_string())?;
        let [main_checked, distributed_checked] = radar_source_checks(&preferences.radar_source);
        self.radar_source_main
            .set_checked(main_checked)
            .map_err(|error| error.to_string())?;
        self.radar_source_distributed
            .set_checked(distributed_checked)
            .map_err(|error| error.to_string())?;
        for (opacity, item) in &self.opacity {
            item.set_checked(*opacity == preferences.opacity_percent)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

pub struct DesktopController {
    preferences: Mutex<DesktopPreferences>,
    projection: Mutex<CompanionProjection>,
    main_expanded: Mutex<bool>,
    main_position_state: Mutex<MainPositionSaveState>,
    main_position_gate: Mutex<()>,
    radar_source_transition: tokio::sync::Mutex<()>,
    menu: DesktopMenu,
    preferences_path: PathBuf,
    main_position_path: PathBuf,
    #[cfg(windows)]
    taskbar_monitor_started: AtomicBool,
}

impl DesktopController {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        ensure_supported_platform()?;
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|error| error.to_string())?;
        let preferences_path = config_dir.join(PREFERENCES_FILE);
        let main_position_path = config_dir.join(MAIN_POSITION_FILE);
        let mut preferences = load_preferences(&preferences_path);
        match app.autolaunch().is_enabled() {
            Ok(enabled) => preferences.launch_at_login = enabled,
            Err(error) => eprintln!(
                "[model-radar] native start-at-login state could not be read; retaining persisted preference: {error}"
            ),
        }
        let main_position = load_json(&main_position_path);
        let menu = DesktopMenu::new(app, &preferences)?;

        Ok(Self {
            preferences: Mutex::new(preferences),
            projection: Mutex::new(CompanionProjection::default()),
            main_expanded: Mutex::new(false),
            main_position_state: Mutex::new(MainPositionSaveState::new(main_position)),
            main_position_gate: Mutex::new(()),
            radar_source_transition: tokio::sync::Mutex::new(()),
            menu,
            preferences_path,
            main_position_path,
            #[cfg(windows)]
            taskbar_monitor_started: AtomicBool::new(false),
        })
    }

    pub fn initialize(&self, app: &AppHandle) -> Result<(), String> {
        if let Err(error) = self.restore_main_window_position(app) {
            eprintln!("[model-radar] main window position restore failed: {error}");
        }
        self.set_main_position_ready()?;
        let mut preferences = self.lock_preferences()?;
        self.apply_always_on_top(app, preferences.always_on_top)?;
        self.apply_click_through(app, preferences.click_through)?;

        if let Err(error) = self.apply_visibility(app, &preferences) {
            eprintln!("[model-radar] taskbar companion unavailable: {error}");
            preferences.show_taskbar_window = false;
            preferences.show_main_window = true;
            self.apply_visibility(app, &preferences)?;
        }

        self.menu.sync(&preferences)?;
        self.sync_companion_tray(app, &preferences)?;
        persist_preferences(&self.preferences_path, &preferences)?;
        let snapshot = preferences.clone();
        drop(preferences);
        let _ = app.emit(PREFERENCES_UPDATED_EVENT, &snapshot);
        #[cfg(windows)]
        self.start_taskbar_monitor(app.clone());
        Ok(())
    }

    pub fn preferences(&self) -> Result<DesktopPreferences, String> {
        Ok(self.lock_preferences()?.clone())
    }

    pub fn main_expanded(&self) -> Result<bool, String> {
        self.main_expanded
            .lock()
            .map(|expanded| *expanded)
            .map_err(|_| "desktop expanded-state lock is poisoned".to_owned())
    }

    pub fn set_option(
        &self,
        app: &AppHandle,
        option: DesktopOption,
        enabled: bool,
    ) -> Result<DesktopPreferences, String> {
        let current = self.lock_preferences()?;
        let next = current.clone().with_option(option, enabled);
        self.commit_option(app, current, option, next)
    }

    fn toggle_option(
        &self,
        app: &AppHandle,
        option: DesktopOption,
    ) -> Result<DesktopPreferences, String> {
        let current = self.lock_preferences()?;
        let enabled = !option.enabled(&current);
        let next = current.clone().with_option(option, enabled);
        self.commit_option(app, current, option, next)
    }

    fn commit_option(
        &self,
        app: &AppHandle,
        mut current: MutexGuard<'_, DesktopPreferences>,
        option: DesktopOption,
        next: DesktopPreferences,
    ) -> Result<DesktopPreferences, String> {
        let previous = current.clone();
        if next == previous {
            self.menu.sync(&previous)?;
            return Ok(previous);
        }

        if let Err(error) = self.apply_option(app, option, &next) {
            let _ = self.apply_option(app, option, &previous);
            let _ = self.menu.sync(&previous);
            return Err(error);
        }
        if let Err(error) = persist_preferences(&self.preferences_path, &next) {
            let _ = self.apply_option(app, option, &previous);
            let _ = persist_preferences(&self.preferences_path, &previous);
            let _ = self.menu.sync(&previous);
            return Err(error);
        }
        if let Err(error) = self.menu.sync(&next) {
            let _ = self.apply_option(app, option, &previous);
            let _ = persist_preferences(&self.preferences_path, &previous);
            let _ = self.menu.sync(&previous);
            return Err(error);
        }

        *current = next.clone();
        drop(current);
        let _ = app.emit(PREFERENCES_UPDATED_EVENT, &next);
        Ok(next)
    }

    pub fn set_opacity(
        &self,
        app: &AppHandle,
        opacity_percent: u8,
    ) -> Result<DesktopPreferences, String> {
        if !VALID_OPACITY.contains(&opacity_percent) {
            return Err(format!(
                "unsupported opacity {opacity_percent}; expected one of 100, 90, 80, 70, 60"
            ));
        }

        let mut current = self.lock_preferences()?;
        let previous = current.clone();
        if previous.opacity_percent == opacity_percent {
            self.menu.sync(&previous)?;
            return Ok(previous);
        }
        let mut next = previous.clone();
        next.opacity_percent = opacity_percent;

        persist_preferences(&self.preferences_path, &next)?;
        if let Err(error) = self.menu.sync(&next) {
            let _ = persist_preferences(&self.preferences_path, &previous);
            let _ = self.menu.sync(&previous);
            return Err(error);
        }
        *current = next.clone();
        drop(current);
        let _ = app.emit(PREFERENCES_UPDATED_EVENT, &next);
        Ok(next)
    }

    fn commit_radar_source(&self, source: RadarSource) -> Result<DesktopPreferences, String> {
        let mut current = self.lock_preferences()?;
        let previous = current.clone();
        let next = previous.clone().with_radar_source(source);
        if next == previous {
            self.menu.sync(&previous)?;
            return Ok(previous);
        }

        if let Err(error) = persist_preferences(&self.preferences_path, &next) {
            let _ = self.menu.sync(&previous);
            return Err(error);
        }
        if let Err(error) = self.menu.sync(&next) {
            let _ = persist_preferences(&self.preferences_path, &previous);
            let _ = self.menu.sync(&previous);
            return Err(error);
        }
        *current = next.clone();
        Ok(next)
    }

    async fn switch_radar_source(
        &self,
        app: &AppHandle,
        source: RadarSource,
    ) -> Result<DesktopPreferences, String> {
        let transition = self.radar_source_transition.lock().await;
        let service = app.state::<RadarService>();
        let (preferences, _) = service
            .transition_source(source, || self.commit_radar_source(source))
            .await?;
        let _ = app.emit(PREFERENCES_UPDATED_EVENT, &preferences);

        service.wake_polling();
        drop(transition);
        let _ = service.refresh_and_publish(app).await;
        Ok(preferences)
    }

    pub fn update_projection(
        &self,
        app: &AppHandle,
        projection: CompanionProjection,
    ) -> Result<(), String> {
        *self.lock_projection()? = projection.normalized();
        let preferences = self.preferences()?;
        self.sync_companion_tray(app, &preferences)
    }

    pub fn hide_window(&self, app: &AppHandle, label: &str) -> Result<DesktopPreferences, String> {
        match label {
            MAIN_WINDOW_LABEL => self.set_option(app, DesktopOption::ShowMainWindow, false),
            TASKBAR_WINDOW_LABEL => self.set_option(app, DesktopOption::ShowTaskbarWindow, false),
            _ => Err(format!("unknown desktop window '{label}'")),
        }
    }

    pub fn toggle_main_window(&self, app: &AppHandle) -> Result<DesktopPreferences, String> {
        let current = self.preferences()?;
        if current.show_main_window {
            self.set_option(app, DesktopOption::ShowMainWindow, false)
        } else {
            // Showing must never depend on a healthy taskbar companion; tray left-click
            // is the last recovery surface when the companion is already broken.
            self.force_show_main_window(app)
        }
    }

    /// Force the main window on-screen and persist `showMainWindow: true`.
    /// Taskbar health is best-effort and must not block tray recovery.
    pub fn force_show_main_window(&self, app: &AppHandle) -> Result<DesktopPreferences, String> {
        match self.set_option(app, DesktopOption::ShowMainWindow, true) {
            Ok(preferences) => {
                if let Err(error) = self.recover_main_window_for_safety(app) {
                    eprintln!(
                        "[model-radar] main recovery after force-show had warnings: {error}"
                    );
                }
                Ok(preferences)
            }
            Err(error) => {
                // Preference transaction failed (often a dead taskbar path). Still try to
                // surface the native window so the user is not stuck with only a tray icon.
                if let Err(recovery_error) = self.emergency_show_main_window(app) {
                    return Err(format!(
                        "{error}; emergency main show failed: {recovery_error}"
                    ));
                }
                match self.preferences() {
                    Ok(preferences) if preferences.show_main_window => Ok(preferences),
                    Ok(_) | Err(_) => Err(error),
                }
            }
        }
    }

    /// Last-resort native show without requiring a successful preference apply.
    fn emergency_show_main_window(&self, app: &AppHandle) -> Result<(), String> {
        self.recover_main_window_for_safety(app)?;

        // Best-effort: persist showMainWindow=true so the next tray click / menu
        // agrees with the visible main window. Leave taskbar preference alone;
        // the monitor demotes a broken companion independently.
        let mut current = self.lock_preferences()?;
        if current.show_main_window {
            return Ok(());
        }
        let mut next = current.clone();
        next.show_main_window = true;
        persist_preferences(&self.preferences_path, &next)?;
        if let Err(error) = self.menu.sync(&next) {
            let _ = persist_preferences(&self.preferences_path, &current);
            let _ = self.menu.sync(&current);
            return Err(error);
        }
        *current = next.clone();
        drop(current);
        let _ = app.emit(PREFERENCES_UPDATED_EVENT, &next);
        Ok(())
    }

    pub fn set_main_expanded(&self, app: &AppHandle, expanded: bool) -> Result<(), String> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let app_for_main = app.clone();
        app.run_on_main_thread(move || {
            let controller = app_for_main.state::<DesktopController>();
            let result = controller.set_main_expanded_on_main_thread(&app_for_main, expanded);
            let _ = sender.send(result);
        })
        .map_err(|error| error.to_string())?;
        receiver
            .recv()
            .map_err(|_| "main-thread window resize result was dropped".to_owned())?
    }

    fn set_main_expanded_on_main_thread(
        &self,
        app: &AppHandle,
        expanded: bool,
    ) -> Result<(), String> {
        let window = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        {
            let _gate = self.lock_main_position_gate()?;
            self.invalidate_main_position_capture()?;
            resize_main_window(&window, expanded)?;
        }
        *self
            .main_expanded
            .lock()
            .map_err(|_| "desktop expanded-state lock is poisoned".to_owned())? = expanded;
        let _ = app.emit(MAIN_EXPANDED_EVENT, expanded);
        self.mark_main_window_geometry_dirty(app)?;
        Ok(())
    }

    pub fn show_main_details(&self, app: &AppHandle) -> Result<DesktopPreferences, String> {
        // Visibility first: expand/resize must not block recovery when the main
        // window is hidden or geometry locks are contended (taskbar/tray click).
        let preferences = self.force_show_main_window(app)?;
        if let Err(error) = self.set_main_expanded(app, true) {
            eprintln!("[model-radar] expand after show-main-details failed: {error}");
        }
        let _ = app.emit(SHOW_MAIN_DETAILS_EVENT, ());
        Ok(preferences)
    }

    pub fn show_context_menu(&self, window: &WebviewWindow) -> Result<(), String> {
        self.menu
            .menu
            .popup(window.as_ref().window())
            .map_err(|error| error.to_string())
    }

    pub fn mark_main_window_geometry_dirty(&self, app: &AppHandle) -> Result<(), String> {
        let should_start = self.lock_main_position_state()?.mark_dirty();
        if should_start {
            Self::start_main_position_writer(app.clone());
        }
        Ok(())
    }

    pub fn flush_main_window_position(&self, app: &AppHandle) -> Result<(), String> {
        let _gate = self.lock_main_position_gate()?;
        let position = self.capture_main_window_position(app)?;
        self.persist_main_window_position(position)
    }

    fn set_main_window_position_preset(
        &self,
        app: &AppHandle,
        preset: MainWindowPositionPreset,
    ) -> Result<(), String> {
        let _gate = self.lock_main_position_gate()?;
        let window = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        let original_position = window.outer_position().map_err(|error| error.to_string())?;
        let window_size = window.outer_size().map_err(|error| error.to_string())?;
        let monitor = window
            .current_monitor()
            .map_err(|error| error.to_string())?
            .or(window
                .primary_monitor()
                .map_err(|error| error.to_string())?)
            .ok_or_else(|| "no monitor is available for main window positioning".to_owned())?;
        let work_area = monitor.work_area();
        let target = preset_position(preset, work_area.position, work_area.size, window_size)
            .ok_or_else(|| {
                "main window does not fit in the current monitor work area".to_owned()
            })?;

        self.invalidate_main_position_capture()?;
        window
            .set_position(target)
            .map_err(|error| error.to_string())?;
        let canonical = canonical_compact_position(
            target,
            window_size,
            work_area.position,
            work_area.size,
            monitor.scale_factor(),
        )
        .ok_or_else(|| "main window compact position could not be calculated".to_owned())?;

        if let Err(error) = self.persist_main_window_position(canonical) {
            let rollback = window.set_position(original_position);
            self.invalidate_main_position_capture()?;
            return match rollback {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(format!(
                    "{error}; restoring the previous main window position failed: {rollback_error}"
                )),
            };
        }
        Ok(())
    }

    fn restore_main_window_position(&self, app: &AppHandle) -> Result<(), String> {
        let saved = self.lock_main_position_state()?.last_saved;
        let Some(saved) = saved else {
            return Ok(());
        };

        let _gate = self.lock_main_position_gate()?;
        let window = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        let window_size = window.outer_size().map_err(|error| error.to_string())?;
        let monitors = window
            .available_monitors()
            .map_err(|error| error.to_string())?;
        let fallback = window
            .current_monitor()
            .map_err(|error| error.to_string())?
            .or(window
                .primary_monitor()
                .map_err(|error| error.to_string())?)
            .or_else(|| monitors.first().cloned());
        let Some(fallback) = fallback else {
            return Ok(());
        };
        let work_areas = monitors
            .iter()
            .map(|monitor| {
                let work_area = monitor.work_area();
                (
                    work_area.position,
                    work_area.size,
                    compact_physical_size(work_area.size, monitor.scale_factor())
                        .unwrap_or(window_size),
                )
            })
            .collect::<Vec<_>>();
        let fallback_work_area = fallback.work_area();
        let fallback_window_size =
            compact_physical_size(fallback_work_area.size, fallback.scale_factor())
                .unwrap_or(window_size);
        let target = restored_main_window_position(
            saved.into(),
            &work_areas,
            (
                fallback_work_area.position,
                fallback_work_area.size,
                fallback_window_size,
            ),
        );

        self.invalidate_main_position_capture()?;
        window
            .set_position(target)
            .map_err(|error| error.to_string())?;
        self.persist_main_window_position(target)
    }

    fn set_main_position_ready(&self) -> Result<(), String> {
        self.lock_main_position_state()?.ready = true;
        Ok(())
    }

    fn invalidate_main_position_capture(&self) -> Result<u64, String> {
        let mut state = self.lock_main_position_state()?;
        state.revision = state.revision.wrapping_add(1);
        Ok(state.revision)
    }

    fn persist_main_window_position(&self, position: PhysicalPosition<i32>) -> Result<(), String> {
        let mut state = self.lock_main_position_state()?;
        state.revision = state.revision.wrapping_add(1);
        let revision = state.revision;
        let saved = SavedMainWindowPosition::from(position);
        persist_json(&self.main_position_path, &saved)?;
        state.last_saved = Some(saved);
        state.persisted_revision = revision;
        Ok(())
    }

    fn capture_main_window_position(
        &self,
        app: &AppHandle,
    ) -> Result<PhysicalPosition<i32>, String> {
        let window = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        let position = window.outer_position().map_err(|error| error.to_string())?;
        let size = window.outer_size().map_err(|error| error.to_string())?;
        let monitor = window
            .current_monitor()
            .map_err(|error| error.to_string())?
            .or(window
                .primary_monitor()
                .map_err(|error| error.to_string())?)
            .ok_or_else(|| "no monitor is available for main window capture".to_owned())?;
        let work_area = monitor.work_area();
        canonical_compact_position(
            position,
            size,
            work_area.position,
            work_area.size,
            monitor.scale_factor(),
        )
        .ok_or_else(|| "main window compact position could not be calculated".to_owned())
    }

    fn start_main_position_writer(app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(MAIN_POSITION_SAVE_DELAY).await;
                let controller = app.state::<DesktopController>();
                let attempted_revision = match controller.lock_main_position_state() {
                    Ok(state) => state.revision,
                    Err(error) => {
                        eprintln!("[model-radar] main window position save failed: {error}");
                        break;
                    }
                };
                match controller.persist_debounced_main_position(&app) {
                    Ok(true) => break,
                    Ok(false) => continue,
                    Err(error) => {
                        let should_retry = controller
                            .lock_main_position_state()
                            .map(|mut state| state.finish_writer_after_error(attempted_revision))
                            .unwrap_or(false);
                        if should_retry {
                            continue;
                        }
                        eprintln!("[model-radar] main window position save failed: {error}");
                        break;
                    }
                }
            }
        });
    }

    fn persist_debounced_main_position(&self, app: &AppHandle) -> Result<bool, String> {
        let revision = {
            let mut state = self.lock_main_position_state()?;
            if state.persisted_revision == state.revision {
                state.writer_active = false;
                return Ok(true);
            }
            state.revision
        };

        // Wry getters called off the main thread synchronously wait on the main event loop.
        // Capture before taking the gate so a main-thread preset or exit flush cannot deadlock.
        let position = self.capture_main_window_position(app)?;
        let _gate = self.lock_main_position_gate()?;
        let mut state = self.lock_main_position_state()?;
        if state.revision != revision {
            return Ok(false);
        }
        let saved = SavedMainWindowPosition::from(position);
        persist_json(&self.main_position_path, &saved)?;
        state.last_saved = Some(saved);
        state.persisted_revision = revision;
        state.writer_active = false;
        Ok(true)
    }

    #[cfg(windows)]
    fn start_taskbar_monitor(&self, app: AppHandle) {
        if !claim_taskbar_monitor(&self.taskbar_monitor_started) {
            return;
        }

        tauri::async_runtime::spawn(async move {
            let mut failure_reported = false;
            loop {
                tokio::time::sleep(TASKBAR_MONITOR_INTERVAL).await;
                let controller = app.state::<DesktopController>();
                match controller.monitor_taskbar_once(&app) {
                    Ok(TaskbarMonitorOutcome::Inactive | TaskbarMonitorOutcome::Placed) => {
                        failure_reported = false;
                    }
                    Ok(TaskbarMonitorOutcome::Disabled {
                        placement_error,
                        recovery_errors,
                    }) => {
                        if !failure_reported {
                            if recovery_errors.is_empty() {
                                eprintln!(
                                    "[model-radar] taskbar companion disabled after runtime placement failure: {placement_error}"
                                );
                            } else {
                                eprintln!(
                                    "[model-radar] taskbar companion disabled after runtime placement failure ({placement_error}); recovery warnings: {}",
                                    recovery_errors.join("; ")
                                );
                            }
                            failure_reported = true;
                        }
                    }
                    Err(error) => {
                        if !failure_reported {
                            eprintln!("[model-radar] taskbar runtime monitor failed: {error}");
                            failure_reported = true;
                        }
                    }
                }
            }
        });
    }

    #[cfg(windows)]
    fn monitor_taskbar_once(&self, app: &AppHandle) -> Result<TaskbarMonitorOutcome, String> {
        // Never hold the preference mutex across Wry/Win32 placement work: tray left-click
        // and menu actions need that lock, and scale_factor/show wait on the event loop.
        let snapshot = {
            let current = self.lock_preferences()?;
            if !current.show_taskbar_window {
                return Ok(TaskbarMonitorOutcome::Inactive);
            }
            current.clone()
        };

        match self.ensure_taskbar_projection(app, &snapshot) {
            Ok(()) => Ok(TaskbarMonitorOutcome::Placed),
            Err(placement_error) => {
                let current = self.lock_preferences()?;
                if !current.show_taskbar_window {
                    return Ok(TaskbarMonitorOutcome::Inactive);
                }
                Ok(self.commit_taskbar_monitor_failure(app, current, placement_error))
            }
        }
    }

    #[cfg(windows)]
    fn ensure_taskbar_projection(
        &self,
        app: &AppHandle,
        preferences: &DesktopPreferences,
    ) -> Result<(), String> {
        let taskbar = windows::create_taskbar_window(app)?;
        // Re-install after recreate / attach; cheap no-op when already hooked.
        windows::install_taskbar_input_hooks(app, &taskbar)?;
        taskbar
            .set_ignore_cursor_events(preferences.click_through)
            .map_err(|error| error.to_string())?;
        windows::place_taskbar_window(&taskbar, TASKBAR_SIZE)?;
        taskbar.show().map_err(|error| error.to_string())?;
        if !windows::taskbar_companion_is_healthy(&taskbar) {
            return Err("taskbar companion is not healthy after placement".to_owned());
        }
        Ok(())
    }

    #[cfg(windows)]
    fn commit_taskbar_monitor_failure(
        &self,
        app: &AppHandle,
        mut current: MutexGuard<'_, DesktopPreferences>,
        placement_error: String,
    ) -> TaskbarMonitorOutcome {
        let next = taskbar_failure_preferences(&current)
            .expect("taskbar monitor failure requires an enabled taskbar preference");
        let mut recovery_errors = Vec::new();

        // Commit preference memory before native recovery so tray clicks observe
        // showMainWindow=true even if window show is slow.
        *current = next.clone();
        drop(current);

        if let Err(error) = self.recover_main_window_for_safety(app) {
            recovery_errors.push(format!("recover main window: {error}"));
        }
        if let Some(taskbar) = app.get_webview_window(TASKBAR_WINDOW_LABEL) {
            if let Err(error) = windows::hide_taskbar_window(&taskbar) {
                recovery_errors.push(format!("hide taskbar companion: {error}"));
            }
        }
        if let Err(error) = persist_preferences(&self.preferences_path, &next) {
            recovery_errors.push(format!("persist preferences: {error}"));
        }
        if let Err(error) = self.menu.sync(&next) {
            recovery_errors.push(format!("sync desktop menu: {error}"));
        }
        if let Err(error) = app.emit(PREFERENCES_UPDATED_EVENT, &next) {
            recovery_errors.push(format!("emit desktop preferences: {error}"));
        }

        TaskbarMonitorOutcome::Disabled {
            placement_error,
            recovery_errors,
        }
    }

    fn apply_option(
        &self,
        app: &AppHandle,
        option: DesktopOption,
        preferences: &DesktopPreferences,
    ) -> Result<(), String> {
        match option {
            DesktopOption::AlwaysOnTop => self.apply_always_on_top(app, preferences.always_on_top),
            DesktopOption::ClickThrough => self.apply_click_through(app, preferences.click_through),
            DesktopOption::PositionLocked => Ok(()),
            DesktopOption::ShowMainWindow => self.apply_main_window_visibility(app, preferences),
            DesktopOption::ShowTaskbarWindow => self.apply_visibility(app, preferences),
            DesktopOption::LaunchAtLogin => {
                self.apply_launch_at_login(app, preferences.launch_at_login)
            }
        }
    }

    fn apply_launch_at_login(&self, app: &AppHandle, enabled: bool) -> Result<(), String> {
        let autolaunch = app.autolaunch();
        if enabled {
            autolaunch.enable().map_err(|error| error.to_string())?;
        } else {
            autolaunch.disable().map_err(|error| error.to_string())?;
        }

        let actual = autolaunch.is_enabled().map_err(|error| error.to_string())?;
        if actual == enabled {
            Ok(())
        } else {
            Err(format!(
                "native start-at-login verification failed: requested {enabled}, found {actual}"
            ))
        }
    }

    fn apply_always_on_top(&self, app: &AppHandle, enabled: bool) -> Result<(), String> {
        let main = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        main.set_always_on_top(enabled)
            .map_err(|error| error.to_string())
    }

    fn apply_click_through(&self, app: &AppHandle, enabled: bool) -> Result<(), String> {
        let main = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        main.set_ignore_cursor_events(enabled)
            .map_err(|error| error.to_string())?;

        #[cfg(windows)]
        if let Some(taskbar) = app.get_webview_window(TASKBAR_WINDOW_LABEL) {
            if let Err(error) = taskbar.set_ignore_cursor_events(enabled) {
                let _ = main.set_ignore_cursor_events(!enabled);
                return Err(error.to_string());
            }
        }

        Ok(())
    }

    /// Main-window-only visibility path used by tray toggle / show-details.
    /// Must not call taskbar placement when showing: a broken companion previously
    /// made left-click appear dead (apply failed → transaction rolled back).
    fn apply_main_window_visibility(
        &self,
        app: &AppHandle,
        preferences: &DesktopPreferences,
    ) -> Result<(), String> {
        let main = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;

        if preferences.show_main_window {
            main.show().map_err(|error| error.to_string())?;
            if let Err(error) = self.ensure_main_window_intersects_work_area(app) {
                eprintln!(
                    "[model-radar] main window on-screen clamp after show failed: {error}"
                );
            }
            return Ok(());
        }

        // Hiding main is only safe when the taskbar companion is healthy (or disabled).
        #[cfg(windows)]
        if preferences.show_taskbar_window {
            if let Err(error) = self.ensure_taskbar_projection(app, preferences) {
                let _ = windows::show_recovery_window(&main);
                let _ = self.ensure_main_window_intersects_work_area(app);
                return Err(error);
            }
        }

        main.hide().map_err(|error| error.to_string())?;
        Ok(())
    }

    fn apply_visibility(
        &self,
        app: &AppHandle,
        preferences: &DesktopPreferences,
    ) -> Result<(), String> {
        let main = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;

        // Establish the taskbar companion before hiding the main window so a
        // failed companion setup never leaves the user with zero surfaces.
        // When the main window is also being shown, taskbar failure is soft:
        // menu "show taskbar" while main is visible must still succeed.
        #[cfg(windows)]
        {
            if preferences.show_taskbar_window {
                if let Err(error) = self.ensure_taskbar_projection(app, preferences) {
                    if preferences.show_main_window {
                        eprintln!(
                            "[model-radar] taskbar companion ensure failed while main stays visible: {error}"
                        );
                    } else {
                        // Taskbar-only request cannot proceed; keep main up.
                        let _ = windows::show_recovery_window(&main);
                        let _ = self.ensure_main_window_intersects_work_area(app);
                        return Err(error);
                    }
                }
            } else if let Some(taskbar) = app.get_webview_window(TASKBAR_WINDOW_LABEL) {
                windows::hide_taskbar_window(&taskbar)?;
            }
        }

        if preferences.show_main_window {
            main.show().map_err(|error| error.to_string())?;
            if let Err(error) = self.ensure_main_window_intersects_work_area(app) {
                eprintln!(
                    "[model-radar] main window on-screen clamp after show failed: {error}"
                );
            }
        } else {
            main.hide().map_err(|error| error.to_string())?;
        }

        #[cfg(target_os = "macos")]
        self.sync_companion_tray(app, preferences)?;

        Ok(())
    }

    /// Show the main window and clamp it into an available work area when fully off-screen.
    fn recover_main_window_for_safety(&self, app: &AppHandle) -> Result<(), String> {
        let main = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        if let Err(error) = self.ensure_main_window_intersects_work_area(app) {
            eprintln!(
                "[model-radar] main window on-screen clamp during recovery failed: {error}"
            );
        }
        #[cfg(windows)]
        {
            windows::show_recovery_window(&main)?;
        }
        #[cfg(not(windows))]
        {
            main.show().map_err(|error| error.to_string())?;
        }
        let _ = main.set_focus();
        Ok(())
    }

    /// When the outer rect has zero intersection with every work area, reclamp
    /// using the same multi-monitor restore rules as startup.
    fn ensure_main_window_intersects_work_area(&self, app: &AppHandle) -> Result<(), String> {
        let window = app
            .get_webview_window(MAIN_WINDOW_LABEL)
            .ok_or_else(|| "main window is unavailable".to_owned())?;
        let position = window.outer_position().map_err(|error| error.to_string())?;
        let window_size = window.outer_size().map_err(|error| error.to_string())?;
        let monitors = window
            .available_monitors()
            .map_err(|error| error.to_string())?;
        if monitors.is_empty() {
            return Ok(());
        }

        let any_intersection = monitors.iter().any(|monitor| {
            let work_area = monitor.work_area();
            window_work_area_intersection(
                position,
                window_size,
                (work_area.position, work_area.size),
            ) > 0
        });
        if any_intersection {
            return Ok(());
        }

        let _gate = self.lock_main_position_gate()?;
        let fallback = window
            .current_monitor()
            .map_err(|error| error.to_string())?
            .or(window
                .primary_monitor()
                .map_err(|error| error.to_string())?)
            .or_else(|| monitors.first().cloned())
            .ok_or_else(|| "no monitor is available for main window recovery".to_owned())?;
        let work_areas = monitors
            .iter()
            .map(|monitor| {
                let work_area = monitor.work_area();
                (
                    work_area.position,
                    work_area.size,
                    compact_physical_size(work_area.size, monitor.scale_factor())
                        .unwrap_or(window_size),
                )
            })
            .collect::<Vec<_>>();
        let fallback_work_area = fallback.work_area();
        let fallback_window_size =
            compact_physical_size(fallback_work_area.size, fallback.scale_factor())
                .unwrap_or(window_size);
        let seed = self
            .lock_main_position_state()?
            .last_saved
            .map(PhysicalPosition::from)
            .unwrap_or(position);
        let target = restored_main_window_position(
            seed,
            &work_areas,
            (
                fallback_work_area.position,
                fallback_work_area.size,
                fallback_window_size,
            ),
        );

        self.invalidate_main_position_capture()?;
        window
            .set_position(target)
            .map_err(|error| error.to_string())?;
        // Persist the recovered compact-equivalent corner so the next launch
        // does not re-apply a fully off-screen seed.
        self.persist_main_window_position(target)
    }

    fn sync_companion_tray(
        &self,
        app: &AppHandle,
        _preferences: &DesktopPreferences,
    ) -> Result<(), String> {
        let Some(tray) = app.tray_by_id(TRAY_ID) else {
            return Ok(());
        };
        let projection = self.lock_projection()?.clone();
        let title = projection.title();
        tray.set_tooltip(Some(&title))
            .map_err(|error| error.to_string())?;

        #[cfg(target_os = "macos")]
        // tray-icon 0.24 does not update NSButton.title for None, so an empty
        // string is required to clear a title that was previously visible.
        tray.set_title(Some(if _preferences.show_taskbar_window {
            title.as_str()
        } else {
            ""
        }))
        .map_err(|error| error.to_string())?;

        Ok(())
    }

    fn lock_preferences(&self) -> Result<MutexGuard<'_, DesktopPreferences>, String> {
        self.preferences
            .lock()
            .map_err(|_| "desktop preferences lock is poisoned".to_owned())
    }

    fn lock_projection(&self) -> Result<MutexGuard<'_, CompanionProjection>, String> {
        self.projection
            .lock()
            .map_err(|_| "desktop projection lock is poisoned".to_owned())
    }

    fn lock_main_position_state(&self) -> Result<MutexGuard<'_, MainPositionSaveState>, String> {
        self.main_position_state
            .lock()
            .map_err(|_| "main window position state lock is poisoned".to_owned())
    }

    fn lock_main_position_gate(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.main_position_gate
            .lock()
            .map_err(|_| "main window geometry lock is poisoned".to_owned())
    }
}

pub fn build_tray(app: &AppHandle) -> Result<(), String> {
    let controller = app.state::<DesktopController>();
    #[cfg(target_os = "macos")]
    let preferences = controller.preferences()?;
    let projection = controller.lock_projection()?.clone();
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(projection.title())
        .menu(&controller.menu.menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| dispatch_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let app = tray.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let controller = app.state::<DesktopController>();
                    let result = match tray_left_click_behavior(cfg!(target_os = "macos")) {
                        TrayLeftClickBehavior::ToggleMain => {
                            controller.toggle_main_window(&app).map(|_| ())
                        }
                        TrayLeftClickBehavior::ShowDetails => {
                            controller.show_main_details(&app).map(|_| ())
                        }
                    };
                    if let Err(error) = result {
                        eprintln!("[model-radar] tray action failed: {error}");
                        // Absolute last resort: ignore preference state and surface main.
                        if let Err(recovery_error) = controller.recover_main_window_for_safety(&app)
                        {
                            eprintln!(
                                "[model-radar] tray emergency main recovery failed: {recovery_error}"
                            );
                        }
                    }
                });
            }
        });

    #[cfg(target_os = "macos")]
    if preferences.show_taskbar_window {
        builder = builder.title(projection.title());
    }

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn handle_close_requested(window: &Window) {
    let app = window.app_handle().clone();
    let label = window.label().to_owned();
    if label == MAIN_WINDOW_LABEL {
        let controller = app.state::<DesktopController>();
        if let Err(error) = controller.flush_main_window_position(&app) {
            eprintln!("[model-radar] main window position flush failed before hide: {error}");
        }
    }
    tauri::async_runtime::spawn(async move {
        let controller = app.state::<DesktopController>();
        if let Err(error) = controller.hide_window(&app, &label) {
            eprintln!("[model-radar] close-to-hide failed: {error}");
        }
    });
}

pub fn handle_main_window_geometry_changed(window: &Window) {
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    let app = window.app_handle();
    let controller = app.state::<DesktopController>();
    if let Err(error) = controller.mark_main_window_geometry_dirty(app) {
        eprintln!("[model-radar] main window position tracking failed: {error}");
    }
}

pub fn handle_app_exit(app: &AppHandle) {
    let controller = app.state::<DesktopController>();
    if let Err(error) = controller.flush_main_window_position(app) {
        eprintln!("[model-radar] final main window position flush failed: {error}");
    }
}

fn dispatch_menu_event(app: &AppHandle, id: &str) {
    if id == MENU_QUIT {
        let controller = app.state::<DesktopController>();
        if let Err(error) = controller.flush_main_window_position(app) {
            eprintln!("[model-radar] main window position flush failed before quit: {error}");
        }
        app.exit(0);
        return;
    }
    if id == MENU_REFRESH {
        let _ = app.emit(REFRESH_REQUESTED_EVENT, ());
        return;
    }
    if let Some(preset) = MainWindowPositionPreset::from_menu_id(id) {
        let controller = app.state::<DesktopController>();
        if let Err(error) = controller.set_main_window_position_preset(app, preset) {
            eprintln!("[model-radar] quick window position failed: {error}");
        }
        return;
    }
    if let Some(source) = radar_source_from_menu_id(id) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let controller = app.state::<DesktopController>();
            if let Err(error) = controller.switch_radar_source(&app, source).await {
                eprintln!("[model-radar] radar source switch failed: {error}");
            }
        });
        return;
    }

    let app = app.clone();
    let id = id.to_owned();
    tauri::async_runtime::spawn(async move {
        let controller = app.state::<DesktopController>();
        let result = if let Some(option) = DesktopOption::from_menu_id(&id) {
            controller.toggle_option(&app, option).map(|_| ())
        } else if let Some(value) = id.strip_prefix(MENU_OPACITY_PREFIX) {
            value
                .parse::<u8>()
                .map_err(|error| error.to_string())
                .and_then(|opacity| controller.set_opacity(&app, opacity).map(|_| ()))
        } else {
            Ok(())
        };

        if let Err(error) = result {
            eprintln!("[model-radar] desktop menu action failed: {error}");
        }
    });
}

#[tauri::command]
pub fn get_desktop_preferences(
    state: State<'_, DesktopController>,
) -> Result<DesktopPreferences, String> {
    state.preferences()
}

#[tauri::command]
pub fn get_main_expanded(state: State<'_, DesktopController>) -> Result<bool, String> {
    state.main_expanded()
}

// These transactions can wait on native window work while the taskbar monitor
// is holding the preference guard. Keep the IPC event loop free to service it.
#[tauri::command(async)]
pub fn set_desktop_option(
    app: AppHandle,
    state: State<'_, DesktopController>,
    option: DesktopOption,
    enabled: bool,
) -> Result<DesktopPreferences, String> {
    ensure_supported_platform()?;
    state.set_option(&app, option, enabled)
}

#[tauri::command(async)]
pub fn set_desktop_opacity(
    app: AppHandle,
    state: State<'_, DesktopController>,
    opacity_percent: u8,
) -> Result<DesktopPreferences, String> {
    ensure_supported_platform()?;
    state.set_opacity(&app, opacity_percent)
}

#[tauri::command]
pub async fn set_desktop_radar_source(
    app: AppHandle,
    state: State<'_, DesktopController>,
    source: RadarSource,
) -> Result<DesktopPreferences, String> {
    ensure_supported_platform()?;
    state.switch_radar_source(&app, source).await
}

#[tauri::command]
pub fn set_main_window_position_preset(
    app: AppHandle,
    state: State<'_, DesktopController>,
    preset: MainWindowPositionPreset,
) -> Result<(), String> {
    ensure_supported_platform()?;
    state.set_main_window_position_preset(&app, preset)
}

#[tauri::command]
pub fn update_companion_projection(
    app: AppHandle,
    state: State<'_, DesktopController>,
    projection: CompanionProjection,
) -> Result<(), String> {
    ensure_supported_platform()?;
    state.update_projection(&app, projection)
}

#[tauri::command]
pub fn set_window_expanded(
    app: AppHandle,
    state: State<'_, DesktopController>,
    expanded: bool,
) -> Result<(), String> {
    state.set_main_expanded(&app, expanded)
}

#[tauri::command]
pub fn hide_window(
    window: WebviewWindow,
    state: State<'_, DesktopController>,
) -> Result<DesktopPreferences, String> {
    state.hide_window(window.app_handle(), window.label())
}

#[tauri::command]
pub fn show_main_details(
    app: AppHandle,
    state: State<'_, DesktopController>,
) -> Result<DesktopPreferences, String> {
    state.show_main_details(&app)
}

#[tauri::command]
pub fn show_desktop_context_menu(
    window: WebviewWindow,
    state: State<'_, DesktopController>,
) -> Result<(), String> {
    state.show_context_menu(&window)
}

fn check_item(
    app: &AppHandle,
    id: &str,
    label: &str,
    checked: bool,
) -> Result<CheckMenuItem<Wry>, String> {
    CheckMenuItem::with_id(app, id, label, true, checked, None::<&str>)
        .map_err(|error| error.to_string())
}

fn command_item(app: &AppHandle, id: &str, label: &str) -> Result<MenuItem<Wry>, String> {
    MenuItem::with_id(app, id, label, true, None::<&str>).map_err(|error| error.to_string())
}

fn opacity_item(app: &AppHandle, opacity: u8, selected: u8) -> Result<CheckMenuItem<Wry>, String> {
    check_item(
        app,
        &format!("{MENU_OPACITY_PREFIX}{opacity}"),
        &format!("{opacity}%"),
        opacity == selected,
    )
}

#[cfg(windows)]
fn claim_taskbar_monitor(started: &AtomicBool) -> bool {
    started
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

#[cfg(windows)]
fn taskbar_failure_preferences(preferences: &DesktopPreferences) -> Option<DesktopPreferences> {
    preferences.show_taskbar_window.then(|| {
        preferences
            .clone()
            .with_option(DesktopOption::ShowTaskbarWindow, false)
    })
}

fn preset_position(
    preset: MainWindowPositionPreset,
    work_position: PhysicalPosition<i32>,
    work_size: PhysicalSize<u32>,
    window_size: PhysicalSize<u32>,
) -> Option<PhysicalPosition<i32>> {
    let [left, center_x, right] =
        aligned_axis_positions(work_position.x, work_size.width, window_size.width)?;
    let [top, center_y, bottom] =
        aligned_axis_positions(work_position.y, work_size.height, window_size.height)?;
    Some(match preset {
        MainWindowPositionPreset::TopLeft => PhysicalPosition::new(left, top),
        MainWindowPositionPreset::TopRight => PhysicalPosition::new(right, top),
        MainWindowPositionPreset::Center => PhysicalPosition::new(center_x, center_y),
        MainWindowPositionPreset::BottomLeft => PhysicalPosition::new(left, bottom),
        MainWindowPositionPreset::BottomRight => PhysicalPosition::new(right, bottom),
    })
}

fn aligned_axis_positions(
    work_start: i32,
    work_length: u32,
    window_length: u32,
) -> Option<[i32; 3]> {
    if work_length == 0 || window_length == 0 || window_length > work_length {
        return None;
    }
    let start = i64::from(work_start);
    let travel = i64::from(work_length - window_length);
    Some([
        i32::try_from(start).ok()?,
        i32::try_from(start.checked_add(travel / 2)?).ok()?,
        i32::try_from(start.checked_add(travel)?).ok()?,
    ])
}

fn canonical_compact_position(
    current_position: PhysicalPosition<i32>,
    current_size: PhysicalSize<u32>,
    work_position: PhysicalPosition<i32>,
    work_size: PhysicalSize<u32>,
    scale: f64,
) -> Option<PhysicalPosition<i32>> {
    if !scale.is_finite() || scale <= 0.0 || current_size.width == 0 || current_size.height == 0 {
        return None;
    }
    let compact_size = compact_physical_size(work_size, scale)?;
    Some(anchored_resize_position(
        current_position,
        current_size,
        compact_size,
        work_position,
        work_size,
    ))
}

fn compact_physical_size(work_size: PhysicalSize<u32>, scale: f64) -> Option<PhysicalSize<u32>> {
    if !scale.is_finite() || scale <= 0.0 || work_size.width == 0 || work_size.height == 0 {
        return None;
    }
    let (width, height) = fit_logical_size(COMPACT_SIZE, work_size, scale);
    Some(PhysicalSize::new(
        (width * scale)
            .round()
            .clamp(1.0, f64::from(work_size.width)) as u32,
        (height * scale)
            .round()
            .clamp(1.0, f64::from(work_size.height)) as u32,
    ))
}

fn restored_main_window_position(
    saved_position: PhysicalPosition<i32>,
    work_areas: &[(PhysicalPosition<i32>, PhysicalSize<u32>, PhysicalSize<u32>)],
    fallback: (PhysicalPosition<i32>, PhysicalSize<u32>, PhysicalSize<u32>),
) -> PhysicalPosition<i32> {
    let best = work_areas
        .iter()
        .copied()
        .map(|work_area| {
            (
                window_work_area_intersection(
                    saved_position,
                    work_area.2,
                    (work_area.0, work_area.1),
                ),
                work_area,
            )
        })
        .max_by_key(|(area, _)| *area);
    if let Some((_, work_area)) = best.filter(|(area, _)| *area > 0) {
        return clamp_position_to_work_area(
            saved_position,
            work_area.2,
            (work_area.0, work_area.1),
        );
    }
    center_position_in_work_area(fallback.2, (fallback.0, fallback.1))
}

fn window_work_area_intersection(
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    work_area: (PhysicalPosition<i32>, PhysicalSize<u32>),
) -> u64 {
    let window_left = i64::from(position.x);
    let window_top = i64::from(position.y);
    let window_right = window_left.saturating_add(i64::from(size.width));
    let window_bottom = window_top.saturating_add(i64::from(size.height));
    let work_left = i64::from(work_area.0.x);
    let work_top = i64::from(work_area.0.y);
    let work_right = work_left.saturating_add(i64::from(work_area.1.width));
    let work_bottom = work_top.saturating_add(i64::from(work_area.1.height));
    let width = window_right
        .min(work_right)
        .saturating_sub(window_left.max(work_left));
    let height = window_bottom
        .min(work_bottom)
        .saturating_sub(window_top.max(work_top));
    if width <= 0 || height <= 0 {
        0
    } else {
        u64::try_from(width.saturating_mul(height)).unwrap_or(u64::MAX)
    }
}

fn clamp_position_to_work_area(
    position: PhysicalPosition<i32>,
    window_size: PhysicalSize<u32>,
    work_area: (PhysicalPosition<i32>, PhysicalSize<u32>),
) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        clamp_axis_to_work_area(
            position.x,
            window_size.width,
            work_area.0.x,
            work_area.1.width,
        ),
        clamp_axis_to_work_area(
            position.y,
            window_size.height,
            work_area.0.y,
            work_area.1.height,
        ),
    )
}

fn clamp_axis_to_work_area(
    current_start: i32,
    window_length: u32,
    work_start: i32,
    work_length: u32,
) -> i32 {
    let start = i64::from(work_start);
    let travel = i64::from(work_length.saturating_sub(window_length));
    let offset = i64::from(current_start)
        .saturating_sub(start)
        .clamp(0, travel);
    start
        .saturating_add(offset)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn center_position_in_work_area(
    window_size: PhysicalSize<u32>,
    work_area: (PhysicalPosition<i32>, PhysicalSize<u32>),
) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        centered_axis_start(window_size.width, work_area.0.x, work_area.1.width),
        centered_axis_start(window_size.height, work_area.0.y, work_area.1.height),
    )
}

fn centered_axis_start(window_length: u32, work_start: i32, work_length: u32) -> i32 {
    let start = i64::from(work_start);
    let travel = i64::from(work_length.saturating_sub(window_length));
    start
        .saturating_add(travel / 2)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn resize_main_window(window: &WebviewWindow, expanded: bool) -> Result<(), String> {
    let (desired_width, desired_height) = if expanded {
        EXPANDED_SIZE
    } else {
        COMPACT_SIZE
    };
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let original_position = window.outer_position().ok();
    let original_size = window.outer_size().ok();
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?;
    let (width, height) = monitor
        .as_ref()
        .map_or((desired_width, desired_height), |monitor| {
            fit_logical_size(
                (desired_width, desired_height),
                monitor.work_area().size,
                scale,
            )
        });

    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|error| error.to_string())?;
    if let (Some(position), Some(current_size), Some(monitor)) =
        (original_position, original_size, monitor)
    {
        let target_size = PhysicalSize::new(
            (width * scale).round().max(1.0) as u32,
            (height * scale).round().max(1.0) as u32,
        );
        let work_area = monitor.work_area();
        let target_position = anchored_resize_position(
            position,
            current_size,
            target_size,
            work_area.position,
            work_area.size,
        );
        window
            .set_position(target_position)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn anchored_resize_position(
    current_position: PhysicalPosition<i32>,
    current_size: PhysicalSize<u32>,
    target_size: PhysicalSize<u32>,
    work_position: PhysicalPosition<i32>,
    work_size: PhysicalSize<u32>,
) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        anchored_axis_start(
            current_position.x,
            current_size.width,
            target_size.width,
            work_position.x,
            work_size.width,
        ),
        anchored_axis_start(
            current_position.y,
            current_size.height,
            target_size.height,
            work_position.y,
            work_size.height,
        ),
    )
}

fn anchored_axis_start(
    current_start: i32,
    current_length: u32,
    target_length: u32,
    work_start: i32,
    work_length: u32,
) -> i32 {
    let work_start = i64::from(work_start);
    let work_length = i64::from(work_length);
    let current_length = i64::from(current_length).min(work_length);
    let target_length = i64::from(target_length).min(work_length);
    let current_travel = work_length.saturating_sub(current_length);
    let target_travel = work_length.saturating_sub(target_length);
    let current_offset = i64::from(current_start)
        .saturating_sub(work_start)
        .clamp(0, current_travel);

    let target_offset = if current_travel == 0 {
        0
    } else {
        let numerator = i128::from(current_offset) * i128::from(target_travel);
        let denominator = i128::from(current_travel);
        ((numerator + denominator / 2) / denominator) as i64
    };

    work_start
        .saturating_add(target_offset)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn fit_logical_size(
    desired: (f64, f64),
    work_area: tauri::PhysicalSize<u32>,
    scale: f64,
) -> (f64, f64) {
    let available_width = (work_area.width as f64 / scale).floor();
    let available_height = (work_area.height as f64 / scale).floor();
    (
        desired.0.min(available_width),
        desired.1.min(available_height),
    )
}

fn bounded_single_line(value: &str, max_chars: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn load_preferences(path: &Path) -> DesktopPreferences {
    load_json::<DesktopPreferences>(path)
        .unwrap_or_default()
        .normalize_loaded()
}

fn persist_preferences(path: &Path, preferences: &DesktopPreferences) -> Result<(), String> {
    persist_json(path, preferences)
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

fn persist_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "desktop preferences path has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn ensure_supported_platform() -> Result<(), String> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Err("desktop companion is supported only on Windows and macOS".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use tauri::{PhysicalPosition, PhysicalSize};

    use super::{
        anchored_resize_position, bounded_single_line, canonical_compact_position,
        fit_logical_size, load_json, persist_json, persist_preferences, preset_position,
        radar_source_checks, radar_source_from_menu_id, restored_main_window_position,
        tray_left_click_behavior, window_work_area_intersection, CompanionProjection,
        DesktopOption, DesktopPreferences,
        MainPositionSaveState, MainWindowPositionPreset, RadarSource, SavedMainWindowPosition,
        TrayLeftClickBehavior, MENU_LAUNCH_AT_LOGIN, MENU_POSITION_BOTTOM_LEFT,
        MENU_POSITION_BOTTOM_RIGHT, MENU_POSITION_CENTER, MENU_POSITION_TOP_LEFT,
        MENU_POSITION_TOP_RIGHT, MENU_RADAR_SOURCE_DISTRIBUTED, MENU_RADAR_SOURCE_MAIN,
        VALID_OPACITY,
    };
    #[cfg(windows)]
    use super::{claim_taskbar_monitor, taskbar_failure_preferences};
    #[cfg(windows)]
    use std::sync::atomic::AtomicBool;

    #[test]
    fn preferences_default_to_recoverable_visible_windows() {
        let preferences = DesktopPreferences::default();
        assert!(preferences.always_on_top);
        assert!(!preferences.click_through);
        assert!(!preferences.position_locked);
        assert!(preferences.show_taskbar_window);
        assert!(preferences.show_main_window);
        assert!(!preferences.launch_at_login);
        assert_eq!(preferences.opacity_percent, 100);
        assert_eq!(preferences.radar_source, RadarSource::Main);
    }

    #[test]
    fn legacy_preferences_default_start_at_login_to_disabled() {
        let legacy = serde_json::json!({
            "alwaysOnTop": false,
            "clickThrough": true,
            "positionLocked": true,
            "showTaskbarWindow": false,
            "showMainWindow": true,
            "opacityPercent": 70,
            "radarSource": "distributed"
        });

        let preferences =
            serde_json::from_value::<DesktopPreferences>(legacy).expect("legacy preferences");

        assert!(!preferences.launch_at_login);
        assert_eq!(preferences.radar_source, RadarSource::Distributed);
        assert_eq!(preferences.opacity_percent, 70);
    }

    #[test]
    fn start_at_login_uses_the_camel_case_wire_field_and_menu_option() {
        let preferences =
            DesktopPreferences::default().with_option(DesktopOption::LaunchAtLogin, true);
        let serialized = serde_json::to_value(&preferences).expect("serialized preferences");

        assert_eq!(
            serialized
                .get("launchAtLogin")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(
            DesktopOption::from_menu_id(MENU_LAUNCH_AT_LOGIN),
            Some(DesktopOption::LaunchAtLogin)
        );
        assert!(DesktopOption::LaunchAtLogin.enabled(&preferences));
    }

    #[test]
    fn start_at_login_transition_preserves_unrelated_preferences() {
        let previous = DesktopPreferences {
            always_on_top: false,
            click_through: true,
            position_locked: true,
            show_taskbar_window: false,
            show_main_window: true,
            launch_at_login: false,
            opacity_percent: 60,
            radar_source: RadarSource::Distributed,
        };

        let next = previous
            .clone()
            .with_option(DesktopOption::LaunchAtLogin, true);

        assert!(next.launch_at_login);
        assert_eq!(next.always_on_top, previous.always_on_top);
        assert_eq!(next.click_through, previous.click_through);
        assert_eq!(next.position_locked, previous.position_locked);
        assert_eq!(next.show_taskbar_window, previous.show_taskbar_window);
        assert_eq!(next.show_main_window, previous.show_main_window);
        assert_eq!(next.opacity_percent, previous.opacity_percent);
        assert_eq!(next.radar_source, previous.radar_source);
    }

    #[test]
    fn radar_source_menu_ids_and_checks_are_exclusive() {
        assert_eq!(
            radar_source_from_menu_id(MENU_RADAR_SOURCE_MAIN),
            Some(RadarSource::Main)
        );
        assert_eq!(
            radar_source_from_menu_id(MENU_RADAR_SOURCE_DISTRIBUTED),
            Some(RadarSource::Distributed)
        );
        assert_eq!(
            radar_source_from_menu_id("desktop.radar-source.unknown"),
            None
        );

        assert_eq!(radar_source_checks(&RadarSource::Main), [true, false]);
        assert_eq!(
            radar_source_checks(&RadarSource::Distributed),
            [false, true]
        );
    }

    #[test]
    fn radar_source_selection_preserves_every_window_preference() {
        let previous = DesktopPreferences {
            always_on_top: false,
            click_through: true,
            position_locked: true,
            show_taskbar_window: false,
            show_main_window: true,
            launch_at_login: true,
            opacity_percent: 70,
            radar_source: RadarSource::Main,
        };

        let next = previous.clone().with_radar_source(RadarSource::Distributed);

        assert_eq!(next.radar_source, RadarSource::Distributed);
        assert_eq!(next.always_on_top, previous.always_on_top);
        assert_eq!(next.click_through, previous.click_through);
        assert_eq!(next.position_locked, previous.position_locked);
        assert_eq!(next.show_taskbar_window, previous.show_taskbar_window);
        assert_eq!(next.show_main_window, previous.show_main_window);
        assert_eq!(next.launch_at_login, previous.launch_at_login);
        assert_eq!(next.opacity_percent, previous.opacity_percent);
    }

    #[test]
    fn distributed_radar_source_round_trips_with_wire_value() {
        let preferences = DesktopPreferences::default().with_radar_source(RadarSource::Distributed);
        let serialized = serde_json::to_value(&preferences).expect("serialized preferences");

        assert_eq!(
            serialized
                .get("radarSource")
                .and_then(serde_json::Value::as_str),
            Some("distributed")
        );
        assert_eq!(
            serde_json::from_value::<DesktopPreferences>(serialized)
                .expect("deserialize preferences")
                .radar_source,
            RadarSource::Distributed
        );
    }

    #[test]
    fn macos_tray_click_opens_details_while_other_platforms_toggle() {
        assert_eq!(
            tray_left_click_behavior(true),
            TrayLeftClickBehavior::ShowDetails
        );
        assert_eq!(
            tray_left_click_behavior(false),
            TrayLeftClickBehavior::ToggleMain
        );
    }

    #[test]
    fn loaded_preferences_validate_opacity_and_visibility() {
        let preferences = DesktopPreferences {
            show_taskbar_window: false,
            show_main_window: false,
            opacity_percent: 42,
            ..DesktopPreferences::default()
        }
        .normalize_loaded();

        assert!(preferences.show_taskbar_window);
        assert!(!preferences.show_main_window);
        assert_eq!(preferences.opacity_percent, 100);
    }

    #[test]
    fn disabling_the_only_visible_projection_enables_the_other_one() {
        let only_main = DesktopPreferences {
            show_taskbar_window: false,
            show_main_window: true,
            ..DesktopPreferences::default()
        };
        let result = only_main.with_option(DesktopOption::ShowMainWindow, false);
        assert!(result.show_taskbar_window);
        assert!(!result.show_main_window);

        let only_taskbar = DesktopPreferences {
            show_taskbar_window: true,
            show_main_window: false,
            ..DesktopPreferences::default()
        };
        let result = only_taskbar.with_option(DesktopOption::ShowTaskbarWindow, false);
        assert!(!result.show_taskbar_window);
        assert!(result.show_main_window);
    }

    #[test]
    fn quick_position_menu_ids_map_to_exact_presets() {
        assert_eq!(
            MainWindowPositionPreset::from_menu_id(MENU_POSITION_TOP_LEFT),
            Some(MainWindowPositionPreset::TopLeft)
        );
        assert_eq!(
            MainWindowPositionPreset::from_menu_id(MENU_POSITION_TOP_RIGHT),
            Some(MainWindowPositionPreset::TopRight)
        );
        assert_eq!(
            MainWindowPositionPreset::from_menu_id(MENU_POSITION_CENTER),
            Some(MainWindowPositionPreset::Center)
        );
        assert_eq!(
            MainWindowPositionPreset::from_menu_id(MENU_POSITION_BOTTOM_LEFT),
            Some(MainWindowPositionPreset::BottomLeft)
        );
        assert_eq!(
            MainWindowPositionPreset::from_menu_id(MENU_POSITION_BOTTOM_RIGHT),
            Some(MainWindowPositionPreset::BottomRight)
        );
        assert_eq!(
            MainWindowPositionPreset::from_menu_id("desktop.position.unknown"),
            None
        );
    }

    #[test]
    fn quick_position_command_presets_decode_kebab_case() {
        let cases = [
            ("top-left", MainWindowPositionPreset::TopLeft),
            ("top-right", MainWindowPositionPreset::TopRight),
            ("center", MainWindowPositionPreset::Center),
            ("bottom-left", MainWindowPositionPreset::BottomLeft),
            ("bottom-right", MainWindowPositionPreset::BottomRight),
        ];

        for (wire_value, expected) in cases {
            assert_eq!(
                serde_json::from_value::<MainWindowPositionPreset>(serde_json::json!(wire_value))
                    .expect("valid preset wire value"),
                expected
            );
        }

        assert!(
            serde_json::from_value::<MainWindowPositionPreset>(serde_json::json!("topLeft"))
                .is_err()
        );
        assert!(
            serde_json::from_value::<MainWindowPositionPreset>(serde_json::json!("unknown"))
                .is_err()
        );
    }

    #[test]
    fn quick_positions_use_work_area_edges_and_odd_centers() {
        let work_position = PhysicalPosition::new(-1920, 100);
        let work_size = PhysicalSize::new(1001, 801);
        let window_size = PhysicalSize::new(201, 101);

        assert_eq!(
            preset_position(
                MainWindowPositionPreset::TopLeft,
                work_position,
                work_size,
                window_size,
            ),
            Some(PhysicalPosition::new(-1920, 100))
        );
        assert_eq!(
            preset_position(
                MainWindowPositionPreset::TopRight,
                work_position,
                work_size,
                window_size,
            ),
            Some(PhysicalPosition::new(-1120, 100))
        );
        assert_eq!(
            preset_position(
                MainWindowPositionPreset::Center,
                work_position,
                work_size,
                window_size,
            ),
            Some(PhysicalPosition::new(-1520, 450))
        );
        assert_eq!(
            preset_position(
                MainWindowPositionPreset::BottomLeft,
                work_position,
                work_size,
                window_size,
            ),
            Some(PhysicalPosition::new(-1920, 800))
        );
        assert_eq!(
            preset_position(
                MainWindowPositionPreset::BottomRight,
                work_position,
                work_size,
                window_size,
            ),
            Some(PhysicalPosition::new(-1120, 800))
        );
    }

    #[test]
    fn quick_positions_reject_invalid_or_oversized_windows() {
        let work_position = PhysicalPosition::new(20, 30);
        let work_size = PhysicalSize::new(800, 600);
        assert_eq!(
            preset_position(
                MainWindowPositionPreset::Center,
                work_position,
                work_size,
                work_size,
            ),
            Some(work_position)
        );
        assert_eq!(
            preset_position(
                MainWindowPositionPreset::TopLeft,
                work_position,
                work_size,
                PhysicalSize::new(801, 600),
            ),
            None
        );
        assert_eq!(
            preset_position(
                MainWindowPositionPreset::TopLeft,
                work_position,
                work_size,
                PhysicalSize::new(360, 0),
            ),
            None
        );
    }

    #[test]
    fn expanded_geometry_persists_the_equivalent_compact_anchor() {
        let canonical = canonical_compact_position(
            PhysicalPosition::new(3240, 1308),
            PhysicalSize::new(600, 780),
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(3840, 2088),
            1.5,
        );

        assert_eq!(canonical, Some(PhysicalPosition::new(3300, 1920)));
    }

    #[test]
    fn restore_clamps_partial_positions_and_centers_disconnected_displays() {
        let window_size = PhysicalSize::new(360, 112);
        let primary = (
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1920, 1040),
            window_size,
        );
        let secondary = (
            PhysicalPosition::new(-1280, 0),
            PhysicalSize::new(1280, 984),
            window_size,
        );
        let work_areas = [primary, secondary];

        assert_eq!(
            restored_main_window_position(PhysicalPosition::new(-1200, 100), &work_areas, primary,),
            PhysicalPosition::new(-1200, 100)
        );
        assert_eq!(
            restored_main_window_position(PhysicalPosition::new(1800, 100), &work_areas, primary,),
            PhysicalPosition::new(1560, 100)
        );
        assert_eq!(
            restored_main_window_position(PhysicalPosition::new(3000, 1500), &work_areas, primary,),
            PhysicalPosition::new(780, 464)
        );

        let oversized_window = PhysicalSize::new(2200, 1200);
        let oversized_primary = (primary.0, primary.1, oversized_window);
        let oversized_work_areas = [
            oversized_primary,
            (secondary.0, secondary.1, oversized_window),
        ];
        assert_eq!(
            restored_main_window_position(
                PhysicalPosition::new(3000, 1500),
                &oversized_work_areas,
                oversized_primary,
            ),
            PhysicalPosition::new(0, 0)
        );
    }

    #[test]
    fn restore_uses_each_monitors_scaled_compact_size_when_clamping() {
        let primary = (
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1920, 1040),
            PhysicalSize::new(360, 112),
        );
        let scaled_secondary = (
            PhysicalPosition::new(1920, 0),
            PhysicalSize::new(2000, 1000),
            PhysicalSize::new(540, 168),
        );

        assert_eq!(
            restored_main_window_position(
                PhysicalPosition::new(3560, 900),
                &[primary, scaled_secondary],
                primary,
            ),
            PhysicalPosition::new(3380, 832)
        );
    }

    #[test]
    fn position_save_state_starts_only_one_ready_writer() {
        let mut state = MainPositionSaveState::new(None);
        assert!(!state.mark_dirty());
        assert_eq!(state.revision, 0);

        state.ready = true;
        assert!(state.mark_dirty());
        assert!(!state.mark_dirty());
        assert_eq!(state.revision, 2);
        state.writer_active = false;
        state.persisted_revision = state.revision;
        assert!(state.mark_dirty());
        assert_eq!(state.revision, 3);
    }

    #[test]
    fn position_writer_error_retries_only_when_a_new_revision_arrived() {
        let mut state = MainPositionSaveState::new(None);
        state.ready = true;
        assert!(state.mark_dirty());
        let attempted_revision = state.revision;

        assert!(!state.finish_writer_after_error(attempted_revision));
        assert!(!state.writer_active);

        assert!(state.mark_dirty());
        let attempted_revision = state.revision;
        assert!(!state.mark_dirty());
        assert!(state.finish_writer_after_error(attempted_revision));
        assert!(state.writer_active);
    }

    #[test]
    fn saved_position_json_round_trips_negative_coordinates_and_rejects_malformed_data() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "model-radar-position-{}-{nonce}",
            std::process::id()
        ));
        let path = directory.join("main-window-position.json");
        let position = SavedMainWindowPosition { x: -1440, y: 72 };

        persist_json(&path, &position).expect("save position");
        assert_eq!(load_json(&path), Some(position));
        fs::write(&path, b"{broken").expect("write malformed position");
        assert_eq!(load_json::<SavedMainWindowPosition>(&path), None);

        fs::remove_file(path).expect("remove saved file");
        fs::remove_dir(directory).expect("remove test directory");
    }

    #[cfg(windows)]
    #[test]
    fn taskbar_monitor_can_only_be_claimed_once() {
        let started = AtomicBool::new(false);
        assert!(claim_taskbar_monitor(&started));
        assert!(!claim_taskbar_monitor(&started));
    }

    #[cfg(windows)]
    #[test]
    fn runtime_taskbar_failure_disables_projection_and_restores_main() {
        let taskbar_only = DesktopPreferences {
            show_taskbar_window: true,
            show_main_window: false,
            ..DesktopPreferences::default()
        };
        let fallback = taskbar_failure_preferences(&taskbar_only).expect("fallback");
        assert!(!fallback.show_taskbar_window);
        assert!(fallback.show_main_window);
        assert!(taskbar_failure_preferences(&fallback).is_none());
    }

    #[test]
    fn enabling_main_window_from_taskbar_only_keeps_taskbar_preference() {
        // Tray left-click force-show must be allowed while the companion is still preferred
        // (possibly broken). Preferencing main on must not require demoting the taskbar first.
        let taskbar_only = DesktopPreferences {
            show_taskbar_window: true,
            show_main_window: false,
            ..DesktopPreferences::default()
        };
        let shown = taskbar_only.with_option(DesktopOption::ShowMainWindow, true);
        assert!(shown.show_main_window);
        assert!(shown.show_taskbar_window);
    }

    #[test]
    fn off_screen_seed_is_centered_when_no_work_area_intersects() {
        let off_screen = PhysicalPosition::new(3300, 1920);
        let primary = (
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(2560u32, 1392u32),
            PhysicalSize::new(360u32, 112u32),
        );
        let secondary = (
            PhysicalPosition::new(2560, 0),
            PhysicalSize::new(2560u32, 1392u32),
            PhysicalSize::new(360u32, 112u32),
        );
        let restored = restored_main_window_position(
            off_screen,
            &[primary, secondary],
            (primary.0, primary.1, primary.2),
        );
        assert_eq!(restored, PhysicalPosition::new((2560 - 360) / 2, (1392 - 112) / 2));
        assert!(
            window_work_area_intersection(restored, primary.2, (primary.0, primary.1)) > 0,
            "recovered position must intersect the primary work area"
        );
    }

    #[test]
    fn partially_visible_multi_monitor_seed_is_clamped_not_recentered() {
        // Mostly on the secondary display with a small primary overlap; choose the
        // greater-intersection work area and clamp without recentering.
        let seed = PhysicalPosition::new(2400, 800);
        let primary = (
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(2560u32, 1392u32),
            PhysicalSize::new(360u32, 112u32),
        );
        let secondary = (
            PhysicalPosition::new(2560, 0),
            PhysicalSize::new(2560u32, 1392u32),
            PhysicalSize::new(360u32, 112u32),
        );
        let restored =
            restored_main_window_position(seed, &[primary, secondary], (primary.0, primary.1, primary.2));
        assert_eq!(restored.x, 2560);
        assert_eq!(restored.y, 800);
        assert!(
            window_work_area_intersection(restored, secondary.2, (secondary.0, secondary.1)) > 0
        );
    }

    #[test]
    fn opacity_selection_is_exclusive() {
        for selected in VALID_OPACITY {
            let checks = VALID_OPACITY.map(|opacity| opacity == selected);
            assert_eq!(checks.into_iter().filter(|checked| *checked).count(), 1);
        }
    }

    #[test]
    fn projection_titles_are_single_line_and_bounded() {
        let projection = CompanionProjection {
            model_name: "a very long\nmodel name that cannot grow forever".to_owned(),
            reasoning_effort: "max\nreasoning".to_owned(),
            score_text: "106.30".to_owned(),
            tie_count: 4,
            status_label: "ready\nnow".to_owned(),
        }
        .normalized();
        let title = projection.title();

        assert!(!title.contains('\n'));
        assert!(title.chars().count() <= 64);
        assert!(title.contains("+4"));
        assert!(!title.contains("reasoning"));
        assert!(!title.contains("ready"));
    }

    #[test]
    fn legacy_show_more_info_preference_is_ignored_and_removed_on_save() {
        let legacy = r#"{
            "alwaysOnTop": false,
            "clickThrough": true,
            "positionLocked": true,
            "showMoreInfo": true,
            "showTaskbarWindow": true,
            "showMainWindow": false,
            "opacityPercent": 80
        }"#;

        let preferences: DesktopPreferences =
            serde_json::from_str(legacy).expect("legacy preferences");
        assert!(!preferences.always_on_top);
        assert!(preferences.click_through);
        assert!(preferences.position_locked);
        assert!(preferences.show_taskbar_window);
        assert!(!preferences.show_main_window);
        assert_eq!(preferences.opacity_percent, 80);
        assert_eq!(preferences.radar_source, RadarSource::Main);

        let saved = serde_json::to_value(preferences).expect("serialized preferences");
        assert!(saved.get("showMoreInfo").is_none());
        assert_eq!(
            saved.get("radarSource").and_then(serde_json::Value::as_str),
            Some("main")
        );
    }

    #[test]
    fn bounded_text_collapses_whitespace_before_truncating() {
        assert_eq!(bounded_single_line("  GPT\n  5.6  ", 7), "GPT 5.6");
    }

    #[test]
    fn preferences_can_be_saved_twice_to_the_same_windows_path() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "model-radar-desktop-{}-{nonce}",
            std::process::id()
        ));
        let path = directory.join("desktop-preferences.json");
        let first = DesktopPreferences::default();
        let second = DesktopPreferences {
            opacity_percent: 80,
            ..DesktopPreferences::default()
        };

        persist_preferences(&path, &first).expect("first save");
        persist_preferences(&path, &second).expect("second save");
        let saved: DesktopPreferences =
            serde_json::from_slice(&fs::read(&path).expect("saved file")).expect("saved JSON");

        assert_eq!(saved, second);
        fs::remove_file(path).expect("remove saved file");
        fs::remove_dir(directory).expect("remove test directory");
    }

    #[test]
    fn expanded_window_is_capped_to_the_scaled_work_area() {
        assert_eq!(
            fit_logical_size((400.0, 520.0), PhysicalSize::new(1920, 1000), 2.0),
            (400.0, 500.0)
        );
        assert_eq!(
            fit_logical_size((400.0, 520.0), PhysicalSize::new(1920, 1040), 1.0),
            (400.0, 520.0)
        );
    }

    #[test]
    fn right_bottom_anchor_round_trips_at_150_percent_scale() {
        let work_position = PhysicalPosition::new(0, 0);
        let work_size = PhysicalSize::new(3840, 2088);
        let compact_size = PhysicalSize::new(540, 168);
        let expanded_size = PhysicalSize::new(600, 780);
        let compact_position = PhysicalPosition::new(3300, 1920);

        let expanded_position = anchored_resize_position(
            compact_position,
            compact_size,
            expanded_size,
            work_position,
            work_size,
        );
        assert_eq!(expanded_position, PhysicalPosition::new(3240, 1308));

        let restored_position = anchored_resize_position(
            expanded_position,
            expanded_size,
            compact_size,
            work_position,
            work_size,
        );
        assert_eq!(restored_position, compact_position);
    }

    #[test]
    fn interior_anchor_round_trips_without_switching_to_another_edge() {
        let work_position = PhysicalPosition::new(-100, 50);
        let work_size = PhysicalSize::new(1000, 800);
        let compact_size = PhysicalSize::new(200, 100);
        let expanded_size = PhysicalSize::new(600, 500);
        let compact_position = PhysicalPosition::new(250, 325);

        let expanded_position = anchored_resize_position(
            compact_position,
            compact_size,
            expanded_size,
            work_position,
            work_size,
        );
        assert_eq!(expanded_position, PhysicalPosition::new(75, 168));

        let restored_position = anchored_resize_position(
            expanded_position,
            expanded_size,
            compact_size,
            work_position,
            work_size,
        );
        assert_eq!(restored_position, compact_position);
    }

    #[test]
    fn anchored_resize_clamps_windows_back_into_the_work_area() {
        let position = anchored_resize_position(
            PhysicalPosition::new(850, 650),
            PhysicalSize::new(360, 112),
            PhysicalSize::new(400, 520),
            PhysicalPosition::new(100, 50),
            PhysicalSize::new(800, 600),
        );

        assert_eq!(position, PhysicalPosition::new(500, 130));
    }
}
