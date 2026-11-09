//! App updates. copy-release.bat puts each new build next to tasks-server.exe,
//! with a file beside it holding its version:
//!
//! - Windows: tasks-app.exe and tasks-app.exe.version
//! - Android: tasks.apk and tasks.apk.version
//!
//! A device asks GET /app/latest?platform=… when the app opens and, if the user
//! says yes, downloads the file from GET /app/download?platform=…. The SHA-256
//! lets the app check the download, and tells two builds of one version apart.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{ApiError, ApiResult};

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Android,
}

impl Platform {
    fn file_name(self) -> &'static str {
        match self {
            Platform::Windows => "tasks-app.exe",
            Platform::Android => "tasks.apk",
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            Platform::Windows => "application/vnd.microsoft.portable-executable",
            Platform::Android => "application/vnd.android.package-archive",
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Release {
    pub version: String,
    pub sha256: String,
    pub size: u64,
}

/// The release files' folder (tasks-server.exe's own), and each file's hash,
/// worked out again only when its size or time changes.
pub struct Releases {
    dir: PathBuf,
    hashes: Mutex<HashMap<Platform, (u64, SystemTime, String)>>,
}

impl Releases {
    pub fn new() -> Releases {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
        Releases { dir, hashes: Mutex::new(HashMap::new()) }
    }

    fn path(&self, p: Platform) -> PathBuf {
        self.dir.join(p.file_name())
    }

    fn not_found(p: Platform) -> ApiError {
        ApiError::new(StatusCode::NOT_FOUND, format!("The server has no {} to hand out", p.file_name()))
    }

    /// The release for `p`, or 404 when its file or version file is missing.
    pub fn latest(&self, p: Platform) -> ApiResult<Release> {
        let path = self.path(p);
        let meta = std::fs::metadata(&path).map_err(|_| Self::not_found(p))?;
        let version_file = self.dir.join(format!("{}.version", p.file_name()));
        let version = std::fs::read_to_string(&version_file).map_err(|_| Self::not_found(p))?.trim().to_string();
        if version.is_empty() {
            return Err(Self::not_found(p));
        }
        let size = meta.len();
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let cached = self.hashes.lock().unwrap_or_else(|e| e.into_inner()).get(&p).cloned();
        let sha256 = match cached {
            Some((s, m, hash)) if s == size && m == modified => hash,
            _ => {
                let bytes = std::fs::read(&path).map_err(|_| Self::not_found(p))?;
                let hash = hex(&Sha256::digest(&bytes));
                self.hashes.lock().unwrap_or_else(|e| e.into_inner()).insert(p, (size, modified, hash.clone()));
                hash
            }
        };
        Ok(Release { version, sha256, size })
    }

    pub fn download(&self, p: Platform) -> ApiResult<Response> {
        let bytes = std::fs::read(self.path(p)).map_err(|_| Self::not_found(p))?;
        let disposition = format!("attachment; filename=\"{}\"", p.file_name());
        Ok((
            [(header::CONTENT_TYPE, p.content_type().to_string()), (header::CONTENT_DISPOSITION, disposition)],
            bytes,
        )
            .into_response())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
