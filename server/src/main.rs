//! tasks-server: the Tasks sync server. On Windows it runs as a tray icon with
//! no console window: see tray.rs. People make their accounts from the app.
//! `tasks-server reset-password` replaces a forgotten password instead of serving.

#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod auth;
mod config;
mod error;
mod routes;
mod store;
#[cfg(windows)]
mod tray;

use std::future::Future;
use std::io::{self, Write};
use std::sync::Arc;

use config::Config;
use tokio::net::TcpListener;

/// The argument that resets a password instead of starting the server.
const RESET_PASSWORD_ARG: &str = "reset-password";

pub struct AppState {
    pub config: Config,
    pub store: Arc<store::Store>,
    pub accounts: Arc<auth::Accounts>,
}

/// RUST_LOG, or the default log levels.
fn log_filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=warn".into())
}

fn wants_password_reset() -> bool {
    std::env::args_os().nth(1).is_some_and(|arg| arg == RESET_PASSWORD_ARG)
}

/// Asks in the console for a username and new password, reports the result and
/// waits for Enter, so a window opened just for this doesn't vanish. Returns the
/// exit code. It's safe while the server runs: accounts.db is in WAL mode, and
/// each sign-in reads the hash from it.
fn reset_password(config: &Config) -> i32 {
    let code = match ask_and_set_password(config) {
        Ok(username) => {
            println!("\nThe password for '{username}' is changed. Devices already signed in stay signed in.");
            0
        }
        Err(message) => {
            println!("\n{message}\nNo password was changed.");
            1
        }
    };
    print!("\nPress Enter to close. ");
    let _ = io::stdout().flush();
    let _ = io::stdin().read_line(&mut String::new());
    code
}

/// Returns the username whose password was changed.
fn ask_and_set_password(config: &Config) -> Result<String, String> {
    let accounts = auth::Accounts::open(&config.accounts_db())
        .map_err(|e| format!("Could not open {}: {}", config.accounts_db().display(), e.detail))?;
    let users = accounts.list_users().map_err(|e| e.detail)?;
    if users.is_empty() {
        return Err(format!("There are no accounts in {}.", config.data_dir.display()));
    }
    println!("Reset a password\n\nAccounts:");
    for user in &users {
        // An older accounts.db can hold disabled users, and a new password doesn't let them back in.
        println!("  {}{}", user.username, if user.disabled_at.is_some() { " (disabled)" } else { "" });
    }

    print!("\nUsername: ");
    let _ = io::stdout().flush();
    let mut username = String::new();
    io::stdin().read_line(&mut username).map_err(|e| format!("Could not read the username: {e}"))?;
    let username = username.trim();

    let password = rpassword::prompt_password("New password: ").map_err(|e| format!("Could not read the password: {e}"))?;
    let again = rpassword::prompt_password("Type it again: ").map_err(|e| format!("Could not read the password: {e}"))?;
    if password != again {
        return Err("The two passwords don't match.".to_string());
    }
    accounts.set_password(username, &password).map_err(|e| e.detail)?;
    Ok(username.to_string())
}

#[cfg(windows)]
fn main() {
    if wants_password_reset() {
        tray::reset_password_window();
    }
    tray::main();
}

/// Other systems have no tray here: it serves until Ctrl+C.
#[cfg(not(windows))]
#[tokio::main]
async fn main() {
    let config = Config::load();
    if wants_password_reset() {
        std::process::exit(reset_password(&config));
    }
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
