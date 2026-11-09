//! What the app keeps: the local tasks DB, where the server is, and the device
//! token (in the Windows Credential Manager on desktop, in the app's private
//! folder on Android).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tasks_core::client::ServerInfo;
use tasks_core::Ctx;
use tokio::sync::{Mutex as AsyncMutex, Notify};

/// config.json in the app data folder.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: Option<ServerInfo>,
    pub username: Option<String>,
    pub user_id: Option<i64>,
    /// The lists and tasks pinned to the desktop. They stay on this device and don't sync.
    #[serde(default)]
    pub pins: Vec<Pin>,
}

/// One pinned window. `kind` is inbox, today or upcoming with no id, or project,
/// category, filter or task with one (app/src/lib/components/tasks/pins.ts).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub kind: String,
    pub id: Option<String>,
}

// Pins are windows, so only the desktop's widgets.rs uses these.
#[cfg(desktop)]
impl Pin {
    /// The window's label, which is how the page knows what it shows.
    pub fn label(&self) -> String {
        match &self.id {
            Some(id) => format!("pin-{}-{id}", self.kind),
            None => format!("pin-{}", self.kind),
        }
    }

    pub fn is_valid(&self) -> bool {
        match &self.id {
            None => matches!(self.kind.as_str(), "inbox" | "today" | "upcoming"),
            Some(id) => {
                matches!(self.kind.as_str(), "project" | "category" | "filter" | "task")
                    && !id.is_empty()
                    && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            }
        }
    }
}

/// Shown by the status chip.
#[derive(Clone, Debug, Serialize)]
pub struct SyncStatus {
    /// setup | syncing | synced | offline | server_down | error | signed_out
    pub state: &'static str,
    pub message: Option<String>,
    pub last_synced_at: Option<String>,
    /// Local changes not pushed yet.
    pub pending: i64,
    /// The first full sync for this account has finished, so the local DB has
    /// the account's tasks. Until then the page waits (it needs the network
    /// once); after that it works offline. Filled in by syncer::set_status.
    pub ready: bool,
}

impl Default for SyncStatus {
    fn default() -> Self {
        SyncStatus { state: "setup", message: None, last_synced_at: None, pending: 0, ready: false }
    }
}

/// The sync_state key that records the first full sync for this account.
pub const FIRST_SYNC_KEY: &str = "first_sync_done";

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub ctx: Ctx,
    pub dir: PathBuf,
    pub config: Mutex<AppConfig>,
    pub status: Mutex<SyncStatus>,
    /// Wakes the sync loop (a change, Sync now, sign-in).
    pub wake: Notify,
    /// Wakes the reminder loop on Windows after an edit or sync, so it re-reads
    /// when the next reminder is due.
    #[cfg_attr(mobile, allow(dead_code))]
    pub reminders_wake: Notify,
    /// The next round pulls everything again and reloads the page (the force-sync button).
    pub force_sync: AtomicBool,
    /// Held for a whole sync round and while signing in, so a round never
    /// lands one account's rows after the switch to another.
    pub sync_lock: AsyncMutex<()>,
    /// A task whose notification was tapped, for the page to open once it is ready.
    pub open_task: Mutex<Option<String>>,
    /// Started minimized (to the tray), from --minimized.
    pub minimized: AtomicBool,
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn open(dir: &Path) -> Result<AppState, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let conn = tasks_core::db::open(&dir.join("tasks.db")).map_err(|e| e.to_string())?;
        let device_id = tasks_core::db::device_id(&conn).map_err(|e| e.to_string())?;
        let config = std::fs::read_to_string(dir.join("config.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Ok(AppState {
            db: Arc::new(Mutex::new(conn)),
            ctx: Ctx::device(device_id),
            dir: dir.to_path_buf(),
            config: Mutex::new(config),
            status: Mutex::new(SyncStatus::default()),
            wake: Notify::new(),
            reminders_wake: Notify::new(),
            force_sync: AtomicBool::new(false),
            sync_lock: AsyncMutex::new(()),
            open_task: Mutex::new(None),
            minimized: AtomicBool::new(false),
        })
    }

    pub fn save_config(&self, config: AppConfig) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
        std::fs::write(self.dir.join("config.json"), text).map_err(|e| e.to_string())?;
        *lock(&self.config) = config;
        Ok(())
    }

    /// A local change was made: sync soon (the loop waits ~2 s for more).
    pub fn changed(&self) {
        self.wake.notify_one();
    }

    // ── Device token ─────────────────────────────────────────────────────────

    #[cfg(desktop)]
    fn entry(&self) -> Option<keyring::Entry> {
        // One entry per data folder, so two copies of the app (two folders) keep their own.
        keyring::Entry::new("tasks-app", &format!("device-token:{}", self.dir.display())).ok()
    }

    pub fn token(&self) -> Option<String> {
        #[cfg(desktop)]
        {
            self.entry().and_then(|e| e.get_password().ok()).filter(|t| !t.is_empty())
        }
        #[cfg(mobile)]
        {
            std::fs::read_to_string(self.dir.join("device-token")).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
        }
    }

    pub fn set_token(&self, token: &str) -> Result<(), String> {
        #[cfg(desktop)]
        {
            self.entry().ok_or("The Windows Credential Manager is not available")?.set_password(token).map_err(|e| e.to_string())
        }
        #[cfg(mobile)]
        {
            std::fs::write(self.dir.join("device-token"), token).map_err(|e| e.to_string())
        }
    }

    pub fn clear_token(&self) {
        #[cfg(desktop)]
        {
            if let Some(e) = self.entry() {
                let _ = e.delete_credential();
            }
        }
        #[cfg(mobile)]
        {
            let _ = std::fs::remove_file(self.dir.join("device-token"));
        }
    }
}
