//! What Android's home screen widget shows (TasksWidget.kt): the lists it can
//! show, and one list's open tasks, as JSON. Kotlin calls these through
//! android.rs, from the widget's own process when the app is closed.
//!
//! A widget shows Today (overdue and due today), Inbox, Upcoming, a project or
//! one of its sections, ordered as the page orders them.

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime};
use rusqlite::Connection;
use serde_json::{json, Value};
use tasks_core::{Task, TaskQuery};

/// A widget lists this many at most; "Open Tasks" shows the rest.
const MAX_TASKS: usize = 100;

/// The lists a widget can show, for its picker: `[{kind, id, name, project?, color?, open, add}]`
/// (`open` and `add` as in [`list`]). Each project comes before its sections;
/// `color` is a project's colour key (PROJECT_COLORS in the app's shared.ts).
pub fn sources(conn: &Connection) -> tasks_core::Result<Value> {
    let source = |kind: &str, id: &str, name: &str, project: Option<&str>, project_id: &str, color: Option<&str>| {
        let (open, add) = target(kind, id, project_id);
        json!({ "kind": kind, "id": id, "name": name, "project": project, "color": color, "open": open, "add": add })
    };
    let mut out = vec![
        source("today", "", "Today", None, "", None),
        source("inbox", "", "Inbox", None, "", None),
        source("upcoming", "", "Upcoming", None, "", None),
    ];
    let sections = tasks_core::ops::list_sections(conn, None)?;
    for p in tasks_core::ops::list_projects(conn)? {
        out.push(source("project", &p.id, &p.name, None, &p.id, p.color.as_deref()));
        let mut own: Vec<_> = sections.iter().filter(|c| c.project_id == p.id).collect();
        own.sort_by(|a, b| a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id)));
        for c in own {
            out.push(source("section", &c.id, &c.name, Some(&p.name), &p.id, p.color.as_deref()));
        }
    }
    Ok(Value::Array(out))
}

/// A list's Tasks page query string, and what a task added to it starts with.
fn target(kind: &str, id: &str, project_id: &str) -> (String, Value) {
    match kind {
        "inbox" => ("view=inbox".into(), json!({ "project_id": null })),
        "upcoming" => ("view=upcoming".into(), json!({ "due": "tomorrow" })),
        "project" => (format!("project={id}"), json!({ "project_id": id })),
        "section" => (format!("project={project_id}#task-cat-{id}"), json!({ "project_id": project_id, "section_id": id })),
        _ => ("view=today".into(), json!({ "due": "today" })),
    }
}

/// One list for the widget:
/// `{name, gone, open, add: {project_id?, section_id?, due?}, more, tasks: [{id, title, due, overdue, tone, priority}]}`.
/// `tone` colours the due label as the page does ([`due_tone`]).
/// `open` is the Tasks page's query string for the list (the widget's title opens it).
/// `gone` when its project or section was deleted. `add` is what a task added
/// from the widget starts with (`due` is "today" or "tomorrow", for the page to
/// date; `project_id: null` is the Inbox).
pub fn list(conn: &Connection, kind: &str, id: &str) -> tasks_core::Result<Value> {
    let now = Local::now().naive_local();
    let today = now.date();
    let hour12 = tasks_core::db::get_state(conn, crate::reminders::CLOCK_12H_KEY).ok().flatten().as_deref() == Some("1");
    let open: Vec<Task> = tasks_core::ops::list_tasks(conn, &TaskQuery::default())?
        .into_iter()
        .filter(|t| t.completed_at.is_none())
        .collect();
    let today_s = today.to_string();
    let top = |t: &&Task| t.parent_id.is_none();

    let (name, project_id, mut tasks): (String, String, Vec<&Task>) = match kind {
        "inbox" => {
            let mut v: Vec<&Task> = open.iter().filter(top).filter(|t| t.project_id.is_none()).collect();
            v.sort_by(by_position);
            ("Inbox".into(), String::new(), v)
        }
        "upcoming" => {
            let mut v: Vec<&Task> = open.iter().filter(|t| t.due_date.as_deref().is_some_and(|d| d > today_s.as_str())).collect();
            v.sort_by(by_due);
            ("Upcoming".into(), String::new(), v)
        }
        "project" => {
            let Some(project) = tasks_core::ops::list_projects(conn)?.into_iter().find(|p| p.id == id) else {
                return Ok(gone());
            };
            // Unsectioned first, then each section in its order, as on the page.
            let order: Vec<String> = tasks_core::ops::list_sections(conn, Some(id))?.into_iter().map(|c| c.id).collect();
            let rank = |t: &Task| t.section_id.as_ref().and_then(|c| order.iter().position(|o| o == c)).map_or(0, |i| i + 1);
            let mut v: Vec<&Task> = open.iter().filter(top).filter(|t| t.project_id.as_deref() == Some(id)).collect();
            v.sort_by(|a, b| rank(*a).cmp(&rank(*b)).then_with(|| by_position(a, b)));
            (project.name, id.to_string(), v)
        }
        "section" => {
            let Ok(section) = tasks_core::ops::get_section(conn, id) else {
                return Ok(gone());
            };
            let mut v: Vec<&Task> = open.iter().filter(top).filter(|t| t.section_id.as_deref() == Some(id)).collect();
            v.sort_by(by_position);
            (section.name, section.project_id, v)
        }
        // "today", and anything unknown.
        _ => {
            let mut v: Vec<&Task> = open.iter().filter(|t| t.due_date.as_deref().is_some_and(|d| d <= today_s.as_str())).collect();
            v.sort_by(by_due);
            ("Today".into(), String::new(), v)
        }
    };

    let (open_query, add) = target(kind, id, &project_id);
    let more = tasks.len().saturating_sub(MAX_TASKS);
    tasks.truncate(MAX_TASKS);
    let rows: Vec<Value> = tasks
        .into_iter()
        .map(|t| {
            let date = t.due_date.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
            json!({
                "id": t.id,
                "title": tasks_core::plain_title(&t.title),
                "due": due_label(date, t.due_time.as_deref(), today, hour12),
                "overdue": date.is_some_and(|d| d < today),
                "tone": due_tone(date, t.due_time.as_deref(), t.duration_minutes, now),
                "priority": t.priority,
            })
        })
        .collect();
    Ok(json!({ "name": name, "gone": false, "open": open_query, "add": add, "more": more, "tasks": rows }))
}

fn gone() -> Value {
    json!({ "name": "List removed", "gone": true, "open": "view=today", "add": {}, "more": 0, "tasks": [] })
}

fn by_position(a: &&Task, b: &&Task) -> std::cmp::Ordering {
    a.position.cmp(&b.position).then_with(|| a.id.cmp(&b.id))
}

/// As the page's byDue: date, time, priority, position.
fn by_due(a: &&Task, b: &&Task) -> std::cmp::Ordering {
    let date = |t: &Task| t.due_date.clone().unwrap_or_else(|| "9999".into());
    let time = |t: &Task| t.due_time.clone().unwrap_or_else(|| "99".into());
    date(*a).cmp(&date(*b)).then_with(|| time(*a).cmp(&time(*b))).then_with(|| a.priority.cmp(&b.priority)).then_with(|| by_position(a, b))
}

/// The due label's colour, as the page's describeDue and .task-due-* classes:
/// "overdue" (an earlier day, or today at a time gone by), "progress" (started,
/// with time left), "today", "tomorrow", "week" (two to six days away), "later",
/// or "" when undated.
fn due_tone(date: Option<NaiveDate>, time: Option<&str>, duration: Option<i64>, now: NaiveDateTime) -> &'static str {
    let Some(date) = date else { return "" };
    let today = now.date();
    let start = time.and_then(|t| NaiveTime::parse_from_str(t, "%H:%M").ok()).map(|t| date.and_time(t));
    if let (Some(start), Some(minutes)) = (start, duration.filter(|m| *m > 0)) {
        if start <= now && now < start + chrono::Duration::minutes(minutes) {
            return "progress";
        }
    }
    // By the minute, as the page's timeOf.
    let now_hm = now.format("%H:%M").to_string();
    if date < today || (date == today && time.is_some_and(|t| t < now_hm.as_str())) {
        return "overdue";
    }
    match (date - today).num_days() {
        0 => "today",
        1 => "tomorrow",
        2..=6 => "week",
        _ => "later",
    }
}

/// "Today", "Tomorrow", "Yesterday", "Mon 17 Nov" (with the year when it isn't
/// this one), then the time if there is one. Empty for an undated task.
fn due_label(date: Option<NaiveDate>, time: Option<&str>, today: NaiveDate, hour12: bool) -> String {
    let Some(date) = date else { return String::new() };
    let mut out = match (date - today).num_days() {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        _ if date.year() == today.year() => date.format("%a %-d %b").to_string(),
        _ => date.format("%-d %b %Y").to_string(),
    };
    if let Some(time) = time.and_then(|t| chrono::NaiveTime::parse_from_str(t, "%H:%M").ok()) {
        out.push(' ');
        out.push_str(&if hour12 { time.format("%-I:%M %p").to_string() } else { time.format("%H:%M").to_string() });
    }
    out
}
