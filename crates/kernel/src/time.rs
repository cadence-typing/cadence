//! Active time and the half-open phase window (L-03). Only explicit
//! pauses leave active time; ordinary hesitation stays in it.
//!
//! Production keeps active time as closed segments plus at most one open
//! segment (`cadence_core::session::TimingSegments`), so the decision here
//! is over that shape: what the completed total plus the open segment's
//! elapsed time is, exactly.

/// What adding the open segment to the completed total decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Through {
    /// Completed active time plus the open segment's elapsed time.
    Time(u64),
    /// `now` precedes the open segment's start.
    Backward,
    /// The total does not fit `u64`.
    Overflow,
}

/// Active time through `now`: `completed` plus, when a segment is open
/// since `open_start`, `now - open_start`. All arithmetic is checked.
#[must_use]
pub fn segment_active(completed: u64, open_start: Option<u64>, now: u64) -> Through {
    match open_start {
        None => Through::Time(completed),
        Some(start) => match now.checked_sub(start) {
            None => Through::Backward,
            Some(open) => match completed.checked_add(open) {
                Some(total) => Through::Time(total),
                None => Through::Overflow,
            },
        },
    }
}

/// Whether `tick` lies in `[start, deadline)`: a key at the exact deadline
/// is outside the scored slice (A-05).
#[must_use]
pub fn in_window(tick: u64, start: u64, deadline: u64) -> bool {
    tick >= start && tick < deadline
}
