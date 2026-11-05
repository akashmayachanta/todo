//! The rules in `ops`, run on an in-memory DB the way a device runs them.

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde_json::json;
use tasks_core::{db, ops, sync, Category, CategoryIn, Ctx, Error, Project, ProjectIn, Task, TaskIn, TaskPatch};

fn setup() -> (Connection, Ctx) {
    (db::open_in_memory().unwrap(), Ctx::device("device-a"))
}

fn from<T: DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).unwrap()
}

fn project(conn: &Connection, ctx: &Ctx, name: &str) -> Project {
    ops::create_project(conn, ctx, from::<ProjectIn>(json!({ "name": name }))).unwrap()
}

fn category(conn: &Connection, ctx: &Ctx, project_id: &str, name: &str) -> Category {
    ops::create_category(conn, ctx, from::<CategoryIn>(json!({ "project_id": project_id, "name": name }))).unwrap()
}

fn subtask_in(parent_id: &str, title: &str) -> TaskIn {
    from(json!({ "title": title, "parent_id": parent_id }))
}

fn task(conn: &Connection, ctx: &Ctx, input: serde_json::Value) -> Task {
    ops::create_task(conn, ctx, from::<TaskIn>(input)).unwrap()
}

// ── Moving a category ────────────────────────────────────────────────────────

#[test]
fn a_moved_category_goes_last_and_takes_its_tasks_and_subtasks() {
    let (conn, ctx) = setup();
    let (from_project, to_project) = (project(&conn, &ctx, "From"), project(&conn, &ctx, "To"));
    category(&conn, &ctx, &to_project.id, "One");
    let taken = category(&conn, &ctx, &to_project.id, "Two");
    let moving = category(&conn, &ctx, &from_project.id, "Moving");
    let parent = task(&conn, &ctx, json!({ "title": "Parent", "project_id": from_project.id, "category_id": moving.id }));
    let sub = task(&conn, &ctx, json!({ "title": "Sub", "parent_id": parent.id }));

    let moved = ops::update_category(
        &conn,
        &ctx,
        &moving.id,
        from(json!({ "project_id": to_project.id, "name": "Moving" })),
    )
    .unwrap();

    assert_eq!(moved.project_id, to_project.id);
    assert_eq!(moved.position, taken.position + 1);
    for id in [&parent.id, &sub.id] {
        let t = ops::get_task(&conn, id).unwrap();
        assert_eq!(t.project_id.as_deref(), Some(to_project.id.as_str()));
        assert_eq!(t.category_id.as_deref(), Some(moving.id.as_str()));
    }
}

#[test]
fn a_rename_keeps_the_category_in_its_place() {
    let (conn, ctx) = setup();
    let p = project(&conn, &ctx, "P");
    let first = category(&conn, &ctx, &p.id, "First");
    category(&conn, &ctx, &p.id, "Second");

    let renamed =
        ops::update_category(&conn, &ctx, &first.id, from(json!({ "project_id": p.id, "name": "Renamed" }))).unwrap();

    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.position, first.position);
}

// ── The subtask limit ────────────────────────────────────────────────────────

#[test]
fn a_subtask_title_can_show_255_characters_but_not_256() {
    let (conn, ctx) = setup();
    let parent = task(&conn, &ctx, json!({ "title": "Parent" }));

    // Counted in characters, not bytes.
    ops::create_task(&conn, &ctx, subtask_in(&parent.id, &"é".repeat(255))).unwrap();
    let err = ops::create_task(&conn, &ctx, subtask_in(&parent.id, &"é".repeat(256))).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
}

#[test]
fn a_links_address_does_not_count_towards_the_limit() {
    let (conn, ctx) = setup();
    let parent = task(&conn, &ctx, json!({ "title": "Parent" }));
    let url = format!("https://example.com/{}", "a".repeat(200));

    let fits = format!("{} [label]({url})", "x".repeat(249));
    ops::create_task(&conn, &ctx, subtask_in(&parent.id, &fits)).unwrap();

    let over = format!("{} [label]({url})", "x".repeat(250));
    assert!(ops::create_task(&conn, &ctx, subtask_in(&parent.id, &over)).is_err());
}

#[test]
fn a_top_level_task_keeps_its_500_character_limit() {
    let (conn, ctx) = setup();
    let long = task(&conn, &ctx, json!({ "title": "x".repeat(500) }));
    assert_eq!(long.title.chars().count(), 500);
    assert!(ops::create_task(&conn, &ctx, from(json!({ "title": "x".repeat(501) }))).is_err());

    let patch: TaskPatch = from(json!({ "title": "y".repeat(400) }));
    ops::update_task(&conn, &ctx, &long.id, patch).unwrap();
}

#[test]
fn renaming_a_subtask_is_held_to_the_limit() {
    let (conn, ctx) = setup();
    let parent = task(&conn, &ctx, json!({ "title": "Parent" }));
    let sub = task(&conn, &ctx, json!({ "title": "Sub", "parent_id": parent.id }));

    let ok: TaskPatch = from(json!({ "title": "x".repeat(255) }));
    ops::update_task(&conn, &ctx, &sub.id, ok).unwrap();
    let over: TaskPatch = from(json!({ "title": "x".repeat(256) }));
    assert!(ops::update_task(&conn, &ctx, &sub.id, over).is_err());
}

#[test]
fn an_older_long_subtask_can_still_be_ticked_and_edited_except_its_title() {
    let (conn, ctx) = setup();
    let parent = task(&conn, &ctx, json!({ "title": "Parent" }));
    let sub = task(&conn, &ctx, json!({ "title": "Sub", "parent_id": parent.id }));
    let long = "x".repeat(400);
    conn.execute("UPDATE tasks SET title = ?1 WHERE id = ?2", (&long, &sub.id)).unwrap();

    let tick: TaskPatch = from(json!({ "completed_at": "2026-11-05T10:00:00Z", "priority": 1 }));
    let ticked = ops::update_task(&conn, &ctx, &sub.id, tick).unwrap();
    assert!(ticked.completed_at.is_some());
    assert_eq!(ticked.title, long);

    // Sent back unchanged, it isn't a new title.
    let same: TaskPatch = from(json!({ "title": long }));
    ops::update_task(&conn, &ctx, &sub.id, same).unwrap();

    let changed: TaskPatch = from(json!({ "title": format!("{long}!") }));
    assert!(ops::update_task(&conn, &ctx, &sub.id, changed).is_err());
}

#[test]
fn a_sync_merge_keeps_a_long_subtask() {
    let (device, ctx) = setup();
    let parent = task(&device, &ctx, json!({ "title": "Parent" }));
    let sub = task(&device, &ctx, json!({ "title": "Sub", "parent_id": parent.id }));
    let long = "x".repeat(400);
    device.execute("UPDATE tasks SET title = ?1 WHERE id = ?2", (&long, &sub.id)).unwrap();

    let server = db::open_in_memory().unwrap();
    let rows = sync::dirty_rows(&device, 100).unwrap();
    let applied = sync::apply(&server, &Ctx::server(), &rows).unwrap();

    assert_eq!(applied.ignored, 0);
    assert_eq!(ops::get_task(&server, &sub.id).unwrap().title, long);
}
