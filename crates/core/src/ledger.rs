//! First-attempt observation ledger (L-02, VK-01).
//!
//! The ledger is the training-correctness core: it owns the immutable
//! target, the write-once first-outcome vector, cumulative event counter
//! facts, the bounded current typed buffer and per-word flags. A later
//! correction never changes a historical position outcome; deleting a
//! correct character and retyping incorrectly leaves the first outcome
//! correct but marks the word non-clean.
//!
//! The scoring decisions are made by `cadence-kernel` (write-once first
//! outcomes, checked counters), which this module calls for every content
//! insertion and deletion; the Lean theorems over that kernel are listed in
//! `docs/TAUTEN.md`. The buffer, word flags and event plumbing around it
//! are ordinary Rust with property tests.

use crate::CoreError;
use crate::alphabet::TrainingChar;
use crate::units::{TextUnitIndex, UnitCount};
use cadence_kernel::counters::{Counters, Step, deletion_step};
use cadence_kernel::first::count_first;
use cadence_kernel::ledger::{
    Insert, clean, complete, flags_after_deletion, flags_after_insert, flags_after_interrupt,
    insert, settle_at,
};

pub use cadence_kernel::first::Slot;

#[cfg(test)]
#[path = "ledger_atomic_tests.rs"]
mod atomic_tests;

/// Cumulative event facts for one exercise attempt. Monotone; never
/// decrease within an attempt. Updated only by the kernel's checked steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LedgerCounters {
    inner: Counters,
}

impl LedgerCounters {
    /// Content insertions including overflow.
    #[must_use]
    pub const fn content_attempts(self) -> u64 {
        self.inner.content
    }

    /// Correct content insertions (overflow is never correct).
    #[must_use]
    pub const fn correct_attempts(self) -> u64 {
        self.inner.correct
    }

    /// Deleted units (backspace and delete-word); correction events, not
    /// content insertions.
    #[must_use]
    pub const fn deletions(self) -> u64 {
        self.inner.deletions
    }

    /// Content insertions beyond the target length; they have no expected
    /// slot, count as incorrect attempts and block completion until
    /// deleted.
    #[must_use]
    pub const fn overflow_attempts(self) -> u64 {
        self.inner.overflow
    }
}

/// Per-word/entry flags for the current attempt (L-02): the kernel's
/// sticky flags; any error, deletion, interruption or paste marks the word
/// non-clean.
pub use cadence_kernel::ledger::WordFlags;

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
    first_outcome: Vec<Slot>,
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
        if len > widen(TextUnitIndex::MAXIMUM.get())? {
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
            let index =
                u32::try_from(word_flags.len().saturating_sub(1)).map_err(|_unrepresentable| {
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
            first_outcome: vec![Slot::Open; len],
            word_of_position,
            target,
            counters: LedgerCounters::default(),
            buffer: Vec::with_capacity(len.checked_add(widen(allowance.get())?).ok_or(
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

    /// First outcomes, indexed by expected position. [`Slot::Open`] means
    /// the position has not been attempted yet.
    #[must_use]
    pub fn first_outcomes(&self) -> &[Slot] {
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
            *flags = flags_after_interrupt(*flags);
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
                let position = self.buffer.len();
                let expected = self.target.get(position).copied();
                // The kernel decides admission under the bound, the position,
                // the match and the counters before anything mutates.
                let (counters, matched, in_target) = match insert(
                    self.counters.inner,
                    position,
                    self.target.len(),
                    widen(self.allowance.get())?,
                    expected == Some(character),
                ) {
                    Insert::Rejected => {
                        self.mark_interrupted();
                        return Ok(Applied::RejectedOverBound);
                    }
                    Insert::Overflow => {
                        return Err(CoreError::IdExhaustion {
                            kind: "ledger counter",
                            reason: "counter exhausted",
                        });
                    }
                    Insert::Counted {
                        counters,
                        matched,
                        in_target,
                    } => (counters, matched, in_target),
                };
                let index = if in_target {
                    Some(
                        TextUnitIndex::new(u32::try_from(position).map_err(
                            |_unrepresentable| CoreError::CapacityExhausted {
                                limit: "maximum target units",
                            },
                        )?)
                        .map_err(|_unrepresentable| {
                            CoreError::CapacityExhausted {
                                limit: "maximum target units",
                            }
                        })?,
                    )
                } else {
                    None
                };
                self.counters.inner = counters;
                self.buffer.push(character);
                // Write-once: the kernel settles only the slot at `position`.
                settle_at(&mut self.first_outcome, position, matched);
                if let Some(word) = self.word_index_at(position)
                    && let Some(flags) = self.word_flags.get_mut(word)
                {
                    *flags = flags_after_insert(*flags, matched);
                }
                Ok(Applied::Content {
                    position: index,
                    matched,
                })
            }
            LedgerEvent::Backspace => {
                self.counters.inner = match deletion_step(self.counters.inner, 1) {
                    Step::Counted(counters) => counters,
                    Step::Overflow => {
                        return Err(CoreError::IdExhaustion {
                            kind: "deletions",
                            reason: "event counter exhausted; refuse instead of wrapping",
                        });
                    }
                };
                // Deletion flags the word being edited (the word at the
                // tail before removal).
                if let Some(flags) = self.tail_word_flags_mut() {
                    *flags = flags_after_deletion(*flags);
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
                let removed_count = u64::try_from(removed).map_err(|_unrepresentable| {
                    CoreError::CapacityExhausted {
                        limit: "deletion count",
                    }
                })?;
                self.counters.inner = match deletion_step(self.counters.inner, removed_count) {
                    Step::Counted(counters) => counters,
                    Step::Overflow => {
                        return Err(CoreError::IdExhaustion {
                            kind: "deletions",
                            reason: "counter exhausted",
                        });
                    }
                };
                if let Some(flags) = self.tail_word_flags_mut() {
                    *flags = flags_after_deletion(*flags);
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
        UnitCount::from_len(self.matching_positions())
    }

    /// Buffer positions whose unit equals the target unit there.
    fn matching_positions(&self) -> usize {
        self.buffer
            .iter()
            .zip(self.target.iter())
            .filter(|(typed, expected)| typed == expected)
            .count()
    }

    /// First-attempted expected positions (slots with an assigned
    /// outcome).
    #[must_use]
    pub fn first_attempted(&self) -> UnitCount {
        UnitCount::from_len(count_first(&self.first_outcome).attempted)
    }

    /// First-correct expected positions (the first insertion matched).
    #[must_use]
    pub fn first_correct(&self) -> UnitCount {
        UnitCount::from_len(count_first(&self.first_outcome).correct)
    }

    /// Whether the buffer exactly equals the target.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        complete(
            self.buffer.len(),
            self.target.len(),
            self.matching_positions(),
        )
    }

    /// Whether the whole attempt was typed cleanly: complete, no error,
    /// no deletion, no overflow, no interruption and no paste.
    #[must_use]
    pub fn is_clean_completion(&self) -> bool {
        self.is_complete()
            && self.word_flags.iter().all(|flags| clean(*flags))
            && self.counters.inner.overflow == 0
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

/// A `u32` bound widened to `usize` for buffer arithmetic.
fn widen(value: u32) -> Result<usize, CoreError> {
    usize::try_from(value).map_err(|_too_wide| CoreError::CapacityExhausted {
        limit: "typed buffer capacity",
    })
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

    use super::*;
    use crate::alphabet::Layout;
    use crate::alphabet::validate_target;

    fn ledger(target: &str) -> AttemptLedger {
        AttemptLedger::new(
            validate_target(target, Layout::Us).unwrap(),
            ExtraInputAllowance::new(8).unwrap(),
        )
        .unwrap()
    }

    fn type_str(ledger: &mut AttemptLedger, text: &str) {
        for character in text.chars() {
            ledger
                .apply(LedgerEvent::Content(
                    TrainingChar::new(character, Layout::Us).unwrap(),
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
            validate_target("cat", Layout::Us).unwrap(),
            ExtraInputAllowance::new(1).unwrap(),
        )
        .unwrap();
        type_str(&mut ledger, "catx");
        let applied = ledger
            .apply(LedgerEvent::Content(
                TrainingChar::new('z', Layout::Us).unwrap(),
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
