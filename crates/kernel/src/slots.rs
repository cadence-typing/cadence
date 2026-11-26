//! Where a queued missed word may be re-served (#43).
//!
//! `engine::learning::Learning::open` places each queued word in a review
//! slot that is not a due review, else in a broad slot; never in a target
//! slot or a due review, and never when the word is already in the
//! exercise (it stays queued for the next one).

/// The lane a slot was planned in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotLane {
    /// A focus-targeted slot.
    Target,
    /// A review slot (due reviews and reinforcements).
    Review,
    /// A broad-coverage slot.
    Broad,
}

/// Whether a queued word may replace a slot of `lane`, where `due_review`
/// says the slot holds a due spaced review and `present` that the word is
/// already in the exercise.
#[must_use]
pub fn substitute(lane: SlotLane, due_review: bool, present: bool) -> bool {
    match lane {
        SlotLane::Target => false,
        SlotLane::Review => !due_review && !present,
        SlotLane::Broad => !due_review && !present,
    }
}
