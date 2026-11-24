//! Active time and the half-open phase window (L-03). Only explicit
//! pauses leave active time; ordinary hesitation stays in it.

/// What subtracting the pauses from the elapsed time decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Active {
    /// Elapsed time minus the total of the pauses.
    Time(u64),
    /// The total of the pauses overflowed.
    PauseOverflow,
    /// The pauses (their total is carried) exceed the elapsed time.
    PausesExceed(u64),
}

/// The total of a list of pauses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Total {
    /// The exact sum.
    Sum(u64),
    /// The sum does not fit `u64`.
    Overflow,
}

/// The total of the pauses from position `i` on, added to `total`.
#[must_use]
pub fn pause_total_loop(pauses: &[u64], i: usize, total: u64) -> Total {
    let mut i = i;
    let mut total = total;
    while i < pauses.len() {
        if let Some(pause) = pauses.get(i) {
            match total.checked_add(*pause) {
                Some(next) => total = next,
                None => return Total::Overflow,
            }
        }
        i = i.saturating_add(1);
    }
    Total::Sum(total)
}

/// The total of all pauses.
#[must_use]
pub fn pause_total(pauses: &[u64]) -> Total {
    pause_total_loop(pauses, 0, 0)
}

/// Active time: `elapsed` minus every pause, all checked.
#[must_use]
pub fn active_time(elapsed: u64, pauses: &[u64]) -> Active {
    match pause_total(pauses) {
        Total::Overflow => Active::PauseOverflow,
        Total::Sum(total) => match elapsed.checked_sub(total) {
            Some(active) => Active::Time(active),
            None => Active::PausesExceed(total),
        },
    }
}

/// Whether `tick` lies in `[start, deadline)`: a key at the exact deadline
/// is outside the scored slice (A-05).
#[must_use]
pub fn in_window(tick: u64, start: u64, deadline: u64) -> bool {
    tick >= start && tick < deadline
}
