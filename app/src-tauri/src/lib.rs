//! Tasks app (Windows and Android): the Tasks UI on a local tasks
//! DB that works offline and syncs with tasks-server.

#[cfg(target_os = "android")]
mod android;
mod commands;
mod reminders;
mod state;
mod syncer;
mod updates;
#[cfg(desktop)]
mod widgets;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Manager;

use state::AppState;

/// Tells the main page to open its own quick add dialog: Alt+N while it's in front.
/// From anywhere else Alt+N brings up the popup instead (widgets.rs).
pub const EVENT_QUICK_ADD: &str = "tasks://quick-add";
/// Tells the page to open a task (a tapped notification).
pub const EVENT_OPEN_TASK: &str = "tasks://open-task";

#[cfg(desktop)]
pub(crate) fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg(desktop)]
fn desktop_setup(app: &mut tauri::App) -> tauri::Result<()> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri_plugin_autostart::ManagerExt;
    use tauri_plugin_global_shortcut::{Builder as ShortcutBuilder, ShortcutState};

    // Alt+N anywhere: the quick add popup, on its own.
    let shortcuts = ShortcutBuilder::new()
        .with_shortcuts(["alt+n"])
        .map(|b| {
            b.with_handler(|app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    widgets::quick_add(app);
                }
            })
            .build()
        });
    match shortcuts {
        Ok(plugin) => {
            if let Err(e) = app.handle().plugin(plugin) {
                eprintln!("Alt+N is not available: {e}");
            }
        }
        Err(e) => eprintln!("Alt+N is not available: {e}"),
    }

    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let open = MenuItem::with_id(app, "open", "Open Tasks", true, None::<&str>)?;
    let add = MenuItem::with_id(app, "add", "Quick add\tAlt+N", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "Sync now", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(app, "autostart", "Start at login", true, autostart_on, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &add, &sync, &sep1, &autostart, &sep2, &quit])?;

    let toggle = autostart.clone();
    let mut tray = TrayIconBuilder::with_id("tasks")
        .tooltip("Tasks")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "add" => widgets::quick_add(app),
            "sync" => {
                if let Some(st) = app.try_state::<Arc<AppState>>() {
                    st.wake.notify_one();
                }
            }
            "autostart" => {
                let manager = app.autolaunch();
                let on = manager.is_enabled().unwrap_or(false);
                let _ = if on { manager.disable() } else { manager.enable() };
                let _ = toggle.set_checked(manager.is_enabled().unwrap_or(!on));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    // Closing the window hides it to the tray; reminders and sync keep running.
    if let Some(w) = app.get_webview_window("main") {
        let hide = w.clone();
        w.on_window_event(move |event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = hide.hide();
            }
            #[cfg(windows)]
            tauri::WindowEvent::Focused(true) => widgets::restore_taskbar_badge(hide.app_handle()),
            _ => {}
        });
    }
    widgets::setup(app.handle())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "android")]
    android::install_panic_log();

    // Only the desktop block below reassigns this, so Android sees an unused `mut`.
    #[cfg_attr(mobile, allow(unused_mut))]
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
                // A toast's button starts a second copy, which lands here with the action.
                if !reminders::handle_toast_action(app, &args) {
                    show_main(app);
                }
            }))
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--minimized"]),
            ))
            .plugin(
                tauri_plugin_window_state::Builder::default()
                    .with_state_flags(widgets::PIN_STATE)
                    .with_denylist(widgets::UNTRACKED_WINDOWS)
                    .build(),
            );
    }

    #[cfg(target_os = "android")]
    {
        builder = builder.plugin(android::init());
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let st = Arc::new(AppState::open(&dir).map_err(|e| format!("Can't open the tasks DB in {}: {e}", dir.display()))?);
            let args: Vec<String> = std::env::args().collect();
            let hidden = args.iter().any(|a| a == "--minimized") || reminders::is_background_action(&args);
            st.minimized.store(hidden, Ordering::Relaxed);
            app.manage(st.clone());

            #[cfg(desktop)]
            desktop_setup(app)?;
            #[cfg(windows)]
            updates::clean_up();

            // A tapped reminder opens its task (Android: android.rs).
            #[cfg(target_os = "android")]
            android::set_app(app.handle(), &st);

            if let Some(w) = app.get_webview_window("main") {
                if !st.minimized.load(Ordering::Relaxed) {
                    let _ = w.show();
                }
            }

            #[cfg(windows)]
            if let Err(e) = reminders::register_toast_app(app.handle(), &dir) {
                eprintln!("Can't register the app for notifications: {e}");
            }
            #[cfg(windows)]
            if let Err(e) = reminders::write_start_menu_shortcut(app.handle()) {
                eprintln!("Can't write the Start Menu shortcut: {e}");
            }
            #[cfg(windows)]
            if let Err(e) = reminders::register_toast_protocol() {
                eprintln!("Can't register the notification buttons: {e}");
            }
            // The app was closed and a toast's button started it.
            #[cfg(desktop)]
            reminders::handle_toast_action(app.handle(), &args);

            tauri::async_runtime::spawn(syncer::run(app.handle().clone(), st.clone()));
            tauri::async_runtime::spawn(reminders::run(app.handle().clone(), st));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::tasks_projects_list,
            commands::tasks_projects_create,
            commands::tasks_projects_update,
            commands::tasks_projects_delete,
            commands::tasks_projects_reorder,
            commands::tasks_categories_list,
            commands::tasks_categories_create,
            commands::tasks_categories_update,
            commands::tasks_categories_delete,
            commands::tasks_filters_list,
            commands::tasks_filters_create,
            commands::tasks_filters_update,
            commands::tasks_filters_delete,
            commands::profile_get,
            commands::profile_update,
            commands::tasks_list,
            commands::tasks_get,
            commands::tasks_completions,
            commands::tasks_create,
            commands::tasks_update,
            commands::tasks_delete,
            commands::tasks_delete_completed,
            commands::tasks_reorder,
            commands::app_status,
            commands::app_sign_in,
            commands::app_sign_out,
            commands::app_sync_now,
            commands::app_force_sync,
            commands::app_take_open_task,
            commands::app_show_task,
            commands::app_pin_open,
            commands::app_pin_close,
            commands::app_quick_hide,
            commands::app_set_taskbar_badge,
            commands::app_set_title_bar,
            commands::app_ask_notification_permission,
            commands::app_reminder_health,
            commands::app_open_reminder_setting,
            updates::app_update_check,
            updates::app_update_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Tasks app");
}
