//! Settings: environment variables, or a `.env` next to the server (in the
//! working folder or the exe's folder, or the file TASKS_ENV_FILE names).
//! Real environment variables win over the file.

use std::env;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    /// Prefix of every route, e.g. /tasks-api (TASKS_API_BASE_PATH).
    pub base_path: String,
    /// Folder holding accounts.db and user_<id>.db.
    pub data_dir: PathBuf,
    /// Anyone who can reach the server may sign up (TASKS_ALLOW_SIGNUP, default true).
    pub allow_signup: bool,
    /// The .env that was read, if any.
    pub env_file: Option<PathBuf>,
}

fn var(key: &str, default: &str) -> String {
    env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string())
}

impl Config {
    pub fn load() -> Config {
        let in_dir = |dir: &Path| Some(dir.join(".env")).filter(|p| p.is_file());
        let env_file = env::var("TASKS_ENV_FILE")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .or_else(|| env::current_dir().ok().and_then(|d| in_dir(&d)))
            .or_else(|| env::current_exe().ok().and_then(|e| e.parent().and_then(in_dir)));
        if let Some(file) = &env_file {
            let _ = dotenvy::from_path(file);
        }
        // A relative TASKS_DATA_DIR is relative to the .env's folder (else the working folder).
        let root = env_file
            .as_ref()
            .and_then(|f| f.parent().map(Path::to_path_buf))
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let data_dir = PathBuf::from(var("TASKS_DATA_DIR", "data"));
        let data_dir = if data_dir.is_absolute() { data_dir } else { root.join(data_dir) };
        let base_path = var("TASKS_API_BASE_PATH", "/tasks-api").trim_matches('/').to_string();
        let base_path = if base_path.is_empty() { String::new() } else { format!("/{base_path}") };

        Config {
            host: var("TASKS_HOST", "0.0.0.0"),
            port: var("TASKS_PORT", "8050").parse().unwrap_or(8050),
            base_path,
            data_dir,
            allow_signup: !matches!(var("TASKS_ALLOW_SIGNUP", "true").to_ascii_lowercase().as_str(), "false" | "0" | "no" | "off"),
            env_file,
        }
    }

    pub fn user_db(&self, user_id: i64) -> PathBuf {
        self.data_dir.join(format!("user_{user_id}.db"))
    }

    pub fn accounts_db(&self) -> PathBuf {
        self.data_dir.join("accounts.db")
    }
}
