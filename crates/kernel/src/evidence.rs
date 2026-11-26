//! Learner evidence attribution (L-02, L-03; #42, #41).
//!
//! `engine::learning::Learning::first_attempt` asks [`pair_first_correct`]
//! whether a transition's first attempt was correct (both of its keys
//! were) and [`clean_interval`] whether the gap before a key is a clean
//! timing sample: adjacent, both keys matched, no pause between them
//! (the timing-segment ordinal is unchanged), a positive gap within the
//! hesitation ceiling.

use crate::first::Slot;

/// Whether a transition into a key is first-correct: the previous
/// position's first attempt was correct and this key matched.
#[must_use]
pub fn pair_first_correct(previous: Slot, matched: bool) -> bool {
    match previous {
        Slot::Correct => matched,
        Slot::Open | Slot::Incorrect => false,
    }
}

/// The last matched key, the anchor of the next interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    /// Whether there is an anchor at all.
    pub present: bool,
    /// Its position in the target.
    pub position: usize,
    /// Active time when it was typed (microseconds).
    pub active_us: u64,
    /// The timing segment it was typed in.
    pub segment: u64,
}

/// Whether the gap before a key is a clean timing sample, or why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interval {
    /// A clean sample of this many microseconds.
    Timed(u64),
    /// No anchor (the start, or after a break).
    NoAnchor,
    /// The anchor is not the previous position.
    NotAdjacent,
    /// This key did not match.
    Error,
    /// A pause separates the two keys.
    CrossesPause,
    /// Zero gap: insufficient timing resolution.
    Unresolved,
    /// Active time went backwards.
    Backward,
    /// Longer than the hesitation ceiling.
    AboveCeiling,
}

/// The interval before a key at `position`, typed at `active_us` in
/// timing segment `segment`.
#[must_use]
pub fn clean_interval(
    mark: Mark,
    position: usize,
    matched: bool,
    active_us: u64,
    segment: u64,
    ceiling_us: u64,
) -> Interval {
    if !mark.present {
        return Interval::NoAnchor;
    }
    let adjacent = match mark.position.checked_add(1) {
        None => false,
        Some(next) => next == position,
    };
    if !adjacent {
        return Interval::NotAdjacent;
    }
    if !matched {
        return Interval::Error;
    }
    if mark.segment != segment {
        return Interval::CrossesPause;
    }
    match active_us.checked_sub(mark.active_us) {
        None => Interval::Backward,
        Some(gap) => {
            if gap == 0 {
                Interval::Unresolved
            } else if gap > ceiling_us {
                Interval::AboveCeiling
            } else {
                Interval::Timed(gap)
            }
        }
    }
}
