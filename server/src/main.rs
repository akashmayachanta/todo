//! tasks-server: the Tasks sync server. On Windows it runs as a tray icon with
//! no console window: see tray.rs. People make their accounts from the app.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod auth;
mod config;
mod error;
mod routes;
mod store;
#[cfg(windows)]
mod tray;

use std::future::Future;
use std::sync::Arc;

use config::Config;
use tokio::net::TcpListener;

pub struct AppState {
    pub config: Config,
    pub store: Arc<store::Store>,
    pub accounts: Arc<auth::Accounts>,
}

/// RUST_LOG, or the default log levels.
fn log_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=warn".into())
}

#[cfg(windows)]
fn main() {
    tray::main();
}

/// Other systems have no tray here: it serves until Ctrl+C.
#[cfg(not(windows))]
#[tokio::main]
async fn main() {
    let config = Config::load();
    tracing_subscriber::fmt().with_env_filter(log_filter()).init();
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let result = match Server::bind(config).await {
        Ok(server) => server.run(shutdown).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// The server with its port taken and its accounts open, ready to run.
pub struct Server {
    listener: TcpListener,
    state: Arc<AppState>,
}

impl Server {
    pub async fn bind(config: Config) -> Result<Server, String> {
        let accounts = auth::Accounts::open(&config.accounts_db())
            .map_err(|e| format!("Could not open {}: {}", config.accounts_db().display(), e.detail))?;
        match &config.env_file {
            Some(f) => tracing::info!("settings from {}", f.display()),
            None => tracing::info!("no .env found; using the environment and defaults"),
        }
        tracing::info!("data in {}", config.data_dir.display());
        match accounts.list_users() {
            Ok(users) if users.is_empty() && !config.allow_signup => {
                tracing::warn!("no users yet and sign-up is off, so nobody can sign in; set TASKS_ALLOW_SIGNUP=true")
            }
            _ => {}
        }
        if config.allow_signup {
            tracing::info!("sign-up is open (TASKS_ALLOW_SIGNUP=false closes it)");
        }

        let addr = format!("{}:{}", config.host, config.port);
        let listener = TcpListener::bind(&addr).await.map_err(|e| format!("Could not listen on {addr}: {e}"))?;
        tracing::info!("listening on http://{addr}{}", config.base_path);
        let state = Arc::new(AppState { store: Arc::new(store::Store::new(config.clone())), accounts: Arc::new(accounts), config });
        Ok(Server { listener, state })
    }

    /// Serves until `shutdown` completes.
    pub async fn run(self, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<(), String> {
        let app = routes::router(self.state);
        axum::serve(self.listener, app).with_graceful_shutdown(shutdown).await.map_err(|e| format!("Server error: {e}"))
    }
}
