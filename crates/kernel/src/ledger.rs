//! One attempt-ledger step (L-02, VK-01, #32): whether a content key is
//! admitted under the buffer bound, where it lands, the first-outcome slot
//! write, the per-word flags and completion. `cadence_core::ledger::
//! AttemptLedger::apply` is the adapter: it owns the vectors and the typed
//! units and asks these functions for every decision.

use crate::counters::{Counters, Step, content_step, matched};
use crate::first::{Slot, settle};

/// What a content key does to the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insert {
    /// The buffer is at its bound (target length plus the extra-input
    /// allowance): nothing is inserted or counted; measurement is
    /// interrupted.
    Rejected,
    /// A counter would overflow; nothing changed.
    Overflow,
    /// The key is inserted at the buffer tail and counted.
    Counted {
        /// The counters after the insertion.
        counters: Counters,
        /// Whether the key equals the expected unit at its position.
        matched: bool,
        /// Whether its position is inside the target (else it is overflow).
        in_target: bool,
    },
}

/// The decision for one content key typed at position `buffer_len`, where
/// `same` is whether it equals the target unit there (false past the
/// target).
#[must_use]
pub fn insert(
    c: Counters,
    buffer_len: usize,
    target_len: usize,
    allowance: usize,
    same: bool,
) -> Insert {
    match target_len.checked_add(allowance) {
        None => Insert::Rejected,
        Some(bound) => {
            if buffer_len >= bound {
                Insert::Rejected
            } else {
                let in_target = buffer_len < target_len;
                match content_step(c, in_target, same) {
                    Step::Overflow => Insert::Overflow,
                    Step::Counted(next) => Insert::Counted {
                        counters: next,
                        matched: matched(in_target, same),
                        in_target,
                    },
                }
            }
        }
    }
}

/// Records a content key's first outcome at `position`: the slot there is
/// settled write-once; every other slot is untouched.
pub fn settle_at(slots: &mut [Slot], position: usize, matched: bool) {
    if let Some(slot) = slots.get_mut(position) {
        *slot = settle(*slot, matched);
    }
}

/// Per-word flags (L-02): any error, deletion, interruption or paste makes
/// the word non-clean. Every flag is sticky within an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WordFlags {
    /// Any first-incorrect or incorrect insertion in this word.
    pub any_error: bool,
    /// Any deletion event while this word was being typed.
    pub any_deletion: bool,
    /// Measurement was interrupted (paste, overload, quit) during this word.
    pub interrupted: bool,
    /// A paste event contributed to this word.
    pub pasted: bool,
}

/// The word's flags after a content key; an unmatched key marks an error.
#[must_use]
pub fn flags_after_insert(f: WordFlags, matched: bool) -> WordFlags {
    WordFlags {
        any_error: f.any_error || !matched,
        any_deletion: f.any_deletion,
        interrupted: f.interrupted,
        pasted: f.pasted,
    }
}

/// The word's flags after a deletion.
#[must_use]
pub fn flags_after_deletion(f: WordFlags) -> WordFlags {
    WordFlags {
        any_error: f.any_error,
        any_deletion: true,
        interrupted: f.interrupted,
        pasted: f.pasted,
    }
}

/// The word's flags after measurement was interrupted.
#[must_use]
pub fn flags_after_interrupt(f: WordFlags) -> WordFlags {
    WordFlags {
        any_error: f.any_error,
        any_deletion: f.any_deletion,
        interrupted: true,
        pasted: f.pasted,
    }
}

/// Whether a word was typed without error, deletion, interruption or paste.
#[must_use]
pub fn clean(f: WordFlags) -> bool {
    !f.any_error && !f.any_deletion && !f.interrupted && !f.pasted
}

/// Whether the buffer equals the target, from `correct_visible`, the number
/// of buffer positions that equal the target unit there: the buffer has
/// exactly the target's length and every position matches.
#[must_use]
pub fn complete(buffer_len: usize, target_len: usize, correct_visible: usize) -> bool {
    buffer_len == target_len && correct_visible == target_len
}
