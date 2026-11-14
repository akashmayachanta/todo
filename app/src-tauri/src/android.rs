//! Android's own reminders and background sync, with the Kotlin side in
//! app/android/java (copied into the project by scripts/android-native.mjs).
//!
//! - The app's plugin, "tasks-android" (TasksPlugin.kt), sets one exact alarm per
//!   upcoming reminder, reports whether Android will deliver them on time, and
//!   opens the settings that fix it.
//! - A WorkManager job (SyncWorker) calls `backgroundSync` every 15 minutes while
//!   the app is closed, so reminders set on another device still get scheduled.
//! - A tapped reminder calls `openTask`.
//! - The home screen widget (TasksWidget.kt) reads its lists through
//!   `widgetSources` and `widgetList` (home_widget.rs), ticks tasks with
//!   `widgetComplete`, and is told to reload by `refresh_widgets`.
//! - The widget's + and the app icon's "Add task" shortcut call `openQuickAdd`;
//!   the widget's title and the app icon's Search, Today and Inbox call `openView`.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use serde::Serialize;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::home_widget;
use crate::state::{lock, AppState};

/// The running app, once its state is ready. Unset when Android started the
/// process only for the background job.
static APP: OnceLock<AppHandle> = OnceLock::new();
/// A tapped reminder's task, when the tap started the app before its state was ready.
static PENDING_TASK: Mutex<Option<String>> = Mutex::new(None);
/// The quick add the widget or the app shortcut asked for (JSON defaults), until
/// the page takes it (app_take_quick_add).
static PENDING_QUICK_ADD: Mutex<Option<String>> = Mutex::new(None);
/// The view the widget or a shortcut asked for (the Tasks page's query string,
/// e.g. "view=today"), until the page takes it (app_take_open_view).
static PENDING_VIEW: Mutex<Option<String>> = Mutex::new(None);

struct Bridge(Arc<PluginHandle<Wry>>);

pub fn init() -> TauriPlugin<Wry> {
    Builder::<Wry>::new("tasks-android")
        .setup(|app, api| {
            let handle = api.register_android_plugin("dev.tasks.app", "TasksPlugin")?;
            app.manage(Bridge(Arc::new(handle)));
            Ok(())
        })
        .build()
}

/// Called once the app's state is managed.
pub fn set_app(app: &AppHandle, st: &AppState) {
    if let Some(task) = lock(&PENDING_TASK).take() {
        *lock(&st.open_task) = Some(task);
    }
    let _ = APP.set(app.clone());
}

fn bridge(app: &AppHandle) -> Option<Arc<PluginHandle<Wry>>> {
    app.try_state::<Bridge>().map(|b| b.0.clone())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleArgs {
    data_dir: String,
    reminders: String,
}

/// Hands the upcoming reminders (JSON, see reminders::upcoming_json) to Android.
/// Off the calling thread, and given up after a while: with no activity the call
/// can wait forever, and the background job schedules them anyway.
pub fn schedule(app: &AppHandle, dir: &Path, reminders: String) {
    let Some(handle) = bridge(app) else { return };
    let args = ScheduleArgs { data_dir: dir.to_string_lossy().into_owned(), reminders };
    tauri::async_runtime::spawn(async move {
        let call = handle.run_mobile_plugin_async::<serde_json::Value>("schedule", args);
        if let Ok(Err(e)) = tokio::time::timeout(Duration::from_secs(10), call).await {
            eprintln!("Can't schedule the reminders: {e}");
        }
    });
}

/// Tells the home screen widgets to load their lists again. A no-op when the app
/// isn't running in this process: whoever changed the DB then refreshes them (Kotlin).
pub fn refresh_widgets() {
    let Some(handle) = APP.get().and_then(bridge) else { return };
    tauri::async_runtime::spawn(async move {
        let call = handle.run_mobile_plugin_async::<serde_json::Value>("refreshWidgets", serde_json::json!({}));
        if let Ok(Err(e)) = tokio::time::timeout(Duration::from_secs(10), call).await {
            eprintln!("Can't refresh the widgets: {e}");
        }
    });
}

pub fn take_quick_add() -> Option<String> {
    lock(&PENDING_QUICK_ADD).take()
}

pub fn take_open_view() -> Option<String> {
    lock(&PENDING_VIEW).take()
}

/// Whether Android will deliver reminders on time: notifications, exact alarms, battery.
pub async fn health(app: &AppHandle) -> Result<serde_json::Value, String> {
    let handle = bridge(app).ok_or("The Android bridge isn't loaded")?;
    handle.run_mobile_plugin_async("health", serde_json::json!({})).await.map_err(|e| e.to_string())
}

/// Opens the system setting for `kind`: notifications, exactAlarms or battery.
pub async fn open_setting(app: &AppHandle, kind: String) -> Result<(), String> {
    let handle = bridge(app).ok_or("The Android bridge isn't loaded")?;
    handle
        .run_mobile_plugin_async::<serde_json::Value>("openSetting", serde_json::json!({ "kind": kind }))
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ── Panics ───────────────────────────────────────────────────────────────────

#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: std::os::raw::c_int, tag: *const std::os::raw::c_char, text: *const std::os::raw::c_char) -> std::os::raw::c_int;
}

/// Logs Rust panics to logcat (tag "tasks-panic"), with a backtrace. On Android
/// a panic's message goes to stderr, which is thrown away. CrashLog.kt puts the
/// app's logcat into each crash report in Downloads/Tasks.
pub fn install_panic_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let text = format!(
            "panic on thread '{}': {info}\n{}",
            thread.name().unwrap_or("?"),
            std::backtrace::Backtrace::force_capture()
        );
        // logcat cuts long lines, so the backtrace goes a line at a time.
        const ERROR: std::os::raw::c_int = 6;
        let tag = b"tasks-panic\0".as_ptr().cast::<std::os::raw::c_char>();
        for line in text.lines() {
            if let Ok(line) = std::ffi::CString::new(line.replace('\0', " ")) {
                unsafe { __android_log_write(ERROR, tag, line.as_ptr()) };
            }
        }
        previous(info);
    }));
}

// ── Called from Kotlin ───────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_openTask(mut env: JNIEnv, _class: JClass, id: JString) {
    let Ok(id) = env.get_string(&id).map(String::from) else { return };
    let Some(app) = APP.get() else {
        *lock(&PENDING_TASK) = Some(id);
        return;
    };
    if let Some(st) = app.try_state::<Arc<AppState>>() {
        *lock(&st.open_task) = Some(id.clone());
    }
    let _ = app.emit(crate::EVENT_OPEN_TASK, id);
}

/// Opens the page's quick add with these defaults (JSON). Kept until the page takes
/// it, since the page may not be listening yet when the tap started the app.
#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_openQuickAdd(mut env: JNIEnv, _class: JClass, defaults: JString) {
    let Ok(defaults) = env.get_string(&defaults).map(String::from) else { return };
    *lock(&PENDING_QUICK_ADD) = Some(defaults);
    if let Some(app) = APP.get() {
        let _ = app.emit(crate::EVENT_QUICK_ADD, ());
    }
}

/// Shows a view of the Tasks page: its query string, e.g. "view=inbox" or
/// "project=<id>#task-cat-<id>". Kept until the page takes it, like openQuickAdd.
#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_openView(mut env: JNIEnv, _class: JClass, query: JString) {
    let Ok(query) = env.get_string(&query).map(String::from) else { return };
    *lock(&PENDING_VIEW) = Some(query);
    if let Some(app) = APP.get() {
        let _ = app.emit(crate::EVENT_OPEN_VIEW, ());
    }
}

/// The lists a widget can show (home_widget::sources), or null.
#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_widgetSources<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, dir: JString<'l>) -> jstring {
    let Ok(dir) = env.get_string(&dir).map(String::from) else { return std::ptr::null_mut() };
    let json = guarded(|| with_db(Path::new(&dir), |conn| home_widget::sources(conn).ok().map(|v| v.to_string())));
    to_java(&mut env, json)
}

/// One list for a widget (home_widget::list), or null.
#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_widgetList<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    dir: JString<'l>,
    kind: JString<'l>,
    id: JString<'l>,
) -> jstring {
    let mut text = |s: &JString<'l>| env.get_string(s).map(String::from).ok();
    let (Some(dir), Some(kind), Some(id)) = (text(&dir), text(&kind), text(&id)) else {
        return std::ptr::null_mut();
    };
    let json = guarded(|| with_db(Path::new(&dir), |conn| home_widget::list(conn, &kind, &id).ok().map(|v| v.to_string())));
    to_java(&mut env, json)
}

/// Ticks a task from a widget, as the page's tick does. Returns the reminders to
/// schedule as JSON when the app is closed (the caller then syncs), else null.
#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_widgetComplete<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, dir: JString<'l>, id: JString<'l>) -> jstring {
    let mut text = |s: &JString<'l>| env.get_string(s).map(String::from).ok();
    let (Some(dir), Some(id)) = (text(&dir), text(&id)) else { return std::ptr::null_mut() };
    let json = guarded(|| {
        if let Some(st) = running_state() {
            // The write wakes the sync loop and refreshes the widgets.
            if let Err(e) = complete(&st, &id) {
                eprintln!("Can't complete the task: {e}");
            }
            return None;
        }
        let st = AppState::open(Path::new(&dir)).ok()?;
        if let Err(e) = complete(&st, &id) {
            eprintln!("Can't complete the task: {e}");
        }
        Some(crate::reminders::upcoming_json(&st))
    });
    to_java(&mut env, json)
}

fn complete(st: &AppState, id: &str) -> Result<bool, String> {
    crate::commands::db(st, true, |conn, ctx| {
        let task = tasks_core::ops::get_task(conn, id)?;
        if task.completed_at.is_some() {
            return Ok(false);
        }
        tasks_core::ops::update_task(conn, ctx, id, crate::reminders::complete_patch(&task))?;
        Ok(true)
    })
}

/// The running app's state, when the app is running in this process.
fn running_state() -> Option<Arc<AppState>> {
    APP.get()?.try_state::<Arc<AppState>>().map(|s| s.inner().clone())
}

/// Reads with the running app's connection, or opens the DB when the app is closed.
fn with_db<T>(dir: &Path, f: impl FnOnce(&rusqlite::Connection) -> Option<T>) -> Option<T> {
    if let Some(st) = running_state() {
        return f(&lock(&st.db));
    }
    let st = AppState::open(dir).ok()?;
    let conn = lock(&st.db);
    f(&conn)
}

/// A panic in Rust must not unwind into Java.
fn guarded<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok().flatten()
}

fn to_java(env: &mut JNIEnv, json: Option<String>) -> jstring {
    match json {
        Some(json) => env.new_string(json).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_backgroundSync<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, dir: JString<'l>) -> jstring {
    let Ok(dir) = env.get_string(&dir).map(String::from) else { return std::ptr::null_mut() };
    let json = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| background_sync(Path::new(&dir)))).ok().flatten();
    match json {
        Some(json) => env.new_string(json).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// A reminder's Snooze or Complete button. Returns the reminders to schedule as
/// JSON, or null when the app is running (its sync loop schedules them) or the
/// button failed. Nothing changes when the task moved on since the reminder
/// showed: done, or its reminder changed.
#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_reminderAction<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    dir: JString<'l>,
    action: JString<'l>,
    id: JString<'l>,
    remind_at: JString<'l>,
) -> jstring {
    let mut text = |s: &JString<'l>| env.get_string(s).map(String::from).ok();
    let (Some(dir), Some(action), Some(id), Some(remind_at)) = (text(&dir), text(&action), text(&id), text(&remind_at)) else {
        return std::ptr::null_mut();
    };
    let json = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reminder_action(Path::new(&dir), &action, &id, &remind_at)))
        .ok()
        .flatten();
    match json {
        Some(json) => env.new_string(json).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

fn reminder_action(dir: &Path, action: &str, id: &str, remind_at: &str) -> Option<String> {
    if !matches!(action, "snooze" | "complete") {
        return None;
    }
    if let Some(app) = APP.get() {
        // The write wakes the sync loop, which pushes it and schedules the alarms.
        let st = app.try_state::<Arc<AppState>>()?.inner().clone();
        match crate::reminders::run_button(&st, action, id, remind_at) {
            Ok(true) => {
                let _ = app.emit(crate::syncer::EVENT_CHANGED, 1);
            }
            Ok(false) => {}
            Err(e) => eprintln!("Can't {action} the task: {e}"),
        }
        return None;
    }
    // Closed app: the caller schedules what this returns and runs a sync to push it.
    let st = AppState::open(dir).ok()?;
    if let Err(e) = crate::reminders::run_button(&st, action, id, remind_at) {
        eprintln!("Can't {action} the task: {e}");
    }
    Some(crate::reminders::upcoming_json(&st))
}

/// One sync round, then what Kotlin needs: `{ok, error, reminders}`, where
/// `reminders` is the schedule (reminders::upcoming_json) and `error` says why the
/// round failed (offline, signed out, ...), for Settings' "Background sync" line.
/// Uses the app's own state when the app is running in this process, else opens
/// the DB itself. None only when the DB can't be opened.
fn background_sync(dir: &Path) -> Option<String> {
    let (st, error) = if let Some(app) = APP.get() {
        let st = app.try_state::<Arc<AppState>>()?.inner().clone();
        tauri::async_runtime::block_on(crate::syncer::round(app, &st));
        let error = {
            let status = lock(&st.status);
            (status.state != "synced").then(|| status.message.clone().unwrap_or_else(|| status.state.replace('_', " ")))
        };
        (st, error)
    } else {
        let st = Arc::new(AppState::open(dir).ok()?);
        let error = sync_closed(&st);
        (st, error)
    };
    let reminders: serde_json::Value = serde_json::from_str(&crate::reminders::upcoming_json(&st)).unwrap_or_default();
    Some(serde_json::json!({ "ok": error.is_none(), "error": error, "reminders": reminders }).to_string())
}

/// A round with the app closed. Returns why it failed, or None when it worked.
fn sync_closed(st: &AppState) -> Option<String> {
    let Some(token) = st.token() else { return Some("Signed out".into()) };
    let Some(server) = lock(&st.config).server.clone() else { return Some("No server set up".into()) };
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => return Some(e.to_string()),
    };
    runtime.block_on(async {
        let client = tasks_core::client::Client::new(server);
        let mut report = tasks_core::client::SyncReport::default();
        match tasks_core::client::sync_once(&client, &token, &st.db, &st.ctx, &mut report).await {
            Ok(()) => None,
            Err(e) => {
                eprintln!("Background sync failed: {e}");
                Some(e.to_string())
            }
        }
    })
}
