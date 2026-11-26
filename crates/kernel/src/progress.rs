//! Validity of one track's cumulative counters.

/// Largest value a persisted counter may take (`i64::MAX`, the SQLite
/// integer range).
pub const COUNTER_MAXIMUM: u64 = 9_223_372_036_854_775_807;

/// The cumulative counters of one track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackCounters {
    /// First-correct positions.
    pub first_correct: u64,
    /// First-attempted positions.
    pub first_attempted: u64,
    /// Correct content insertions.
    pub correct_attempts: u64,
    /// All content insertions.
    pub content_attempts: u64,
    /// Deleted units.
    pub deletions: u64,
    /// Insertions beyond the target.
    pub overflow_attempts: u64,
    /// Positionally correct units.
    pub correct_units: u64,
    /// Positionally correct digits.
    pub correct_digits: u64,
    /// Track-active duration in microseconds.
    pub active_micros: u64,
}

/// Whether every counter is within the persisted range.
#[must_use]
pub fn in_range(t: TrackCounters) -> bool {
    t.first_correct <= COUNTER_MAXIMUM
        && t.first_attempted <= COUNTER_MAXIMUM
        && t.correct_attempts <= COUNTER_MAXIMUM
        && t.content_attempts <= COUNTER_MAXIMUM
        && t.deletions <= COUNTER_MAXIMUM
        && t.overflow_attempts <= COUNTER_MAXIMUM
        && t.correct_units <= COUNTER_MAXIMUM
        && t.correct_digits <= COUNTER_MAXIMUM
        && t.active_micros <= COUNTER_MAXIMUM
}

/// Whether the counters are ordered: first-correct at most first-attempted
/// at most all content insertions, correct at most all insertions, and so
/// on.
#[must_use]
pub fn ordered(t: TrackCounters) -> bool {
    t.first_correct <= t.first_attempted
        && t.first_attempted <= t.content_attempts
        && t.correct_attempts <= t.content_attempts
        && t.first_correct <= t.correct_attempts
        && t.overflow_attempts <= t.content_attempts
        && t.correct_units <= t.correct_attempts
        && t.correct_digits <= t.correct_units
}

/// Whether correct plus overflow insertions fit within all insertions.
#[must_use]
pub fn split_fits(t: TrackCounters) -> bool {
    match t.correct_attempts.checked_add(t.overflow_attempts) {
        Some(sum) => sum <= t.content_attempts,
        None => false,
    }
}

/// Whether the counters are internally consistent: every value within the
/// persisted range, first-correct at most first-attempted at most all
/// content insertions, and correct plus overflow insertions at most all
/// insertions.
#[must_use]
pub fn track_valid(t: TrackCounters) -> bool {
    in_range(t) && ordered(t) && split_fits(t)
}

/// The result of merging an exercise's counters into a track total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merge {
    /// Every counter is the exact sum.
    Merged(TrackCounters),
    /// A counter's sum does not fit `u64`; nothing was merged.
    Overflow,
}

/// Merges one exercise's counters into a track total: each of the nine
/// counters is the exact sum, or the merge is refused as a whole.
#[must_use]
pub fn merge(total: TrackCounters, exercise: TrackCounters) -> Merge {
    match total.first_correct.checked_add(exercise.first_correct) {
        None => Merge::Overflow,
        Some(first_correct) => match total.first_attempted.checked_add(exercise.first_attempted) {
            None => Merge::Overflow,
            Some(first_attempted) => match total
                .correct_attempts
                .checked_add(exercise.correct_attempts)
            {
                None => Merge::Overflow,
                Some(correct_attempts) => match total
                    .content_attempts
                    .checked_add(exercise.content_attempts)
                {
                    None => Merge::Overflow,
                    Some(content_attempts) => match total.deletions.checked_add(exercise.deletions)
                    {
                        None => Merge::Overflow,
                        Some(deletions) => match total
                            .overflow_attempts
                            .checked_add(exercise.overflow_attempts)
                        {
                            None => Merge::Overflow,
                            Some(overflow_attempts) => {
                                match total.correct_units.checked_add(exercise.correct_units) {
                                    None => Merge::Overflow,
                                    Some(correct_units) => match total
                                        .correct_digits
                                        .checked_add(exercise.correct_digits)
                                    {
                                        None => Merge::Overflow,
                                        Some(correct_digits) => match total
                                            .active_micros
                                            .checked_add(exercise.active_micros)
                                        {
                                            None => Merge::Overflow,
                                            Some(active_micros) => Merge::Merged(TrackCounters {
                                                first_correct,
                                                first_attempted,
                                                correct_attempts,
                                                content_attempts,
                                                deletions,
                                                overflow_attempts,
                                                correct_units,
                                                correct_digits,
                                                active_micros,
                                            }),
                                        },
                                    },
                                }
                            }
                        },
                    },
                },
            },
        },
    }
}

/// Whether the exercise opened as `closing` may be credited, given the last
/// credited one: only an exercise newer than every credited one is, so no
/// exercise's counters are merged twice (the engine half of `NoDoubleCredit`).
#[must_use]
pub fn credit_once(last_credited: Option<u64>, closing: u64) -> bool {
    match last_credited {
        None => true,
        Some(last) => closing > last,
    }
}
