//! First-attempt observation ledger (L-02, VK-01).
//!
//! The ledger is the training-correctness core: it owns the immutable
//! target, the write-once first-outcome vector, cumulative event counter
//! facts, the bounded current typed buffer and per-word flags. A later
//! correction never changes a historical position outcome; deleting a
//! correct character and retyping incorrectly leaves the first outcome
//! correct but marks the word non-clean.
//!
//! Ordinary Rust implementation with property tests. No formal-proof
//! qualification is claimed for this production path.

use crate::CoreError;
use crate::alphabet::TrainingChar;
use crate::units::{TextUnitIndex, UnitCount};

#[cfg(test)]
#[path = "ledger_atomic_tests.rs"]
mod atomic_tests;

/// First content insertion outcome at an expected position, assigned
/// exactly once and never replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The first content insertion at this position matched the target.
    Correct,
    /// The first content insertion at this position did not match.
    Incorrect,
}

/// Cumulative event facts for one exercise attempt. Monotone; never
/// decrease within an attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LedgerCounters {
    /// All content insertions, including overflow attempts.
    content_attempts: u64,
    /// Content insertions that matched the expected position (overflow is
    /// never correct).
    correct_attempts: u64,
    /// Backspace (and delete-word) events; correction events, not content
    /// insertions.
    deletions: u64,
    /// Content insertions beyond the target length; they have no expected
    /// slot, count as incorrect attempts and block completion until
    /// deleted.
    overflow_attempts: u64,
}

impl LedgerCounters {
    /// Content insertions including overflow.
    #[must_use]
    pub const fn content_attempts(self) -> u64 {
        self.content_attempts
    }

    /// Correct content insertions.
    #[must_use]
    pub const fn correct_attempts(self) -> u64 {
        self.correct_attempts
    }

    /// Deletion events.
    #[must_use]
    pub const fn deletions(self) -> u64 {
        self.deletions
    }

    /// Overflow content attempts.
    #[must_use]
    pub const fn overflow_attempts(self) -> u64 {
        self.overflow_attempts
    }
}

/// Per-word/entry flags for the current attempt (L-02): any error,
/// deletion, interruption or paste marks the word non-clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WordFlags {
    /// Any first-incorrect or incorrect insertion in this word.
    pub any_error: bool,
    /// Any deletion event while this word was being typed.
    pub any_deletion: bool,
    /// Measurement was interrupted (paste, overload, quit) during this
    /// word.
    pub interrupted: bool,
    /// A paste event contributed to this word.
    pub pasted: bool,
}

impl WordFlags {
    /// Whether the word was typed without any error, deletion,
    /// interruption or paste.
    #[must_use]
    pub const fn is_clean(self) -> bool {
        !(self.any_error || self.any_deletion || self.interrupted || self.pasted)
    }
}

/// Result of applying one input event to the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// A content insertion was admitted at the typed tail.
    Content {
        /// Position the insertion was compared against, when one existed.
        position: Option<TextUnitIndex>,
        /// Whether the insertion matched the expected unit.
        matched: bool,
    },
    /// A deletion removed one unit from the typed buffer.
    Deleted,
    /// A deletion arrived with an empty buffer; counted, no buffer
    /// change.
    DeletedNothing,
    /// The extra-input bound was reached: the content insertion is
    /// refused and measurement must be marked interrupted (L-02: never
    /// silently discard keys while claiming an ordinary score).
    RejectedOverBound,
}

/// The bounded extra-input allowance past the target length. A named
/// policy limit (runtime policy `maximum_extra_input_units`); the ledger
/// is constructed with the validated value so the kernel itself only
/// trusts a typed bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraInputAllowance(u32);

impl ExtraInputAllowance {
    /// Constructs an allowance.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidIdentity`] when `units` is zero (a
    /// zero bound would make any mistype immediately unrecoverable, which
    /// is not a legal practice configuration).
    pub const fn new(units: u32) -> Result<Self, CoreError> {
        if units == 0 {
            return Err(CoreError::InvalidIdentity {
                kind: "ExtraInputAllowance",
                reason: "extra-input allowance must allow at least one correction",
            });
        }
        Ok(Self(units))
    }

    /// The allowance in units.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One exercise attempt against an immutable target. Created per attempt;
/// a restart creates a new ledger linked to the abandoned one by the
/// engine, never by mutating this one's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptLedger {
    target: Vec<TrainingChar>,
    first_outcome: Vec<Option<Outcome>>,
    counters: LedgerCounters,
    buffer: Vec<TrainingChar>,
    /// Word index for each target position (spaces/newlines belong to
    /// the word they terminate); replaces index arithmetic at hot sites.
    word_of_position: Vec<u32>,
    word_flags: Vec<WordFlags>,
    allowance: ExtraInputAllowance,
    /// the attempt interrupted; excludes the attempt from ordinary-score
    /// claims.
    measurement_interrupted: bool,
}

impl AttemptLedger {
    /// Creates a ledger for a validated target. Words are maximal runs
    /// of non-boundary units; a space or newline terminates a word.
    ///
    /// # Errors
    /// Returns [`CoreError::CapacityExhausted`] when the target exceeds
    /// the maximum target length ([`TextUnitIndex`] range).
    pub fn new(
        target: Vec<TrainingChar>,
        allowance: ExtraInputAllowance,
    ) -> Result<Self, CoreError> {
        let len = target.len();
        if len > TextUnitIndex::MAXIMUM.get() as usize {
            return Err(CoreError::CapacityExhausted {
                limit: "maximum target units",
            });
        }
        let mut word_flags = Vec::new();
        let mut word_of_position = Vec::with_capacity(len);
        let mut open = false;
        for unit in &target {
            let boundary = matches!(unit.get(), ' ' | '\n');
            if !boundary && !open {
                word_flags.push(WordFlags::default());
                open = true;
            }
            if boundary && !open {
                // A leading or doubled boundary terminates an empty word
                // slot so every position maps to a word.
                word_flags.push(WordFlags::default());
            }
            let index = u32::try_from(word_flags.len().saturating_sub(1)).map_err(|_| {
                CoreError::CapacityExhausted {
                    limit: "maximum target units",
                }
            })?;
            word_of_position.push(index);
            if boundary {
                open = false;
            }
        }
        if word_flags.is_empty() {
            word_flags.push(WordFlags::default());
        }
        Ok(Self {
            first_outcome: vec![None; len],
            word_of_position,
            target,
            counters: LedgerCounters::default(),
            buffer: Vec::with_capacity(len.checked_add(allowance.get() as usize).ok_or(
                CoreError::CapacityExhausted {
                    limit: "typed buffer capacity",
                },
            )?),
            word_flags,
            allowance,
            measurement_interrupted: false,
        })
    }

    /// The immutable target units.
    #[must_use]
    pub fn target(&self) -> &[TrainingChar] {
        &self.target
    }

    /// The bounded current typed buffer.
    #[must_use]
    pub fn buffer(&self) -> &[TrainingChar] {
        &self.buffer
    }

    /// Cumulative event facts.
    #[must_use]
    pub const fn counters(&self) -> LedgerCounters {
        self.counters
    }

    /// First outcomes, indexed by expected position. `None` means the
    /// position has not been attempted yet.
    #[must_use]
    pub fn first_outcomes(&self) -> &[Option<Outcome>] {
        &self.first_outcome
    }

    /// Whether measurement was marked interrupted (over-bound content,
    /// paste, overload, abandonment).
    #[must_use]
    pub const fn measurement_interrupted(&self) -> bool {
        self.measurement_interrupted
    }

    /// Marks the attempt's measurement interrupted (engine decision:
    /// paste, overload, interruption). Word-level flags mark the word in
    /// progress at the typed tail.
    pub fn mark_interrupted(&mut self) {
        self.measurement_interrupted = true;
        if let Some(flags) = self.tail_word_flags_mut() {
            flags.interrupted = true;
        }
    }

    /// Applies one input event (VK-01 kernel).
    ///
    /// # Errors
    /// Returns [`CoreError::IdExhaustion`] if a bounded counter would
    /// overflow (never wraps).
    pub fn apply(&mut self, event: LedgerEvent) -> Result<Applied, CoreError> {
        match event {
            LedgerEvent::Content(character) => {
                let bound = self
                    .target
                    .len()
                    .checked_add(self.allowance.get() as usize)
                    .ok_or(CoreError::CapacityExhausted {
                        limit: "typed buffer capacity",
                    })?;
                if self.buffer.len() >= bound {
                    self.measurement_interrupted = true;
                    if let Some(flags) = self.tail_word_flags_mut() {
                        flags.interrupted = true;
                    }
                    return Ok(Applied::RejectedOverBound);
                }
                let position = self.buffer.len();
                let expected = self.target.get(position).copied();
                let matched = expected == Some(character);
                let index = if expected.is_some() {
                    Some(
                        TextUnitIndex::new(u32::try_from(position).map_err(|_| {
                            CoreError::CapacityExhausted {
                                limit: "maximum target units",
                            }
                        })?)
                        .map_err(|_| CoreError::CapacityExhausted {
                            limit: "maximum target units",
                        })?,
                    )
                } else {
                    None
                };
                let mut counters = self.counters;
                counters.content_attempts = increment(counters.content_attempts)?;
                if matched {
                    counters.correct_attempts = increment(counters.correct_attempts)?;
                }
                if expected.is_none() {
                    counters.overflow_attempts = increment(counters.overflow_attempts)?;
                }
                self.counters = counters;
                self.buffer.push(character);
                if expected.is_none() {
                    if let Some(flags) = self.tail_word_flags_mut() {
                        flags.any_error = true;
                    }
                    return Ok(Applied::Content {
                        position: None,
                        matched: false,
                    });
                }
                // Write-once: assign first outcome only if the slot is empty.
                if let Some(outcome) = self.first_outcome.get_mut(position)
                    && outcome.is_none()
                {
                    *outcome = Some(if matched {
                        Outcome::Correct
                    } else {
                        Outcome::Incorrect
                    });
                }
                if !matched
                    && let Some(index) = self.word_index_at(position)
                    && let Some(flags) = self.word_flags.get_mut(index)
                {
                    flags.any_error = true;
                }
                // All fallible arithmetic precedes mutation.
                Ok(Applied::Content {
                    position: index,
                    matched,
                })
            }
            LedgerEvent::Backspace => {
                self.counters.deletions =
                    self.counters
                        .deletions
                        .checked_add(1)
                        .ok_or(CoreError::IdExhaustion {
                            kind: "deletions",
                            reason: "event counter exhausted; refuse instead of wrapping",
                        })?;
                // Deletion flags the word being edited (the word at the
                // tail before removal).
                if let Some(flags) = self.tail_word_flags_mut() {
                    flags.any_deletion = true;
                }
                if self.buffer.pop().is_some() {
                    Ok(Applied::Deleted)
                } else {
                    Ok(Applied::DeletedNothing)
                }
            }
            LedgerEvent::DeleteWord(units) => {
                // One explicit action with a counted deletion range; it
                // does not erase first outcomes (U-03).
                let removed = usize::try_from(units)
                    .unwrap_or(self.buffer.len())
                    .min(self.buffer.len());
                self.counters.deletions = self
                    .counters
                    .deletions
                    .checked_add(u64::try_from(removed).map_err(|_| {
                        CoreError::CapacityExhausted {
                            limit: "deletion count",
                        }
                    })?)
                    .ok_or(CoreError::IdExhaustion {
                        kind: "deletions",
                        reason: "counter exhausted",
                    })?;
                if let Some(flags) = self.tail_word_flags_mut() {
                    flags.any_deletion = true;
                }
                let new_len = self.buffer.len().saturating_sub(removed);
                self.buffer.truncate(new_len);
                Ok(Applied::Deleted)
            }
        }
    }

    /// Positionally matching units in the current buffer (L-02: correct
    /// units at an unfinished slice).
    #[must_use]
    pub fn correct_visible_units(&self) -> UnitCount {
        let count = self
            .buffer
            .iter()
            .zip(self.target.iter())
            .filter(|(typed, expected)| typed == expected)
            .count();
        UnitCount::from_raw(count as u64)
    }

    /// First-attempted expected positions (slots with an assigned
    /// outcome).
    #[must_use]
    pub fn first_attempted(&self) -> UnitCount {
        let count = self.first_outcome.iter().flatten().count();
        UnitCount::from_raw(count as u64)
    }

    /// First-correct positions.
    #[must_use]
    pub fn first_correct(&self) -> UnitCount {
        let count = self
            .first_outcome
            .iter()
            .flatten()
            .filter(|outcome| matches!(outcome, Outcome::Correct))
            .count();
        UnitCount::from_raw(count as u64)
    }

    /// Whether the buffer exactly equals the target.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.buffer == self.target
    }

    /// Whether the whole attempt was typed cleanly: complete, no error,
    /// no deletion, no overflow, no interruption and no paste.
    #[must_use]
    pub fn is_clean_completion(&self) -> bool {
        self.is_complete()
            && self.word_flags.iter().all(|flags| flags.is_clean())
            && self.counters.overflow_attempts == 0
            && !self.measurement_interrupted
    }

    /// Per-word flags of the current attempt.
    #[must_use]
    pub fn word_flags(&self) -> &[WordFlags] {
        &self.word_flags
    }

    /// The extra-input allowance this ledger was constructed with.
    #[must_use]
    pub const fn allowance(&self) -> ExtraInputAllowance {
        self.allowance
    }

    /// Index of the word a target position belongs to. Spaces and
    /// newlines belong to the word they terminate (completion
    /// boundary); overflow positions beyond the target belong to the
    /// final word.
    fn word_index_at(&self, position: usize) -> Option<usize> {
        match self.word_of_position.get(position) {
            Some(index) => usize::try_from(*index).ok(),
            None => self.word_flags.len().checked_sub(1),
        }
    }

    /// Word index for the buffer tail (the word being typed now).
    fn word_index_at_buffer_tail(&self) -> Option<usize> {
        let last = self.buffer.len().checked_sub(1)?;
        self.word_index_at(last)
    }

    /// Mutable flags of the word at the typed tail.
    fn tail_word_flags_mut(&mut self) -> Option<&mut WordFlags> {
        let index = self.word_index_at_buffer_tail()?;
        self.word_flags.get_mut(index)
    }
}

/// Input events the ledger understands (U-03: Backspace deletes one
/// logical unit; delete-word is one explicit counted action).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerEvent {
    /// A content insertion at the typed tail.
    Content(TrainingChar),
    /// Delete one logical training unit.
    Backspace,
    /// Delete a counted range of units (explicit action).
    DeleteWord(u64),
}

fn increment(value: u64) -> Result<u64, CoreError> {
    value.checked_add(1).ok_or(CoreError::IdExhaustion {
        kind: "ledger counter",
        reason: "counter exhausted",
    })
}
#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::alphabet::LayoutSupportsPound;
    use crate::alphabet::validate_target;

    fn ledger(target: &str) -> AttemptLedger {
        AttemptLedger::new(
            validate_target(target, LayoutSupportsPound::US).unwrap(),
            ExtraInputAllowance::new(8).unwrap(),
        )
        .unwrap()
    }

    fn type_str(ledger: &mut AttemptLedger, text: &str) {
        for character in text.chars() {
            ledger
                .apply(LedgerEvent::Content(
                    TrainingChar::new(character, LayoutSupportsPound::US).unwrap(),
                ))
                .unwrap();
        }
    }

    #[test]
    fn handover_example_cat_corrected() {
        // target "cat", input c x Backspace a t
        let mut ledger = ledger("cat");
        type_str(&mut ledger, "cx");
        ledger.apply(LedgerEvent::Backspace).unwrap();
        type_str(&mut ledger, "at");
        assert_eq!(ledger.first_correct().get(), 2);
        assert_eq!(ledger.first_attempted().get(), 3);
        assert_eq!(ledger.counters().correct_attempts(), 3);
        assert_eq!(ledger.counters().content_attempts(), 4);
        assert_eq!(ledger.counters().deletions(), 1);
        assert_eq!(ledger.correct_visible_units().get(), 3);
        assert!(ledger.is_complete());
        assert!(!ledger.is_clean_completion());
    }

    #[test]
    fn first_correct_later_edited_stays_correct() {
        let mut ledger = ledger("cat");
        type_str(&mut ledger, "ca");
        ledger.apply(LedgerEvent::Backspace).unwrap();
        type_str(&mut ledger, "x");
        ledger.apply(LedgerEvent::Backspace).unwrap();
        type_str(&mut ledger, "at");
        assert_eq!(ledger.first_correct().get(), 3);
        assert_eq!(ledger.first_attempted().get(), 3);
        assert_eq!(ledger.counters().correct_attempts(), 4);
        assert_eq!(ledger.counters().content_attempts(), 5);
        assert_eq!(ledger.counters().deletions(), 2);
        assert!(!ledger.is_clean_completion());
    }

    #[test]
    fn overflow_counts_incorrect_and_blocks_completion() {
        let mut ledger = ledger("cat");
        type_str(&mut ledger, "caxz");
        assert_eq!(ledger.counters().overflow_attempts(), 1);
        assert!(!ledger.is_complete());
        ledger.apply(LedgerEvent::Backspace).unwrap();
        ledger.apply(LedgerEvent::Backspace).unwrap();
        type_str(&mut ledger, "t");
        assert!(ledger.is_complete());
        assert_eq!(ledger.first_correct().get(), 2);
        assert_eq!(ledger.counters().content_attempts(), 5);
    }

    #[test]
    fn over_bound_content_is_rejected_and_interrupts_measurement() {
        let mut ledger = AttemptLedger::new(
            validate_target("cat", LayoutSupportsPound::US).unwrap(),
            ExtraInputAllowance::new(1).unwrap(),
        )
        .unwrap();
        type_str(&mut ledger, "catx");
        let applied = ledger
            .apply(LedgerEvent::Content(
                TrainingChar::new('z', LayoutSupportsPound::US).unwrap(),
            ))
            .unwrap();
        assert_eq!(applied, Applied::RejectedOverBound);
        assert!(ledger.measurement_interrupted());
        assert_eq!(ledger.buffer().len(), 4);
    }

    #[test]
    fn clean_completion_requires_no_deletion() {
        let mut clean = ledger("cat");
        type_str(&mut clean, "cat");
        assert!(clean.is_clean_completion());
        let mut corrected = ledger("cat");
        type_str(&mut corrected, "caz");
        corrected.apply(LedgerEvent::Backspace).unwrap();
        type_str(&mut corrected, "t");
        assert!(corrected.is_complete());
        assert!(!corrected.is_clean_completion());
    }

    #[test]
    fn incomplete_attempt_keeps_partial_facts() {
        let mut ledger = ledger("cat");
        type_str(&mut ledger, "cx");
        assert_eq!(ledger.first_correct().get(), 1);
        assert_eq!(ledger.first_attempted().get(), 2);
        assert!(!ledger.is_complete());
        assert!(!ledger.is_clean_completion());
    }

    #[test]
    fn numeric_string_identity_preserved() {
        let mut ledger = ledger("005.20");
        type_str(&mut ledger, "005.20");
        assert!(ledger.is_complete());
        assert!(ledger.is_clean_completion());
        // Digits for the numpad numerator: 5 of 6 units are digits.
        let digits = ledger
            .target()
            .iter()
            .filter(|unit| unit.is_digit())
            .count();
        assert_eq!(digits, 5);
    }
}
