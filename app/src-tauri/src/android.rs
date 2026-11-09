//! Android's own reminders and background sync, with the Kotlin side in
//! app/android/java (copied into the project by scripts/android-native.mjs).
//!
//! - The app's plugin, "tasks-android" (TasksPlugin.kt), sets one exact alarm per
//!   upcoming reminder, reports whether Android will deliver them on time, and
//!   opens the settings that fix it.
//! - A WorkManager job (SyncWorker) calls `backgroundSync` every 15 minutes while
//!   the app is closed, so reminders set on another device still get scheduled.
//! - A tapped reminder calls `openTask`.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;
use serde::Serialize;
use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::{lock, AppState};

/// The running app, once its state is ready. Unset when Android started the
/// process only for the background job.
static APP: OnceLock<AppHandle> = OnceLock::new();
/// A tapped reminder's task, when the tap started the app before its state was ready.
static PENDING_TASK: Mutex<Option<String>> = Mutex::new(None);

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

#[no_mangle]
pub extern "system" fn Java_dev_tasks_app_Native_backgroundSync<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, dir: JString<'l>) -> jstring {
    let Ok(dir) = env.get_string(&dir).map(String::from) else { return std::ptr::null_mut() };
    let json = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| background_sync(Path::new(&dir)))).ok().flatten();
    match json {
        Some(json) => env.new_string(json).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

/// One sync round, then the reminders to schedule. Uses the app's own state when
/// the app is running in this process, else opens the DB itself.
fn background_sync(dir: &Path) -> Option<String> {
    if let Some(app) = APP.get() {
        let st = app.try_state::<Arc<AppState>>()?.inner().clone();
        tauri::async_runtime::block_on(crate::syncer::round(app, &st));
        return Some(crate::reminders::upcoming_json(&st));
    }

    let st = AppState::open(dir).ok()?;
    let token = st.token()?;
    let server = lock(&st.config).server.clone()?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
    runtime.block_on(async {
        let client = tasks_core::client::Client::new(server);
        let mut report = tasks_core::client::SyncReport::default();
        if let Err(e) = tasks_core::client::sync_once(&client, &token, &st.db, &st.ctx, &mut report).await {
            eprintln!("Background sync failed: {e}");
        }
    });
    Some(crate::reminders::upcoming_json(&st))
}
