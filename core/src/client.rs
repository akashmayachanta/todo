//! The device side of sync over HTTP (feature `client`, used by the app).
//!
//! [`sync_once`] runs one round: push the dirty rows in batches, then pull
//! everything above the last seq seen, then push again anything the merge's
//! repairs changed. The DB sits behind a std Mutex that is never held across
//! an await.

use std::sync::Mutex;
use std::time::Duration;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::sync::{self, PullResponse, PushRequest, PushResponse};
use crate::Ctx;

const BATCH: usize = 500;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The server couldn't be reached, and we don't know why yet;
    /// [`diagnose`] tells [`ClientError::Offline`] from [`ClientError::ServerDown`].
    #[error("can't reach the server: {0}")]
    Unreachable(String),
    /// This device has no internet connection.
    #[error("offline: {0}")]
    Offline(String),
    /// The internet works but the server doesn't answer: it is stopped, or its
    /// tunnel (ngrok) or proxy says it is offline (502, 503, 504).
    #[error("server down: {0}")]
    ServerDown(String),
    /// 401: the device token was revoked or is wrong; sign in again.
    #[error("signed out: {0}")]
    Unauthorized(String),
    #[error("server said {status}: {detail}")]
    Server { status: u16, detail: String },
    #[error(transparent)]
    Core(#[from] crate::Error),
}

impl From<reqwest::Error> for ClientError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_connect() || e.is_timeout() || e.is_request() {
            ClientError::Unreachable(e.to_string())
        } else {
            ClientError::Server { status: e.status().map(|s| s.as_u16()).unwrap_or(0), detail: e.to_string() }
        }
    }
}

/// Where the server is: its origin (an ngrok domain, or a LAN address) and
/// the server's TASKS_API_BASE_PATH.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerInfo {
    /// e.g. https://example.ngrok.app (no trailing slash).
    pub url: String,
    /// TASKS_API_BASE_PATH (default /tasks-api; empty for the root).
    #[serde(default = "default_tasks_base")]
    pub tasks_base: String,
}

fn default_tasks_base() -> String {
    "/tasks-api".into()
}

impl ServerInfo {
    pub fn tasks_url(&self, route: &str) -> String {
        let base = self.tasks_base.trim().trim_matches('/');
        let base = if base.is_empty() { String::new() } else { format!("/{base}") };
        format!("{}{}{}", self.url.trim_end_matches('/'), base, route)
    }
}

/// Well-known addresses that answer on 443 when the internet works.
const PROBES: [&str; 3] = ["1.1.1.1:443", "8.8.8.8:443", "9.9.9.9:443"];

/// True if this device can reach the internet at all (any probe connects
/// within a few seconds). Tells "offline" from "the server is down".
pub async fn internet_reachable() -> bool {
    let tries = PROBES.iter().map(|addr| async move {
        matches!(
            tokio::time::timeout(Duration::from_secs(4), tokio::net::TcpStream::connect(*addr)).await,
            Ok(Ok(_))
        )
    });
    let mut set = tokio::task::JoinSet::new();
    for t in tries {
        set.spawn(t);
    }
    while let Some(done) = set.join_next().await {
        if matches!(done, Ok(true)) {
            return true;
        }
    }
    false
}

/// Turns [`ClientError::Unreachable`] into [`ClientError::Offline`] (no
/// internet here) or [`ClientError::ServerDown`] (the internet works, the
/// server doesn't answer). Other errors pass through.
pub async fn diagnose(e: ClientError) -> ClientError {
    match e {
        // The raw error just repeats the URL; say what it means instead.
        ClientError::Unreachable(msg) => {
            if internet_reachable().await {
                ClientError::ServerDown("nothing answered at that address".into())
            } else {
                ClientError::Offline(msg)
            }
        }
        other => other,
    }
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    pub server: ServerInfo,
}

#[derive(Deserialize)]
struct Detail {
    detail: serde_json::Value,
}

async fn check(res: reqwest::Response) -> Result<reqwest::Response, ClientError> {
    let status = res.status();
    if status.is_success() {
        return Ok(res);
    }
    // ngrok answers for a tunnel whose server is gone, with its own error code;
    // a proxy in front of a stopped server says 502, 503 or 504; Cloudflare says
    // 520 to 530 (530 when its tunnel, cloudflared, isn't running).
    if res.headers().contains_key("ngrok-error-code") || matches!(status.as_u16(), 502..=504 | 520..=530) {
        let code = res.headers().get("ngrok-error-code").and_then(|v| v.to_str().ok()).map(|c| format!(" ({c})")).unwrap_or_default();
        return Err(ClientError::ServerDown(format!("the server answered {status}{code}")));
    }
    let text = res.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<Detail>(&text)
        .map(|d| match d.detail {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        })
        .unwrap_or(text);
    if status.as_u16() == 401 {
        Err(ClientError::Unauthorized(detail))
    } else {
        Err(ClientError::Server { status: status.as_u16(), detail })
    }
}

/// A signed-in device: its token and whose it is.
#[derive(Deserialize)]
pub struct DeviceOut {
    pub token: String,
    pub user_id: i64,
    pub username: String,
}

impl Client {
    pub fn new(server: ServerInfo) -> Client {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("tasks-app/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Client { http, server }
    }

    /// Signs this device in with a username and password; returns its device token.
    pub async fn login(&self, username: &str, password: &str, device_name: &str) -> Result<DeviceOut, ClientError> {
        self.auth("/auth/login", username, password, device_name).await
    }

    /// Makes a new account and signs this device in to it.
    pub async fn register(&self, username: &str, password: &str, device_name: &str) -> Result<DeviceOut, ClientError> {
        self.auth("/auth/register", username, password, device_name).await
    }

    async fn auth(&self, route: &str, username: &str, password: &str, device_name: &str) -> Result<DeviceOut, ClientError> {
        let res = self
            .http
            .post(self.server.tasks_url(route))
            .json(&serde_json::json!({ "username": username, "password": password, "device_name": device_name }))
            .send()
            .await?;
        Ok(check(res).await?.json().await?)
    }

    pub async fn push(&self, token: &str, req: &PushRequest) -> Result<PushResponse, ClientError> {
        let res = self.http.post(self.server.tasks_url("/sync/push")).bearer_auth(token).json(req).send().await?;
        Ok(check(res).await?.json().await?)
    }

    pub async fn pull(&self, token: &str, since: i64) -> Result<PullResponse, ClientError> {
        let res = self
            .http
            .get(self.server.tasks_url("/sync/pull"))
            .query(&[("since", since.to_string()), ("limit", BATCH.to_string())])
            .bearer_auth(token)
            .send()
            .await?;
        Ok(check(res).await?.json().await?)
    }

    /// The newest build of the app the server has for `platform` ("windows" or
    /// "android"), or None when it has none.
    pub async fn latest_release(&self, token: &str, platform: &str) -> Result<Option<Release>, ClientError> {
        let res = self
            .http
            .get(self.server.tasks_url("/app/latest"))
            .query(&[("platform", platform)])
            .bearer_auth(token)
            .send()
            .await?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(check(res).await?.json().await?))
    }

    /// Downloads that build. It can be tens of MB, so it gets longer than a sync call.
    pub async fn download_release(&self, token: &str, platform: &str) -> Result<Vec<u8>, ClientError> {
        let res = self
            .http
            .get(self.server.tasks_url("/app/download"))
            .query(&[("platform", platform)])
            .bearer_auth(token)
            .timeout(Duration::from_secs(15 * 60))
            .send()
            .await?;
        Ok(check(res).await?.bytes().await?.to_vec())
    }
}

impl Client {
    /// Opens the server's live notices (GET /sync/events). An older server
    /// without them answers 404, a [`ClientError::Server`].
    pub async fn events(&self, token: &str) -> Result<Events, ClientError> {
        // Its own client: the stream stays open, so no overall timeout. The server
        // sends a keep-alive every 20 s, so silence for longer is a dead connection.
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(EVENTS_SILENCE)
            .user_agent(concat!("tasks-app/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        let res = http
            .get(self.server.tasks_url("/sync/events"))
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .bearer_auth(token)
            .send()
            .await?;
        Ok(Events { res: check(res).await?, buf: Vec::new() })
    }
}

/// Longer than this with nothing from the server, not even a keep-alive, and
/// the event stream counts as dead.
const EVENTS_SILENCE: Duration = Duration::from_secs(60);

/// What the server's event stream says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// The stream is open (sent once, first).
    Ready,
    /// Another device changed something: sync.
    Changed,
}

/// An open event stream from [`Client::events`].
pub struct Events {
    res: reqwest::Response,
    buf: Vec<u8>,
}

impl Events {
    /// The next notice, or None when the server closed the stream. Safe to
    /// cancel (e.g. in a `select!`): nothing read is lost.
    pub async fn next(&mut self) -> Result<Option<Notice>, ClientError> {
        loop {
            while let Some(block) = take_event(&mut self.buf) {
                if let Some(notice) = parse_event(&block) {
                    return Ok(Some(notice));
                }
            }
            match self.res.chunk().await? {
                Some(bytes) => self.buf.extend(bytes.iter().filter(|b| **b != b'\r')),
                None => return Ok(None),
            }
        }
    }
}

/// Takes one whole event (up to its blank line) off the front of `buf`.
fn take_event(buf: &mut Vec<u8>) -> Option<String> {
    let end = buf.windows(2).position(|w| w == b"\n\n")?;
    let block = String::from_utf8_lossy(&buf[..end]).into_owned();
    buf.drain(..end + 2);
    Some(block)
}

/// An event's notice; None for keep-alive comments and events this app doesn't know.
fn parse_event(block: &str) -> Option<Notice> {
    let name = block.lines().find_map(|line| line.strip_prefix("event:")).map(str::trim)?;
    match name {
        "ready" => Some(Notice::Ready),
        "changed" => Some(Notice::Changed),
        _ => None,
    }
}

/// A build of the app on the server (GET /app/latest).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SyncReport {
    pub pushed: usize,
    pub pulled: usize,
    /// Pulled rows that replaced the local version.
    pub changed: usize,
    pub seq: i64,
}

fn lock(db: &Mutex<Connection>) -> std::sync::MutexGuard<'_, Connection> {
    db.lock().unwrap_or_else(|e| e.into_inner())
}

async fn push_all(client: &Client, token: &str, db: &Mutex<Connection>, ctx: &Ctx) -> Result<usize, ClientError> {
    let mut pushed = 0;
    loop {
        let rows = sync::dirty_rows(&lock(db), BATCH)?;
        if rows.is_empty() {
            return Ok(pushed);
        }
        let n = rows.len();
        let req = PushRequest { device_id: ctx.device_id.clone(), rows };
        client.push(token, &req).await?;
        sync::mark_pushed(&lock(db), &req.rows)?;
        pushed += n;
        if n < BATCH {
            return Ok(pushed);
        }
    }
}

/// One sync round. Safe to call while the user keeps editing: a row changed
/// during the push stays dirty and goes next time. `report` is filled as it goes,
/// so a round that fails partway still tells what it already changed.
pub async fn sync_once(client: &Client, token: &str, db: &Mutex<Connection>, ctx: &Ctx, report: &mut SyncReport) -> Result<(), ClientError> {
    report.pushed += push_all(client, token, db, ctx).await?;
    let mut restarted = false;
    loop {
        let since = sync::last_pulled(&lock(db))?;
        let page = client.pull(token, since).await?;
        {
            let conn = lock(db);
            // A replaced server DB (new epoch) or one restored from an older copy
            // (its counter is behind ours): start over from 0 and push everything.
            let known = crate::db::get_state(&conn, "server_epoch")?;
            let replaced = matches!((&known, &page.epoch), (Some(k), Some(e)) if k != e);
            let rewound = page.seq.is_some_and(|seq| seq < since);
            if let Some(e) = &page.epoch {
                crate::db::set_state(&conn, "server_epoch", e)?;
            }
            if (replaced || rewound) && !restarted {
                sync::reset_device(&conn)?;
                restarted = true;
                continue;
            }
            let tx = conn.unchecked_transaction().map_err(crate::Error::from)?;
            let applied = sync::apply(&tx, ctx, &page.rows)?;
            sync::set_last_pulled(&tx, page.next)?;
            tx.commit().map_err(crate::Error::from)?;
            report.pulled += page.rows.len();
            report.changed += applied.accepted;
        }
        report.seq = page.next;
        if !page.more || page.rows.is_empty() {
            break;
        }
    }
    // Repairs made by the merge (e.g. a task whose project was deleted elsewhere).
    report.pushed += push_all(client, token, db, ctx).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notices(stream: &str) -> Vec<Notice> {
        let mut buf: Vec<u8> = stream.bytes().filter(|b| *b != b'\r').collect();
        std::iter::from_fn(|| take_event(&mut buf)).filter_map(|b| parse_event(&b)).collect()
    }

    #[test]
    fn reads_the_servers_events() {
        let stream = "event: ready\ndata: {}\n\n:\n\nevent: changed\ndata: {}\n\nevent: other\ndata: x\n\n";
        assert_eq!(notices(stream), vec![Notice::Ready, Notice::Changed]);
    }

    #[test]
    fn handles_crlf_and_an_unfinished_event() {
        assert_eq!(notices("event:changed\r\ndata: {}\r\n\r\nevent: changed\n"), vec![Notice::Changed]);
    }
}
