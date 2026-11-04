//! Sync between the server and the devices.
//!
//! Every row carries `updated_at` (ms) and `updated_by` (device id). The merge
//! rule is last write wins per row: an incoming version replaces the stored one
//! only if its `(updated_at, updated_by)` is greater. A delete is a version too
//! (`deleted_at` set), so it travels like any edit.
//!
//! - A device marks each local change `dirty`. A sync pushes the dirty rows
//!   ([`dirty_rows`]), the server merges them ([`apply`]) and gives each
//!   accepted row the next `server_seq`; the device then clears the flag on the
//!   rows that haven't changed since ([`mark_pushed`]).
//! - The device pulls the rows above the last `server_seq` it saw
//!   ([`changes_since`]) and merges them the same way. A local change still
//!   waiting to be pushed survives a pull if it is the later one.
//!
//! After a merge, [`repair`] fixes rows whose parent was deleted on another
//! device in the meantime (a task added offline to a project deleted elsewhere).

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use crate::time::now_ms;
use crate::{Ctx, Error, Result, Role};

pub struct Table {
    pub name: &'static str,
    /// The data columns, id first.
    pub columns: &'static [&'static str],
    /// Columns that must be present (NOT NULL without a default).
    pub required: &'static [&'static str],
}

/// The synced tables, parents before children (the order a push sends them).
pub const TABLES: &[Table] = &[
    Table {
        name: "task_projects",
        columns: &["id", "name", "color", "position", "created_at"],
        required: &["name", "created_at"],
    },
    Table {
        name: "task_categories",
        columns: &["id", "project_id", "name", "position", "created_at"],
        required: &["project_id", "name", "created_at"],
    },
    Table {
        name: "tasks",
        columns: &[
            "id", "project_id", "category_id", "parent_id", "title", "description", "priority", "due_date",
            "due_time", "duration_minutes", "location", "remind_at", "recurrence", "position", "completed_at",
            "created_at",
        ],
        required: &["title", "created_at"],
    },
    Table {
        name: "task_filters",
        columns: &["id", "name", "color", "criteria", "fields", "group_by", "position", "created_at"],
        required: &["name", "criteria", "fields", "created_at"],
    },
    Table {
        name: "task_completions",
        columns: &["id", "task_id", "completed_at", "created_at"],
        required: &["completed_at", "created_at"],
    },
];

const SYNC_COLUMNS: &[&str] = &["updated_at", "updated_by", "deleted_at"];

fn table(name: &str) -> Result<&'static Table> {
    TABLES
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| Error::Invalid(format!("unknown table '{name}'")))
}

/// One version of one row. `row` holds the data columns plus updated_at,
/// updated_by and deleted_at. `seq` is the server_seq it was accepted at
/// (set on rows the server sends; ignored on a push).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncRow {
    pub table: String,
    pub row: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushRequest {
    pub device_id: String,
    pub rows: Vec<SyncRow>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PushResponse {
    /// Rows that replaced the server's version.
    pub accepted: usize,
    /// Rows the server already had a later version of (it comes back on the next pull).
    pub ignored: usize,
    /// The server's change counter after the push.
    pub seq: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PullResponse {
    pub rows: Vec<SyncRow>,
    /// Pull again from here.
    pub next: i64,
    /// More rows are waiting above `next`.
    pub more: bool,
    /// Names this server DB; a new one (an import with --replace) means start over.
    #[serde(default)]
    pub epoch: Option<String>,
    /// The server's change counter now; below the device's last seq means the DB
    /// was restored from an older copy.
    #[serde(default)]
    pub seq: Option<i64>,
}

// ── Stamping local writes ────────────────────────────────────────────────────

/// The server's next change number.
pub(crate) fn next_seq(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "INSERT INTO sync_state (key, value) VALUES ('seq', '1')
         ON CONFLICT (key) DO UPDATE SET value = CAST(value AS INTEGER) + 1
         RETURNING CAST(value AS INTEGER)",
        [],
        |r| r.get(0),
    )?)
}

/// The id of this server DB, made on first use.
pub fn epoch(conn: &Connection) -> Result<String> {
    if let Some(e) = crate::db::get_state(conn, "epoch")? {
        return Ok(e);
    }
    let e = uuid::Uuid::new_v4().to_string();
    crate::db::set_state(conn, "epoch", &e)?;
    Ok(e)
}

/// After the server DB was replaced or restored: pull everything again, and
/// push every local row again (the server may have lost the latest versions).
pub fn reset_device(conn: &Connection) -> Result<()> {
    for t in TABLES {
        conn.execute(&format!("UPDATE {} SET dirty = 1", t.name), [])?;
    }
    set_last_pulled(conn, 0)
}

/// The server's change counter now.
pub fn current_seq(conn: &Connection) -> Result<i64> {
    Ok(crate::db::get_state(conn, "seq")?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Stamps a row just written: a new version by this device. The time never
/// goes backwards for a row, so a local edit always beats the version it was
/// made on, even if this device's clock is behind.
pub(crate) fn touch(conn: &Connection, ctx: &Ctx, table: &str, id: &str) -> Result<()> {
    let now = now_ms();
    match ctx.role {
        Role::Server => {
            let seq = next_seq(conn)?;
            conn.execute(
                &format!(
                    "UPDATE {table} SET updated_at = MAX(?1, updated_at + 1), updated_by = ?2,
                     server_seq = ?3, dirty = 0 WHERE id = ?4"
                ),
                params![now, ctx.device_id, seq, id],
            )?;
        }
        Role::Device => {
            conn.execute(
                &format!(
                    "UPDATE {table} SET updated_at = MAX(?1, updated_at + 1), updated_by = ?2, dirty = 1
                     WHERE id = ?3"
                ),
                params![now, ctx.device_id, id],
            )?;
        }
    }
    Ok(())
}

/// Marks a row deleted (a tombstone), if it isn't already.
pub(crate) fn tombstone(conn: &Connection, ctx: &Ctx, table: &str, id: &str) -> Result<()> {
    let changed = conn.execute(
        &format!("UPDATE {table} SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL"),
        params![now_ms(), id],
    )?;
    if changed > 0 {
        touch(conn, ctx, table, id)?;
    }
    Ok(())
}

/// Ids of the live rows matching a WHERE clause.
pub(crate) fn live_ids(conn: &Connection, table: &str, where_sql: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("SELECT id FROM {table} WHERE deleted_at IS NULL AND ({where_sql})"))?;
    let ids = stmt.query_map(args, |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(ids)
}

// ── Rows as JSON ─────────────────────────────────────────────────────────────

fn to_json(value: SqlValue) -> Value {
    match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => Value::Number(i.into()),
        SqlValue::Real(f) => Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null),
        SqlValue::Text(s) => Value::String(s),
        SqlValue::Blob(_) => Value::Null,
    }
}

fn to_sql(value: &Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::Number(n) => n
            .as_i64()
            .map(SqlValue::Integer)
            .or_else(|| n.as_f64().map(SqlValue::Real))
            .unwrap_or(SqlValue::Null),
        Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

/// The rows of one table matching a WHERE clause, as sync rows.
fn select_rows(conn: &Connection, t: &Table, where_sql: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<SyncRow>> {
    let cols: Vec<&str> = t.columns.iter().chain(SYNC_COLUMNS).copied().collect();
    let sql = format!("SELECT {}, server_seq FROM {} WHERE {}", cols.join(", "), t.name, where_sql);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(args, |r| {
            let mut row = Map::new();
            for (i, col) in cols.iter().enumerate() {
                row.insert((*col).to_string(), to_json(r.get::<_, SqlValue>(i)?));
            }
            let seq: Option<i64> = r.get(cols.len())?;
            Ok(SyncRow { table: t.name.to_string(), row, seq })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ── Device side ──────────────────────────────────────────────────────────────

/// The local changes not pushed yet, parents first.
pub fn dirty_rows(conn: &Connection, limit: usize) -> Result<Vec<SyncRow>> {
    let mut out = Vec::new();
    for t in TABLES {
        let left = limit.saturating_sub(out.len());
        if left == 0 {
            break;
        }
        let mut rows = select_rows(conn, t, "dirty = 1 ORDER BY updated_at LIMIT ?1", &[&(left as i64)])?;
        out.append(&mut rows);
    }
    Ok(out)
}

/// After a successful push: clears `dirty` on the rows that still hold the
/// version that was sent (one edited during the push stays dirty).
pub fn mark_pushed(conn: &Connection, rows: &[SyncRow]) -> Result<()> {
    for r in rows {
        let t = table(&r.table)?;
        let id = r.row.get("id").and_then(Value::as_str).unwrap_or_default();
        let updated_at = r.row.get("updated_at").and_then(Value::as_i64).unwrap_or_default();
        conn.execute(
            &format!("UPDATE {} SET dirty = 0 WHERE id = ?1 AND updated_at = ?2", t.name),
            params![id, updated_at],
        )?;
    }
    Ok(())
}

/// The highest server_seq this device has pulled.
pub fn last_pulled(conn: &Connection) -> Result<i64> {
    Ok(crate::db::get_state(conn, "last_seq")?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

pub fn set_last_pulled(conn: &Connection, seq: i64) -> Result<()> {
    crate::db::set_state(conn, "last_seq", &seq.to_string())
}

/// How many local changes wait to be pushed.
pub fn pending_count(conn: &Connection) -> Result<i64> {
    let mut total = 0;
    for t in TABLES {
        total += conn.query_row(&format!("SELECT count(*) FROM {} WHERE dirty = 1", t.name), [], |r| {
            r.get::<_, i64>(0)
        })?;
    }
    Ok(total)
}

// ── Server side ──────────────────────────────────────────────────────────────

/// The rows accepted after `since`, oldest first, up to `limit`.
pub fn changes_since(conn: &Connection, since: i64, limit: usize) -> Result<PullResponse> {
    let limit = limit.max(1);
    let mut rows = Vec::new();
    for t in TABLES {
        let mut found = select_rows(
            conn,
            t,
            "server_seq > ?1 ORDER BY server_seq LIMIT ?2",
            &[&since, &((limit + 1) as i64)],
        )?;
        rows.append(&mut found);
    }
    rows.sort_by_key(|r| r.seq.unwrap_or(0));
    let more = rows.len() > limit;
    rows.truncate(limit);
    let next = rows.last().and_then(|r| r.seq).unwrap_or(since).max(since);
    Ok(PullResponse { rows, next, more, epoch: Some(epoch(conn)?), seq: Some(current_seq(conn)?) })
}

// ── Merge (both sides) ───────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy)]
pub struct Applied {
    pub accepted: usize,
    pub ignored: usize,
}

/// Merges incoming versions: each replaces the stored row only if it is the
/// later one. On the server an accepted row gets the next server_seq; on a
/// device it keeps the seq the server sent and is no longer dirty.
pub fn apply(conn: &Connection, ctx: &Ctx, rows: &[SyncRow]) -> Result<Applied> {
    let mut applied = Applied::default();
    for r in rows {
        let t = table(&r.table)?;
        let id = r
            .row
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::Invalid(format!("{}: a row without an id", t.name)))?;
        let updated_at = r
            .row
            .get("updated_at")
            .and_then(Value::as_i64)
            .ok_or_else(|| Error::Invalid(format!("{}.{id}: updated_at is missing", t.name)))?;
        let updated_by = r.row.get("updated_by").and_then(Value::as_str).unwrap_or_default();
        for col in t.required {
            if r.row.get(*col).is_none_or(Value::is_null) {
                return Err(Error::Invalid(format!("{}.{id}: {col} is missing", t.name)));
            }
        }

        let stored: Option<(i64, String)> = conn
            .query_row(&format!("SELECT updated_at, updated_by FROM {} WHERE id = ?1", t.name), [id], |x| {
                Ok((x.get(0)?, x.get(1)?))
            })
            .optional()?;
        if let Some((at, by)) = stored {
            if (at, by.as_str()) >= (updated_at, updated_by) {
                applied.ignored += 1;
                continue;
            }
        }

        let seq = match ctx.role {
            Role::Server => Some(next_seq(conn)?),
            Role::Device => r.seq,
        };
        let cols: Vec<&str> = t.columns.iter().chain(SYNC_COLUMNS).copied().collect();
        let mut values: Vec<SqlValue> = cols
            .iter()
            .map(|c| match *c {
                "updated_at" => SqlValue::Integer(updated_at),
                "updated_by" => SqlValue::Text(updated_by.to_string()),
                _ => r.row.get(*c).map(to_sql).unwrap_or(SqlValue::Null),
            })
            .collect();
        values.push(seq.map(SqlValue::Integer).unwrap_or(SqlValue::Null));
        let placeholders: Vec<String> = (1..=cols.len() + 1).map(|i| format!("?{i}")).collect();
        let updates: Vec<String> = cols.iter().skip(1).map(|c| format!("{c} = excluded.{c}")).collect();
        let sql = format!(
            "INSERT INTO {table} ({cols}, server_seq, dirty) VALUES ({ph}, 0)
             ON CONFLICT (id) DO UPDATE SET {updates}, server_seq = excluded.server_seq, dirty = 0",
            table = t.name,
            cols = cols.join(", "),
            ph = placeholders.join(", "),
            updates = updates.join(", "),
        );
        conn.execute(&sql, params_from_iter(values))?;
        applied.accepted += 1;
    }
    repair(conn, ctx)?;
    Ok(applied)
}

/// Fixes rows a merge left inconsistent. Only a parent that is here and
/// deleted counts: one that hasn't arrived yet may come in the next page.
/// - categories and tasks of a deleted project are deleted;
/// - subtasks of a deleted task are deleted;
/// - tasks in a deleted category move out of it;
/// - a task follows its category's project;
/// - a subtask follows its parent's project and category.
pub fn repair(conn: &Connection, ctx: &Ctx) -> Result<()> {
    let deleted_project = "project_id IN (SELECT id FROM task_projects WHERE deleted_at IS NOT NULL)";
    for id in live_ids(conn, "task_categories", deleted_project, &[])? {
        tombstone(conn, ctx, "task_categories", &id)?;
    }
    for id in live_ids(conn, "tasks", deleted_project, &[])? {
        tombstone(conn, ctx, "tasks", &id)?;
    }
    for id in live_ids(conn, "tasks", "parent_id IN (SELECT id FROM tasks WHERE deleted_at IS NOT NULL)", &[])? {
        tombstone(conn, ctx, "tasks", &id)?;
    }
    for id in live_ids(
        conn,
        "tasks",
        "category_id IN (SELECT id FROM task_categories WHERE deleted_at IS NOT NULL)",
        &[],
    )? {
        conn.execute("UPDATE tasks SET category_id = NULL WHERE id = ?1", [&id])?;
        touch(conn, ctx, "tasks", &id)?;
    }
    // A task follows its category's project (a category moved on one device
    // while the task was edited on another would leave it in neither).
    let mut stmt = conn.prepare(
        "SELECT t.id, c.project_id FROM tasks t JOIN task_categories c ON c.id = t.category_id
         WHERE t.deleted_at IS NULL AND c.deleted_at IS NULL AND t.parent_id IS NULL
           AND t.project_id IS NOT c.project_id",
    )?;
    let moved = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (id, project_id) in moved {
        conn.execute("UPDATE tasks SET project_id = ?1 WHERE id = ?2", params![project_id, id])?;
        touch(conn, ctx, "tasks", &id)?;
    }
    let mut stmt = conn.prepare(
        "SELECT t.id, p.project_id, p.category_id FROM tasks t JOIN tasks p ON p.id = t.parent_id
         WHERE t.deleted_at IS NULL AND p.deleted_at IS NULL
           AND (t.project_id IS NOT p.project_id OR t.category_id IS NOT p.category_id)",
    )?;
    let strays = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, project_id, category_id) in strays {
        conn.execute(
            "UPDATE tasks SET project_id = ?1, category_id = ?2 WHERE id = ?3",
            params![project_id, category_id, id],
        )?;
        touch(conn, ctx, "tasks", &id)?;
    }
    Ok(())
}
