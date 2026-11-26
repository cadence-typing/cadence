//! What a served item is called (#39, provides #7).
//!
//! An item is `Fresh` only when nothing makes it a repetition: it was not
//! served earlier this session, it is not a reinforcement or a due review,
//! its list has not wrapped and, for prose, the passage is outside the
//! no-repeat window. Precedence: `Again` over `DueReview` over `Repeat`.
//! Callers: `engine::presentation::exercise_context` and
//! `trainer::plan_content` (the prose caption).

/// The facts a label is decided from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    /// Times this item was served earlier in the session.
    pub served_before: u32,
    /// The item is a same-session reinforcement of a miss.
    pub reinforcement: bool,
    /// The item is a due review.
    pub due_review: bool,
    /// The prepared list it comes from has wrapped.
    pub wrapped: bool,
    /// A prose passage typed within the no-repeat window.
    pub recent_passage: bool,
}

/// The label shown for an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// Never served before in any sense above.
    Fresh,
    /// A same-session reinforcement of a miss.
    Again,
    /// A due review.
    DueReview,
    /// Any other repetition.
    Repeat,
}

/// The label of an item.
#[must_use]
pub fn of(f: Facts) -> Label {
    if f.reinforcement {
        Label::Again
    } else if f.due_review {
        Label::DueReview
    } else if f.served_before > 0 || f.wrapped || f.recent_passage {
        Label::Repeat
    } else {
        Label::Fresh
    }
}
