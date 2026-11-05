//! The shapes the API and the Tauri commands speak (app/src/lib/api.ts has
//! the same types for the UI).

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::time::parse_instant;
use crate::{Error, Result};

// ── Output ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A section of a project.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// No project_id means the Inbox. A subtask (parent_id) shares its parent's
/// project and category.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: Option<String>,
    pub category_id: Option<String>,
    pub parent_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub priority: i64,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub duration_minutes: Option<i64>,
    pub location: Option<String>,
    pub remind_at: Option<String>,
    pub recurrence: Option<String>,
    pub position: i64,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A saved view of the open tasks; the browser does the matching.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Filter {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub criteria: Value,
    pub fields: Value,
    pub group_by: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// One tick in the Karma log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Completion {
    pub task_id: Option<String>,
    pub completed_at: String,
}

/// total counts every tick ever logged; items are those since the instant asked.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Completions {
    pub total: i64,
    pub items: Vec<Completion>,
}

// ── Input ────────────────────────────────────────────────────────────────────

/// Some(None) when the key was sent as null, None when it was left out.
fn double<'de, D, T>(de: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProjectIn {
    pub name: String,
    #[serde(default, deserialize_with = "double")]
    pub color: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub position: Option<Option<i64>>,
}

/// Every project id, in the order the pane shows them.
#[derive(Clone, Debug, Deserialize)]
pub struct ProjectOrder {
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CategoryIn {
    pub project_id: String,
    pub name: String,
    #[serde(default, deserialize_with = "double")]
    pub position: Option<Option<i64>>,
}

/// A new task. No project_id puts it in the Inbox; a parent_id makes it a
/// subtask, which takes its parent's project and category.
#[derive(Clone, Debug, Deserialize)]
pub struct TaskIn {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub due_time: Option<String>,
    #[serde(default)]
    pub duration_minutes: Option<i64>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub remind_at: Option<String>,
    #[serde(default)]
    pub recurrence: Option<String>,
    #[serde(default)]
    pub position: Option<i64>,
}

/// Only the fields sent are changed; null clears one.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TaskPatch {
    #[serde(default, deserialize_with = "double")]
    pub title: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub project_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub category_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub priority: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double")]
    pub due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub due_time: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub duration_minutes: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double")]
    pub location: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub remind_at: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub recurrence: Option<Option<String>>,
    #[serde(default, deserialize_with = "double")]
    pub position: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double")]
    pub completed_at: Option<Option<String>>,
    /// Set with the next due_date when a repeating task's occurrence is
    /// ticked: the task stays open, and the completion is logged for Karma.
    #[serde(default)]
    pub completes_occurrence: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct FilterCriteria {
    #[serde(default)]
    pub projects: Vec<Option<String>>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub priorities: Vec<i64>,
    #[serde(default)]
    pub due: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FilterIn {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub criteria: FilterCriteria,
    #[serde(default = "default_fields")]
    pub fields: Vec<String>,
    #[serde(default = "default_group_by")]
    pub group_by: String,
    #[serde(default)]
    pub position: Option<i64>,
}

fn default_fields() -> Vec<String> {
    ["due", "place", "description"].map(String::from).to_vec()
}

fn default_group_by() -> String {
    "none".into()
}

/// What `GET /` (and the `tasks_list` command) take.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct TaskQuery {
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub inbox: bool,
    #[serde(default)]
    pub limit: Option<i64>,
    /// Search the open tasks' titles and descriptions (global search).
    #[serde(default)]
    pub q: Option<String>,
}

// ── Validation ───────────────────────────────────────────────────────────────

const DUE_WINDOWS: &[&str] = &["overdue", "today", "tomorrow", "next_7", "next_30", "no_date"];
const ROW_FIELDS: &[&str] = &["due", "place", "description", "duration", "repeat", "reminder", "subtasks"];
const GROUP_BYS: &[&str] = &["none", "priority", "project", "category", "due"];

fn invalid(field: &str, msg: &str) -> Error {
    Error::Invalid(format!("{field}: {msg}"))
}

pub(crate) fn check_title(field: &str, value: &str) -> Result<()> {
    let len = value.chars().count();
    if len < 1 {
        return Err(invalid(field, "String should have at least 1 character"));
    }
    if len > 500 {
        return Err(invalid(field, "String should have at most 500 characters"));
    }
    Ok(())
}

/// The most characters a subtask's title shows. Links count as their label, so
/// a long address doesn't use up the limit (`check_title` still caps the stored
/// text). Sync merges aren't checked, so an older, longer subtask stays intact.
pub const SUBTASK_TITLE_MAX: usize = 255;

pub(crate) fn check_subtask_title(value: &str) -> Result<()> {
    if crate::plain_title(value).chars().count() > SUBTASK_TITLE_MAX {
        return Err(invalid("title", &format!("A subtask can have at most {SUBTASK_TITLE_MAX} characters")));
    }
    Ok(())
}

pub(crate) fn check_priority(value: i64) -> Result<()> {
    if (1..=4).contains(&value) {
        Ok(())
    } else {
        Err(invalid("priority", "Input should be between 1 and 4"))
    }
}

pub(crate) fn check_due_date(value: &str) -> Result<()> {
    let b = value.as_bytes();
    let ok = b.len() == 10
        && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() });
    if ok {
        Ok(())
    } else {
        Err(invalid("due_date", "String should match pattern YYYY-MM-DD"))
    }
}

pub(crate) fn check_due_time(value: &str) -> Result<()> {
    let b = value.as_bytes();
    let ok = b.len() == 5
        && b[2] == b':'
        && matches!((b[0], b[1]), (b'0'..=b'1', b'0'..=b'9') | (b'2', b'0'..=b'3'))
        && (b'0'..=b'5').contains(&b[3])
        && b[4].is_ascii_digit();
    if ok {
        Ok(())
    } else {
        Err(invalid("due_time", "String should match pattern HH:MM"))
    }
}

pub(crate) fn check_duration(value: i64) -> Result<()> {
    if (1..=10080).contains(&value) {
        Ok(())
    } else {
        Err(invalid("duration_minutes", "Input should be between 1 and 10080"))
    }
}

pub(crate) fn check_location(value: &str) -> Result<()> {
    if value.chars().count() > 500 {
        Err(invalid("location", "String should have at most 500 characters"))
    } else {
        Ok(())
    }
}

pub(crate) fn instant(field: &str, value: &str) -> Result<String> {
    parse_instant(value).ok_or_else(|| invalid(field, "Input should be a valid datetime"))
}

fn unique<T: PartialEq + Clone>(items: &[T]) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        if !out.contains(item) {
            out.push(item.clone());
        }
    }
    out
}

impl FilterIn {
    /// Checks the input and returns it cleaned: name trimmed, lists without repeats.
    pub fn cleaned(mut self) -> Result<Self> {
        self.name = self.name.trim().to_string();
        if self.name.is_empty() {
            return Err(invalid("name", "name is required"));
        }
        check_title("name", &self.name)?;
        let c = &mut self.criteria;
        if c.projects.len() > 500 {
            return Err(invalid("criteria.projects", "List should have at most 500 items"));
        }
        if c.categories.len() > 2000 {
            return Err(invalid("criteria.categories", "List should have at most 2000 items"));
        }
        for p in &c.priorities {
            check_priority(*p)?;
        }
        if let Some(bad) = c.due.iter().find(|d| !DUE_WINDOWS.contains(&d.as_str())) {
            return Err(invalid("criteria.due", &format!("unknown due window '{bad}'")));
        }
        c.projects = unique(&c.projects);
        c.categories = unique(&c.categories);
        c.priorities = unique(&c.priorities);
        c.due = unique(&c.due);
        if let Some(bad) = self.fields.iter().find(|f| !ROW_FIELDS.contains(&f.as_str())) {
            return Err(invalid("fields", &format!("unknown field '{bad}'")));
        }
        self.fields = unique(&self.fields);
        if !GROUP_BYS.contains(&self.group_by.as_str()) {
            return Err(invalid("group_by", &format!("unknown grouping '{}'", self.group_by)));
        }
        Ok(self)
    }

    pub fn criteria_json(&self) -> Value {
        let c = &self.criteria;
        serde_json::json!({
            "projects": c.projects,
            "categories": c.categories,
            "priorities": c.priorities,
            "due": c.due,
        })
    }
}
