//! The sync loop: push the local changes, then pull since the last seq. It runs
//! at start, ~2 s after a change, every 2 minutes, and while it can't sync:
//! every 30 s when this device is offline, every minute when the server is
//! down. The page also wakes it when the network comes back or the app is
//! brought back to the front. A forced round (the force-sync button) starts at
//! once, pulls every row again, and always reloads the page.
//!
//! The first full sync after signing in marks the account ready (see
//! SyncStatus::ready); until then the page shows that it is fetching the tasks.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tasks_core::client::{diagnose, sync_once, Client, ClientError, SyncReport};
use tauri::{AppHandle, Emitter};

use crate::state::{lock, AppState, SyncStatus, FIRST_SYNC_KEY};

const EVERY: Duration = Duration::from_secs(120);
const RETRY: Duration = Duration::from_secs(30);
const RETRY_SERVER: Duration = Duration::from_secs(60);
const IDLE: Duration = Duration::from_secs(3600);
const DEBOUNCE: Duration = Duration::from_secs(2);

/// Tells the page: the sync status changed.
pub const EVENT_STATUS: &str = "tasks://sync";
/// Tells the page: local rows changed under it, reload.
pub const EVENT_CHANGED: &str = "tasks://changed";

pub fn set_status(app: &AppHandle, st: &AppState, mut status: SyncStatus) {
    status.ready = is_ready(st);
    *lock(&st.status) = status.clone();
    let _ = app.emit(EVENT_STATUS, status);
}

/// The first full sync for this account has finished.
pub fn is_ready(st: &AppState) -> bool {
    tasks_core::db::get_state(&lock(&st.db), FIRST_SYNC_KEY).ok().flatten().is_some()
}

fn pending(st: &AppState) -> i64 {
    tasks_core::sync::pending_count(&lock(&st.db)).unwrap_or(0)
}

/// One round; returns how long to wait before the next one.
async fn round(app: &AppHandle, st: &Arc<AppState>) -> Duration {
    let _round = st.sync_lock.lock().await;
    let force = st.force_sync.swap(false, Ordering::Relaxed);
    let server = lock(&st.config).server.clone();
    let Some(server) = server else {
        set_status(app, st, SyncStatus { state: "setup", pending: pending(st), ..Default::default() });
        return IDLE;
    };
    let Some(token) = st.token() else {
        set_status(app, st, SyncStatus { state: "signed_out", pending: pending(st), ..Default::default() });
        return IDLE;
    };
    let last = lock(&st.status).last_synced_at.clone();
    set_status(app, st, SyncStatus { state: "syncing", last_synced_at: last.clone(), pending: pending(st), ..Default::default() });

    if force {
        let _ = tasks_core::sync::set_last_pulled(&lock(&st.db), 0);
    }
    let client = Client::new(server.clone());
    let mut report = SyncReport::default();
    let result = match sync_once(&client, &token, &st.db, &st.ctx, &mut report).await {
        Ok(()) => Ok(()),
        Err(e) => Err(diagnose(e).await),
    };
    // Also after a round that failed partway: the rows it pulled won't count as
    // changes next time, so this is the only chance to show them.
    if force || report.changed > 0 {
        let _ = app.emit(EVENT_CHANGED, report.changed);
    }
    let pending = pending(st);
    match result {
        Ok(()) => {
            let _ = tasks_core::db::set_state(&lock(&st.db), FIRST_SYNC_KEY, &tasks_core::time::now_iso());
            set_status(
                app,
                st,
                SyncStatus {
                    state: "synced",
                    message: None,
                    last_synced_at: Some(tasks_core::time::now_iso()),
                    pending,
                    ready: true,
                },
            );
            EVERY
        }
        Err(ClientError::Offline(_) | ClientError::Unreachable(_)) => {
            let message = "This device is offline. Changes are kept here and sync when you're back online.".to_string();
            set_status(app, st, SyncStatus { state: "offline", message: Some(message), last_synced_at: last, pending, ..Default::default() });
            RETRY
        }
        Err(ClientError::ServerDown(msg)) => {
            let message = format!("Can't reach the Tasks server at {}: {msg}. Changes are kept here and sync when it's back.", server.url);
            set_status(app, st, SyncStatus { state: "server_down", message: Some(message), last_synced_at: last, pending, ..Default::default() });
            RETRY_SERVER
        }
        Err(ClientError::Unauthorized(msg)) => {
            set_status(app, st, SyncStatus { state: "signed_out", message: Some(msg), last_synced_at: last, pending, ..Default::default() });
            IDLE
        }
        Err(e) => {
            set_status(app, st, SyncStatus { state: "error", message: Some(e.to_string()), last_synced_at: last, pending, ..Default::default() });
            RETRY * 2
        }
    }
}

pub async fn run(app: AppHandle, st: Arc<AppState>) {
    let mut delay = Duration::from_millis(500);
    loop {
        let woke = tokio::select! {
            _ = st.wake.notified() => true,
            _ = tokio::time::sleep(delay) => false,
        };
        if woke && !st.force_sync.load(Ordering::Relaxed) {
            // More edits in the next moment wake it again; one sync covers them.
            tokio::time::sleep(DEBOUNCE).await;
        }
        crate::reminders::refresh(&app, &st);
        delay = round(&app, &st).await;
        crate::reminders::refresh(&app, &st);
    }
}
