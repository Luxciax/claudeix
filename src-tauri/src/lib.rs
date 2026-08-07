mod app_paths;
mod claude;
mod fs_bridge;
mod projects;
mod updater;
mod models;
mod transcript;
mod trash;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager, PhysicalPosition, PhysicalSize,
};

use claude::{RuntimeRegistry, SessionCatalog};
use transcript::editor::TranscriptEditor;
use trash::service::TrashService;

/// Keep the main window fully inside the current monitor's work area: shrink it
/// if it is larger than the work area, then clamp its position. Without this,
/// Windows may place the frameless window with its bottom edge under the
/// taskbar (or partially off-screen), which visually clips the composer at the
/// bottom of the UI even though the DOM itself is fine.
fn clamp_to_work_area(window: &tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        return; // maximized windows already fill the work area
    }
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let wa = monitor.work_area();
    let Ok(pos) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };

    // Shrink to fit the work area when the window is larger than it.
    let w = size.width.min(wa.size.width);
    let h = size.height.min(wa.size.height);
    if w != size.width || h != size.height {
        let _ = window.set_size(PhysicalSize::new(w, h));
    }

    // Re-read the size (the platform may push set_size back up to the min size).
    let Ok(size) = window.outer_size() else {
        return;
    };
    let max_x = wa.position.x + wa.size.width as i32 - size.width as i32;
    let max_y = wa.position.y + wa.size.height as i32 - size.height as i32;
    if max_x < wa.position.x || max_y < wa.position.y {
        return; // window cannot fit (min size > work area) — leave it be
    }
    let x = pos.x.clamp(wa.position.x, max_x);
    let y = pos.y.clamp(wa.position.y, max_y);
    if x != pos.x || y != pos.y {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// Open an http(s) URL in the system default browser (terminal web-links).
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("only http(s) urls allowed".into());
    }
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&url).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

/// Build the system tray so the main window can be minimized to tray and
/// restored (or quit) from outside the window. Left-click toggles the window,
/// right-click (or the menu) shows 显示 / 隐藏 / 退出.
fn create_tray(app: &App) -> tauri::Result<()> {
    let show_i = MenuItem::with_id(app, "show", "显示", true, None::<&str>)?;
    let hide_i = MenuItem::with_id(app, "hide", "隐藏", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &hide_i, &quit_i])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("app must have a default window icon for the tray");

    TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Claudeix")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    clamp_to_work_area(&w);
                    let _ = w.set_focus();
                }
            }
            "hide" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            "quit" => {
                app.state::<RuntimeRegistry>().stop_all();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let is_left_click = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            );

            if !is_left_click {
                return;
            }

            // Windows fires tray clicks with button_state == Down, while macOS
            // uses Up. Ignore the Up event on Windows because the actual toggle
            // already happened on Down; on macOS Up is the correct state.
            #[cfg(target_os = "windows")]
            let should_toggle = matches!(
                event,
                TrayIconEvent::Click {
                    button_state: MouseButtonState::Down,
                    ..
                } | TrayIconEvent::DoubleClick { .. }
            );

            #[cfg(not(target_os = "windows"))]
            let should_toggle = matches!(
                event,
                TrayIconEvent::Click {
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick { .. }
            );

            if !should_toggle {
                return;
            }

            let app = tray.app_handle();
            if let Some(w) = app.get_webview_window("main") {
                if w.is_visible().unwrap_or(false) {
                    let _ = w.hide();
                } else {
                    let _ = w.show();
                    let _ = w.unminimize();
                    clamp_to_work_area(&w);
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(RuntimeRegistry::default())
        .manage(TranscriptEditor::default())
        .manage(TrashService::default())
        .manage(SessionCatalog::default())
        .invoke_handler(tauri::generate_handler![
            claude::commands::claude_status,
            claude::commands::claude_config,
            claude::commands::session_scan,
            claude::commands::session_get,
            claude::commands::session_refresh,
            claude::commands::session_set_archived,
            claude::runtime_registry::runtime_start,
            claude::runtime_registry::runtime_stop,
            claude::runtime_registry::runtime_permission_decide,
            claude::runtime_registry::runtime_get,
            claude::runtime_registry::runtime_list,
            transcript::editor::transcript_edit,
            transcript::editor::session_rename,
            transcript::editor::transcript_snapshots_list,
            transcript::editor::transcript_audit_list,
            transcript::editor::transcript_recovery_list,
            transcript::editor::transcript_recovery_resolve,
            transcript::editor::transcript_snapshot_restore,
            trash::service::trash_delete_preview,
            trash::service::trash_move,
            trash::service::trash_list,
            trash::service::trash_restore_preview,
            trash::service::trash_restore,
            trash::service::trash_purge_preview,
            trash::service::trash_purge,
            claude::runtime_registry::runtime_probe_external,
            fs_bridge::workspace_root,
            projects::projects_recent,
            projects::project_open,
            projects::project_remove_recent,
            projects::project_pick,
            updater::update_check,
            updater::update_apply,
            open_external
        ])
        .setup(|app| {
            if let Err(e) = create_tray(app) {
                eprintln!("[tray] failed to create system tray: {e}");
            }
            // Clamp the main window into its monitor's work area at launch.
            // Windows may otherwise place the frameless window under the
            // taskbar, clipping the bottom of the UI (the composer). Tray
            // restore paths clamp too; user drags are never overridden.
            if let Some(main_win) = app.get_webview_window("main") {
                clamp_to_work_area(&main_win);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Pi desktop");
}
