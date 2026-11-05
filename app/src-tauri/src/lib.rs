//! Tasks app (Windows and Android): the Tasks UI on a local tasks
//! DB that works offline and syncs with tasks-server.

mod commands;
mod reminders;
mod state;
mod syncer;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{Emitter, Manager};

use state::AppState;

/// Tells the page to open the quick add (Alt+N, or the tray menu).
pub const EVENT_QUICK_ADD: &str = "tasks://quick-add";
/// Tells the page to open a task (a tapped notification).
pub const EVENT_OPEN_TASK: &str = "tasks://open-task";

#[cfg(desktop)]
fn show_main(app: &tauri::AppHandle) {
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

    // Alt+N anywhere: the app comes up with the quick add open.
    let shortcuts = ShortcutBuilder::new()
        .with_shortcuts(["alt+n"])
        .map(|b| {
            b.with_handler(|app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    show_main(app);
                    let _ = app.emit(EVENT_QUICK_ADD, ());
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
            "add" => {
                show_main(app);
                let _ = app.emit(EVENT_QUICK_ADD, ());
            }
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
        w.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = hide.hide();
            }
        });
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Only the desktop block below reassigns this, so Android sees an unused `mut`.
    #[cfg_attr(mobile, allow(unused_mut))]
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(vec!["--minimized"]),
            ));
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let st = Arc::new(AppState::open(&dir).map_err(|e| format!("Can't open the tasks DB in {}: {e}", dir.display()))?);
            st.minimized.store(std::env::args().any(|a| a == "--minimized"), Ordering::Relaxed);
            app.manage(st.clone());

            #[cfg(desktop)]
            desktop_setup(app)?;

            // A tapped notification opens its task (Android; desktop toasts carry no task).
            {
                use tauri_plugin_notification::NotificationExt;
                let handle = app.handle().clone();
                let tapped = st.clone();
                let _ = app.notification().on_action(move |action| {
                    let task = action
                        .notification()
                        .and_then(|n| n.extra().get("task_id"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    if let Some(task) = task {
                        *state::lock(&tapped.open_task) = Some(task.clone());
                        let _ = handle.emit(EVENT_OPEN_TASK, task);
                    }
                });
            }

            if let Some(w) = app.get_webview_window("main") {
                if !st.minimized.load(Ordering::Relaxed) {
                    let _ = w.show();
                }
            }

            #[cfg(windows)]
            if let Err(e) = reminders::register_toast_app(app.handle(), &dir) {
                eprintln!("Can't register the app for notifications: {e}");
            }

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
            commands::tasks_list,
            commands::tasks_get,
            commands::tasks_completions,
            commands::tasks_create,
            commands::tasks_update,
            commands::tasks_delete,
            commands::app_status,
            commands::app_sign_in,
            commands::app_sign_out,
            commands::app_sync_now,
            commands::app_force_sync,
            commands::app_take_open_task,
            commands::app_ask_notification_permission,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Tasks app");
}
