//! Block quotas and repetition caps (L-05, VK-05, #36).
//!
//! `cadence_trainer::lanes::QuotaTracker` keeps the per-block counts in its
//! maps and asks these functions for every decision: whether a word is
//! admissible (occurrence cap, consecutive cap, focus-owned target cap),
//! whether a slot may be recorded (its lane quota, then the same word
//! checks) and what the counts become, and whether a block met its quotas
//! exactly.

/// The counts and caps one decision reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    /// Slots already served in the requested lane this block.
    pub served: u32,
    /// The lane's quota per block.
    pub quota: u32,
    /// Occurrences of the word this block.
    pub occurrences: u32,
    /// The per-block occurrence cap.
    pub word_cap: u32,
    /// Whether the word is the one served in the previous slot.
    pub same_word: bool,
    /// The length of the current run of that word.
    pub run: u32,
    /// The consecutive-occurrence cap.
    pub run_cap: u32,
    /// Whether the slot is a target slot with a focus.
    pub focused: bool,
    /// Target slots the focus already owns this block.
    pub owned: u32,
    /// The focus-owned target slot cap.
    pub focus_cap: u32,
}

/// Whether a word may take a slot, or which rule refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    /// Every rule admits it.
    Admit,
    /// The lane's quota is used up.
    Quota,
    /// The word reached its per-block occurrence cap.
    WordCap,
    /// The word reached the consecutive-occurrence cap.
    Consecutive,
    /// The focus owns its maximum number of target slots.
    FocusCap,
}

/// The counts after a recorded slot, or the refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Record {
    /// The slot is recorded with these next counts.
    Recorded {
        /// Slots served in the lane.
        served: u32,
        /// Occurrences of the word.
        occurrences: u32,
        /// The run length of the word.
        run: u32,
        /// Target slots owned by the focus (unchanged unless `focused`).
        owned: u32,
    },
    /// The slot is refused for this reason; no count changes.
    Refused(Admit),
}

/// The word-level rules (not the lane quota): occurrence cap, consecutive
/// cap, then the focus-owned cap for a focused target slot.
#[must_use]
pub fn admissible(f: Facts) -> Admit {
    if f.occurrences >= f.word_cap {
        Admit::WordCap
    } else if f.same_word && f.run >= f.run_cap {
        Admit::Consecutive
    } else if f.focused && f.owned >= f.focus_cap {
        Admit::FocusCap
    } else {
        Admit::Admit
    }
}

/// Records a slot: refused when the lane quota is used up or the word is
/// not admissible; otherwise every count moves by one (the run restarts at
/// one for a different word).
#[must_use]
pub fn record(f: Facts) -> Record {
    if f.served >= f.quota {
        return Record::Refused(Admit::Quota);
    }
    match admissible(f) {
        Admit::Admit => {}
        Admit::Quota => return Record::Refused(Admit::Quota),
        Admit::WordCap => return Record::Refused(Admit::WordCap),
        Admit::Consecutive => return Record::Refused(Admit::Consecutive),
        Admit::FocusCap => return Record::Refused(Admit::FocusCap),
    }
    // Each count is below a `u32` cap here, so `+ 1` fits; saturating
    // keeps the function total without a branch.
    let served = f.served.saturating_add(1);
    let occurrences = f.occurrences.saturating_add(1);
    let run = if f.same_word {
        f.run.saturating_add(1)
    } else {
        1
    };
    let owned = if f.focused {
        f.owned.saturating_add(1)
    } else {
        f.owned
    };
    Record::Recorded {
        served,
        occurrences,
        run,
        owned,
    }
}

/// Whether a block met every lane quota exactly.
#[must_use]
pub fn complete(
    target: u32,
    target_quota: u32,
    review: u32,
    review_quota: u32,
    broad: u32,
    broad_quota: u32,
) -> bool {
    target == target_quota && review == review_quota && broad == broad_quota
}
