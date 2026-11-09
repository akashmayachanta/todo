//! Tauri commands. The tasks_* ones match the tasks-server REST routes one for
//! one and run the same tasks-core rules on the local DB; the page reaches
//! them through its TasksClient (app/src/app/tauriClient.ts).
//!
//! Errors come back as "<status>: {\"detail\": ...}", the server's shape, so
//! $lib/api's errorDetail() reads both.

use std::sync::Arc;

use rusqlite::Connection;
use serde::Serialize;
use tasks_core::client::{diagnose, Client, ClientError, ServerInfo};
use tasks_core::{
    ops, Category, CategoryIn, Completions, Ctx, Filter, FilterIn, Project, ProjectIn, Task, TaskIn, TaskPatch, TaskQuery,
};
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::state::{lock, AppConfig, AppState, Pin, SyncStatus, FIRST_SYNC_KEY};

type St<'a> = State<'a, Arc<AppState>>;
type Res<T> = Result<T, String>;

fn fail(status: u16, detail: impl Into<String>) -> String {
    format!("{status}: {}", serde_json::json!({ "detail": detail.into() }))
}

fn core_err(e: tasks_core::Error) -> String {
    fail(e.status(), e.to_string())
}

/// Runs f in one transaction on the local DB; a write wakes the sync loop.
pub(crate) fn db<T>(st: &AppState, write: bool, f: impl FnOnce(&Connection, &Ctx) -> tasks_core::Result<T>) -> Res<T> {
    let out = {
        let conn = lock(&st.db);
        let tx = conn.unchecked_transaction().map_err(|e| core_err(e.into()))?;
        let out = f(&tx, &st.ctx).map_err(core_err)?;
        tx.commit().map_err(|e| core_err(e.into()))?;
        out
    };
    if write {
        st.changed();
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct Ok {
    ok: bool,
}

const OK: Ok = Ok { ok: true };

// ── Projects ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn tasks_projects_list(st: St) -> Res<Vec<Project>> {
    db(&st, false, |c, _| ops::list_projects(c))
}

#[tauri::command]
pub fn tasks_projects_create(st: St, data: ProjectIn) -> Res<Project> {
    db(&st, true, |c, x| ops::create_project(c, x, data))
}

#[tauri::command]
pub fn tasks_projects_update(st: St, id: String, data: ProjectIn) -> Res<Project> {
    db(&st, true, |c, x| ops::update_project(c, x, &id, data))
}

#[tauri::command]
pub fn tasks_projects_delete(st: St, id: String) -> Res<Ok> {
    db(&st, true, |c, x| ops::delete_project(c, x, &id)).map(|_| OK)
}

#[tauri::command]
pub fn tasks_projects_reorder(st: St, ids: Vec<String>) -> Res<Vec<Project>> {
    db(&st, true, |c, x| ops::reorder_projects(c, x, &ids))
}

// ── Categories ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn tasks_categories_list(st: St, project: Option<String>) -> Res<Vec<Category>> {
    db(&st, false, |c, _| ops::list_categories(c, project.as_deref()))
}

#[tauri::command]
pub fn tasks_categories_create(st: St, data: CategoryIn) -> Res<Category> {
    db(&st, true, |c, x| ops::create_category(c, x, data))
}

#[tauri::command]
pub fn tasks_categories_update(st: St, id: String, data: CategoryIn) -> Res<Category> {
    db(&st, true, |c, x| ops::update_category(c, x, &id, data))
}

#[tauri::command]
pub fn tasks_categories_delete(st: St, id: String) -> Res<Ok> {
    db(&st, true, |c, x| ops::delete_category(c, x, &id)).map(|_| OK)
}

// ── Filters ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn tasks_filters_list(st: St) -> Res<Vec<Filter>> {
    db(&st, false, |c, _| ops::list_filters(c))
}

#[tauri::command]
pub fn tasks_filters_create(st: St, data: FilterIn) -> Res<Filter> {
    db(&st, true, |c, x| ops::create_filter(c, x, data))
}

#[tauri::command]
pub fn tasks_filters_update(st: St, id: String, data: FilterIn) -> Res<Filter> {
    db(&st, true, |c, x| ops::update_filter(c, x, &id, data))
}

#[tauri::command]
pub fn tasks_filters_delete(st: St, id: String) -> Res<Ok> {
    db(&st, true, |c, x| ops::delete_filter(c, x, &id)).map(|_| OK)
}

// ── Tasks ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn tasks_list(st: St, opts: Option<TaskQuery>) -> Res<Vec<Task>> {
    let opts = opts.unwrap_or_default();
    db(&st, false, |c, _| ops::list_tasks(c, &opts))
}

#[tauri::command]
pub fn tasks_get(st: St, id: String) -> Res<Task> {
    db(&st, false, |c, _| ops::get_task(c, &id))
}

#[tauri::command]
pub fn tasks_completions(st: St, since: String) -> Res<Completions> {
    db(&st, false, |c, _| ops::list_completions(c, &since))
}

#[tauri::command]
pub fn tasks_create(st: St, data: TaskIn) -> Res<Task> {
    db(&st, true, |c, x| ops::create_task(c, x, data))
}

#[tauri::command]
pub fn tasks_update(st: St, id: String, patch: TaskPatch) -> Res<Task> {
    db(&st, true, |c, x| ops::update_task(c, x, &id, patch))
}

#[tauri::command]
pub fn tasks_delete(st: St, id: String) -> Res<Ok> {
    db(&st, true, |c, x| ops::delete_task(c, x, &id)).map(|_| OK)
}

#[tauri::command]
pub fn tasks_reorder(st: St, ids: Vec<String>) -> Res<Vec<Task>> {
    db(&st, true, |c, x| ops::reorder_tasks(c, x, &ids))
}

// ── The app: setup, sign-in, sync ────────────────────────────────────────────

#[derive(Serialize)]
pub struct AppStatus {
    /// A server is set and this device has a token.
    configured: bool,
    server: Option<ServerInfo>,
    username: Option<String>,
    sync: SyncStatus,
    platform: &'static str,
}

fn status_of(st: &AppState) -> AppStatus {
    let config = lock(&st.config).clone();
    let mut sync = lock(&st.status).clone();
    sync.ready = crate::syncer::is_ready(st);
    AppStatus {
        configured: config.server.is_some() && st.token().is_some(),
        server: config.server,
        username: config.username,
        sync,
        platform: if cfg!(target_os = "android") { "android" } else { std::env::consts::OS },
    }
}

#[tauri::command]
pub fn app_status(st: St) -> AppStatus {
    status_of(&st)
}

fn client_err(e: ClientError) -> String {
    match e {
        ClientError::Offline(_) | ClientError::Unreachable(_) => {
            fail(503, "This device is offline. Connect to the internet to sign in and get your tasks.")
        }
        ClientError::ServerDown(m) => {
            fail(503, format!("Can't reach the Tasks server: {m}. Check the address, and that the server is running."))
        }
        ClientError::Unauthorized(m) => fail(401, if m.is_empty() { "Wrong username or password".into() } else { m }),
        ClientError::Server { status, detail } => fail(status, detail),
        ClientError::Core(e) => core_err(e),
    }
}

fn clean_server(mut server: ServerInfo) -> Res<ServerInfo> {
    server.url = server.url.trim().trim_end_matches('/').to_string();
    if !(server.url.starts_with("https://") || server.url.starts_with("http://")) {
        return Err(fail(422, "The server address starts with https:// (or http:// on your own network)"));
    }
    Ok(server)
}

fn device_name() -> String {
    if cfg!(target_os = "android") {
        "Android phone".into()
    } else {
        std::env::var("COMPUTERNAME").map(|n| format!("Windows ({n})")).unwrap_or_else(|_| "Windows".into())
    }
}

/// Keeps the local DB to one account: signing in as someone else clears it.
fn adopt_user(st: &AppState, user_id: i64) -> Res<()> {
    let conn = lock(&st.db);
    let previous = tasks_core::db::get_state(&conn, "user_id").map_err(core_err)?;
    if previous.as_deref().is_some_and(|p| p != user_id.to_string()) {
        conn.execute_batch(
            "DELETE FROM task_completions; DELETE FROM tasks; DELETE FROM task_filters;
             DELETE FROM task_categories; DELETE FROM task_projects;
             DELETE FROM sync_state WHERE key IN ('last_seq', 'reminders_fired', 'server_epoch');",
        )
        .map_err(|e| core_err(e.into()))?;
        conn.execute("DELETE FROM sync_state WHERE key = ?1", [FIRST_SYNC_KEY]).map_err(|e| core_err(e.into()))?;
    }
    tasks_core::db::set_state(&conn, "user_id", &user_id.to_string()).map_err(core_err)
}

async fn finish_sign_in(app: &AppHandle, st: &Arc<AppState>, server: ServerInfo, username: Option<String>, token: String, user_id: i64) -> Res<AppStatus> {
    // No sync round runs while the account changes under it.
    let _no_sync = st.sync_lock.lock().await;
    // The token first: if it can't be saved, nothing local is wiped.
    st.set_token(&token).map_err(|e| fail(500, e))?;
    adopt_user(st, user_id)?;
    // Pins point at this account's lists, so another account starts without them.
    let previous = lock(&st.config).clone();
    let pins = if previous.user_id == Some(user_id) { previous.pins } else { Vec::new() };
    st.save_config(AppConfig { server: Some(server), username, user_id: Some(user_id), pins }).map_err(|e| fail(500, e))?;
    #[cfg(desktop)]
    {
        crate::widgets::close_windows(app);
        crate::widgets::reopen_pins(app);
    }
    // The page may still show another account's tasks.
    let _ = app.emit(crate::syncer::EVENT_CHANGED, 0);
    crate::syncer::set_status(app, st, SyncStatus { state: "syncing", ..Default::default() });
    st.wake.notify_one();
    Ok(status_of(st))
}

/// Signs this device in to the tasks server with a username and password, or
/// with `register`, makes a new account first (the Register button).
#[tauri::command]
pub async fn app_sign_in(
    app: AppHandle,
    st: St<'_>,
    server: ServerInfo,
    username: String,
    password: String,
    register: bool,
) -> Res<AppStatus> {
    let server = clean_server(server)?;
    let client = Client::new(server.clone());
    let username = username.trim();
    let result = if register {
        client.register(username, &password, &device_name()).await
    } else {
        client.login(username, &password, &device_name()).await
    };
    let device = match result {
        Ok(device) => device,
        Err(e) => return Err(client_err(diagnose(e).await)),
    };
    let st = st.inner().clone();
    finish_sign_in(&app, &st, server, Some(device.username), device.token, device.user_id).await
}

/// Forgets the token; the local tasks stay.
#[tauri::command]
pub fn app_sign_out(app: AppHandle, st: St) -> AppStatus {
    st.clear_token();
    #[cfg(desktop)]
    crate::widgets::close_windows(&app);
    let pending = tasks_core::sync::pending_count(&lock(&st.db)).unwrap_or(0);
    crate::syncer::set_status(&app, &st, SyncStatus { state: "signed_out", pending, ..Default::default() });
    status_of(&st)
}

#[tauri::command]
pub fn app_sync_now(st: St) {
    st.wake.notify_one();
}

/// Sync right away, pull everything from the server again, and reload the page.
#[tauri::command]
pub fn app_force_sync(st: St) {
    st.force_sync.store(true, std::sync::atomic::Ordering::Relaxed);
    st.wake.notify_one();
}

/// The task whose notification was tapped, once.
#[tauri::command]
pub fn app_take_open_task(st: St) -> Option<String> {
    lock(&st.open_task).take()
}

/// Opens a task in the main window, from a pin.
#[tauri::command]
pub fn app_show_task(app: AppHandle, id: String) {
    #[cfg(desktop)]
    crate::show_main(&app);
    let _ = app.emit(crate::EVENT_OPEN_TASK, id);
}

// ── The desktop's extra windows; none of them exist on Android ───────────────

/// Pins a list or a task to the desktop. Async: see widgets.rs.
#[tauri::command]
pub async fn app_pin_open(app: AppHandle, kind: String, id: Option<String>) -> Res<()> {
    #[cfg(desktop)]
    {
        crate::widgets::pin_open(&app, Pin { kind, id }).map_err(|e| fail(422, e))
    }
    #[cfg(mobile)]
    {
        let _ = (app, Pin { kind, id });
        Err(fail(400, "Pins are only on the desktop"))
    }
}

/// Closes the calling pin window, which unpins it.
#[tauri::command]
pub fn app_pin_close(window: WebviewWindow) {
    #[cfg(desktop)]
    let _ = window.close();
    #[cfg(mobile)]
    let _ = window;
}

/// Hides the quick add popup; it stays loaded for next time.
#[tauri::command]
pub fn app_quick_hide(window: WebviewWindow) {
    #[cfg(desktop)]
    let _ = window.hide();
    #[cfg(mobile)]
    let _ = window;
}

/// Today's count as a taskbar badge, in `size`² RGBA pixels; none removes the badge.
#[tauri::command]
pub fn app_set_taskbar_badge(app: AppHandle, size: u32, rgba: Option<Vec<u8>>) -> Res<()> {
    #[cfg(windows)]
    {
        crate::widgets::set_taskbar_badge(&app, size, rgba).map_err(|e| fail(500, e))
    }
    #[cfg(not(windows))]
    {
        let _ = (app, size, rgba);
        Result::Ok(())
    }
}

#[tauri::command]
pub fn app_ask_notification_permission(app: AppHandle) {
    use tauri_plugin_notification::{NotificationExt, PermissionState};
    let n = app.notification();
    if !matches!(n.permission_state(), Ok(PermissionState::Granted)) {
        let _ = n.request_permission();
    }
}

/// Whether Android will deliver reminders on time: `{notifications, exactAlarms,
/// batteryUnrestricted}`. None off Android.
#[tauri::command]
pub async fn app_reminder_health(app: AppHandle) -> Res<Option<serde_json::Value>> {
    #[cfg(target_os = "android")]
    {
        crate::android::health(&app).await.map(Some).map_err(|e| fail(500, e))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Result::Ok(None)
    }
}

/// Opens the Android setting that fixes `kind`: notifications, exactAlarms or battery.
#[tauri::command]
pub async fn app_open_reminder_setting(app: AppHandle, kind: String) -> Res<()> {
    #[cfg(target_os = "android")]
    {
        crate::android::open_setting(&app, kind).await.map_err(|e| fail(500, e))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, kind);
        Result::Ok(())
    }
}
