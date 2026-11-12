//! Opening a tasks DB and keeping its schema current (PRAGMA user_version).

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use crate::Result;

/// The schema migrations, oldest first. Entry N moves a DB from
/// `user_version` N to N+1, so the order is the version history.
///
/// To change the schema, add `core/migrations/NNNN_<what>.sql` and append it
/// here. Never edit or reorder a migration that has shipped: DBs already past
/// it won't run it again.
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_initial", include_str!("../migrations/0001_initial.sql")),
    ("0002_profile", include_str!("../migrations/0002_profile.sql")),
    ("0003_project_details", include_str!("../migrations/0003_project_details.sql")),
];

/// Opens (creating if needed) a tasks DB and migrates it to the newest schema.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    migrate(&conn)?;
    Ok(conn)
}

/// An in-memory DB on the newest schema.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    run_migrations(conn, MIGRATIONS)
}

/// Brings any SQLite DB up to date with a list of `(name, sql)` migrations,
/// oldest first: runs the ones past its `user_version`, each in a transaction.
/// The server uses it for its accounts DB too.
pub fn run_migrations(conn: &Connection, migrations: &[(&str, &str)]) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, (_name, sql)) in migrations.iter().enumerate().skip(version.max(0) as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn get_state(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM sync_state WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn set_state(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_state (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

/// This device's id, made on first use.
pub fn device_id(conn: &Connection) -> Result<String> {
    if let Some(id) = get_state(conn, "device_id")? {
        return Ok(id);
    }
    let id = uuid::Uuid::new_v4().to_string();
    set_state(conn, "device_id", &id)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_bring_a_new_db_to_the_latest_version() {
        let conn = open_in_memory().unwrap();
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        // Running again on an up-to-date DB is a no-op.
        migrate(&conn).unwrap();
    }
}
