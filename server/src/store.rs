//! One tasks DB per user, opened on first use and kept open.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::config::Config;
use crate::error::ApiError;

pub struct Store {
    config: Config,
    conns: Mutex<HashMap<i64, Arc<Mutex<Connection>>>>,
}

impl Store {
    pub fn new(config: Config) -> Store {
        Store { config, conns: Mutex::new(HashMap::new()) }
    }

    fn conn(&self, user_id: i64) -> Result<Arc<Mutex<Connection>>, ApiError> {
        let mut conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = conns.get(&user_id) {
            return Ok(c.clone());
        }
        let conn = tasks_core::db::open(&self.config.user_db(user_id))?;
        let conn = Arc::new(Mutex::new(conn));
        conns.insert(user_id, conn.clone());
        Ok(conn)
    }

    /// Runs `f` on the user's DB in one transaction, off the async runtime.
    /// An error rolls everything back.
    pub async fn run<T, F>(self: &Arc<Self>, user_id: i64, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> tasks_core::Result<T> + Send + 'static,
    {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let conn = store.conn(user_id)?;
            let conn = conn.lock().unwrap_or_else(|e| e.into_inner());
            let tx = conn.unchecked_transaction()?;
            let out = f(&tx)?;
            tx.commit()?;
            Ok(out)
        })
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?
    }
}
