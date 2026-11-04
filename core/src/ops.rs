//! The tasks rules, one function per API route. Each write runs on the
//! connection it is given; wrap a call in a transaction to make it atomic
//! (the server and the app both do).
//!
//! - A task with no project is in the Inbox.
//! - A subtask is one level deep and shares its parent's project and category.
//! - A time needs a date; clearing the date clears the time.
//! - Moving a category moves its tasks; moving a task moves its subtasks.
//! - Completing a task completes its open subtasks (they aren't logged).
//! - Karma: a tick adds a completion, an untick deletes the task's latest one,
//!   and ticking one occurrence of a repeating task (completes_occurrence)
//!   adds one too.
//! - A delete is a tombstone, and cascades as tombstones: a project takes its
//!   categories and tasks, a task its subtasks; a deleted category's tasks stay
//!   in the project without a category.

use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row, ToSql};
use serde_json::Value;

use crate::model::*;
use crate::sync::{live_ids, tombstone, touch};
use crate::time::{ms_to_iso, now_iso};
use crate::{new_id, Ctx, Error, Result};

// ── Reading rows ─────────────────────────────────────────────────────────────

const PROJECT_COLS: &str = "id, name, color, position, created_at, updated_at";
const CATEGORY_COLS: &str = "id, project_id, name, position, created_at, updated_at";
const FILTER_COLS: &str = "id, name, color, criteria, fields, group_by, position, created_at, updated_at";
const TASK_COLS: &str = "t.id, t.project_id, t.category_id, t.parent_id, t.title, t.description, t.priority, \
    t.due_date, t.due_time, t.duration_minutes, t.location, t.remind_at, t.recurrence, t.position, \
    t.completed_at, t.created_at, t.updated_at";

fn project_row(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        color: r.get(2)?,
        position: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: ms_to_iso(r.get(5)?),
    })
}

fn category_row(r: &Row) -> rusqlite::Result<Category> {
    Ok(Category {
        id: r.get(0)?,
        project_id: r.get(1)?,
        name: r.get(2)?,
        position: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: ms_to_iso(r.get(5)?),
    })
}

fn json_col(text: String, fallback: Value) -> Value {
    serde_json::from_str(&text).unwrap_or(fallback)
}

fn filter_row(r: &Row) -> rusqlite::Result<Filter> {
    Ok(Filter {
        id: r.get(0)?,
        name: r.get(1)?,
        color: r.get(2)?,
        criteria: json_col(
            r.get(3)?,
            serde_json::json!({"projects": [], "categories": [], "priorities": [], "due": []}),
        ),
        fields: json_col(r.get(4)?, Value::Array(vec![])),
        group_by: r.get(5)?,
        position: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: ms_to_iso(r.get(8)?),
    })
}

fn task_row(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        project_id: r.get(1)?,
        category_id: r.get(2)?,
        parent_id: r.get(3)?,
        title: r.get(4)?,
        description: r.get(5)?,
        priority: r.get(6)?,
        due_date: r.get(7)?,
        due_time: r.get(8)?,
        duration_minutes: r.get(9)?,
        location: r.get(10)?,
        remind_at: r.get(11)?,
        recurrence: r.get(12)?,
        position: r.get(13)?,
        completed_at: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: ms_to_iso(r.get(16)?),
    })
}

fn query<T>(
    conn: &Connection,
    sql: &str,
    args: &[&dyn ToSql],
    map: fn(&Row) -> rusqlite::Result<T>,
) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(args, map)?.collect::<rusqlite::Result<Vec<T>>>()?;
    Ok(rows)
}

/// One past the highest position among the live rows matching where, so a new row goes last.
fn next_position(conn: &Connection, table: &str, where_sql: &str, args: &[&dyn ToSql]) -> Result<i64> {
    Ok(conn.query_row(
        &format!("SELECT COALESCE(MAX(position), 0) + 1 FROM {table} WHERE deleted_at IS NULL AND ({where_sql})"),
        args,
        |r| r.get(0),
    )?)
}

/// Applies column changes to one row and stamps it.
fn set_columns(conn: &Connection, ctx: &Ctx, table: &str, id: &str, changes: Vec<(&str, SqlValue)>) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    }
    let sets: Vec<String> = changes.iter().enumerate().map(|(i, (c, _))| format!("{c} = ?{}", i + 1)).collect();
    let mut values: Vec<SqlValue> = changes.into_iter().map(|(_, v)| v).collect();
    values.push(SqlValue::Text(id.to_string()));
    conn.execute(
        &format!("UPDATE {table} SET {} WHERE id = ?{}", sets.join(", "), values.len()),
        params_from_iter(values),
    )?;
    touch(conn, ctx, table, id)
}

fn text(value: Option<String>) -> SqlValue {
    value.map(SqlValue::Text).unwrap_or(SqlValue::Null)
}

fn int(value: Option<i64>) -> SqlValue {
    value.map(SqlValue::Integer).unwrap_or(SqlValue::Null)
}

// ── Projects ─────────────────────────────────────────────────────────────────

pub fn list_projects(conn: &Connection) -> Result<Vec<Project>> {
    query(
        conn,
        &format!("SELECT {PROJECT_COLS} FROM task_projects WHERE deleted_at IS NULL ORDER BY position, id"),
        &[],
        project_row,
    )
}

pub fn get_project(conn: &Connection, id: &str) -> Result<Project> {
    conn.query_row(
        &format!("SELECT {PROJECT_COLS} FROM task_projects WHERE id = ?1 AND deleted_at IS NULL"),
        [id],
        project_row,
    )
    .optional()?
    .ok_or(Error::NotFound("Project not found"))
}

pub fn create_project(conn: &Connection, ctx: &Ctx, input: ProjectIn) -> Result<Project> {
    check_title("name", &input.name)?;
    let position = match input.position.flatten() {
        Some(p) if p != 0 => p,
        _ => next_position(conn, "task_projects", "1", &[])?,
    };
    let id = new_id();
    conn.execute(
        "INSERT INTO task_projects (id, name, color, position, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, input.name, input.color.flatten(), position, now_iso()],
    )?;
    touch(conn, ctx, "task_projects", &id)?;
    get_project(conn, &id)
}

pub fn update_project(conn: &Connection, ctx: &Ctx, id: &str, input: ProjectIn) -> Result<Project> {
    get_project(conn, id)?;
    check_title("name", &input.name)?;
    let mut changes = vec![("name", SqlValue::Text(input.name))];
    if let Some(color) = input.color {
        changes.push(("color", text(color)));
    }
    if let Some(Some(position)) = input.position {
        changes.push(("position", SqlValue::Integer(position)));
    }
    set_columns(conn, ctx, "task_projects", id, changes)?;
    get_project(conn, id)
}

/// Sets positions 1..n in the order given. Projects left out keep their
/// relative order after the listed ones; unknown ids are ignored.
pub fn reorder_projects(conn: &Connection, ctx: &Ctx, ids: &[String]) -> Result<Vec<Project>> {
    let mut rest = list_projects(conn)?;
    let mut ordered = Vec::with_capacity(rest.len());
    for id in ids {
        if let Some(i) = rest.iter().position(|p| &p.id == id) {
            ordered.push(rest.remove(i));
        }
    }
    ordered.append(&mut rest);
    for (i, project) in ordered.iter().enumerate() {
        let position = i as i64 + 1;
        if project.position != position {
            set_columns(conn, ctx, "task_projects", &project.id, vec![("position", SqlValue::Integer(position))])?;
        }
    }
    list_projects(conn)
}

/// Its categories and tasks go with it.
pub fn delete_project(conn: &Connection, ctx: &Ctx, id: &str) -> Result<()> {
    get_project(conn, id)?;
    tombstone(conn, ctx, "task_projects", id)?;
    for cat in live_ids(conn, "task_categories", "project_id = ?1", &[&id])? {
        tombstone(conn, ctx, "task_categories", &cat)?;
    }
    for task in live_ids(conn, "tasks", "project_id = ?1", &[&id])? {
        tombstone(conn, ctx, "tasks", &task)?;
    }
    Ok(())
}

// ── Categories ───────────────────────────────────────────────────────────────

pub fn list_categories(conn: &Connection, project_id: Option<&str>) -> Result<Vec<Category>> {
    match project_id {
        Some(p) => query(
            conn,
            &format!(
                "SELECT {CATEGORY_COLS} FROM task_categories WHERE deleted_at IS NULL AND project_id = ?1
                 ORDER BY project_id, position, id"
            ),
            &[&p],
            category_row,
        ),
        None => query(
            conn,
            &format!(
                "SELECT {CATEGORY_COLS} FROM task_categories WHERE deleted_at IS NULL ORDER BY project_id, position, id"
            ),
            &[],
            category_row,
        ),
    }
}

pub fn get_category(conn: &Connection, id: &str) -> Result<Category> {
    conn.query_row(
        &format!("SELECT {CATEGORY_COLS} FROM task_categories WHERE id = ?1 AND deleted_at IS NULL"),
        [id],
        category_row,
    )
    .optional()?
    .ok_or(Error::NotFound("Category not found"))
}

pub fn create_category(conn: &Connection, ctx: &Ctx, input: CategoryIn) -> Result<Category> {
    check_title("name", &input.name)?;
    get_project(conn, &input.project_id)?;
    let position = match input.position.flatten() {
        Some(p) if p != 0 => p,
        _ => next_position(conn, "task_categories", "project_id = ?1", &[&input.project_id])?,
    };
    let id = new_id();
    conn.execute(
        "INSERT INTO task_categories (id, project_id, name, position, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, input.project_id, input.name, position, now_iso()],
    )?;
    touch(conn, ctx, "task_categories", &id)?;
    get_category(conn, &id)
}

/// A category moves with its tasks.
pub fn update_category(conn: &Connection, ctx: &Ctx, id: &str, input: CategoryIn) -> Result<Category> {
    let item = get_category(conn, id)?;
    check_title("name", &input.name)?;
    if input.project_id != item.project_id {
        get_project(conn, &input.project_id)?;
        for task in live_ids(conn, "tasks", "category_id = ?1", &[&id])? {
            set_columns(conn, ctx, "tasks", &task, vec![("project_id", SqlValue::Text(input.project_id.clone()))])?;
        }
    }
    let mut changes = vec![("project_id", SqlValue::Text(input.project_id)), ("name", SqlValue::Text(input.name))];
    if let Some(Some(position)) = input.position {
        changes.push(("position", SqlValue::Integer(position)));
    }
    set_columns(conn, ctx, "task_categories", id, changes)?;
    get_category(conn, id)
}

/// Its tasks stay in the project, without a category.
pub fn delete_category(conn: &Connection, ctx: &Ctx, id: &str) -> Result<()> {
    get_category(conn, id)?;
    tombstone(conn, ctx, "task_categories", id)?;
    for task in live_ids(conn, "tasks", "category_id = ?1", &[&id])? {
        set_columns(conn, ctx, "tasks", &task, vec![("category_id", SqlValue::Null)])?;
    }
    Ok(())
}

fn check_placement(conn: &Connection, project_id: Option<&str>, category_id: Option<&str>) -> Result<()> {
    if let Some(p) = project_id {
        get_project(conn, p)?;
    }
    if let Some(c) = category_id {
        let category = get_category(conn, c)?;
        if Some(category.project_id.as_str()) != project_id {
            return Err(Error::BadRequest("The category belongs to another project".into()));
        }
    }
    Ok(())
}

// ── Filters ──────────────────────────────────────────────────────────────────
// Saved views of the open tasks. Only the definition is kept; the browser
// matches and groups the tasks. Ids of deleted projects or categories may stay
// in a filter's criteria until it is next saved; the browser ignores them.

pub fn list_filters(conn: &Connection) -> Result<Vec<Filter>> {
    query(
        conn,
        &format!("SELECT {FILTER_COLS} FROM task_filters WHERE deleted_at IS NULL ORDER BY position, id"),
        &[],
        filter_row,
    )
}

pub fn get_filter(conn: &Connection, id: &str) -> Result<Filter> {
    conn.query_row(
        &format!("SELECT {FILTER_COLS} FROM task_filters WHERE id = ?1 AND deleted_at IS NULL"),
        [id],
        filter_row,
    )
    .optional()?
    .ok_or(Error::NotFound("Filter not found"))
}

pub fn create_filter(conn: &Connection, ctx: &Ctx, input: FilterIn) -> Result<Filter> {
    let input = input.cleaned()?;
    let position = match input.position {
        Some(p) if p != 0 => p,
        _ => next_position(conn, "task_filters", "1", &[])?,
    };
    let id = new_id();
    conn.execute(
        "INSERT INTO task_filters (id, name, color, criteria, fields, group_by, position, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            input.name,
            input.color,
            input.criteria_json().to_string(),
            serde_json::to_string(&input.fields)?,
            input.group_by,
            position,
            now_iso()
        ],
    )?;
    touch(conn, ctx, "task_filters", &id)?;
    get_filter(conn, &id)
}

/// A full update; leaving out position (or sending 0) keeps its place.
pub fn update_filter(conn: &Connection, ctx: &Ctx, id: &str, input: FilterIn) -> Result<Filter> {
    get_filter(conn, id)?;
    let input = input.cleaned()?;
    let mut changes = vec![
        ("criteria", SqlValue::Text(input.criteria_json().to_string())),
        ("fields", SqlValue::Text(serde_json::to_string(&input.fields)?)),
        ("name", SqlValue::Text(input.name)),
        ("color", text(input.color)),
        ("group_by", SqlValue::Text(input.group_by)),
    ];
    if let Some(p) = input.position.filter(|p| *p != 0) {
        changes.push(("position", SqlValue::Integer(p)));
    }
    set_columns(conn, ctx, "task_filters", id, changes)?;
    get_filter(conn, id)
}

pub fn delete_filter(conn: &Connection, ctx: &Ctx, id: &str) -> Result<()> {
    get_filter(conn, id)?;
    tombstone(conn, ctx, "task_filters", id)
}

// ── Tasks ────────────────────────────────────────────────────────────────────

/// Open tasks by default, in every project unless project_id is given (or
/// inbox, the tasks with no project). An open task comes with all its
/// subtasks, done ones too, so the page can count them. `completed` returns
/// the completed tasks instead, newest first, up to limit (default 200).
/// `q` searches the open tasks' titles and descriptions instead (global
/// search): soonest due first, then priority, up to limit (default 5).
pub fn list_tasks(conn: &Connection, q: &TaskQuery) -> Result<Vec<Task>> {
    if let Some(limit) = q.limit {
        if !(1..=1000).contains(&limit) {
            return Err(Error::Invalid("limit: Input should be between 1 and 1000".into()));
        }
    }
    let mut sql = format!("SELECT {TASK_COLS} FROM tasks t LEFT JOIN tasks p ON p.id = t.parent_id WHERE t.deleted_at IS NULL");
    let mut args: Vec<SqlValue> = Vec::new();

    if let Some(term) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let like = format!("%{}%", term.to_lowercase());
        sql.push_str(
            " AND t.completed_at IS NULL AND (lower(t.title) LIKE ?1 OR lower(COALESCE(t.description, '')) LIKE ?1)
              ORDER BY t.due_date IS NULL, t.due_date, t.priority, t.id LIMIT ?2",
        );
        args.push(SqlValue::Text(like));
        args.push(SqlValue::Integer(q.limit.unwrap_or(5)));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), task_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        return Ok(rows);
    }

    if let Some(project_id) = &q.project_id {
        args.push(SqlValue::Text(project_id.clone()));
        sql.push_str(&format!(" AND t.project_id = ?{}", args.len()));
    } else if q.inbox {
        sql.push_str(" AND t.project_id IS NULL");
    }
    if q.completed {
        args.push(SqlValue::Integer(q.limit.unwrap_or(200)));
        sql.push_str(&format!(
            " AND t.completed_at IS NOT NULL ORDER BY t.completed_at DESC, t.id DESC LIMIT ?{}",
            args.len()
        ));
    } else {
        sql.push_str(
            " AND (t.parent_id IS NULL OR p.completed_at IS NULL)
              AND (t.completed_at IS NULL OR t.parent_id IS NOT NULL)
              ORDER BY t.position, t.id",
        );
    }
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args), task_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_task(conn: &Connection, id: &str) -> Result<Task> {
    conn.query_row(
        &format!("SELECT {TASK_COLS} FROM tasks t WHERE t.id = ?1 AND t.deleted_at IS NULL"),
        [id],
        task_row,
    )
    .optional()?
    .ok_or(Error::NotFound("Task not found"))
}

/// The open tasks with a reminder at or after `from` (an ISO instant), soonest first.
pub fn upcoming_reminders(conn: &Connection, from: &str) -> Result<Vec<Task>> {
    query(
        conn,
        &format!(
            "SELECT {TASK_COLS} FROM tasks t WHERE t.deleted_at IS NULL AND t.completed_at IS NULL
             AND t.remind_at IS NOT NULL AND t.remind_at >= ?1 ORDER BY t.remind_at"
        ),
        &[&from],
        task_row,
    )
}

/// The open tasks with a reminder in [from, to), soonest first.
pub fn reminders_between(conn: &Connection, from: &str, to: &str) -> Result<Vec<Task>> {
    query(
        conn,
        &format!(
            "SELECT {TASK_COLS} FROM tasks t WHERE t.deleted_at IS NULL AND t.completed_at IS NULL
             AND t.remind_at >= ?1 AND t.remind_at < ?2 ORDER BY t.remind_at"
        ),
        &[&from, &to],
        task_row,
    )
}

pub fn create_task(conn: &Connection, ctx: &Ctx, input: TaskIn) -> Result<Task> {
    check_title("title", &input.title)?;
    let priority = input.priority.unwrap_or(4);
    check_priority(priority)?;
    if let Some(d) = &input.due_date {
        check_due_date(d)?;
    }
    if let Some(t) = &input.due_time {
        check_due_time(t)?;
    }
    if let Some(m) = input.duration_minutes {
        check_duration(m)?;
    }
    if let Some(l) = &input.location {
        check_location(l)?;
    }
    let remind_at = input.remind_at.as_deref().map(|v| instant("remind_at", v)).transpose()?;
    if input.due_time.is_some() && input.due_date.is_none() {
        return Err(Error::BadRequest("A time needs a date".into()));
    }

    let (project_id, category_id) = if let Some(parent_id) = &input.parent_id {
        let parent = get_task(conn, parent_id)?;
        if parent.parent_id.is_some() {
            return Err(Error::BadRequest("Subtasks can't have subtasks".into()));
        }
        (parent.project_id, parent.category_id)
    } else if let (Some(c), None) = (&input.category_id, &input.project_id) {
        // A category alone is enough to place a task.
        (Some(get_category(conn, c)?.project_id), Some(c.clone()))
    } else {
        (input.project_id.clone(), input.category_id.clone())
    };
    check_placement(conn, project_id.as_deref(), category_id.as_deref())?;

    let position = match input.position {
        Some(p) if p != 0 => p,
        _ => next_position(conn, "tasks", "project_id IS ?1 AND parent_id IS ?2", &[&project_id, &input.parent_id])?,
    };
    let id = new_id();
    conn.execute(
        "INSERT INTO tasks (id, project_id, category_id, parent_id, title, description, priority, due_date,
            due_time, duration_minutes, location, remind_at, recurrence, position, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            id,
            project_id,
            category_id,
            input.parent_id,
            input.title,
            input.description,
            priority,
            input.due_date,
            input.due_time,
            input.duration_minutes,
            input.location,
            remind_at,
            input.recurrence,
            position,
            now_iso()
        ],
    )?;
    touch(conn, ctx, "tasks", &id)?;
    get_task(conn, &id)
}

/// Its subtasks go with it.
pub fn delete_task(conn: &Connection, ctx: &Ctx, id: &str) -> Result<()> {
    get_task(conn, id)?;
    tombstone(conn, ctx, "tasks", id)?;
    for sub in live_ids(conn, "tasks", "parent_id = ?1", &[&id])? {
        tombstone(conn, ctx, "tasks", &sub)?;
    }
    Ok(())
}

fn log_completion(conn: &Connection, ctx: &Ctx, task_id: &str, completed_at: &str) -> Result<()> {
    let id = new_id();
    conn.execute(
        "INSERT INTO task_completions (id, task_id, completed_at, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, task_id, completed_at, now_iso()],
    )?;
    touch(conn, ctx, "task_completions", &id)
}

pub fn update_task(conn: &Connection, ctx: &Ctx, id: &str, patch: TaskPatch) -> Result<Task> {
    let item = get_task(conn, id)?;
    let mut p = patch;

    if matches!(p.title, Some(None)) {
        return Err(Error::BadRequest("A task needs a title".into()));
    }
    if let Some(Some(t)) = &p.title {
        check_title("title", t)?;
    }
    if matches!(p.priority, Some(None)) {
        p.priority = Some(Some(4));
    }
    if let Some(Some(v)) = p.priority {
        check_priority(v)?;
    }
    if matches!(p.position, Some(None)) {
        p.position = None;
    }
    if let Some(Some(d)) = &p.due_date {
        check_due_date(d)?;
    }
    if let Some(Some(t)) = &p.due_time {
        check_due_time(t)?;
    }
    if let Some(Some(m)) = p.duration_minutes {
        check_duration(m)?;
    }
    if let Some(Some(l)) = &p.location {
        check_location(l)?;
    }
    if let Some(Some(v)) = &p.remind_at {
        p.remind_at = Some(Some(instant("remind_at", v)?));
    }
    if let Some(Some(v)) = &p.completed_at {
        p.completed_at = Some(Some(instant("completed_at", v)?));
    }
    let completes_occurrence = p.completes_occurrence.unwrap_or(false);

    // A time belongs to a date: clearing the date clears the time, and a time
    // can't be set on a task that has no date.
    if matches!(p.due_date, Some(None)) {
        p.due_time = Some(None);
    } else if matches!(p.due_time, Some(Some(_))) {
        let date = match &p.due_date {
            Some(d) => d.clone(),
            None => item.due_date.clone(),
        };
        if date.is_none() {
            return Err(Error::BadRequest("A time needs a date".into()));
        }
    }

    let mut changes: Vec<(&str, SqlValue)> = Vec::new();
    if item.parent_id.is_some() {
        // A subtask stays where its parent is.
        p.project_id = None;
        p.category_id = None;
    } else if p.project_id.is_some() || p.category_id.is_some() {
        let mut project_id = p.project_id.clone().unwrap_or_else(|| item.project_id.clone());
        let mut category_id = p.category_id.clone().unwrap_or_else(|| item.category_id.clone());
        if p.category_id.is_some() && p.project_id.is_none() {
            if let Some(c) = &category_id {
                project_id = Some(get_category(conn, c)?.project_id);
            }
        } else if p.project_id.is_some() && p.category_id.is_none() && project_id != item.project_id {
            category_id = None; // the old category was in the old project
        }
        check_placement(conn, project_id.as_deref(), category_id.as_deref())?;
        for sub in live_ids(conn, "tasks", "parent_id = ?1", &[&id])? {
            set_columns(
                conn,
                ctx,
                "tasks",
                &sub,
                vec![("project_id", text(project_id.clone())), ("category_id", text(category_id.clone()))],
            )?;
        }
        changes.push(("project_id", text(project_id)));
        changes.push(("category_id", text(category_id)));
    }

    if let Some(Some(done)) = &p.completed_at {
        if item.parent_id.is_none() && item.completed_at.is_none() {
            // Completing a task completes its open subtasks too (they aren't logged on their own).
            for sub in live_ids(conn, "tasks", "parent_id = ?1 AND completed_at IS NULL", &[&id])? {
                set_columns(conn, ctx, "tasks", &sub, vec![("completed_at", SqlValue::Text(done.clone()))])?;
            }
        }
    }

    // The Karma log: a tick adds a row, an untick takes the task's latest one back.
    match &p.completed_at {
        Some(Some(done)) if item.completed_at.is_none() => log_completion(conn, ctx, id, done)?,
        Some(None) if item.completed_at.is_some() => {
            let latest: Option<String> = conn
                .query_row(
                    "SELECT id FROM task_completions WHERE task_id = ?1 AND deleted_at IS NULL
                     ORDER BY completed_at DESC, id DESC LIMIT 1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(latest) = latest {
                tombstone(conn, ctx, "task_completions", &latest)?;
            }
        }
        Some(_) => {}
        None if completes_occurrence => log_completion(conn, ctx, id, &now_iso())?,
        None => {}
    }

    let opt_text = |name: &'static str, v: Option<Option<String>>, out: &mut Vec<(&str, SqlValue)>| {
        if let Some(v) = v {
            out.push((name, text(v)));
        }
    };
    let opt_int = |name: &'static str, v: Option<Option<i64>>, out: &mut Vec<(&str, SqlValue)>| {
        if let Some(v) = v {
            out.push((name, int(v)));
        }
    };
    opt_text("title", p.title, &mut changes);
    opt_text("description", p.description, &mut changes);
    opt_int("priority", p.priority, &mut changes);
    opt_text("due_date", p.due_date, &mut changes);
    opt_text("due_time", p.due_time, &mut changes);
    opt_int("duration_minutes", p.duration_minutes, &mut changes);
    opt_text("location", p.location, &mut changes);
    opt_text("remind_at", p.remind_at, &mut changes);
    opt_text("recurrence", p.recurrence, &mut changes);
    opt_int("position", p.position, &mut changes);
    opt_text("completed_at", p.completed_at, &mut changes);
    if changes.is_empty() {
        // Still a new version (e.g. only completes_occurrence was sent).
        touch(conn, ctx, "tasks", id)?;
    } else {
        set_columns(conn, ctx, "tasks", id, changes)?;
    }
    get_task(conn, id)
}

/// The Karma log since an instant, oldest first. task_id is null once the task is deleted.
pub fn list_completions(conn: &Connection, since: &str) -> Result<Completions> {
    let since = instant("since", since)?;
    let items = query(
        conn,
        "SELECT CASE WHEN t.id IS NOT NULL AND t.deleted_at IS NULL THEN c.task_id END, c.completed_at
         FROM task_completions c LEFT JOIN tasks t ON t.id = c.task_id
         WHERE c.deleted_at IS NULL AND c.completed_at >= ?1 ORDER BY c.completed_at, c.id",
        &[&since],
        |r| Ok(Completion { task_id: r.get(0)?, completed_at: r.get(1)? }),
    )?;
    let total = conn.query_row("SELECT count(*) FROM task_completions WHERE deleted_at IS NULL", [], |r| r.get(0))?;
    Ok(Completions { total, items })
}
