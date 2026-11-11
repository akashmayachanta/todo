//! The sync loop: push the local changes, then pull since the last seq. It runs
//! at start, ~2 s after a change, every 30 s, and while it can't sync:
//! every 30 s when this device is offline, every minute when the server is
//! down. The page also wakes it when the network comes back or the app is
//! brought back to the front. A forced round (the force-sync button) starts at
//! once, pulls every row again, and always reloads the page.
//!
//! [`listen`] holds the server's event stream open, and another device's
//! change wakes the loop at once. While the stream is open the loop polls only
//! every 5 minutes, in case a notice went missing.
//!
//! The first full sync after signing in marks the account ready (see
//! SyncStatus::ready); until then the page shows that it is fetching the tasks.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tasks_core::client::{diagnose, sync_once, Client, ClientError, Notice, SyncReport};
use tauri::{AppHandle, Emitter};

use crate::state::{lock, AppState, SyncStatus, FIRST_SYNC_KEY};

const EVERY: Duration = Duration::from_secs(30);
/// Between rounds while the event stream is open.
const EVERY_LIVE: Duration = Duration::from_secs(300);
/// After the event stream fails, it tries again after this, doubling up to LIVE_RETRY_MAX.
const LIVE_RETRY: Duration = Duration::from_secs(5);
const LIVE_RETRY_MAX: Duration = Duration::from_secs(300);
/// An older server has no event stream. Look again this often, in case it was updated.
const LIVE_UNSUPPORTED: Duration = Duration::from_secs(3600);
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
pub(crate) async fn round(app: &AppHandle, st: &Arc<AppState>) -> Duration {
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
            if st.live.load(Ordering::Relaxed) { EVERY_LIVE } else { EVERY }
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

/// Holds the server's event stream (GET /sync/events) open, and wakes the sync
/// loop when another device changed something. On Android only while the app is
/// on screen: an open connection keeps the phone's radio awake.
pub async fn listen(st: Arc<AppState>) {
    let mut retry = LIVE_RETRY;
    loop {
        if let Some(wait) = listen_once(&st, &mut retry).await {
            tokio::select! {
                _ = st.live_reset.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }
}

/// One connection, until it ends. Returns how long to wait before the next one,
/// or None to start it again now.
async fn listen_once(st: &AppState, retry: &mut Duration) -> Option<Duration> {
    if cfg!(mobile) && !st.foreground.load(Ordering::Relaxed) {
        return Some(IDLE);
    }
    let server = lock(&st.config).server.clone();
    let (Some(server), Some(token)) = (server, st.token()) else {
        return Some(IDLE);
    };
    let mut events = match Client::new(server).events(&token).await {
        Ok(events) => events,
        // An older server: the loop keeps polling every 30 s.
        Err(ClientError::Server { status: 404, .. }) => return Some(LIVE_UNSUPPORTED),
        // The sync loop shows it; sign-in starts the stream again.
        Err(ClientError::Unauthorized(_)) => return Some(IDLE),
        Err(_) => return Some(backoff(retry)),
    };
    st.live.store(true, Ordering::Relaxed);
    *retry = LIVE_RETRY;
    let dropped = loop {
        tokio::select! {
            _ = st.live_reset.notified() => break false,
            notice = events.next() => match notice {
                Ok(Some(Notice::Changed)) => st.wake.notify_one(),
                Ok(Some(Notice::Ready)) => {}
                Ok(None) | Err(_) => break true,
            },
        }
    };
    st.live.store(false, Ordering::Relaxed);
    if !dropped {
        return None;
    }
    // The loop may be in its long wait, and notices sent from now on are lost:
    // sync now and go back to polling every 30 s. The round also shows whether
    // this device went offline or the server went down.
    st.wake.notify_one();
    Some(backoff(retry))
}

fn backoff(retry: &mut Duration) -> Duration {
    let wait = *retry;
    *retry = (*retry * 2).min(LIVE_RETRY_MAX);
    wait
}
