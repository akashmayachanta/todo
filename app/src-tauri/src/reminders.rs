//! Task reminders as system notifications.
//!
//! - Windows: a loop checks `remind_at` every 30 s and shows a toast for each
//!   reminder that came due (one missed while the app was closed shows if it is
//!   under a day old). Each fires once: fired `id:remind_at` keys are kept in
//!   the DB.
//! - Android: scheduled notifications (AlarmManager), so they fire while the
//!   app is closed and the phone is offline. After every sync or edit the app
//!   cancels the ones it scheduled last time and schedules the open tasks with
//!   a future `remind_at`. Tapping one opens its task. The plugin only restores
//!   alarms after a reboot for notifications sent in a batch, which its Rust API
//!   doesn't offer, so after a reboot they come back when the app next opens.

use std::sync::Arc;

use tauri::AppHandle;

use crate::state::AppState;

#[cfg(desktop)]
const FIRED_KEY: &str = "reminders_fired";

/// Called after each sync and edit. Windows needs nothing here: its loop reads the DB.
#[cfg(desktop)]
pub fn refresh(_app: &AppHandle, _st: &Arc<AppState>) {}

#[cfg(desktop)]
pub async fn run(app: AppHandle, st: Arc<AppState>) {
    loop {
        fire_due(&app, &st);
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

#[cfg(desktop)]
fn fire_due(app: &AppHandle, st: &Arc<AppState>) {
    use chrono::{Duration, Utc};
    use tauri_plugin_notification::NotificationExt;

    let now = Utc::now();
    let from = tasks_core::time::iso(now - Duration::hours(24));
    let to = tasks_core::time::iso(now + Duration::seconds(1));
    let (due, mut fired) = {
        let conn = crate::state::lock(&st.db);
        let due = tasks_core::ops::reminders_between(&conn, &from, &to).unwrap_or_default();
        let fired: Vec<String> = tasks_core::db::get_state(&conn, FIRED_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        (due, fired)
    };
    let mut any = false;
    for task in due {
        let key = format!("{}:{}", task.id, task.remind_at.as_deref().unwrap_or_default());
        if fired.contains(&key) {
            continue;
        }
        let _ = app.notification().builder().title("Task reminder").body(&tasks_core::plain_title(&task.title)).show();
        fired.push(key);
        any = true;
    }
    if any {
        let keep = fired.len().saturating_sub(200);
        let text = serde_json::to_string(&fired[keep..]).unwrap_or_default();
        let _ = tasks_core::db::set_state(&crate::state::lock(&st.db), FIRED_KEY, &text);
    }
}

/// A notification id for a task (Android wants an i32).
#[cfg(mobile)]
fn notification_id(task_id: &str) -> i32 {
    // FNV-1a, folded to a positive i32.
    let mut h: u32 = 0x811c9dc5;
    for b in task_id.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    (h & 0x7fff_ffff) as i32
}

/// Android keeps a limited number of alarms per app; the soonest are scheduled.
#[cfg(mobile)]
const MAX_SCHEDULED: usize = 200;

#[cfg(mobile)]
pub fn refresh(app: &AppHandle, st: &Arc<AppState>) {
    use tauri_plugin_notification::{NotificationExt, Schedule};

    let upcoming = {
        let conn = crate::state::lock(&st.db);
        let now = tasks_core::time::iso(chrono::Utc::now() + chrono::Duration::seconds(5));
        tasks_core::ops::upcoming_reminders(&conn, &now).unwrap_or_default()
    };
    let n = app.notification();
    // cancel_all() only knows ids the plugin saved, and show() saves none, so
    // the ids scheduled last time are kept here and cancelled by name.
    let previous: Vec<i32> = tasks_core::db::get_state(&crate::state::lock(&st.db), SCHEDULED_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if !previous.is_empty() {
        let _ = n.cancel(previous);
    }
    let mut scheduled = Vec::new();
    for task in upcoming.into_iter().take(MAX_SCHEDULED) {
        let Some(ms) = task.remind_at.as_deref().and_then(tasks_core::time::instant_ms) else { continue };
        let Ok(date) = time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000) else { continue };
        let id = notification_id(&task.id);
        scheduled.push(id);
        let _ = n
            .builder()
            .id(id)
            .title("Task reminder")
            .body(&tasks_core::plain_title(&task.title))
            .extra("task_id", &task.id)
            .auto_cancel()
            .schedule(Schedule::At { date, repeating: false, allow_while_idle: true })
            .show();
    }
    let text = serde_json::to_string(&scheduled).unwrap_or_default();
    let _ = tasks_core::db::set_state(&crate::state::lock(&st.db), SCHEDULED_KEY, &text);
}

/// The notification ids scheduled by the last refresh().
#[cfg(mobile)]
const SCHEDULED_KEY: &str = "reminders_scheduled";

/// Mobile has no loop; scheduling is done by refresh().
#[cfg(mobile)]
pub async fn run(_app: AppHandle, _st: Arc<AppState>) {}
