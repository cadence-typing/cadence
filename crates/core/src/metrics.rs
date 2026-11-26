//! Exact metric arithmetic (L-02, VK-02).
//!
//! Ratios are stored as exact `(numerator, denominator)` pairs and only
//! formatted at the UI edge. Empty attempts and zero-duration
//! observations yield `None`, never infinity, 0% accuracy, or a made-up
//! baseline. Numerators and denominators are aggregated, never averaged
//! per exercise.
//!
//! The arithmetic is decided by `cadence-kernel` (`ratio`, `time`), which
//! carries Lean theorems for exactness, overflow-freedom and monotonicity
//! (`docs/TAUTEN.md`); this module adapts its results to the core types.

use crate::ledger::AttemptLedger;
use crate::tick::Micros;
use crate::units::UnitCount;
use cadence_kernel::ratio::{
    Fraction, Pace, digits_per_minute as kernel_digits_per_minute, fraction, wpm,
};

pub use cadence_kernel::ratio::{MICROSECONDS_PER_MINUTE, WPM_UNITS_PER_WORD};

/// An exact non-negative rational `numerator / denominator`. Constructed
/// only through checked paths; never reduced silently (fixtures pin the
/// unreduced form).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    numerator: u128,
    denominator: u128,
}

impl Ratio {
    /// Constructs an exact ratio; the denominator must be nonzero.
    ///
    /// # Errors
    /// Returns [`crate::CoreError::InvalidDenominator`] when
    /// `denominator == 0`.
    pub const fn new(numerator: u128, denominator: u128) -> Result<Self, crate::CoreError> {
        if denominator == 0 {
            return Err(crate::CoreError::InvalidDenominator);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// The numerator.
    #[must_use]
    pub const fn numerator(self) -> u128 {
        self.numerator
    }

    /// The denominator.
    #[must_use]
    pub const fn denominator(self) -> u128 {
        self.denominator
    }

    /// A kernel fraction, whose denominator the kernel proves nonzero.
    const fn from_fraction(fraction: Fraction) -> Self {
        Self {
            numerator: fraction.num,
            denominator: fraction.den,
        }
    }

    /// The ratio as an f64, for UI formatting only (the exact pair is the
    /// source of truth).
    #[must_use]
    #[expect(
        clippy::as_conversions,
        reason = "display only: no lossless integer to f64 conversion exists; the exact pair is the source of truth"
    )]
    pub fn as_f64(self) -> f64 {
        let numerator = self.numerator as f64;
        let denominator = self.denominator as f64;
        numerator / denominator
    }
}

/// First-pass accuracy: first-correct positions / first-attempted
/// expected positions. `None` when nothing was attempted (L-02).
#[must_use]
pub fn first_pass_accuracy(ledger: &AttemptLedger) -> Option<Ratio> {
    ratio_or_none(ledger.first_correct().get(), ledger.first_attempted().get())
}

/// Attempt accuracy: correct content insertions / all content insertions,
/// including overflow. `None` when no content was inserted.
#[must_use]
pub fn attempt_accuracy(ledger: &AttemptLedger) -> Option<Ratio> {
    ratio_or_none(
        ledger.counters().correct_attempts(),
        ledger.counters().content_attempts(),
    )
}

/// Correct WPM: `correct_units × 60 / (5 × active_seconds)` using exact
/// microsecond arithmetic. Correct units at an unfinished slice are
/// positionally matching units (L-02). `None` when active time or units
/// are zero.
#[must_use]
pub fn correct_wpm(correct_units: u64, active: Micros) -> Option<Ratio> {
    pace_ratio(wpm(correct_units, active.as_us()))
}

/// Numpad correct digits per minute: matching digit units only; decimal
/// points and operators contribute to accuracy, not the digit numerator
/// (L-02).
#[must_use]
pub fn digits_per_minute(correct_digits: u64, active: Micros) -> Option<Ratio> {
    pace_ratio(kernel_digits_per_minute(correct_digits, active.as_us()))
}

/// Positionally matching digit units in the ledger buffer.
#[must_use]
pub fn correct_digits(ledger: &AttemptLedger) -> UnitCount {
    let count = ledger
        .buffer()
        .iter()
        .zip(ledger.target())
        .filter(|(typed, expected)| typed == expected && typed.is_digit())
        .count();
    UnitCount::from_len(count)
}

/// All digit units in the target (numpad denominators count digits).
#[must_use]
pub fn target_digits(ledger: &AttemptLedger) -> UnitCount {
    let count = ledger
        .target()
        .iter()
        .filter(|unit| unit.is_digit())
        .count();
    UnitCount::from_len(count)
}

/// Whether every target digit was typed correctly at its position and the
/// buffer completes the target (exact submitted string, L-02).
#[must_use]
pub fn submitted_exact(ledger: &AttemptLedger) -> bool {
    ledger.is_complete()
}

/// Aggregate exact metrics for one finished or in-flight exercise
/// attempt. Numerators and denominators are carried separately so later
/// aggregation adds like terms instead of averaging percentages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExerciseMetrics {
    /// First-correct positions.
    pub first_correct: u64,
    /// First-attempted expected positions.
    pub first_attempted: u64,
    /// Correct content insertions.
    pub correct_attempts: u64,
    /// All content insertions, including overflow.
    pub content_attempts: u64,
    /// Deletion events.
    pub deletions: u64,
    /// Overflow content attempts.
    pub overflow_attempts: u64,
    /// Positionally matching units at this slice.
    pub correct_visible_units: u64,
    /// Whether the attempt completed exactly and cleanly.
    pub clean_completion: bool,
    /// Whether the attempt completed (buffer equals target).
    pub completed: bool,
    /// Whether measurement was uninterrupted.
    pub measurement_uninterrupted: bool,
    /// Correct WPM, exact; `None` when active time or units are zero.
    pub correct_wpm: Option<Ratio>,
    /// First-pass accuracy, exact; `None` when nothing was attempted.
    pub first_pass_accuracy: Option<Ratio>,
    /// Attempt accuracy, exact; `None` when no content was inserted.
    pub attempt_accuracy: Option<Ratio>,
}

/// Computes the full metric set for a ledger under a measured active
/// duration (timing segments are owned by the engine/session layer).
#[must_use]
pub fn exercise_metrics(ledger: &AttemptLedger, active: Micros) -> ExerciseMetrics {
    ExerciseMetrics {
        first_correct: ledger.first_correct().get(),
        first_attempted: ledger.first_attempted().get(),
        correct_attempts: ledger.counters().correct_attempts(),
        content_attempts: ledger.counters().content_attempts(),
        deletions: ledger.counters().deletions(),
        overflow_attempts: ledger.counters().overflow_attempts(),
        correct_visible_units: ledger.correct_visible_units().get(),
        clean_completion: ledger.is_clean_completion(),
        completed: ledger.is_complete(),
        measurement_uninterrupted: !ledger.measurement_interrupted(),
        correct_wpm: correct_wpm(ledger.correct_visible_units().get(), active),
        first_pass_accuracy: first_pass_accuracy(ledger),
        attempt_accuracy: attempt_accuracy(ledger),
    }
}

/// Numpad-specific metrics (L-02): digits/min, exact submission and
/// clean-entry (the primary numeric precision measure).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericMetrics {
    /// Positionally matching digit units.
    pub correct_digits: u64,
    /// All digit units in the target.
    pub target_digits: u64,
    /// Digits per minute, exact; `None` without positive time and digits.
    pub digits_per_minute: Option<Ratio>,
    /// Whether the submitted string equals the target exactly.
    pub submitted_exact: bool,
    /// Whether the entry was exact with no error/deletion in the first
    /// attempt.
    pub clean_entry: bool,
}

/// Computes numpad metrics for a ledger under its active duration.
#[must_use]
pub fn numeric_metrics(ledger: &AttemptLedger, active: Micros) -> NumericMetrics {
    NumericMetrics {
        correct_digits: correct_digits(ledger).get(),
        target_digits: target_digits(ledger).get(),
        digits_per_minute: digits_per_minute(correct_digits(ledger).get(), active),
        submitted_exact: submitted_exact(ledger),
        clean_entry: ledger.is_clean_completion(),
    }
}

/// Ratio of two counts, or `None` when the denominator is zero.
fn ratio_or_none(numerator: u64, denominator: u64) -> Option<Ratio> {
    fraction(numerator, denominator).map(Ratio::from_fraction)
}

/// A pace as a ratio. The kernel proves `Overflow` unreachable for `u64`
/// inputs (`wpm_spec`), so it reads as "no pace" rather than a made-up one.
fn pace_ratio(pace: Pace) -> Option<Ratio> {
    match pace {
        Pace::Rate(rate) => Some(Ratio::from_fraction(rate)),
        Pace::Empty | Pace::Overflow => None,
    }
}

/// Re-exports for metric consumers.
pub use crate::alphabet::TrainingChar as MetricChar;

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

    use super::*;

    #[test]
    fn empty_attempts_yield_none_everywhere() {
        assert_eq!(ratio_or_none(0, 0), None);
        assert_eq!(correct_wpm(0, Micros::from_us(1_000)), None);
        assert_eq!(correct_wpm(3, Micros::ZERO), None);
        assert_eq!(digits_per_minute(0, Micros::from_us(1)), None);
    }

    #[test]
    fn wpm_matches_fixture_arithmetic() {
        // 3 correct units in 3 seconds:
        // 3 × 60e6 / (5 × 3e6) = 180e6/15e6 (unreduced fixture form).
        let ratio = correct_wpm(3, Micros::from_seconds(3).unwrap()).unwrap();
        assert_eq!(ratio.numerator(), 180_000_000);
        assert_eq!(ratio.denominator(), 15_000_000);
    }

    #[test]
    fn digits_per_minute_matches_fixture() {
        let ratio = digits_per_minute(5, Micros::from_seconds(60).unwrap()).unwrap();
        assert_eq!(ratio.numerator(), 300_000_000);
        assert_eq!(ratio.denominator(), 60_000_000);
    }
}
