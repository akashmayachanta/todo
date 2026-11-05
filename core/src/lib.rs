//! Tasks: projects, their categories (sections), tasks, saved filters and the
//! Karma log. The rules live here once, so the server (REST API for the web
//! page) and the apps (Tauri commands on a local DB) can't drift apart.
//!
//! Every write goes through [`Ctx`], which stamps the row for sync: on the
//! server it gets the next `server_seq`, on a device it is marked `dirty` until
//! it is pushed. See `core/migrations/` for the columns and `sync` for the
//! merge rule.

#[cfg(feature = "client")]
pub mod client;
pub mod db;
pub mod model;
pub mod ops;
pub mod sync;
pub mod time;

pub use model::*;

/// Who is writing: the sync server, or one device's local DB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Server,
    Device,
}

/// Context for a write: the role, and the device id written to `updated_by`.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub role: Role,
    pub device_id: String,
}

impl Ctx {
    pub fn server() -> Self {
        Ctx { role: Role::Server, device_id: SERVER_DEVICE_ID.to_string() }
    }

    pub fn device(device_id: impl Into<String>) -> Self {
        Ctx { role: Role::Device, device_id: device_id.into() }
    }
}

/// `updated_by` for edits made through the server's REST API.
pub const SERVER_DEVICE_ID: &str = "server";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 404, e.g. "Task not found".
    #[error("{0}")]
    NotFound(&'static str),
    /// 400, a rule was broken, e.g. "A time needs a date".
    #[error("{0}")]
    BadRequest(String),
    /// 422, the input didn't validate.
    #[error("{0}")]
    Invalid(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// The HTTP status the server answers with.
    pub fn status(&self) -> u16 {
        match self {
            Error::NotFound(_) => 404,
            Error::BadRequest(_) => 400,
            Error::Invalid(_) => 422,
            Error::Db(_) | Error::Json(_) => 500,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A new row id: UUIDv7, so ids made on different devices never collide and
/// sort by creation time.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// A title as read, for a notification: "[Talk](https://…) notes" is "Talk notes".
pub fn plain_title(title: &str) -> String {
    let mut out = String::new();
    let mut rest = title;
    while let Some(mid) = rest.find("](http") {
        let (Some(open), Some(close)) = (rest[..mid].rfind('['), rest[mid..].find(')')) else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..mid]);
        rest = &rest[mid + close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod title_tests {
    use super::plain_title;

    #[test]
    fn plain_title_keeps_only_the_labels() {
        assert_eq!(plain_title("Plain"), "Plain");
        assert_eq!(plain_title("[Talk](https://a.b/c?d=1) notes [x](http://e)"), "Talk notes x");
        assert_eq!(plain_title("Odd ](https://a.b"), "Odd ](https://a.b");
    }
}
