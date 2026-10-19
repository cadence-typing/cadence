//! Session headline facts, aggregated by counts and durations rather than averages.

use cadence_core::metrics::{Ratio, correct_wpm, digits_per_minute};
use cadence_core::progress::{Track, TrackProgress};
use cadence_core::{BasisPoints, CoreError, Micros, UnitCount};

use crate::view::RateUnit;

/// Descriptive session totals, not a benchmark or a cross-skill performance score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryMetrics {
    /// Correct throughput; absent for invalid or unmeasurable timing.
    pub speed: Option<Ratio>,
    /// Typing tracks use WPM; a keypad-only session uses correct digits/minute.
    pub speed_unit: RateUnit,
    /// All first-attempt positions, including corrected mistakes, truncated for display.
    pub first_pass_accuracy: Option<BasisPoints>,
}

impl SummaryMetrics {
    /// Derives headlines from the same admitted reports used by the per-track table.
    /// Numpad duration/digits never enter typing WPM; all typing-track hesitation does.
    ///
    /// # Errors
    /// Refuses counter/duration exhaustion or an invalid accuracy ratio. No saturation
    /// or silently omitted track can improve the displayed session result.
    pub fn from_reports(reports: &[TrackProgress], rate_valid: bool) -> Result<Self, CoreError> {
        let mut typing_units = UnitCount::ZERO;
        let mut typing_time = Micros::ZERO;
        let mut keypad_digits = UnitCount::ZERO;
        let mut keypad_time = Micros::ZERO;
        let mut first_correct = UnitCount::ZERO;
        let mut first_attempted = UnitCount::ZERO;
        for report in reports {
            first_correct = first_correct.checked_add(UnitCount::from_raw(report.first_correct))?;
            first_attempted =
                first_attempted.checked_add(UnitCount::from_raw(report.first_attempted))?;
            if report.track == Track::Numpad {
                keypad_digits =
                    keypad_digits.checked_add(UnitCount::from_raw(report.correct_digits))?;
                keypad_time = keypad_time.checked_add(Micros::from_us(report.active_micros))?;
            } else {
                typing_units =
                    typing_units.checked_add(UnitCount::from_raw(report.correct_units))?;
                typing_time = typing_time.checked_add(Micros::from_us(report.active_micros))?;
            }
        }
        let speed_unit =
            if !reports.is_empty() && reports.iter().all(|report| report.track == Track::Numpad) {
                RateUnit::DigitsPerMinute
            } else {
                RateUnit::WordsPerMinute
            };
        let speed = if rate_valid {
            match speed_unit {
                RateUnit::WordsPerMinute => correct_wpm(typing_units.get(), typing_time),
                RateUnit::DigitsPerMinute => digits_per_minute(keypad_digits.get(), keypad_time),
            }
        } else {
            None
        };
        let first_pass_accuracy = if first_attempted == UnitCount::ZERO {
            None
        } else {
            Some(BasisPoints::from_ratio_truncating(
                first_correct.get(),
                first_attempted.get(),
            )?)
        };
        Ok(Self {
            speed,
            speed_unit,
            first_pass_accuracy,
        })
    }
}
