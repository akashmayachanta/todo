//! The next date of a repeating task, for ticking one outside the page (a
//! notification's Complete button). `nextOccurrence` in the app's taskParse.ts is
//! the same rule for the page, so change the two together.

use chrono::{Datelike, Days, Months, NaiveDate};

enum Rule {
    Days(u32),
    Months(u32),
    /// Days of the week, counted from Sunday = 0.
    OnDays(Vec<u32>),
}

const WEEKDAYS: [&str; 7] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

fn unit(name: &str, every: u32) -> Option<Rule> {
    match name {
        "day" => Some(Rule::Days(every)),
        "week" => Some(Rule::Days(every.checked_mul(7)?)),
        "month" => Some(Rule::Months(every)),
        "year" => Some(Rule::Months(every.checked_mul(12)?)),
        _ => None,
    }
}

fn rule(recurrence: &str) -> Option<Rule> {
    let lower = recurrence.to_lowercase();
    let r = lower.strip_prefix("every").unwrap_or(&lower).trim();
    if r == "weekday" {
        return Some(Rule::OnDays(vec![1, 2, 3, 4, 5]));
    }
    if let Some((count, name)) = r.split_once(char::is_whitespace) {
        if let Ok(every) = count.parse::<u32>() {
            // "every 0 days" never moves on, so it isn't a rule.
            return if every == 0 { None } else { unit(name.trim().strip_suffix('s')?, every) };
        }
    }
    if let Some(found) = unit(r, 1) {
        return Some(found);
    }
    let days = r.split(',').map(|d| WEEKDAYS.iter().position(|w| *w == d.trim()).map(|i| i as u32));
    days.collect::<Option<Vec<u32>>>().map(Rule::OnDays)
}

/// The next due date after `due`, rolled on past `today` so finishing an overdue
/// "every day" doesn't leave it overdue. None when the recurrence isn't understood.
pub fn next_occurrence(recurrence: &str, due: NaiveDate, today: NaiveDate) -> Option<NaiveDate> {
    match rule(recurrence)? {
        Rule::Days(n) => {
            let mut next = due.checked_add_days(Days::new(n.into()))?;
            while next <= today {
                next = next.checked_add_days(Days::new(n.into()))?;
            }
            Some(next)
        }
        // Counted from the first date, so the 31st comes back after a shorter month.
        Rule::Months(n) => (1u32..)
            .map(|k| due.checked_add_months(Months::new(k.checked_mul(n)?)))
            .find(|next| next.is_none_or(|d| d > today))
            .flatten(),
        Rule::OnDays(days) => {
            let mut next = due.checked_add_days(Days::new(1))?;
            while next <= today || !days.contains(&next.weekday().num_days_from_sunday()) {
                next = next.checked_add_days(Days::new(1))?;
            }
            Some(next)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::next_occurrence;
    use chrono::NaiveDate;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn next(recurrence: &str, due: &str, today: &str) -> Option<String> {
        next_occurrence(recurrence, d(due), d(today)).map(|x| x.to_string())
    }

    #[test]
    fn steps_by_days_and_weeks() {
        assert_eq!(next("every day", "2026-11-05", "2026-11-05").unwrap(), "2026-11-06");
        assert_eq!(next("every 2 days", "2026-11-05", "2026-11-05").unwrap(), "2026-11-07");
        assert_eq!(next("every week", "2026-11-05", "2026-11-05").unwrap(), "2026-11-12");
        assert_eq!(next("every 2 weeks", "2026-11-05", "2026-11-05").unwrap(), "2026-11-19");
    }

    #[test]
    fn rolls_past_today_when_overdue() {
        assert_eq!(next("every day", "2026-11-01", "2026-11-05").unwrap(), "2026-11-06");
        assert_eq!(next("every week", "2026-10-20", "2026-11-05").unwrap(), "2026-11-10");
    }

    #[test]
    fn months_clamp_but_count_from_the_first_date() {
        assert_eq!(next("every month", "2026-01-31", "2026-01-31").unwrap(), "2026-02-28");
        assert_eq!(next("every month", "2026-01-31", "2026-02-28").unwrap(), "2026-03-31");
        assert_eq!(next("every year", "2028-02-29", "2028-02-29").unwrap(), "2029-02-28");
    }

    #[test]
    fn weekdays_and_named_days() {
        // 2026-11-06 is a Friday.
        assert_eq!(next("every weekday", "2026-11-06", "2026-11-06").unwrap(), "2026-11-09");
        assert_eq!(next("every monday, thursday", "2026-11-05", "2026-11-05").unwrap(), "2026-11-09");
        assert_eq!(next("every monday, thursday", "2026-11-02", "2026-11-02").unwrap(), "2026-11-05");
    }

    #[test]
    fn nonsense_is_not_a_rule() {
        assert_eq!(next("every 0 days", "2026-11-05", "2026-11-05"), None);
        assert_eq!(next("sometimes", "2026-11-05", "2026-11-05"), None);
        assert_eq!(next("", "2026-11-05", "2026-11-05"), None);
    }
}
