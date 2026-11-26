//! Same-session reinforcement queue admission (L-05, VK-06, #55).
//!
//! A missed word may be queued for one extra repetition in the same
//! session. `cadence_engine::learning::Learning::enqueue` computes the facts
//! and asks [`admit`]; a refusal only forgoes the optional repetition (the
//! miss itself is already recorded).

/// The admission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    /// The word joins the queue's tail.
    Admitted,
    /// The word is already queued.
    Duplicate,
    /// The queue is at capacity.
    Full,
    /// The word used its per-session repetitions.
    Exhausted,
}

/// Whether a missed word joins the reinforcement queue.
#[must_use]
pub fn admit(
    already_queued: bool,
    queue_len: usize,
    capacity: usize,
    served_for_word: u32,
    per_word_limit: u32,
) -> Admit {
    if served_for_word >= per_word_limit {
        Admit::Exhausted
    } else if queue_len >= capacity {
        Admit::Full
    } else if already_queued {
        Admit::Duplicate
    } else {
        Admit::Admitted
    }
}
