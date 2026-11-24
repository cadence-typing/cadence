//! First-attempt outcomes: write-once per expected position (AGENTS.md
//! rule 1). A later correction never changes a historical outcome.

/// The first content insertion outcome at one expected position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Nothing has been typed at this position yet.
    Open,
    /// The first content insertion at this position matched the target.
    Correct,
    /// The first content insertion at this position did not match.
    Incorrect,
}

/// The slot after a content insertion at its position, where `matched`
/// says whether the insertion equals the expected unit. An open slot takes
/// the outcome; a settled slot is returned unchanged, whatever was typed.
#[must_use]
pub fn settle(slot: Slot, matched: bool) -> Slot {
    match slot {
        Slot::Open => {
            if matched {
                Slot::Correct
            } else {
                Slot::Incorrect
            }
        }
        Slot::Correct => Slot::Correct,
        Slot::Incorrect => Slot::Incorrect,
    }
}

/// First-correct and first-attempted position counts of a slot list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirstCounts {
    /// Positions whose first insertion matched.
    pub correct: usize,
    /// Positions that have a first insertion at all.
    pub attempted: usize,
}

/// Counts first outcomes: `attempted` settled slots, `correct` of them
/// correct. Never exceeds the list length, so the saturating additions
/// never saturate.
#[must_use]
pub fn count_first(slots: &[Slot]) -> FirstCounts {
    count_first_loop(slots, 0, 0, 0)
}

/// The counting loop of [`count_first`] from position `i` with the counts
/// of the positions before it.
#[must_use]
pub fn count_first_loop(slots: &[Slot], i: usize, correct: usize, attempted: usize) -> FirstCounts {
    let mut i = i;
    let mut correct = correct;
    let mut attempted = attempted;
    while i < slots.len() {
        if let Some(slot) = slots.get(i) {
            match slot {
                Slot::Correct => {
                    correct = correct.saturating_add(1);
                    attempted = attempted.saturating_add(1);
                }
                Slot::Incorrect => {
                    attempted = attempted.saturating_add(1);
                }
                Slot::Open => {}
            }
        }
        i = i.saturating_add(1);
    }
    FirstCounts { correct, attempted }
}
