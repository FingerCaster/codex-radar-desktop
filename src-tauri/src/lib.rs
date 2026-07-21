mod desktop;
mod radar;

use std::io;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use radar::{get_radar_snapshot, refresh_radar, start_background_polling, RadarService};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Single-instance must register first so a second launch can recover the UI
        // when the tray/taskbar surfaces are stuck or the main window is hidden.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // The callback can run on Tauri's event-loop thread. A background
            // preference transition may hold its gate while waiting for Wry,
            // so never make the event loop wait for that gate here.
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let controller = app.state::<desktop::DesktopController>();
                if let Err(error) = controller.force_show_main_window(&app) {
                    eprintln!("[model-radar] second-instance force-show failed: {error}");
                }
            });
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let controller =
                desktop::DesktopController::new(app.handle()).map_err(io::Error::other)?;
            let initial_source = controller
                .preferences()
                .map_err(io::Error::other)?
                .radar_source;
            let service = RadarService::new(initial_source).map_err(io::Error::other)?;
            let polling_service = service.clone();
            if !app.manage(service) {
                return Err(io::Error::other("radar service was already managed").into());
            }
            if !app.manage(controller) {
                return Err(io::Error::other("desktop controller was already managed").into());
            }
            desktop::build_tray(app.handle()).map_err(io::Error::other)?;
            app.state::<desktop::DesktopController>()
                .initialize(app.handle())
                .map_err(io::Error::other)?;
            start_background_polling(app.handle().clone(), polling_service);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. }
                if desktop::close_request_should_hide(window.label()) =>
            {
                api.prevent_close();
                desktop::handle_close_requested(window);
            }
            WindowEvent::CloseRequested { .. } => {}
            WindowEvent::Moved(_)
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. } => {
                desktop::handle_main_window_geometry_changed(window);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_radar_snapshot,
            refresh_radar,
            desktop::get_desktop_preferences,
            desktop::get_main_expanded,
            desktop::set_desktop_option,
            desktop::set_desktop_opacity,
            desktop::set_desktop_radar_source,
            desktop::set_main_window_position_preset,
            desktop::update_companion_projection,
            desktop::set_window_expanded,
            desktop::hide_window,
            desktop::show_main_details,
            desktop::show_desktop_context_menu,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Model Radar");

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. }) {
            desktop::handle_app_exit(app_handle);
        }
    });
}
