//! App updates from the Tasks server (server/src/releases.rs). When the app
//! opens, the page asks `app_update_check`; if the server has a newer build, it
//! asks the user, and `app_update_install` downloads it and puts it in place.
//!
//! - Windows: the running exe can't be overwritten but can be renamed, so it
//!   becomes tasks-app.exe.old, the download takes its name, and the app starts
//!   the new one and exits. The .old file goes on the next start.
//! - Android: the APK goes to the app's cache and Android's installer opens on
//!   it (TasksPlugin.kt), where the user taps Install.
//!
//! A build is newer when its version is higher. Each build gets a new version
//! (scripts/bump-version.mjs), and on Windows a different hash with the same
//! version counts as newer too.

use std::sync::Arc;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tasks_core::client::{Client, Release};
use tauri::{AppHandle, State};

use crate::state::{lock, AppState};

type St<'a> = State<'a, Arc<AppState>>;

#[cfg(target_os = "android")]
const PLATFORM: &str = "android";
#[cfg(not(target_os = "android"))]
const PLATFORM: &str = "windows";

#[derive(Serialize)]
pub struct UpdateInfo {
    /// This build's version.
    current: String,
    /// The server's build.
    version: String,
    size: u64,
}

fn client(st: &AppState) -> Result<(Client, String), String> {
    let server = lock(&st.config).server.clone().ok_or("This device isn't set up with a server")?;
    let token = st.token().ok_or("This device is signed out")?;
    Ok((Client::new(server), token))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// "1.2.3" as numbers, so 0.1.10 is above 0.1.9. Anything else counts as 0.
fn parse_version(v: &str) -> Vec<u64> {
    v.trim().split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// The hash of the running exe, worked out once: it can't change under a running app.
#[cfg(not(target_os = "android"))]
fn own_hash() -> Option<String> {
    static HASH: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let bytes = std::fs::read(std::env::current_exe().ok()?).ok()?;
        Some(hex(&Sha256::digest(&bytes)))
    })
    .clone()
}

fn is_newer(current: &str, release: &Release) -> bool {
    let (mine, theirs) = (parse_version(current), parse_version(&release.version));
    if theirs != mine {
        return theirs > mine;
    }
    // The same version: a different build only shows on Windows, where the exe can be hashed.
    #[cfg(not(target_os = "android"))]
    if let Some(hash) = own_hash() {
        return hash != release.sha256;
    }
    false
}

/// The server's build, when it is newer than this one. Errors (offline, an
/// older server without updates) are None too: there's nothing to offer.
#[tauri::command]
pub async fn app_update_check(app: AppHandle, st: St<'_>) -> Result<Option<UpdateInfo>, String> {
    let Ok((client, token)) = client(&st) else { return Ok(None) };
    let Ok(Some(release)) = client.latest_release(&token, PLATFORM).await else { return Ok(None) };
    let current = app.package_info().version.to_string();
    let newer = tauri::async_runtime::spawn_blocking({
        let (current, release) = (current.clone(), release.clone());
        move || is_newer(&current, &release)
    })
    .await
    .unwrap_or(false);
    Ok(newer.then(|| UpdateInfo { current, version: release.version, size: release.size }))
}

/// Downloads the server's build, checks its hash and installs it. On Windows
/// the app restarts into it; on Android the system installer opens.
#[tauri::command]
pub async fn app_update_install(app: AppHandle, st: St<'_>) -> Result<(), String> {
    let (client, token) = client(&st)?;
    let release = client
        .latest_release(&token, PLATFORM)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("The server has no update right now")?;
    let bytes = client.download_release(&token, PLATFORM).await.map_err(|e| format!("The download failed: {e}"))?;
    // The server's copy can change mid-download (copy-release.bat); then try again later.
    if hex(&Sha256::digest(&bytes)) != release.sha256 {
        return Err("The download doesn't match the server's build. Try again in a minute.".into());
    }
    install(&app, &st, bytes).await
}

#[cfg(windows)]
fn old_path(exe: &std::path::Path) -> std::path::PathBuf {
    let mut name = exe.as_os_str().to_owned();
    name.push(".old");
    name.into()
}

/// Removes the exe the last update left behind. Called on start.
#[cfg(windows)]
pub fn clean_up() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
    }
}

#[cfg(windows)]
async fn install(app: &AppHandle, _st: &AppState, bytes: Vec<u8>) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut new = exe.as_os_str().to_owned();
    new.push(".new");
    let new = std::path::PathBuf::from(new);
    let old = old_path(&exe);
    let folder = exe.parent().map(|p| p.display().to_string()).unwrap_or_default();
    let cant = |e: std::io::Error| format!("Can't replace the app in {folder}: {e}");

    std::fs::write(&new, &bytes).map_err(cant)?;
    let _ = std::fs::remove_file(&old);
    if let Err(e) = std::fs::rename(&exe, &old) {
        let _ = std::fs::remove_file(&new);
        return Err(cant(e));
    }
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(cant(e));
    }

    // Start the new exe once this one has gone: started sooner, it would find
    // this one still running and hand over to it (single instance).
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd.exe")
        .raw_arg(format!("/c ping -n 3 127.0.0.1 >nul & start \"\" \"{}\"", exe.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("The update is in place, but the app couldn't restart. Start it again yourself. ({e})"))?;
    app.exit(0);
    Ok(())
}

#[cfg(not(any(windows, target_os = "android")))]
async fn install(_app: &AppHandle, _st: &AppState, _bytes: Vec<u8>) -> Result<(), String> {
    Err("Updates only install on Windows and Android".into())
}

#[cfg(target_os = "android")]
async fn install(app: &AppHandle, _st: &AppState, bytes: Vec<u8>) -> Result<(), String> {
    use tauri::Manager;

    let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let apk = dir.join("tasks.apk");
    std::fs::write(&apk, &bytes).map_err(|e| format!("Can't save the update: {e}"))?;
    crate::android::install_apk(app, &apk).await
}
