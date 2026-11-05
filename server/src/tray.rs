//! The Windows program: the server as an icon in the notification area, with no
//! console window. Hover over it for the address to reach it on, and right-click
//! it for "Start with Windows" and "Exit".
//!
//! The log goes to tasks-server.log in the data folder.

use std::fs::{self, OpenOptions};
use std::net::{IpAddr, UdpSocket};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::oneshot;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetSystemMetrics, MessageBoxW, TranslateMessage, MB_ICONERROR, MB_OK, MSG, SM_CXSMICON,
};

use crate::config::Config;
use crate::{log_filter, Server};

/// The icon's id in resources.rc.
const ICON_RESOURCE: u16 = 1;
/// A log bigger than this starts over the next time the server starts.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

pub fn main() {
    if let Err(message) = run() {
        tracing::error!("{message}");
        message_box(&message);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Could not find this program's path: {e}"))?;
    // Windows starts this from an unrelated folder (at sign-in) or from
    // target\release (a double-click). Without this it would miss the .env and
    // quietly start on a new, empty data folder.
    if let Some(root) = app_root(&exe) {
        let _ = std::env::set_current_dir(root);
    }
    let config = Config::load();
    init_logging(&config);
    let (host, port) = (config.host.clone(), config.port);

    let runtime = tokio::runtime::Runtime::new().map_err(|e| format!("Could not start the server: {e}"))?;
    let server = runtime.block_on(Server::bind(config))?;
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = runtime.spawn(async move {
        let shutdown = async move {
            let _ = stopped.await;
        };
        if let Err(message) = server.run(shutdown).await {
            tracing::error!("{message}");
            message_box(&message);
            std::process::exit(1);
        }
    });

    let startup_command = format!("\"{}\"", exe.display());
    let startup_item = CheckMenuItem::new("Start with Windows", true, startup::is_enabled(&startup_command), None);
    let exit_item = MenuItem::new("Exit", true, None);
    let menu = Menu::new();
    menu.append_items(&[&startup_item, &PredefinedMenuItem::separator(), &exit_item])
        .map_err(|e| format!("Could not build the tray menu: {e}"))?;

    // The small icon size follows the display scale.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) } as u32;
    let icon = Icon::from_resource(ICON_RESOURCE, Some((size, size))).map_err(|e| format!("Could not load the tray icon: {e}"))?;
    let mut shown_tooltip = tooltip(&host, port);
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(&shown_tooltip)
        .with_icon(icon)
        .build()
        .map_err(|e| format!("Could not add the tray icon: {e}"))?;

    // The tray icon and its menu need a message loop on this thread. A menu
    // click is handled while its message is dispatched, so its event is ready
    // as soon as DispatchMessageW returns.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    'messages: while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id() == exit_item.id() {
                break 'messages;
            }
            if event.id() == startup_item.id() {
                // The click has already flipped the checkmark; show what the registry really says.
                if let Err(e) = startup::set(startup_item.is_checked(), &startup_command) {
                    message_box(&format!("Could not change the startup setting: {e}"));
                }
                startup_item.set_checked(startup::is_enabled(&startup_command));
            }
        }
        // The IP can change while this runs, and at sign-in the network may not be up yet.
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(event, TrayIconEvent::Enter { .. }) {
                let current = tooltip(&host, port);
                if current != shown_tooltip {
                    let _ = tray.set_tooltip(Some(&current));
                    shown_tooltip = current;
                }
            }
        }
    }

    drop(tray);
    tracing::info!("exiting");
    let _ = stop.send(());
    let _ = runtime.block_on(tokio::time::timeout(Duration::from_secs(3), serving));
    runtime.shutdown_timeout(Duration::from_secs(1));
    Ok(())
}

/// The icon's hover text. A server listening on every address shows this PC's IP.
fn tooltip(host: &str, port: u16) -> String {
    let host = match host {
        "0.0.0.0" | "::" => local_ip().map_or_else(|| "localhost".to_string(), |ip| ip.to_string()),
        specific => specific.to_string(),
    };
    format!("Tasks server\n{host}:{port}")
}

/// The IP of the network adapter this PC uses to reach other networks. Connecting
/// a UDP socket only picks the route; nothing is sent.
fn local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// The folder to work in: the exe's folder, or the repo root for a build in
/// target\release or target\debug.
fn app_root(exe: &Path) -> Option<&Path> {
    let dir = exe.parent()?;
    let in_cargo_target = matches!(dir.file_name()?.to_str(), Some("release" | "debug"))
        && dir.parent().and_then(Path::file_name).is_some_and(|name| name == "target");
    if in_cargo_target {
        dir.parent()?.parent()
    } else {
        Some(dir)
    }
}

/// There's no console to log to, so the log goes to a file in the data folder.
fn init_logging(config: &Config) {
    let path = config.data_dir.join("tasks-server.log");
    let _ = fs::create_dir_all(&config.data_dir);
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = fs::remove_file(&path);
    }
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
        tracing_subscriber::fmt().with_env_filter(log_filter()).with_ansi(false).with_writer(Mutex::new(file)).init();
    }
}

fn message_box(text: &str) {
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    unsafe {
        MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("Tasks server").as_ptr(), MB_OK | MB_ICONERROR);
    }
}

/// The per-user Run key, the same list Task Manager's Startup tab shows.
mod startup {
    use std::io;

    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Where Task Manager records that it turned an entry off (odd first byte).
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
    const VALUE: &str = "TasksServer";

    /// Whether this exe, at its current path, will start at sign-in.
    pub fn is_enabled(command: &str) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let registered =
            hkcu.open_subkey(RUN).and_then(|k| k.get_value::<String, _>(VALUE)).is_ok_and(|v| v.eq_ignore_ascii_case(command));
        let turned_off = hkcu
            .open_subkey(APPROVED)
            .and_then(|k| k.get_raw_value(VALUE))
            .is_ok_and(|v| v.bytes.first().is_some_and(|b| b & 1 == 1));
        registered && !turned_off
    }

    pub fn set(enabled: bool, command: &str) -> io::Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (run, _) = hkcu.create_subkey(RUN)?;
        if enabled {
            run.set_value(VALUE, &command.to_owned())?;
            // Clears an earlier "disabled" from Task Manager, which would otherwise still win.
            if let Ok(approved) = hkcu.open_subkey_with_flags(APPROVED, KEY_SET_VALUE) {
                let _ = approved.delete_value(VALUE);
            }
            return Ok(());
        }
        match run.delete_value(VALUE) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::app_root;
    use std::path::Path;

    #[test]
    fn app_root_is_the_repo_for_a_cargo_build_and_the_exe_folder_otherwise() {
        assert_eq!(app_root(Path::new("C:/repo/target/release/s.exe")), Some(Path::new("C:/repo")));
        assert_eq!(app_root(Path::new("C:/repo/target/debug/s.exe")), Some(Path::new("C:/repo")));
        assert_eq!(app_root(Path::new("C:/apps/tasks/s.exe")), Some(Path::new("C:/apps/tasks")));
        assert_eq!(app_root(Path::new("C:/apps/release/s.exe")), Some(Path::new("C:/apps/release")));
    }
}
