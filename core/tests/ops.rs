//! The rules in `ops`, run on an in-memory DB the way a device runs them.

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde_json::json;
use tasks_core::{db, ops, sync, Section, SectionIn, Ctx, Error, Project, ProjectIn, Task, TaskIn, TaskPatch};

fn setup() -> (Connection, Ctx) {
    (db::open_in_memory().unwrap(), Ctx::device("device-a"))
}

fn from<T: DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).unwrap()
}

fn project(conn: &Connection, ctx: &Ctx, name: &str) -> Project {
    ops::create_project(conn, ctx, from::<ProjectIn>(json!({ "name": name }))).unwrap()
}

fn section(conn: &Connection, ctx: &Ctx, project_id: &str, name: &str) -> Section {
    ops::create_section(conn, ctx, from::<SectionIn>(json!({ "project_id": project_id, "name": name }))).unwrap()
}

fn subtask_in(parent_id: &str, title: &str) -> TaskIn {
    from(json!({ "title": title, "parent_id": parent_id }))
}

fn task(conn: &Connection, ctx: &Ctx, input: serde_json::Value) -> Task {
    ops::create_task(conn, ctx, from::<TaskIn>(input)).unwrap()
}

// ── Clearing the date ────────────────────────────────────────────────────────

#[test]
fn clearing_the_date_clears_the_time_and_the_reminder() {
    let (conn, ctx) = setup();
    let t = task(&conn, &ctx, json!({
        "title": "Call", "due_date": "2026-11-30", "due_time": "10:00", "remind_at": "2026-11-30T04:30:00Z"
    }));
    let cleared = ops::update_task(&conn, &ctx, &t.id, from(json!({ "due_date": null }))).unwrap();
    assert_eq!((cleared.due_date, cleared.due_time, cleared.remind_at), (None, None, None));
}

#[test]
fn clearing_the_date_keeps_a_reminder_set_in_the_same_change() {
    let (conn, ctx) = setup();
    let t = task(&conn, &ctx, json!({
        "title": "Call", "due_date": "2026-11-30", "remind_at": "2026-11-30T04:30:00Z"
    }));
    let patch: TaskPatch = from(json!({ "due_date": null, "remind_at": "2026-12-01T05:00:00Z" }));
    let cleared = ops::update_task(&conn, &ctx, &t.id, patch).unwrap();
    assert_eq!(cleared.due_date, None);
    assert_eq!(cleared.remind_at.as_deref(), Some("2026-12-01T05:00:00.000Z"));
}

// ── Moving a section ────────────────────────────────────────────────────────

#[test]
fn a_moved_section_goes_last_and_takes_its_tasks_and_subtasks() {
    let (conn, ctx) = setup();
    let (from_project, to_project) = (project(&conn, &ctx, "From"), project(&conn, &ctx, "To"));
    section(&conn, &ctx, &to_project.id, "One");
    let taken = section(&conn, &ctx, &to_project.id, "Two");
    let moving = section(&conn, &ctx, &from_project.id, "Moving");
    let parent = task(&conn, &ctx, json!({ "title": "Parent", "project_id": from_project.id, "section_id": moving.id }));
    let sub = task(&conn, &ctx, json!({ "title": "Sub", "parent_id": parent.id }));

    let moved = ops::update_section(
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
        assert_eq!(t.section_id.as_deref(), Some(moving.id.as_str()));
    }
}

#[test]
fn a_rename_keeps_the_section_in_its_place() {
    let (conn, ctx) = setup();
    let p = project(&conn, &ctx, "P");
    let first = section(&conn, &ctx, &p.id, "First");
    section(&conn, &ctx, &p.id, "Second");

    let renamed =
        ops::update_section(&conn, &ctx, &first.id, from(json!({ "project_id": p.id, "name": "Renamed" }))).unwrap();

    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.position, first.position);
}

// ── Reordering ───────────────────────────────────────────────────────────────

fn order(conn: &Connection, ids: &[&str]) -> tasks_core::Result<Vec<Task>> {
    let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    ops::reorder_tasks(conn, &Ctx::device("device-a"), &ids)
}

#[test]
fn reordering_writes_only_the_rows_that_change() {
    let (conn, ctx) = setup();
    let a = task(&conn, &ctx, json!({ "title": "A" }));
    let b = task(&conn, &ctx, json!({ "title": "B" }));
    let c = task(&conn, &ctx, json!({ "title": "C" }));
    conn.execute("UPDATE tasks SET dirty = 0", []).unwrap();

    let out = order(&conn, &[&a.id, &c.id, &b.id]).unwrap();

    assert_eq!(out.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["A", "C", "B"]);
    assert!(out[0].position < out[1].position && out[1].position < out[2].position);
    let dirty: Vec<String> = sync::dirty_rows(&conn, 10)
        .unwrap()
        .iter()
        .map(|r| r.row["title"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(dirty.len(), 2);
    assert!(!dirty.contains(&"A".to_string()));
}

#[test]
fn reordering_gives_tasks_sharing_a_position_their_own() {
    let (conn, ctx) = setup();
    let a = task(&conn, &ctx, json!({ "title": "A" }));
    let b = task(&conn, &ctx, json!({ "title": "B" }));
    conn.execute("UPDATE tasks SET position = 7", []).unwrap();

    let out = order(&conn, &[&b.id, &a.id]).unwrap();

    assert_eq!((out[0].position, out[1].position), (7, 8));
}

#[test]
fn only_siblings_can_be_reordered_together() {
    let (conn, ctx) = setup();
    let p = project(&conn, &ctx, "P");
    let cat = section(&conn, &ctx, &p.id, "Cat");
    let inbox = task(&conn, &ctx, json!({ "title": "Inbox" }));
    let in_project = task(&conn, &ctx, json!({ "title": "Project", "project_id": p.id }));
    let in_section = task(&conn, &ctx, json!({ "title": "Section", "project_id": p.id, "section_id": cat.id }));
    let sub = task(&conn, &ctx, json!({ "title": "Sub", "parent_id": in_project.id }));
    let other_sub = task(&conn, &ctx, json!({ "title": "Other", "parent_id": in_section.id }));

    assert!(order(&conn, &[&inbox.id, &in_project.id]).is_err());
    assert!(order(&conn, &[&in_project.id, &in_section.id]).is_err());
    assert!(order(&conn, &[&in_project.id, &sub.id]).is_err());
    assert!(order(&conn, &[&sub.id, &other_sub.id]).is_err());
    assert!(order(&conn, &[&inbox.id, &inbox.id]).is_err());
    assert!(order(&conn, &["no-such-task"]).is_err());
    assert!(order(&conn, &[]).unwrap().is_empty());
}

// ── Moving a subtask to another task ─────────────────────────────────────────

#[test]
fn a_subtask_moves_to_another_task_and_takes_its_place() {
    let (conn, ctx) = setup();
    let p = project(&conn, &ctx, "P");
    let cat = section(&conn, &ctx, &p.id, "Cat");
    let from_task = task(&conn, &ctx, json!({ "title": "From" }));
    let to_task = task(&conn, &ctx, json!({ "title": "To", "project_id": p.id, "section_id": cat.id }));
    let existing = task(&conn, &ctx, json!({ "title": "Existing", "parent_id": to_task.id }));
    let sub = task(&conn, &ctx, json!({ "title": "Sub", "parent_id": from_task.id }));

    let moved = ops::update_task(&conn, &ctx, &sub.id, from(json!({ "parent_id": to_task.id }))).unwrap();

    assert_eq!(moved.parent_id.as_deref(), Some(to_task.id.as_str()));
    assert_eq!(moved.project_id.as_deref(), Some(p.id.as_str()));
    assert_eq!(moved.section_id.as_deref(), Some(cat.id.as_str()));
    assert!(moved.position > existing.position);
}

#[test]
fn only_a_subtask_moves_and_only_under_a_top_level_task() {
    let (conn, ctx) = setup();
    let a = task(&conn, &ctx, json!({ "title": "A" }));
    let b = task(&conn, &ctx, json!({ "title": "B" }));
    let sub = task(&conn, &ctx, json!({ "title": "Sub", "parent_id": a.id }));
    let other = task(&conn, &ctx, json!({ "title": "Other", "parent_id": b.id }));

    let try_move = |id: &str, value: serde_json::Value| ops::update_task(&conn, &ctx, id, from(json!({ "parent_id": value })));
    assert!(try_move(&a.id, json!(b.id)).is_err(), "a task can't become a subtask");
    assert!(try_move(&sub.id, json!(other.id)).is_err(), "no subtasks of subtasks");
    assert!(try_move(&sub.id, json!(sub.id)).is_err(), "not under itself");
    assert!(try_move(&sub.id, json!(null)).is_err(), "a subtask can't become a task");
    assert!(try_move(&sub.id, json!("no-such-task")).is_err());
    assert_eq!(ops::get_task(&conn, &sub.id).unwrap().parent_id.as_deref(), Some(a.id.as_str()));
    // Its own parent again is no change.
    let before = ops::get_task(&conn, &sub.id).unwrap().position;
    assert_eq!(try_move(&sub.id, json!(a.id)).unwrap().position, before);
}

// ── Search ───────────────────────────────────────────────────────────────────

fn search(conn: &Connection, term: &str, completed: bool) -> Vec<String> {
    let q = from(json!({ "q": term, "completed": completed, "limit": 50 }));
    ops::list_tasks(conn, &q).unwrap().into_iter().map(|t| t.title).collect()
}

#[test]
fn search_finds_done_tasks_only_when_asked() {
    let (conn, ctx) = setup();
    task(&conn, &ctx, json!({ "title": "Open report" }));
    let done = task(&conn, &ctx, json!({ "title": "Done report" }));
    let tick: TaskPatch = from(json!({ "completed_at": "2026-11-05T10:00:00Z" }));
    ops::update_task(&conn, &ctx, &done.id, tick).unwrap();

    assert_eq!(search(&conn, "REPORT", false), ["Open report"]);
    assert_eq!(search(&conn, "report", true), ["Done report"]);
}

#[test]
fn search_matches_percent_and_underscore_literally() {
    let (conn, ctx) = setup();
    task(&conn, &ctx, json!({ "title": "Reach 100% coverage" }));
    task(&conn, &ctx, json!({ "title": "Reach 100 users" }));
    task(&conn, &ctx, json!({ "title": "file_name" }));
    task(&conn, &ctx, json!({ "title": "filename" }));
    task(&conn, &ctx, json!({ "title": r"back\slash" }));

    assert_eq!(search(&conn, "100%", false), ["Reach 100% coverage"]);
    assert_eq!(search(&conn, "file_n", false), ["file_name"]);
    assert_eq!(search(&conn, r"k\s", false), [r"back\slash"]);
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

// ── Profile ──────────────────────────────────────────────────────────────────

fn photo(n: usize) -> String {
    format!("data:image/jpeg;base64,{}", "A".repeat(n))
}

#[test]
fn a_profile_is_checked_and_syncs_to_another_device() {
    let (a, ctx_a) = setup();
    assert!(ops::get_profile(&a).unwrap().display_name.is_none());

    let patch = |v: serde_json::Value| from::<tasks_core::ProfilePatch>(v);
    assert!(ops::update_profile(&a, &ctx_a, patch(json!({ "photo": "data:image/png;base64,AAAA" }))).is_err());
    assert!(ops::update_profile(&a, &ctx_a, patch(json!({ "photo": photo(40_000) }))).is_err());
    assert!(ops::update_profile(&a, &ctx_a, patch(json!({ "display_name": "x".repeat(61) }))).is_err());
    assert_eq!(sync::pending_count(&a).unwrap(), 0);

    let p = ops::update_profile(&a, &ctx_a, patch(json!({ "display_name": "  Akash  ", "photo": photo(100) }))).unwrap();
    assert_eq!(p.display_name.as_deref(), Some("Akash"));
    // Leaving a field out keeps it; saving the same thing again writes nothing.
    ops::update_profile(&a, &ctx_a, patch(json!({ "display_name": "Akash" }))).unwrap();
    assert_eq!(sync::pending_count(&a).unwrap(), 1);

    // Through the server to a second device.
    let server = db::open_in_memory().unwrap();
    sync::apply(&server, &Ctx::server(), &sync::dirty_rows(&a, 100).unwrap()).unwrap();
    let b = db::open_in_memory().unwrap();
    let pulled = sync::changes_since(&server, 0, 100).unwrap();
    sync::apply(&b, &Ctx::device("device-b"), &pulled.rows).unwrap();
    let got = ops::get_profile(&b).unwrap();
    assert_eq!(got.display_name.as_deref(), Some("Akash"));
    assert_eq!(got.photo, Some(photo(100)));

    // Clearing the photo is a later version and travels too.
    ops::update_profile(&a, &ctx_a, patch(json!({ "photo": null }))).unwrap();
    assert!(ops::get_profile(&a).unwrap().photo.is_none());
}

#[test]
fn a_sync_row_from_an_unknown_table_is_skipped() {
    let (conn, _) = setup();
    let row: sync::SyncRow = from(json!({ "table": "from_the_future", "row": { "id": "x" } }));
    let applied = sync::apply(&conn, &Ctx::server(), &[row]).unwrap();
    assert_eq!((applied.accepted, applied.ignored), (0, 1));
}

#[test]
fn reorder_sections_sets_the_order_within_a_project() {
    let (conn, ctx) = setup();
    let p = project(&conn, &ctx, "Learning");
    let other = project(&conn, &ctx, "Work");
    let a = section(&conn, &ctx, &p.id, "A");
    section(&conn, &ctx, &p.id, "B");
    let c = section(&conn, &ctx, &p.id, "C");
    let elsewhere = section(&conn, &ctx, &other.id, "X");

    // Listed ones first, the rest after in their old order; another project's id is ignored.
    let out = ops::reorder_sections(&conn, &ctx, &p.id, &[c.id.clone(), elsewhere.id.clone(), a.id.clone()]).unwrap();
    let names: Vec<_> = out.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["C", "A", "B"]);
    assert_eq!(out.iter().map(|c| c.position).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(ops::get_section(&conn, &elsewhere.id).unwrap().position, elsewhere.position);

    assert!(matches!(ops::reorder_sections(&conn, &ctx, "missing", &[]), Err(Error::NotFound(_))));
}

// ── Completing ───────────────────────────────────────────────────────────────

fn day(s: &str) -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn completing_a_repeating_task_moves_it_on_with_its_reminder() {
    let (conn, ctx) = setup();
    let t = task(&conn, &ctx, json!({
        "title": "Water plants", "due_date": "2026-11-05", "recurrence": "every day", "remind_at": "2026-11-05T09:00:00Z"
    }));
    let done = ops::complete_task(&conn, &ctx, &t.id, day("2026-11-05")).unwrap();
    assert_eq!(done.completed_at, None);
    assert_eq!(done.due_date.as_deref(), Some("2026-11-06"));
    // The reminder moved a whole day, keeping its local time of day.
    let moved = tasks_core::time::instant_ms(done.remind_at.as_deref().unwrap()).unwrap();
    let before = tasks_core::time::instant_ms("2026-11-05T09:00:00Z").unwrap();
    assert!((moved - before - 86_400_000).abs() <= 3_600_000);
    // The tick is in the Karma log.
    assert_eq!(ops::list_completions(&conn, "2000-01-01T00:00:00Z").unwrap().total, 1);
}

#[test]
fn the_report_pages_newest_first_and_counts_whole_days() {
    let (conn, ctx) = setup();
    let home = project(&conn, &ctx, "Home");
    let walk = task(&conn, &ctx, json!({ "title": "Walk", "project_id": home.id }));
    let read = task(&conn, &ctx, json!({ "title": "Read" }));
    let gone = task(&conn, &ctx, json!({ "title": "Gone" }));
    // Five ticks: three on 2026-11-02 in India (one of them late on 1 Nov UTC), two on 3 Nov.
    for (id, task_id, at) in [
        ("c1", &walk.id, "2026-11-01T20:00:00.000Z"),
        ("c2", &read.id, "2026-11-02T08:00:00.000Z"),
        ("c3", &gone.id, "2026-11-02T09:00:00.000Z"),
        ("c4", &walk.id, "2026-11-03T08:00:00.000Z"),
        ("c5", &read.id, "2026-11-03T09:00:00.000Z"),
    ] {
        conn.execute(
            "INSERT INTO task_completions (id, task_id, completed_at, created_at) VALUES (?1, ?2, ?3, ?3)",
            [id, task_id.as_str(), at],
        )
        .unwrap();
    }
    ops::delete_task(&conn, &ctx, &gone.id).unwrap();
    let q = |before: Option<String>, project: Option<&str>| tasks_core::ReportQuery {
        since: "2000-01-01T00:00:00Z".into(),
        project: project.map(str::to_string),
        before,
        limit: Some(2),
        utc_offset: 330,
    };

    let first = ops::report(&conn, &q(None, None)).unwrap();
    let ids: Vec<_> = first.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["c5", "c4"]);
    assert_eq!(first.items[0].task.as_ref().unwrap().title, "Read");
    assert_eq!(first.days.iter().map(|d| (d.day.as_str(), d.count)).collect::<Vec<_>>(), [("2026-11-03", 2)]);

    let second = ops::report(&conn, &q(first.next.clone(), None)).unwrap();
    let ids: Vec<_> = second.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["c3", "c2"]);
    // The deleted task's tick stays, without its task.
    assert!(second.items[0].task.is_none());
    // The whole day is counted, though this page holds two of its three.
    assert_eq!(second.days.iter().map(|d| (d.day.as_str(), d.count)).collect::<Vec<_>>(), [("2026-11-02", 3)]);

    let last = ops::report(&conn, &q(second.next.clone(), None)).unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].day, "2026-11-02");
    assert!(last.next.is_none());

    let home_only = ops::report(&conn, &q(None, Some(home.id.as_str()))).unwrap();
    assert_eq!(home_only.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["c4", "c1"]);
    let inbox = ops::report(&conn, &q(None, Some("inbox"))).unwrap();
    assert_eq!(inbox.items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["c5", "c2"]);
}

#[test]
fn completing_an_overdue_repeating_task_rolls_past_today() {
    let (conn, ctx) = setup();
    let t = task(&conn, &ctx, json!({ "title": "Stretch", "due_date": "2026-11-01", "recurrence": "every day" }));
    let done = ops::complete_task(&conn, &ctx, &t.id, day("2026-11-05")).unwrap();
    assert_eq!(done.due_date.as_deref(), Some("2026-11-06"));
}

#[test]
fn completing_a_plain_task_finishes_it_and_reopening_takes_the_tick_back() {
    let (conn, ctx) = setup();
    let t = task(&conn, &ctx, json!({ "title": "Pay rent", "due_date": "2026-11-05", "recurrence": "sometimes" }));
    let done = ops::complete_task(&conn, &ctx, &t.id, day("2026-11-05")).unwrap();
    assert!(done.completed_at.is_some());
    assert_eq!(done.due_date.as_deref(), Some("2026-11-05"));
    assert_eq!(ops::list_completions(&conn, "2000-01-01T00:00:00Z").unwrap().total, 1);
    // Ticking again changes nothing.
    ops::complete_task(&conn, &ctx, &t.id, day("2026-11-05")).unwrap();
    assert_eq!(ops::list_completions(&conn, "2000-01-01T00:00:00Z").unwrap().items.len(), 1);

    let open = ops::reopen_task(&conn, &ctx, &t.id).unwrap();
    assert_eq!(open.completed_at, None);
    assert!(ops::list_completions(&conn, "2000-01-01T00:00:00Z").unwrap().items.is_empty());
}

#[test]
fn a_ticked_subtasks_time_leaves_its_tasks_total_and_comes_back() {
    let (conn, ctx) = setup();
    let parent = task(&conn, &ctx, json!({ "title": "Move house" }));
    let a = ops::create_task(&conn, &ctx, from(json!({ "title": "Pack", "parent_id": parent.id, "duration_minutes": 60 }))).unwrap();
    ops::create_task(&conn, &ctx, from(json!({ "title": "Clean", "parent_id": parent.id, "duration_minutes": 30 }))).unwrap();
    // Pack is done, so only Clean is left.
    ops::complete_task(&conn, &ctx, &a.id, day("2026-11-05")).unwrap();
    assert_eq!(ops::get_task(&conn, &parent.id).unwrap().duration_minutes, Some(30));
    ops::reopen_task(&conn, &ctx, &a.id).unwrap();
    assert_eq!(ops::get_task(&conn, &parent.id).unwrap().duration_minutes, Some(90));
}

#[test]
fn ticking_the_last_timed_subtask_clears_its_tasks_duration() {
    let (conn, ctx) = setup();
    let parent = task(&conn, &ctx, json!({ "title": "Trip", "duration_minutes": 45 }));
    let a = ops::create_task(&conn, &ctx, from(json!({ "title": "Book", "parent_id": parent.id, "duration_minutes": 45 }))).unwrap();
    ops::complete_task(&conn, &ctx, &a.id, day("2026-11-05")).unwrap();
    assert_eq!(ops::get_task(&conn, &parent.id).unwrap().duration_minutes, None);
}

#[test]
fn a_task_with_untimed_subtasks_keeps_its_own_duration() {
    let (conn, ctx) = setup();
    let parent = task(&conn, &ctx, json!({ "title": "Plan", "duration_minutes": 20 }));
    let a = ops::create_task(&conn, &ctx, from(json!({ "title": "Outline", "parent_id": parent.id }))).unwrap();
    ops::complete_task(&conn, &ctx, &a.id, day("2026-11-05")).unwrap();
    assert_eq!(ops::get_task(&conn, &parent.id).unwrap().duration_minutes, Some(20));
}
