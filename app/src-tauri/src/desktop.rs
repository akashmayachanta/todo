//! Settings → Desktop: the global shortcuts (quick add, show / hide the window),
//! launch at startup and start minimized.
//!
//! The shortcuts are kept in config.json (`AppConfig::desktop`) and registered
//! with the global-shortcut plugin at startup and on every change; one handler
//! tells them apart by comparing with the saved keys. Launch at startup is the
//! autostart plugin's registry entry, which the tray's "Start at login" also flips.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, MenuItem};
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{Builder, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::{lock, AppState, DesktopPrefs};

/// What Settings → Desktop shows and saves.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesktopSettings {
    pub quick_add: Option<String>,
    pub show_hide: Option<String>,
    pub launch_at_startup: bool,
    pub start_minimized: bool,
}

/// The tray's items that show a setting, so a change in Settings reaches them.
pub struct TrayItems {
    pub add: MenuItem<Wry>,
    pub autostart: CheckMenuItem<Wry>,
}

/// The plugin, with the one handler for every shortcut registered later.
pub fn plugin() -> TauriPlugin<Wry> {
    Builder::new()
        .with_handler(|app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let prefs = prefs(app);
            if is(&prefs.quick_add, shortcut) {
                crate::widgets::quick_add(app);
            } else if is(&prefs.show_hide, shortcut) {
                toggle_main(app);
            }
        })
        .build()
}

fn prefs(app: &AppHandle) -> DesktopPrefs {
    app.try_state::<Arc<AppState>>().map(|st| lock(&st.config).desktop.clone()).unwrap_or_default()
}

fn parse(keys: &str) -> Option<Shortcut> {
    keys.parse::<Shortcut>().ok()
}

fn is(keys: &Option<String>, pressed: &Shortcut) -> bool {
    keys.as_deref().and_then(parse).is_some_and(|k| &k == pressed)
}

/// Shows the main window, or hides it when it is already in front.
fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(false) {
            let _ = w.hide();
            return;
        }
    }
    crate::show_main(app);
}

/// Registers the saved shortcuts; at startup a failure is only logged.
pub fn register_saved(app: &AppHandle) {
    if let Err(e) = register(app, &prefs(app)) {
        eprintln!("{e}");
    }
}

/// Swaps the registered shortcuts for `prefs`'. Fails, registering none of them,
/// when one isn't understood, both are the same, or another app holds one.
fn register(app: &AppHandle, prefs: &DesktopPrefs) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let wanted = [("Quick add", &prefs.quick_add), ("Show / hide", &prefs.show_hide)];
    let mut done: Vec<Shortcut> = Vec::new();
    for (name, keys) in wanted {
        let Some(keys) = keys else { continue };
        let shortcut = parse(keys).ok_or_else(|| format!("{name}: {} isn't a shortcut", display(keys)))?;
        if done.contains(&shortcut) {
            let _ = gs.unregister_all();
            return Err("Quick add and Show / hide need different shortcuts".into());
        }
        if let Err(e) = gs.register(shortcut) {
            let _ = gs.unregister_all();
            return Err(format!("{name}: {} can't be used, another app may have it ({e})", display(keys)));
        }
        done.push(shortcut);
    }
    Ok(())
}

/// "Super+Alt+S" as Windows names it: "Win+Alt+S".
fn display(keys: &str) -> String {
    keys.replace("Super", "Win").replace("Control", "Ctrl")
}

pub fn get(app: &AppHandle) -> DesktopSettings {
    let prefs = prefs(app);
    DesktopSettings {
        quick_add: prefs.quick_add,
        show_hide: prefs.show_hide,
        launch_at_startup: app.autolaunch().is_enabled().unwrap_or(false),
        start_minimized: prefs.start_minimized,
    }
}

pub fn set(app: &AppHandle, st: &AppState, data: DesktopSettings) -> Result<DesktopSettings, String> {
    let blank = |k: Option<String>| k.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    let next = DesktopPrefs {
        quick_add: blank(data.quick_add),
        show_hide: blank(data.show_hide),
        start_minimized: data.start_minimized,
    };
    let before = lock(&st.config).desktop.clone();
    if next.quick_add != before.quick_add || next.show_hide != before.show_hide {
        if let Err(e) = register(app, &next) {
            let _ = register(app, &before);
            return Err(e);
        }
    }

    let autolaunch = app.autolaunch();
    if autolaunch.is_enabled().unwrap_or(false) != data.launch_at_startup {
        let done = if data.launch_at_startup { autolaunch.enable() } else { autolaunch.disable() };
        done.map_err(|e| format!("Can't change launch at startup: {e}"))?;
    }

    let mut config = lock(&st.config).clone();
    config.desktop = next;
    st.save_config(config)?;
    refresh_tray(app);
    Ok(get(app))
}

/// The tray's Quick add shows its shortcut, and Start at login its state.
pub fn refresh_tray(app: &AppHandle) {
    let Some(items) = app.try_state::<TrayItems>() else { return };
    let label = match prefs(app).quick_add {
        Some(keys) => format!("Quick add\t{}", display(&keys)),
        None => "Quick add".into(),
    };
    let _ = items.add.set_text(label);
    let _ = items.autostart.set_checked(app.autolaunch().is_enabled().unwrap_or(false));
}
