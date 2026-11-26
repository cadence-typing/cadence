//! Local-calendar days (L-06, #52).
//!
//! A session's day is the learner's local calendar day at session start:
//! the UTC time plus the local UTC offset, in whole days since the epoch.
//! `cadence::main` (the only place that reads the wall clock) calls
//! [`local_day`]; the review recap counts days with [`days_ago`].

/// The largest UTC offset accepted: 18 hours either way.
pub const MAX_OFFSET_SECONDS: i32 = 64_800;

/// Seconds per day.
pub const DAY_SECONDS: i64 = 86_400;

/// A local day, or why there is none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Day {
    /// Whole local days since 1970-01-01.
    Number(i64),
    /// The offset is beyond 18 hours either way.
    OffsetOutOfRange,
    /// The local time is before the epoch or does not fit.
    Overflow,
}

/// The local calendar day of `unix_seconds` (UTC) at `offset_seconds` east
/// of UTC.
#[must_use]
pub fn local_day(unix_seconds: i64, offset_seconds: i32) -> Day {
    if offset_seconds > MAX_OFFSET_SECONDS {
        return Day::OffsetOutOfRange;
    }
    if offset_seconds < -MAX_OFFSET_SECONDS {
        return Day::OffsetOutOfRange;
    }
    let local = match unix_seconds.checked_add(i64::from(offset_seconds)) {
        None => return Day::Overflow,
        Some(t) => t,
    };
    if local < 0 {
        return Day::Overflow;
    }
    match local.checked_div(DAY_SECONDS) {
        None => Day::Overflow,
        Some(d) => Day::Number(d),
    }
}

/// Whole days from `then` to `today`, 0 when `then` is today or later
/// (never negative).
#[must_use]
pub fn days_ago(today: i64, then: i64) -> i64 {
    if then >= today {
        return 0;
    }
    today.checked_sub(then).unwrap_or(i64::MAX)
}
