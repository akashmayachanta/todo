//! The reminder rules, in one place for every device, the server and the AI
//! (GLOSSARY.md: Due moment, Default reminder, Lead):
//!
//! - A task whose due date or time is set while it has no reminder, and none is
//!   asked for, is reminded at its due moment, if that is still ahead.
//! - When the due date or time changes and no reminder is asked for, the reminder
//!   moves so it stays as long before the due moment as it was.
//!
//! The zone and the clock are parameters, so the tests can fix them; `ops` passes
//! the device's own zone (`chrono::Local`) and the current time.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};

use crate::time::{instant_ms, iso};

/// The time of day a date with no time counts from: its due moment, and its default reminder.
pub const DATE_ONLY_TIME: &str = "10:00";

/// A task's due date and time, as stored (`YYYY-MM-DD`, `HH:MM`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Due<'a> {
    pub date: Option<&'a str>,
    pub time: Option<&'a str>,
}

/// The instant a task is due in `tz`: its date at its time, or at 10:00 for a date
/// alone. None without a date. A time skipped by a clock change counts from an hour later.
pub fn due_moment<Tz: TimeZone>(tz: &Tz, due: Due) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(due.date?, "%Y-%m-%d").ok()?;
    let time = NaiveTime::parse_from_str(due.time.unwrap_or(DATE_ONLY_TIME), "%H:%M").ok()?;
    let local = date.and_time(time);
    let at = tz
        .from_local_datetime(&local)
        .earliest()
        .or_else(|| tz.from_local_datetime(&(local + Duration::hours(1))).earliest())?;
    Some(at.with_timezone(&Utc))
}

/// The reminder a task gets by default: at its due moment, when that is after `now`.
pub fn default_reminder<Tz: TimeZone>(tz: &Tz, due: Due, now: DateTime<Utc>) -> Option<String> {
    due_moment(tz, due).filter(|at| *at > now).map(iso)
}

/// A reminder moved from one due moment to another, keeping its lead. A reminder
/// that can't be read, or a task that had no due moment, keeps the reminder as it is.
/// None when the task no longer has a date.
pub fn follow<Tz: TimeZone>(tz: &Tz, remind_at: &str, was: Due, now_due: Due) -> Option<String> {
    let to = due_moment(tz, now_due)?;
    let (Some(from), Some(at)) = (due_moment(tz, was), instant_ms(remind_at)) else {
        return Some(remind_at.to_string());
    };
    let lead = from.timestamp_millis() - at;
    Some(iso(to - Duration::milliseconds(lead)))
}

/// The reminder after a change of due date or time from `was` to `now_due`, when
/// the change doesn't ask for one: the old reminder moved with the due moment, or
/// for a task without one the default reminder, if the change `sets` a date or a
/// time (only taking the time off brings none).
pub fn after_due_change<Tz: TimeZone>(
    tz: &Tz,
    now: DateTime<Utc>,
    remind_at: Option<&str>,
    was: Due,
    now_due: Due,
    sets: bool,
) -> Option<String> {
    match remind_at {
        Some(at) => follow(tz, at, was, now_due),
        None if sets => default_reminder(tz, now_due, now),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn india() -> FixedOffset {
        FixedOffset::east_opt(330 * 60).unwrap()
    }

    fn new_york() -> FixedOffset {
        FixedOffset::west_opt(5 * 3600).unwrap()
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn due<'a>(date: &'a str, time: Option<&'a str>) -> Due<'a> {
        Due { date: Some(date), time }
    }

    #[test]
    fn the_due_moment_is_local_and_a_date_alone_counts_from_ten() {
        assert_eq!(due_moment(&india(), due("2026-11-30", Some("15:00"))), Some(at("2026-11-30T09:30:00Z")));
        assert_eq!(due_moment(&india(), due("2026-11-30", None)), Some(at("2026-11-30T04:30:00Z")));
        assert_eq!(due_moment(&new_york(), due("2026-11-30", None)), Some(at("2026-11-30T15:00:00Z")));
        assert_eq!(due_moment(&india(), Due::default()), None);
    }

    #[test]
    fn the_default_reminder_is_at_the_due_moment_while_that_is_ahead() {
        let now = at("2026-11-20T06:00:00Z"); // 11:30 in India
        assert_eq!(default_reminder(&india(), due("2026-11-20", Some("15:00")), now).as_deref(), Some("2026-11-20T09:30:00.000Z"));
        // 10:00 today has passed: no reminder.
        assert_eq!(default_reminder(&india(), due("2026-11-20", None), now), None);
        assert_eq!(default_reminder(&india(), Due::default(), now), None);
    }

    #[test]
    fn a_reminder_keeps_its_lead_when_the_date_or_time_changes() {
        // Due 15:00, reminded 14:30 (India). Moved to the next day: 14:30 then.
        let moved = follow(&india(), "2026-11-30T09:00:00.000Z", due("2026-11-30", Some("15:00")), due("2026-12-01", Some("15:00")));
        assert_eq!(moved.as_deref(), Some("2026-12-01T09:00:00.000Z"));
        // Moved to 18:00 the same day: 17:30.
        let later = follow(&india(), "2026-11-30T09:00:00.000Z", due("2026-11-30", Some("15:00")), due("2026-11-30", Some("18:00")));
        assert_eq!(later.as_deref(), Some("2026-11-30T12:00:00.000Z"));
        // The time taken off: it counts from 10:00, so 9:30.
        let untimed = follow(&india(), "2026-11-30T09:00:00.000Z", due("2026-11-30", Some("15:00")), due("2026-11-30", None));
        assert_eq!(untimed.as_deref(), Some("2026-11-30T04:00:00.000Z"));
    }

    #[test]
    fn a_reminder_with_nothing_to_follow_is_kept() {
        assert_eq!(follow(&india(), "2026-11-30T09:00:00.000Z", Due::default(), due("2026-12-01", None)).as_deref(), Some("2026-11-30T09:00:00.000Z"));
        assert_eq!(follow(&india(), "not a time", due("2026-11-30", None), due("2026-12-01", None)).as_deref(), Some("not a time"));
        assert_eq!(follow(&india(), "2026-11-30T09:00:00.000Z", due("2026-11-30", None), Due::default()), None);
    }

    #[test]
    fn after_a_change_a_task_without_a_reminder_gets_the_default() {
        let now = at("2026-11-20T06:00:00Z");
        let got = after_due_change(&india(), now, None, Due::default(), due("2026-11-25", Some("09:00")), true);
        assert_eq!(got.as_deref(), Some("2026-11-25T03:30:00.000Z"));
        // Only the time taken off: no reminder comes of it.
        assert_eq!(after_due_change(&india(), now, None, due("2026-11-25", Some("09:00")), due("2026-11-25", None), false), None);
        let kept = after_due_change(&india(), now, Some("2026-11-25T03:00:00.000Z"), due("2026-11-25", Some("09:00")), due("2026-11-26", Some("09:00")), false);
        assert_eq!(kept.as_deref(), Some("2026-11-26T03:00:00.000Z"));
    }
}
