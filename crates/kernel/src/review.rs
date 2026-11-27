//! The review ladder (L-06, VK-06, #37), over day numbers.
//!
//! An error puts a word on the first rung (due the next day). Clean typing
//! before the word is due is practice, not a retention check. A clean
//! completion on or after the due day counts toward promotion; two of them
//! on distinct days promote one rung, and the top rung does not wrap. A
//! completion dated before the recorded clean day is refused (calendar
//! rollback). `cadence_trainer::review::ladder_step` is the adapter and
//! `trainer::learner::due_words` its caller.

/// The index of the first (1-day) rung.
const FIRST_RUNG: usize = 0;

/// Clean completions counted toward promotion, with the latest one's day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Streak {
    /// Clean completions counted.
    pub count: u32,
    /// The day of the latest one.
    pub day: i64,
}

/// A word's place on the ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// Index into the interval ladder (0 = the 1-day rung).
    pub rung: usize,
    /// The clean streak toward promotion.
    pub streak: Option<Streak>,
    /// The day the word is next due, if scheduled.
    pub due: Option<i64>,
}

/// The ladder's answer to one recorded outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The word moves to this state.
    Next(State),
    /// Clean typing before the word is due: practice; the state is kept.
    Practice,
    /// The ladder is not `1, then strictly increasing positive days`, or
    /// the rung is outside it.
    Invalid,
    /// The completion predates the recorded clean day.
    Rollback,
    /// A day number does not fit.
    Overflow,
}

/// Whether the ladder starts at 1 and strictly increases.
#[must_use]
pub fn valid_ladder(intervals: &[i64]) -> bool {
    match intervals.get(FIRST_RUNG) {
        None => return false,
        Some(first) => {
            if *first != 1 {
                return false;
            }
        }
    }
    let mut previous: i64 = 1;
    let mut i = 1;
    while i < intervals.len() {
        match intervals.get(i) {
            None => return false,
            Some(current) => {
                if *current <= previous {
                    return false;
                }
                previous = *current;
            }
        }
        i = i.saturating_add(1);
    }
    true
}

/// Whether an outcome on `day` predates the counted clean day.
#[must_use]
pub fn rolled_back(streak: Option<Streak>, day: i64) -> bool {
    match streak {
        None => false,
        Some(s) => day < s.day,
    }
}

/// The streak after a due clean completion on `day`: the first one counts
/// one; a later day adds one; the same day adds nothing.
#[must_use]
pub fn next_streak(streak: Option<Streak>, day: i64) -> Streak {
    match streak {
        None => Streak { count: 1, day },
        Some(s) => {
            if day > s.day {
                Streak {
                    count: s.count.saturating_add(1),
                    day,
                }
            } else {
                s
            }
        }
    }
}

/// Whether a due clean completion on `day` promotes: it is on a day
/// strictly after a counted clean completion, so it completes the pair of
/// clean days.
#[must_use]
pub fn promotes(streak: Option<Streak>, day: i64) -> bool {
    match streak {
        None => false,
        Some(s) => day > s.day && s.count >= 1,
    }
}

/// The rung after a due clean completion: one up on promotion, never past
/// the top of a ladder of `len` rungs.
#[must_use]
pub fn next_rung(rung: usize, promote: bool, len: usize) -> usize {
    if promote && rung.saturating_add(1) < len {
        rung.saturating_add(1)
    } else {
        rung
    }
}

/// The next state of a word after a recorded outcome on `day`.
#[must_use]
pub fn ladder_step(state: State, error: bool, day: i64, intervals: &[i64]) -> Step {
    if !valid_ladder(intervals) || state.rung >= intervals.len() {
        return Step::Invalid;
    }
    if rolled_back(state.streak, day) {
        return Step::Rollback;
    }
    if error {
        let first = match intervals.get(FIRST_RUNG) {
            None => return Step::Invalid,
            Some(f) => *f,
        };
        return match day.checked_add(first) {
            None => Step::Overflow,
            Some(due) => Step::Next(State {
                rung: 0,
                streak: None,
                due: Some(due),
            }),
        };
    }
    let due = match state.due {
        None => return Step::Practice,
        Some(d) => d,
    };
    if day < due {
        return Step::Practice;
    }
    let promote = promotes(state.streak, day);
    let rung = next_rung(state.rung, promote, intervals.len());
    let interval = match intervals.get(rung) {
        None => return Step::Invalid,
        Some(i) => *i,
    };
    match day.checked_add(interval) {
        None => Step::Overflow,
        Some(next_due) => Step::Next(State {
            rung,
            streak: if promote {
                None
            } else {
                Some(next_streak(state.streak, day))
            },
            due: Some(next_due),
        }),
    }
}

/// Whether a word is due on `today`.
#[must_use]
pub fn is_due(due: Option<i64>, today: i64) -> bool {
    match due {
        None => false,
        Some(d) => d <= today,
    }
}

/// The due-review FIFO's indices (L-05, VK-06, #90): the live ids are
/// `ids[head..len]` of a slice the caller owns. Serving advances `head`, so
/// an item's rank (how many are served before it) is its index minus `head`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fifo {
    /// The index of the next item to serve.
    pub head: usize,
    /// One past the last live item.
    pub len: usize,
}

/// One FIFO decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FifoStep {
    /// Admitted at the tail: append the id at index `fifo.len - 1`.
    Appended(Fifo),
    /// The id is already queued; nothing changes.
    Duplicate,
    /// The queue is at capacity; nothing changes.
    Full,
    /// Serve the item at `index` (the head); the queue becomes `fifo`.
    Served {
        /// The queue after serving.
        fifo: Fifo,
        /// The served item's index.
        index: usize,
    },
    /// Nothing is queued.
    Empty,
}

/// Whether `id` is live in `fifo` over `ids`.
#[must_use]
pub fn fifo_contains(ids: &[u64], fifo: Fifo, id: u64) -> bool {
    let mut i = fifo.head;
    while i < fifo.len {
        if let Some(value) = ids.get(i)
            && *value == id
        {
            return true;
        }
        i = i.saturating_add(1);
    }
    false
}

/// Admits `id` at the tail (a newly due item never jumps ahead), unless it
/// is already queued or the queue holds `capacity` ids.
#[must_use]
pub fn fifo_admit(ids: &[u64], fifo: Fifo, id: u64, capacity: usize) -> FifoStep {
    if fifo_contains(ids, fifo, id) {
        return FifoStep::Duplicate;
    }
    if fifo.len >= capacity {
        return FifoStep::Full;
    }
    match fifo.len.checked_add(1) {
        Some(len) => FifoStep::Appended(Fifo {
            head: fifo.head,
            len,
        }),
        None => FifoStep::Full,
    }
}

/// Serves the head, the oldest live item.
#[must_use]
pub fn fifo_serve(fifo: Fifo) -> FifoStep {
    if fifo.head < fifo.len {
        match fifo.head.checked_add(1) {
            Some(head) => FifoStep::Served {
                fifo: Fifo {
                    head,
                    len: fifo.len,
                },
                index: fifo.head,
            },
            None => FifoStep::Empty,
        }
    } else {
        FifoStep::Empty
    }
}
