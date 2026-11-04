//! tasks-server: the Tasks sync server.
//!
//!   tasks-server [serve]                     run the API
//!   tasks-server user add|passwd|disable|enable|list ...
//!                                            manage the accounts

mod auth;
mod config;
mod error;
mod routes;
mod store;

use std::io::{BufRead, IsTerminal};
use std::sync::Arc;

use config::Config;

pub struct AppState {
    pub config: Config,
    pub store: Arc<store::Store>,
    pub accounts: Arc<auth::Accounts>,
}

const USAGE: &str = "\
tasks-server: the Tasks sync server

USAGE:
  tasks-server [serve]
      Runs the API on TASKS_HOST:TASKS_PORT under TASKS_API_BASE_PATH.
  tasks-server user add <username> [--id <n>]
      Adds a user (people can also sign up from the app); asks for the password. --id picks the user id, so an
      existing user_<n>.db in the data folder becomes theirs.
  tasks-server user passwd <username>
      Sets a new password. Devices that are signed in stay signed in.
  tasks-server user disable <username>
  tasks-server user enable <username>
      A disabled user can't sign in, and their devices are refused.
  tasks-server user list

A password is read from the terminal, or from the first line of stdin when it
isn't one.

Settings come from the environment, or a .env in the working folder or next
to the exe (or the file TASKS_ENV_FILE names): TASKS_HOST, TASKS_PORT,
TASKS_API_BASE_PATH, TASKS_DATA_DIR, TASKS_ALLOW_SIGNUP (true or false).";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=warn".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = Config::load();
    let code = match args.first().map(String::as_str) {
        None | Some("serve") => serve(config).await,
        Some("user") => user(&config, &args[1..]),
        Some("help" | "--help" | "-h") => {
            println!("{USAGE}");
            0
        }
        Some(other) => {
            eprintln!("Unknown command '{other}'\n\n{USAGE}");
            2
        }
    };
    std::process::exit(code);
}

fn open_accounts(config: &Config) -> Option<auth::Accounts> {
    match auth::Accounts::open(&config.accounts_db()) {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("Could not open {}: {}", config.accounts_db().display(), e.detail);
            None
        }
    }
}

async fn serve(config: Config) -> i32 {
    let Some(accounts) = open_accounts(&config) else { return 1 };
    match &config.env_file {
        Some(f) => tracing::info!("settings from {}", f.display()),
        None => tracing::info!("no .env found; using the environment and defaults"),
    }
    tracing::info!("data in {}", config.data_dir.display());
    match accounts.list_users() {
        Ok(users) if users.is_empty() && !config.allow_signup => {
            tracing::warn!("no users yet and sign-up is off; add one with `tasks-server user add <username>`")
        }
        _ => {}
    }
    if config.allow_signup {
        tracing::info!("sign-up is open (TASKS_ALLOW_SIGNUP=false closes it)");
    }

    let addr = format!("{}:{}", config.host, config.port);
    let state = Arc::new(AppState { store: Arc::new(store::Store::new(config.clone())), accounts: Arc::new(accounts), config });
    let app = routes::router(state.clone());
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Could not listen on {addr}: {e}");
            return 1;
        }
    };
    tracing::info!("listening on http://{addr}{}", state.config.base_path);
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    if let Err(e) = axum::serve(listener, app).with_graceful_shutdown(shutdown).await {
        eprintln!("Server error: {e}");
        return 1;
    }
    0
}

/// A password from the terminal (asked twice), or the first line of stdin.
fn read_password() -> Option<String> {
    if !std::io::stdin().is_terminal() {
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line).ok()?;
        return Some(line.trim_end_matches(['\r', '\n']).to_string());
    }
    let first = rpassword::prompt_password("Password: ").ok()?;
    let again = rpassword::prompt_password("Again: ").ok()?;
    if first != again {
        eprintln!("The passwords don't match");
        return None;
    }
    Some(first)
}

fn user(config: &Config, args: &[String]) -> i32 {
    let (cmd, rest) = match args.split_first() {
        Some((cmd, rest)) => (cmd.as_str(), rest),
        None => {
            eprintln!("{USAGE}");
            return 2;
        }
    };
    let name = rest.first().map(String::as_str);
    if cmd != "list" && name.is_none() {
        eprintln!("tasks-server user {cmd} needs a username\n\n{USAGE}");
        return 2;
    }
    let Some(accounts) = open_accounts(config) else { return 1 };
    let name = name.unwrap_or_default();
    let result = match cmd {
        "add" => {
            let id = match rest.iter().position(|a| a == "--id").map(|i| rest.get(i + 1).and_then(|v| v.parse::<i64>().ok())) {
                None => None,
                Some(Some(id)) => Some(id),
                Some(None) => {
                    eprintln!("--id needs a number");
                    return 2;
                }
            };
            let Some(password) = read_password() else { return 1 };
            accounts.add_user(name, &password, id).map(|u| {
                let db = config.user_db(u.id);
                let note = if db.is_file() { " (it already exists)" } else { "" };
                println!("Added {} (id {}); their tasks are in {}{note}", u.username, u.id, db.display());
            })
        }
        "passwd" => {
            let Some(password) = read_password() else { return 1 };
            accounts.set_password(name, &password).map(|_| println!("Changed the password of {name}"))
        }
        "disable" => accounts.set_disabled(name, true).map(|_| println!("Disabled {name}")),
        "enable" => accounts.set_disabled(name, false).map(|_| println!("Enabled {name}")),
        "list" => accounts.list_users().map(|users| {
            if users.is_empty() {
                println!("No users yet");
            }
            for u in users {
                let state = if u.disabled_at.is_some() { "disabled" } else { "active" };
                println!("{:>4}  {:<24} {:<8} since {}", u.id, u.username, state, &u.created_at[..10.min(u.created_at.len())]);
            }
        }),
        other => {
            eprintln!("Unknown user command '{other}'\n\n{USAGE}");
            return 2;
        }
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", e.detail);
            1
        }
    }
}
