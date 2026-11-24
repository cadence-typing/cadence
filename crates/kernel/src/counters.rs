//! Cumulative event counters of one exercise attempt (L-02). Every
//! increment is checked; a refusal leaves the caller's counters unchanged.

/// Cumulative event facts. Monotone within an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters {
    /// All content insertions, including overflow attempts.
    pub content: u64,
    /// Content insertions that matched the expected position.
    pub correct: u64,
    /// Deleted units (backspace and delete-word).
    pub deletions: u64,
    /// Content insertions beyond the target; never correct.
    pub overflow: u64,
}

/// Whether an insertion counts as correct: it needs an expected unit at
/// its position (`expected_present`) and must equal it (`same`). An
/// insertion past the target has no expected unit and never matches.
#[must_use]
pub fn matched(expected_present: bool, same: bool) -> bool {
    expected_present && same
}

/// What one counter update decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The updated counters.
    Counted(Counters),
    /// A counter would overflow; nothing changed.
    Overflow,
}

/// The counters after one content insertion, or [`Step::Overflow`] when a
/// counter would overflow. `expected_present` is whether the target has a
/// unit at the insertion position; `same` whether the typed unit equals it.
#[must_use]
pub fn content_step(c: Counters, expected_present: bool, same: bool) -> Step {
    let add_correct: u64 = if matched(expected_present, same) {
        1
    } else {
        0
    };
    let add_overflow: u64 = if expected_present { 0 } else { 1 };
    match c.content.checked_add(1) {
        None => Step::Overflow,
        Some(content) => match c.correct.checked_add(add_correct) {
            None => Step::Overflow,
            Some(correct) => match c.overflow.checked_add(add_overflow) {
                None => Step::Overflow,
                Some(overflow) => Step::Counted(Counters {
                    content,
                    correct,
                    deletions: c.deletions,
                    overflow,
                }),
            },
        },
    }
}

/// The counters after `removed` units were deleted, or [`Step::Overflow`].
#[must_use]
pub fn deletion_step(c: Counters, removed: u64) -> Step {
    match c.deletions.checked_add(removed) {
        None => Step::Overflow,
        Some(deletions) => Step::Counted(Counters {
            content: c.content,
            correct: c.correct,
            deletions,
            overflow: c.overflow,
        }),
    }
}
