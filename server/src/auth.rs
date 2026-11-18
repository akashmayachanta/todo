//! Who is calling. The server keeps its own accounts in accounts.db: users
//! and their device tokens. A device signs up (POST /auth/register) or signs in
//! (POST /auth/login) once with a username and password and gets a long-lived
//! token, sent as `Authorization: Bearer <token>` from then on.
//! Tokens are stored hashed and can be revoked; a disabled user's devices are
//! refused.
//!
//! MCP tokens (`mcp_…`) are a second kind, for AI clients on {base}/mcp only.
//! A user makes them from a signed-in device; each has a scope (read, or read
//! and write) and an optional expiry.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ApiError;
use crate::AppState;

/// Device tokens start with this.
pub const TOKEN_PREFIX: &str = "dt_";

/// MCP tokens start with this. They work only on {base}/mcp.
pub const MCP_TOKEN_PREFIX: &str = "mcp_";

/// How long a new MCP token lasts when the request doesn't say.
pub const MCP_TOKEN_DEFAULT_DAYS: i64 = 30;

/// Each entry moves accounts.db up one version; see server/migrations.
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_accounts", include_str!("../migrations/0001_accounts.sql")),
    ("0002_mcp_tokens", include_str!("../migrations/0002_mcp_tokens.sql")),
];

/// The signed-in user, from a device token.
#[derive(Clone, Debug)]
pub struct AuthUser {
    pub user_id: i64,
    pub username: String,
    pub device_id: String,
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::unauthorized("Not authenticated"))?;
        let token = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .ok_or_else(|| ApiError::unauthorized("Not authenticated"))?
            .trim()
            .to_string();
        let accounts = state.accounts.clone();
        tokio::task::spawn_blocking(move || accounts.check(&token))
            .await
            .map_err(|e| ApiError::internal(e.to_string()))??
            .ok_or_else(|| ApiError::unauthorized("Invalid or revoked device token"))
    }
}

/// What an MCP token may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpScope {
    /// List and read only.
    Read,
    /// Read, add, change, tick off and delete tasks.
    ReadWrite,
}

impl McpScope {
    fn as_str(self) -> &'static str {
        match self {
            McpScope::Read => "read",
            McpScope::ReadWrite => "read_write",
        }
    }

    fn parse(s: &str) -> McpScope {
        if s == "read_write" { McpScope::ReadWrite } else { McpScope::Read }
    }

    pub fn can_write(self) -> bool {
        self == McpScope::ReadWrite
    }
}

/// An MCP token as the AI tab lists it; the token itself is shown only once.
#[derive(Clone, Debug, Serialize)]
pub struct McpToken {
    pub id: String,
    pub name: String,
    pub scope: McpScope,
    pub created_at: String,
    /// None: never expires.
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
}

/// The caller of {base}/mcp, from an MCP token.
#[derive(Clone, Debug)]
pub struct McpUser {
    pub user_id: i64,
    pub token_id: String,
    pub scope: McpScope,
}

/// A 401 for /mcp: MCP clients look for `WWW-Authenticate: Bearer`.
fn mcp_unauthorized(detail: &str) -> Response {
    let mut res = ApiError::unauthorized(detail).into_response();
    res.headers_mut().insert(axum::http::header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    res
}

impl FromRequestParts<Arc<AppState>> for McpUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &Arc<AppState>) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer ")))
            .map(|t| t.trim().to_string())
            .ok_or_else(|| mcp_unauthorized("Not authenticated: send Authorization: Bearer <MCP token>"))?;
        let accounts = state.accounts.clone();
        tokio::task::spawn_blocking(move || accounts.check_mcp(&token))
            .await
            .map_err(|e| ApiError::internal(e.to_string()).into_response())?
            .map_err(IntoResponse::into_response)?
            .ok_or_else(|| mcp_unauthorized("Invalid, expired or deleted MCP token"))
    }
}

/// Checks an expiry a user picked: a future instant, returned in canonical form.
/// None stays None (never expires).
pub fn check_expiry(expires_at: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = expires_at else { return Ok(None) };
    let at = tasks_core::time::parse_instant(value)
        .ok_or_else(|| ApiError::unprocessable("expires_at must be a date and time like 2026-12-31T23:59:59Z"))?;
    if at <= tasks_core::time::now_iso() {
        return Err(ApiError::unprocessable("expires_at must be in the future"));
    }
    Ok(Some(at))
}

/// The default expiry for a new MCP token.
pub fn default_expiry() -> String {
    tasks_core::time::iso(chrono::Utc::now() + chrono::Duration::days(MCP_TOKEN_DEFAULT_DAYS))
}

#[derive(Clone, Debug, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub created_at: String,
    pub disabled_at: Option<String>,
}

pub struct Accounts {
    conn: Mutex<Connection>,
}

fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn random_bytes<const N: usize>() -> Result<[u8; N], ApiError> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(buf)
}

fn random_token(prefix: &str) -> Result<String, ApiError> {
    Ok(format!("{prefix}{}", random_bytes::<32>()?.iter().map(|b| format!("{b:02x}")).collect::<String>()))
}

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::encode_b64(&random_bytes::<16>()?).map_err(|e| ApiError::internal(e.to_string()))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::internal(e.to_string()))
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// A hash to check against when the username is unknown, so a wrong username
/// takes as long as a wrong password.
fn dummy_hash() -> &'static str {
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password("not a real password").unwrap_or_default())
}

fn bad_login() -> ApiError {
    ApiError::unauthorized("Wrong username or password")
}

pub fn check_username(username: &str) -> Result<&str, ApiError> {
    let name = username.trim();
    if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_whitespace) {
        return Err(ApiError::unprocessable("A username is 1 to 64 characters, with no spaces"));
    }
    Ok(name)
}

pub fn check_password(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < 8 {
        return Err(ApiError::unprocessable("A password is at least 8 characters"));
    }
    Ok(())
}

impl Accounts {
    pub fn open(path: &Path) -> Result<Accounts, ApiError> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        tasks_core::db::run_migrations(&conn, MIGRATIONS)?;
        Ok(Accounts { conn: Mutex::new(conn) })
    }

    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ── Users ────────────────────────────────────────────────────────────────

    pub fn add_user(&self, username: &str, password: &str) -> Result<User, ApiError> {
        let username = check_username(username)?;
        check_password(password)?;
        let hash = hash_password(password)?;
        let created_at = tasks_core::time::now_iso();
        let conn = self.lock();
        let taken: bool = conn.query_row("SELECT EXISTS (SELECT 1 FROM users WHERE username = ?1)", [username], |r| r.get(0))?;
        if taken {
            return Err(ApiError::new(StatusCode::CONFLICT, format!("There is already a user '{username}'")));
        }
        conn.execute(
            "INSERT INTO users (username, password_hash, created_at) VALUES (?1, ?2, ?3)",
            params![username, hash, created_at],
        )?;
        Ok(User { id: conn.last_insert_rowid(), username: username.to_string(), created_at, disabled_at: None })
    }

    /// Replaces a forgotten password. Devices already signed in stay signed in.
    pub fn set_password(&self, username: &str, password: &str) -> Result<(), ApiError> {
        check_password(password)?;
        let hash = hash_password(password)?;
        let changed = self.lock().execute("UPDATE users SET password_hash = ?1 WHERE username = ?2", params![hash, username.trim()])?;
        if changed == 0 {
            return Err(no_user(username));
        }
        Ok(())
    }

    pub fn list_users(&self) -> Result<Vec<User>, ApiError> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, username, created_at, disabled_at FROM users ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| Ok(User { id: r.get(0)?, username: r.get(1)?, created_at: r.get(2)?, disabled_at: r.get(3)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ── Sign-in and device tokens ────────────────────────────────────────────

    /// Checks a username and password, then makes a token for a new device.
    /// The token is returned once; only its hash is kept.
    pub fn login(&self, username: &str, password: &str, device_name: &str) -> Result<(User, Device, String), ApiError> {
        let found: Option<(i64, String, String, String, Option<String>)> = self
            .lock()
            .query_row(
                "SELECT id, username, password_hash, created_at, disabled_at FROM users WHERE username = ?1",
                [username.trim()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        // The hash is checked without holding the DB lock: it is slow on purpose.
        let Some((id, username, hash, created_at, disabled_at)) = found else {
            verify_password(password, dummy_hash());
            return Err(bad_login());
        };
        if !verify_password(password, &hash) || disabled_at.is_some() {
            return Err(bad_login());
        }
        let (device, token) = self.new_device(id, device_name)?;
        Ok((User { id, username, created_at, disabled_at }, device, token))
    }

    /// Signs up: a new user, signed in on this device.
    pub fn register(&self, username: &str, password: &str, device_name: &str) -> Result<(User, Device, String), ApiError> {
        let user = self.add_user(username, password)?;
        let (device, token) = self.new_device(user.id, device_name)?;
        Ok((user, device, token))
    }

    /// A token for a new device; returned once, only its hash is kept.
    fn new_device(&self, user_id: i64, name: &str) -> Result<(Device, String), ApiError> {
        let token = random_token(TOKEN_PREFIX)?;
        let device = Device {
            id: tasks_core::new_id(),
            name: name.to_string(),
            created_at: tasks_core::time::now_iso(),
            last_seen_at: None,
            revoked_at: None,
        };
        self.lock().execute(
            "INSERT INTO devices (id, user_id, name, token_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![device.id, user_id, device.name, hash_token(&token), device.created_at],
        )?;
        Ok((device, token))
    }

    /// The user behind a live token of an enabled user.
    pub fn check(&self, token: &str) -> Result<Option<AuthUser>, ApiError> {
        if !token.starts_with(TOKEN_PREFIX) {
            return Ok(None);
        }
        let conn = self.lock();
        let found: Option<(String, i64, String, Option<String>)> = conn
            .query_row(
                "SELECT d.id, u.id, u.username, d.last_seen_at FROM devices d JOIN users u ON u.id = d.user_id
                 WHERE d.token_hash = ?1 AND d.revoked_at IS NULL AND u.disabled_at IS NULL",
                [hash_token(token)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((device_id, user_id, username, last_seen)) = found else { return Ok(None) };
        // last_seen_at to the minute is plenty; skip the write otherwise.
        let now = tasks_core::time::now_iso();
        if last_seen.as_deref().map(|s| &s[..16.min(s.len())]) != Some(&now[..16]) {
            conn.execute("UPDATE devices SET last_seen_at = ?1 WHERE id = ?2", params![now, device_id])?;
        }
        Ok(Some(AuthUser { user_id, username, device_id }))
    }

    pub fn list_devices(&self, user_id: i64) -> Result<Vec<Device>, ApiError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, created_at, last_seen_at, revoked_at FROM devices WHERE user_id = ?1
             ORDER BY created_at",
        )?;
        let rows = stmt
            .query_map([user_id], |r| {
                Ok(Device {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    last_seen_at: r.get(3)?,
                    revoked_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// False if the user has no such device.
    pub fn revoke(&self, user_id: i64, device_id: &str) -> Result<bool, ApiError> {
        let changed = self.lock().execute(
            "UPDATE devices SET revoked_at = COALESCE(revoked_at, ?1) WHERE id = ?2 AND user_id = ?3",
            params![tasks_core::time::now_iso(), device_id, user_id],
        )?;
        Ok(changed > 0)
    }

    // ── MCP tokens ───────────────────────────────────────────────────────────

    /// A new MCP token; returned once, only its hash is kept. `expires_at` is a
    /// canonical instant (see check_expiry), or None for never.
    pub fn create_mcp_token(
        &self,
        user_id: i64,
        name: &str,
        scope: McpScope,
        expires_at: Option<String>,
    ) -> Result<(McpToken, String), ApiError> {
        let token = random_token(MCP_TOKEN_PREFIX)?;
        let info = McpToken {
            id: tasks_core::new_id(),
            name: name.to_string(),
            scope,
            created_at: tasks_core::time::now_iso(),
            expires_at,
            last_used_at: None,
        };
        self.lock().execute(
            "INSERT INTO mcp_tokens (id, user_id, name, scope, token_hash, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![info.id, user_id, info.name, scope.as_str(), hash_token(&token), info.created_at, info.expires_at],
        )?;
        Ok((info, token))
    }

    /// Newest first, expired ones included.
    pub fn list_mcp_tokens(&self, user_id: i64) -> Result<Vec<McpToken>, ApiError> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, scope, created_at, expires_at, last_used_at FROM mcp_tokens
             WHERE user_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([user_id], mcp_token_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Moves a token's expiry (None: never). None if the user has no such token.
    pub fn set_mcp_token_expiry(&self, user_id: i64, id: &str, expires_at: Option<String>) -> Result<Option<McpToken>, ApiError> {
        let conn = self.lock();
        let changed =
            conn.execute("UPDATE mcp_tokens SET expires_at = ?1 WHERE id = ?2 AND user_id = ?3", params![expires_at, id, user_id])?;
        if changed == 0 {
            return Ok(None);
        }
        let token = conn.query_row(
            "SELECT id, name, scope, created_at, expires_at, last_used_at FROM mcp_tokens WHERE id = ?1",
            [id],
            mcp_token_row,
        )?;
        Ok(Some(token))
    }

    /// Removes a token. False if the user has no such token.
    pub fn delete_mcp_token(&self, user_id: i64, id: &str) -> Result<bool, ApiError> {
        let changed = self.lock().execute("DELETE FROM mcp_tokens WHERE id = ?1 AND user_id = ?2", params![id, user_id])?;
        Ok(changed > 0)
    }

    /// The caller behind an MCP token that exists, hasn't expired, and whose
    /// user isn't disabled.
    pub fn check_mcp(&self, token: &str) -> Result<Option<McpUser>, ApiError> {
        if !token.starts_with(MCP_TOKEN_PREFIX) {
            return Ok(None);
        }
        let now = tasks_core::time::now_iso();
        let conn = self.lock();
        // expires_at is canonical ISO, so it compares as text.
        let found: Option<(String, i64, String, Option<String>)> = conn
            .query_row(
                "SELECT t.id, u.id, t.scope, t.last_used_at FROM mcp_tokens t JOIN users u ON u.id = t.user_id
                 WHERE t.token_hash = ?1 AND u.disabled_at IS NULL AND (t.expires_at IS NULL OR t.expires_at > ?2)",
                params![hash_token(token), now],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((token_id, user_id, scope, last_used)) = found else { return Ok(None) };
        if last_used.as_deref().map(|s| &s[..16.min(s.len())]) != Some(&now[..16]) {
            conn.execute("UPDATE mcp_tokens SET last_used_at = ?1 WHERE id = ?2", params![now, token_id])?;
        }
        Ok(Some(McpUser { user_id, token_id, scope: McpScope::parse(&scope) }))
    }
}

fn mcp_token_row(r: &rusqlite::Row) -> rusqlite::Result<McpToken> {
    Ok(McpToken {
        id: r.get(0)?,
        name: r.get(1)?,
        scope: McpScope::parse(&r.get::<_, String>(2)?),
        created_at: r.get(3)?,
        expires_at: r.get(4)?,
        last_used_at: r.get(5)?,
    })
}

fn no_user(username: &str) -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, format!("No user '{}'", username.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts() -> (tempdir::Dir, Accounts) {
        let dir = tempdir::Dir::new();
        let accounts = Accounts::open(&dir.0.join("accounts.db")).unwrap();
        (dir, accounts)
    }

    #[test]
    fn sign_in_gives_a_token_that_disabling_and_revoking_refuse() {
        let (_dir, a) = accounts();
        let user = a.add_user("akash", "correct horse").unwrap();
        assert!(a.login("akash", "wrong password", "PC").is_err());
        assert!(a.login("nobody", "correct horse", "PC").is_err());

        let (_, device, token) = a.login("AKASH", "correct horse", "PC").unwrap();
        assert_eq!(a.check(&token).unwrap().unwrap().user_id, user.id);
        assert!(a.check("dt_not-a-token").unwrap().is_none());

        // Nothing here disables anyone now, but an older accounts.db can hold disabled users.
        a.lock().execute("UPDATE users SET disabled_at = 'now' WHERE id = ?1", [user.id]).unwrap();
        assert!(a.check(&token).unwrap().is_none());
        assert!(a.login("akash", "correct horse", "PC").is_err());
        a.lock().execute("UPDATE users SET disabled_at = NULL WHERE id = ?1", [user.id]).unwrap();
        assert!(a.check(&token).unwrap().is_some());

        assert!(a.revoke(user.id, &device.id).unwrap());
        assert!(a.check(&token).unwrap().is_none());
    }

    #[test]
    fn signing_up_signs_the_device_in() {
        let (_dir, a) = accounts();
        let (user, _, token) = a.register("new", "password1", "Phone").unwrap();
        assert_eq!(a.check(&token).unwrap().unwrap().user_id, user.id);
        assert!(a.register("NEW", "password1", "Phone").is_err());
    }

    #[test]
    fn users_are_unique_and_get_ids_in_order() {
        let (_dir, a) = accounts();
        assert_eq!(a.add_user("one", "password1").unwrap().id, 1);
        assert!(a.add_user("ONE", "password1").is_err());
        assert!(a.add_user("three", "short").is_err());
        assert_eq!(a.add_user("two", "password1").unwrap().id, 2);
    }

    #[test]
    fn resetting_a_password_replaces_it_and_keeps_devices_signed_in() {
        let (_dir, a) = accounts();
        let (user, _, token) = a.register("akash", "old password", "PC").unwrap();

        assert!(a.set_password("akash", "short").is_err());
        assert!(a.set_password("nobody", "new password").is_err());
        assert!(a.login("akash", "old password", "PC").is_ok());

        a.set_password("AKASH", "new password").unwrap();
        assert!(a.login("akash", "old password", "PC").is_err());
        assert!(a.login("akash", "new password", "PC").is_ok());
        assert_eq!(a.check(&token).unwrap().unwrap().user_id, user.id);
    }

    #[test]
    fn mcp_tokens_work_until_deleted_and_only_as_mcp_tokens() {
        let (_dir, a) = accounts();
        let (user, _, device_token) = a.register("akash", "password1", "PC").unwrap();
        let (info, token) = a.create_mcp_token(user.id, "Claude", McpScope::Read, Some(default_expiry())).unwrap();
        assert!(token.starts_with(MCP_TOKEN_PREFIX));

        let caller = a.check_mcp(&token).unwrap().unwrap();
        assert_eq!((caller.user_id, caller.token_id.as_str(), caller.scope), (user.id, info.id.as_str(), McpScope::Read));
        assert!(a.list_mcp_tokens(user.id).unwrap()[0].last_used_at.is_some());

        // Neither kind of token works in the other's place.
        assert!(a.check(&token).unwrap().is_none());
        assert!(a.check_mcp(&device_token).unwrap().is_none());

        assert!(a.delete_mcp_token(user.id, &info.id).unwrap());
        assert!(a.check_mcp(&token).unwrap().is_none());
        assert!(a.list_mcp_tokens(user.id).unwrap().is_empty());
        assert!(!a.delete_mcp_token(user.id, &info.id).unwrap());
    }

    #[test]
    fn an_expired_mcp_token_is_refused_until_its_expiry_moves() {
        let (_dir, a) = accounts();
        let user = a.add_user("akash", "password1").unwrap();
        let (info, token) =
            a.create_mcp_token(user.id, "Claude", McpScope::ReadWrite, Some("2020-01-01T00:00:00.000Z".into())).unwrap();
        assert!(a.check_mcp(&token).unwrap().is_none());

        let moved = a.set_mcp_token_expiry(user.id, &info.id, Some(default_expiry())).unwrap().unwrap();
        assert!(moved.expires_at.is_some());
        assert!(a.check_mcp(&token).unwrap().unwrap().scope.can_write());

        a.set_mcp_token_expiry(user.id, &info.id, None).unwrap().unwrap();
        assert!(a.check_mcp(&token).unwrap().is_some());

        a.lock().execute("UPDATE users SET disabled_at = 'now' WHERE id = ?1", [user.id]).unwrap();
        assert!(a.check_mcp(&token).unwrap().is_none());
    }

    #[test]
    fn a_user_cannot_touch_another_users_mcp_tokens() {
        let (_dir, a) = accounts();
        let one = a.add_user("one", "password1").unwrap();
        let two = a.add_user("two", "password1").unwrap();
        let (info, token) = a.create_mcp_token(one.id, "Claude", McpScope::Read, None).unwrap();
        assert!(a.set_mcp_token_expiry(two.id, &info.id, None).unwrap().is_none());
        assert!(!a.delete_mcp_token(two.id, &info.id).unwrap());
        assert!(a.list_mcp_tokens(two.id).unwrap().is_empty());
        assert!(a.check_mcp(&token).unwrap().is_some());
    }

    #[test]
    fn an_expiry_must_be_a_future_instant() {
        assert_eq!(check_expiry(None).unwrap(), None);
        assert!(check_expiry(Some("2020-01-01T00:00:00Z")).is_err());
        assert!(check_expiry(Some("next week")).is_err());
        assert_eq!(check_expiry(Some("2999-12-31T23:59:59+00:00")).unwrap().unwrap(), "2999-12-31T23:59:59.000Z");
    }

    /// A throwaway folder, removed afterwards.
    mod tempdir {
        pub struct Dir(pub std::path::PathBuf);

        impl Dir {
            pub fn new() -> Dir {
                let dir = std::env::temp_dir().join(format!("tasks-server-test-{}", tasks_core::new_id()));
                std::fs::create_dir_all(&dir).unwrap();
                Dir(dir)
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
