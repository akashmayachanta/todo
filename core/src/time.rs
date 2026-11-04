//! Instants: `updated_at` / `deleted_at` are epoch milliseconds; the instants a
//! user sees (created_at, completed_at, remind_at) are ISO strings in one
//! canonical form, `2026-11-04T09:30:00.000Z`, so they compare as text.

use chrono::{DateTime, NaiveDateTime, SecondsFormat, TimeZone, Utc};

pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn now_iso() -> String {
    iso(Utc::now())
}

pub fn iso(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn ms_to_iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms).single().map(iso).unwrap_or_else(now_iso)
}

/// Parses an instant the way the old API did: RFC 3339 with an offset, or a
/// naive date-time taken as UTC (`2026-11-04 09:30:00.123456`, `…T09:30`).
/// Returns the canonical form.
pub fn parse_instant(value: &str) -> Option<String> {
    let value = value.trim();
    if let Ok(at) = DateTime::parse_from_rfc3339(value) {
        return Some(iso(at.with_timezone(&Utc)));
    }
    const NAIVE: &[&str] = &[
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
    ];
    for format in NAIVE {
        if let Ok(at) = NaiveDateTime::parse_from_str(value, format) {
            return Some(iso(Utc.from_utc_datetime(&at)));
        }
    }
    // An offset written without a colon, e.g. +0530.
    for format in ["%Y-%m-%dT%H:%M:%S%.f%z", "%Y-%m-%d %H:%M:%S%.f%z"] {
        if let Ok(at) = DateTime::parse_from_str(value, format) {
            return Some(iso(at.with_timezone(&Utc)));
        }
    }
    None
}

/// Milliseconds since the epoch for a canonical (or any parseable) instant.
pub fn instant_ms(value: &str) -> Option<i64> {
    let canonical = parse_instant(value)?;
    DateTime::parse_from_rfc3339(&canonical).ok().map(|at| at.timestamp_millis())
}
