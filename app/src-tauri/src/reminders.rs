//! Task reminders as system notifications.
//!
//! - Windows: a loop checks `remind_at` every 30 s and shows a toast for each
//!   reminder that came due (one missed while the app was closed shows if it is
//!   under a day old). Each fires once: fired `id:remind_at` keys are kept in
//!   the DB. The toast has Snooze and Complete buttons. They run through a
//!   tasks-app: link instead of a callback, so they still work from the
//!   notification centre and after the app was closed: Windows starts the exe with
//!   the link, and the single-instance plugin hands it to the running copy.
//! - Android: scheduled notifications (AlarmManager), so they fire while the
//!   app is closed and the phone is offline. After every sync or edit the app
//!   cancels the ones it scheduled last time and schedules the open tasks with
//!   a future `remind_at`. Tapping one opens its task. The plugin only restores
//!   alarms after a reboot for notifications sent in a batch, which its Rust API
//!   doesn't offer, so after a reboot they come back when the app next opens.

use std::sync::Arc;

use tauri::AppHandle;

use crate::state::AppState;

#[cfg(desktop)]
const FIRED_KEY: &str = "reminders_fired";

/// How far Snooze moves a reminder.
#[cfg(desktop)]
const SNOOZE_MINUTES: i64 = 10;

/// The link a toast's buttons open: `open/<id>`, `snooze/<id>/<remind_at>` or
/// `complete/<id>/<remind_at>`. A bare `open` just shows the app.
const TOAST_LINK: &str = "tasks-app:";

/// Is this a button that does its work without showing the window?
pub fn is_background_action(args: &[String]) -> bool {
    args.iter().filter_map(|a| a.strip_prefix(TOAST_LINK)).any(|a| !a.starts_with("open"))
}

/// Exes in target\ are test builds. They don't take over the shortcut or the link.
#[cfg(windows)]
fn is_test_build(exe: &std::path::Path) -> bool {
    let folder = exe.parent().and_then(|d| d.parent()).and_then(|d| d.file_name());
    folder.is_some_and(|name| name == "target")
}

/// Points the tasks-app: link at this exe. Written on every start so it follows
/// the exe if it moves.
#[cfg(windows)]
pub fn register_toast_protocol() -> std::io::Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let exe = std::env::current_exe()?;
    if is_test_build(&exe) {
        return Ok(());
    }
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Software\Classes\tasks-app")?;
    key.set_value("", &"URL:Tasks")?;
    key.set_value("URL Protocol", &"")?;
    let (command, _) = key.create_subkey(r"shell\open\command")?;
    command.set_value("", &format!("\"{}\" \"%1\"", exe.display()))
}

/// Registers the app ID that toasts are sent under. Windows silently drops toasts
/// for an unknown ID. The installer's Start Menu shortcut registers it, but an exe
/// that was copied somewhere and run directly has no shortcut. The icon has to be
/// a file on disk, so it is written to the app's data folder.
#[cfg(windows)]
pub fn register_toast_app(app: &AppHandle, data_dir: &std::path::Path) -> std::io::Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let icon = data_dir.join("notification-icon.png");
    std::fs::write(&icon, include_bytes!("../icons/128x128.png"))?;

    let path = format!(r"Software\Classes\AppUserModelId\{}", app.config().identifier);
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(path)?;
    key.set_value("DisplayName", &app.package_info().name)?;
    key.set_value("IconUri", &icon.to_string_lossy().into_owned())
}

/// Points a per-user Start Menu shortcut at this exe, carrying the toast app ID.
/// Toasts take their name and icon from it: the registry entry above alone still
/// showed the raw ID and no icon. Written on every start so it follows the exe if
/// it moves. Skipped for test builds.
#[cfg(windows)]
pub fn write_start_menu_shortcut(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    use windows::core::{Interface, HSTRING};
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, SHStrDupW, ShellLink};

    let exe = std::env::current_exe()?;
    if is_test_build(&exe) {
        return Ok(());
    }
    let programs = app.path().data_dir()?.join(r"Microsoft\Windows\Start Menu\Programs");
    let shortcut = programs.join(format!("{}.lnk", app.package_info().name));

    let id = HSTRING::from(app.config().identifier.as_str());
    let mut value = PROPVARIANT::default();

    // SAFETY: COM is used on this thread only, after CoInitializeEx. Dropping a
    // PROPVARIANT calls PropVariantClear, which frees the string with CoTaskMemFree,
    // so the string must come from COM's allocator: SHStrDupW. A pointer to Rust
    // memory here corrupts the heap and crashes the app.
    unsafe {
        // Already initialized on the app's main thread, which is fine.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        link.SetPath(&HSTRING::from(exe.as_os_str()))?;
        link.SetIconLocation(&HSTRING::from(exe.as_os_str()), 0)?;

        // The ID must be a VT_LPWSTR.
        (*value.Anonymous.Anonymous).Anonymous.pwszVal = SHStrDupW(&id)?;
        (*value.Anonymous.Anonymous).vt = VT_LPWSTR;
        let store: IPropertyStore = link.cast()?;
        store.SetValue(&PKEY_AppUserModel_ID, &value)?;
        store.Commit()?;

        link.cast::<IPersistFile>()?.Save(&HSTRING::from(shortcut.as_os_str()), true)?;
    }
    Ok(())
}

/// Called after each sync and edit. Windows needs nothing here: its loop reads the DB.
#[cfg(desktop)]
pub fn refresh(_app: &AppHandle, _st: &Arc<AppState>) {}

#[cfg(desktop)]
pub async fn run(app: AppHandle, st: Arc<AppState>) {
    loop {
        fire_due(&app, &st);
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

#[cfg(desktop)]
fn fire_due(app: &AppHandle, st: &Arc<AppState>) {
    use chrono::{Duration, Utc};
    use tauri_plugin_notification::NotificationExt;

    let now = Utc::now();
    let from = tasks_core::time::iso(now - Duration::hours(24));
    let to = tasks_core::time::iso(now + Duration::seconds(1));
    let (due, mut fired) = {
        let conn = crate::state::lock(&st.db);
        let due = tasks_core::ops::reminders_between(&conn, &from, &to).unwrap_or_default();
        let fired: Vec<String> = tasks_core::db::get_state(&conn, FIRED_KEY)
            .ok()
            .flatten()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        (due, fired)
    };
    let mut any = false;
    for task in due {
        let key = format!("{}:{}", task.id, task.remind_at.as_deref().unwrap_or_default());
        if fired.contains(&key) {
            continue;
        }
        #[cfg(windows)]
        let shown = show_toast(app, &task).inspect_err(|e| eprintln!("Can't show the reminder toast: {e}")).is_ok();
        #[cfg(not(windows))]
        let shown = false;
        if !shown {
            let _ = app.notification().builder().title("Task reminder").body(&tasks_core::plain_title(&task.title)).show();
        }
        fired.push(key);
        any = true;
    }
    if any {
        let keep = fired.len().saturating_sub(200);
        let text = serde_json::to_string(&fired[keep..]).unwrap_or_default();
        let _ = tasks_core::db::set_state(&crate::state::lock(&st.db), FIRED_KEY, &text);
    }
}

/// `remind_at` in local time: the toast's header ("15:00") and its line ("Today 15:00").
/// Only reminders under a day old show, so the day is today or yesterday.
#[cfg(windows)]
fn toast_time(remind_at: &str) -> Option<(String, String)> {
    use chrono::{Local, TimeZone};

    let at = Local.timestamp_millis_opt(tasks_core::time::instant_ms(remind_at)?).single()?;
    let day = if at.date_naive() == Local::now().date_naive() { "Today" } else { "Yesterday" };
    let time = at.format("%H:%M").to_string();
    Some((time.clone(), format!("{day} {time}")))
}

#[cfg(windows)]
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// The toast's XML. Toasts sharing a header group under it in the notification centre.
#[cfg(windows)]
fn toast_xml(id: &str, title: &str, remind_at: &str) -> Option<String> {
    let (time, line) = toast_time(remind_at)?;
    let title = xml_escape(title);
    let open = xml_escape(&format!("{TOAST_LINK}open/{id}"));
    let snooze = xml_escape(&format!("{TOAST_LINK}snooze/{id}/{remind_at}"));
    let complete = xml_escape(&format!("{TOAST_LINK}complete/{id}/{remind_at}"));
    Some(format!(
        r#"<toast scenario="reminder" launch="{open}" activationType="protocol">
  <header id="{time}" title="{time}" arguments="{TOAST_LINK}open" activationType="protocol"/>
  <visual><binding template="ToastGeneric"><text>{title}</text><text>{line}</text></binding></visual>
  <actions>
    <action content="Snooze" arguments="{snooze}" activationType="protocol"/>
    <action content="Complete" arguments="{complete}" activationType="protocol"/>
  </actions>
</toast>"#
    ))
}

#[cfg(windows)]
fn show_toast(app: &AppHandle, task: &tasks_core::Task) -> Result<(), Box<dyn std::error::Error>> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

    let title = tasks_core::plain_title(&task.title);
    let xml = toast_xml(&task.id, &title, task.remind_at.as_deref().unwrap_or_default()).ok_or("bad reminder time")?;
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app.config().identifier.as_str()))?;
    notifier.Show(&toast)?;
    Ok(())
}

/// Runs a toast's button if `args` holds its link. False when there is no link.
#[cfg(desktop)]
pub fn handle_toast_action(app: &AppHandle, args: &[String]) -> bool {
    use tauri::{Emitter, Manager};

    let Some(link) = args.iter().find_map(|a| a.strip_prefix(TOAST_LINK)) else { return false };
    let Some(st) = app.try_state::<Arc<AppState>>() else { return true };
    let open = |id: &str| {
        crate::show_main(app);
        *crate::state::lock(&st.open_task) = Some(id.to_string());
        let _ = app.emit(crate::EVENT_OPEN_TASK, id);
    };
    let mut parts = link.splitn(3, '/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some("open"), Some(id), _) => open(id),
        (Some(action @ ("snooze" | "complete")), Some(id), Some(at)) => match run_button(&st, action, id, at) {
            Ok(true) => {
                let _ = app.emit(crate::syncer::EVENT_CHANGED, 1);
            }
            // The task moved on since this toast showed, so show it as it is now.
            Ok(false) => open(id),
            Err(e) => eprintln!("Can't {action} the task: {e}"),
        },
        _ => crate::show_main(app),
    }
    true
}

/// Applies Snooze or Complete. False when the toast is out of date: the task was
/// completed, or its reminder changed, on this device or another. Ticking a
/// repeating task again would skip a date.
#[cfg(desktop)]
fn run_button(st: &AppState, action: &str, id: &str, remind_at: &str) -> Result<bool, String> {
    crate::commands::db(st, true, |conn, ctx| {
        let task = tasks_core::ops::get_task(conn, id)?;
        if task.completed_at.is_some() || task.remind_at.as_deref() != Some(remind_at) {
            return Ok(false);
        }
        let patch = if action == "snooze" { snooze_patch() } else { complete_patch(&task) };
        tasks_core::ops::update_task(conn, ctx, id, patch)?;
        Ok(true)
    })
}

#[cfg(desktop)]
fn snooze_patch() -> tasks_core::TaskPatch {
    let at = chrono::Utc::now() + chrono::Duration::minutes(SNOOZE_MINUTES);
    tasks_core::TaskPatch { remind_at: Some(Some(tasks_core::time::iso(at))), ..Default::default() }
}

/// What the page's tick does: a repeating task moves to its next date and its
/// reminder by as many days; any other task is done.
#[cfg(desktop)]
fn complete_patch(task: &tasks_core::Task) -> tasks_core::TaskPatch {
    use chrono::{Local, NaiveDate};

    let today = Local::now().date_naive();
    let due = task.due_date.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    let next = match (task.recurrence.as_deref(), due) {
        (Some(rule), Some(due)) => tasks_core::recurrence::next_occurrence(rule, due, today).map(|next| (due, next)),
        _ => None,
    };
    match next {
        Some((due, next)) => tasks_core::TaskPatch {
            due_date: Some(Some(next.to_string())),
            remind_at: Some(task.remind_at.as_deref().and_then(|at| shift_days(at, (next - due).num_days()))),
            completes_occurrence: Some(true),
            ..Default::default()
        },
        None => tasks_core::TaskPatch { completed_at: Some(Some(tasks_core::time::now_iso())), ..Default::default() },
    }
}

/// Moves an instant by whole days, keeping its local time of day across a clock change.
#[cfg(desktop)]
fn shift_days(at: &str, days: i64) -> Option<String> {
    use chrono::{Duration, Local, TimeZone, Utc};

    let local = Local.timestamp_millis_opt(tasks_core::time::instant_ms(at)?).single()?;
    let same_time = (local.date_naive() + Duration::days(days)).and_time(local.time());
    let moved = same_time.and_local_timezone(Local).earliest().unwrap_or(local + Duration::days(days));
    Some(tasks_core::time::iso(moved.with_timezone(&Utc)))
}

/// A notification id for a task (Android wants an i32).
#[cfg(mobile)]
fn notification_id(task_id: &str) -> i32 {
    // FNV-1a, folded to a positive i32.
    let mut h: u32 = 0x811c9dc5;
    for b in task_id.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    (h & 0x7fff_ffff) as i32
}

/// Android keeps a limited number of alarms per app; the soonest are scheduled.
#[cfg(mobile)]
const MAX_SCHEDULED: usize = 200;

#[cfg(mobile)]
pub fn refresh(app: &AppHandle, st: &Arc<AppState>) {
    use tauri_plugin_notification::{NotificationExt, Schedule};

    let upcoming = {
        let conn = crate::state::lock(&st.db);
        let now = tasks_core::time::iso(chrono::Utc::now() + chrono::Duration::seconds(5));
        tasks_core::ops::upcoming_reminders(&conn, &now).unwrap_or_default()
    };
    let n = app.notification();
    // cancel_all() only knows ids the plugin saved, and show() saves none, so
    // the ids scheduled last time are kept here and cancelled by name.
    let previous: Vec<i32> = tasks_core::db::get_state(&crate::state::lock(&st.db), SCHEDULED_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if !previous.is_empty() {
        let _ = n.cancel(previous);
    }
    let mut scheduled = Vec::new();
    for task in upcoming.into_iter().take(MAX_SCHEDULED) {
        let Some(ms) = task.remind_at.as_deref().and_then(tasks_core::time::instant_ms) else { continue };
        let Ok(date) = time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000) else { continue };
        let id = notification_id(&task.id);
        scheduled.push(id);
        let _ = n
            .builder()
            .id(id)
            .title("Task reminder")
            .body(&tasks_core::plain_title(&task.title))
            .extra("task_id", &task.id)
            .auto_cancel()
            .schedule(Schedule::At { date, repeating: false, allow_while_idle: true })
            .show();
    }
    let text = serde_json::to_string(&scheduled).unwrap_or_default();
    let _ = tasks_core::db::set_state(&crate::state::lock(&st.db), SCHEDULED_KEY, &text);
}

/// The notification ids scheduled by the last refresh().
#[cfg(mobile)]
const SCHEDULED_KEY: &str = "reminders_scheduled";

/// Mobile has no loop; scheduling is done by refresh().
#[cfg(mobile)]
pub async fn run(_app: AppHandle, _st: Arc<AppState>) {}
