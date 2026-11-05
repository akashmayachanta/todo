//! The desktop's extra windows and the taskbar count.
//!
//! - The quick add popup (`quick`): frameless, always on top and kept loaded, so
//!   Alt+N shows it at once without bringing up the main window.
//! - Pins (`pin-<kind>[-<id>]`): small always-on-top windows showing one list.
//!   They are remembered in config.json. Closing one unpins it; quitting the app
//!   doesn't, because a quit doesn't send the close request.
//! - The taskbar count: the number is drawn into the taskbar icon itself, because
//!   Windows only ever puts an overlay icon at the bottom right.
//!
//! Each window loads a route under /widget (app/src/routes/widget). Tauri deadlocks
//! creating a window from a sync command on Windows, so commands that do are async.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

use crate::state::{lock, AppConfig, AppState, Pin};

const QUICK: &str = "quick";
/// Tells the popup it was opened, so it starts empty with its line focused.
const EVENT_QUICK_OPEN: &str = "tasks://quick-open";

/// What the window-state plugin remembers for a pin. The main window and the popup aren't tracked.
pub const PIN_STATE: StateFlags = StateFlags::POSITION.union(StateFlags::SIZE);
pub const UNTRACKED_WINDOWS: &[&str] = &["main", QUICK];

/// Loads the popup, and reopens the saved pins.
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, QUICK, WebviewUrl::App("widget/quick".into()))
        .title("Add task")
        .inner_size(560.0, 360.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .visible(false)
        .build()?;
    let hide = window.clone();
    window.on_window_event(move |event| {
        match event {
            WindowEvent::Focused(false) => {
                let _ = hide.hide();
            }
            // It's kept for next time; Alt+F4 only hides it.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = hide.hide();
            }
            _ => {}
        }
    });
    reopen_pins(app);
    Ok(())
}

fn signed_in_and_synced(app: &AppHandle) -> bool {
    let Some(st) = app.try_state::<Arc<AppState>>() else {
        return false;
    };
    let has_server = lock(&st.config).server.is_some();
    has_server && st.token().is_some() && crate::syncer::is_ready(&st)
}

/// Alt+N and the tray's Quick add. Only the popup comes up, unless the main window
/// is in front (its own dialog knows which project you're in) or the app can't add
/// a task yet (the main window shows why).
pub fn quick_add(app: &AppHandle) {
    if !signed_in_and_synced(app) {
        crate::show_main(app);
        return;
    }
    if let Some(main) = app.get_webview_window("main") {
        if main.is_visible().unwrap_or(false) && main.is_focused().unwrap_or(false) {
            let _ = app.emit(crate::EVENT_QUICK_ADD, ());
            return;
        }
    }
    let Some(window) = app.get_webview_window(QUICK) else {
        crate::show_main(app);
        return;
    };
    // Pressed again while it's open: keep what has been typed.
    if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
        return;
    }
    center_on_cursor_monitor(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit(EVENT_QUICK_OPEN, ());
}

/// On the screen you're working on, not always the main one.
fn center_on_cursor_monitor(app: &AppHandle, window: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|at| window.monitor_from_point(at.x, at.y).ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else {
        let _ = window.center();
        return;
    };
    let (origin, screen) = (monitor.position(), monitor.size());
    let _ = window.set_position(PhysicalPosition::new(
        origin.x + (screen.width as i32 - size.width as i32) / 2,
        origin.y + (screen.height as i32 - size.height as i32) / 2,
    ));
}

// ── Pins ─────────────────────────────────────────────────────────────────────

fn open_pin_window(app: &AppHandle, pin: &Pin, focus: bool) -> tauri::Result<()> {
    let label = pin.label();
    if let Some(window) = app.get_webview_window(&label) {
        window.show()?;
        return window.set_focus();
    }
    // The page reads which list it shows from the window's label.
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("widget/pin".into()))
        .title("Tasks")
        .inner_size(320.0, 440.0)
        .min_inner_size(240.0, 180.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .maximizable(false)
        .focused(focus)
        .build()?;
    let handle = app.clone();
    let closing = pin.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { .. } = event {
            forget_pin(&handle, &closing);
        }
    });
    Ok(())
}

fn forget_pin(app: &AppHandle, pin: &Pin) {
    let Some(st) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let mut config: AppConfig = lock(&st.config).clone();
    config.pins.retain(|p| p != pin);
    if let Err(e) = st.save_config(config) {
        eprintln!("Can't save the pins: {e}");
    }
}

/// Opens a pin, or brings it to the front if it's already open.
pub fn pin_open(app: &AppHandle, pin: Pin) -> Result<(), String> {
    if !pin.is_valid() {
        return Err(format!("Can't pin '{}'", pin.label()));
    }
    let st = app.state::<Arc<AppState>>();
    let mut config: AppConfig = lock(&st.config).clone();
    if !config.pins.contains(&pin) {
        config.pins.push(pin.clone());
        st.save_config(config)?;
    }
    open_pin_window(app, &pin, true).map_err(|e| e.to_string())
}

/// Opens the saved pins, if the app is signed in. They don't take the focus.
pub fn reopen_pins(app: &AppHandle) {
    let Some(st) = app.try_state::<Arc<AppState>>() else {
        return;
    };
    let config = lock(&st.config).clone();
    if config.server.is_none() || st.token().is_none() {
        return;
    }
    for pin in config.pins.iter().filter(|p| p.is_valid()) {
        if let Err(e) = open_pin_window(app, pin, false) {
            eprintln!("Can't open the pin {}: {e}", pin.label());
        }
    }
}

/// Signing out: the windows go, but the pins stay saved for the next sign-in.
/// `destroy` skips the close request, which would unpin them.
pub fn close_windows(app: &AppHandle) {
    if let Err(e) = app.save_window_state(PIN_STATE) {
        eprintln!("Can't save where the pins are: {e}");
    }
    for (label, window) in app.webview_windows() {
        if label.starts_with("pin-") {
            let _ = window.destroy();
        } else if label == QUICK {
            let _ = window.hide();
        }
    }
}

// ── The taskbar count ────────────────────────────────────────────────────────

/// Sets the main window's big icon, which the taskbar shows, from RGBA pixels the
/// page drew (app/src/app/taskbar.ts). None goes back to the app's own icon.
#[cfg(windows)]
pub fn set_taskbar_icon(app: &AppHandle, size: u32, rgba: Option<Vec<u8>>) -> Result<(), String> {
    use std::sync::Mutex;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{CreateIcon, DestroyIcon, SendMessageW, HICON, ICON_BIG, WM_SETICON};

    /// The icon set last, destroyed once it's replaced.
    static CURRENT: Mutex<isize> = Mutex::new(0);

    let window = app.get_webview_window("main").ok_or("The main window is gone")?;
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);

    let icon = match rgba {
        None => HICON::default(),
        Some(mut pixels) => {
            if pixels.len() != (size * size * 4) as usize {
                return Err(format!("Expected {size}x{size} RGBA pixels"));
            }
            // CreateIcon takes BGRA, and an AND mask that is the inverted alpha.
            let mut mask = Vec::with_capacity(pixels.len() / 4);
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
                mask.push(px[3].wrapping_sub(u8::MAX));
            }
            unsafe { CreateIcon(None, size as i32, size as i32, 1, 32, mask.as_ptr(), pixels.as_ptr()) }
                .map_err(|e| e.to_string())?
        }
    };
    unsafe {
        SendMessageW(hwnd, WM_SETICON, Some(WPARAM(ICON_BIG as usize)), Some(LPARAM(icon.0 as isize)));
    }
    let previous = std::mem::replace(&mut *lock(&CURRENT), icon.0 as isize);
    if previous != 0 {
        unsafe {
            let _ = DestroyIcon(HICON(previous as _));
        }
    }
    Ok(())
}
