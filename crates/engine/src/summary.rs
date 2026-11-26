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
        // The closing review retypes words missed minutes earlier: easier
        // than first-pass typing, so it never reaches the headline (#60).
        let reports: Vec<&TrackProgress> = reports
            .iter()
            .filter(|report| report.track.in_headline())
            .collect();
        Self::sum(&reports, rate_valid)
    }

    /// One track's series across comparable sessions (#81): the same exact
    /// sums of numerators and denominators, never an average of
    /// percentages. Unlike [`Self::from_reports`] it takes any one track,
    /// the transfer probe's included.
    ///
    /// # Errors
    /// As [`Self::from_reports`].
    pub fn for_series(reports: &[TrackProgress]) -> Result<Self, CoreError> {
        let reports: Vec<&TrackProgress> = reports.iter().collect();
        Self::sum(&reports, true)
    }

    fn sum(reports: &[&TrackProgress], rate_valid: bool) -> Result<Self, CoreError> {
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

const BASIS_POINTS_PER_PERCENT: u16 = 100;
const BASIS_POINTS_PER_TENTH: u16 = 10;

/// One-decimal percentage, truncated: a remaining first-pass error never
/// displays as a perfect 100.0%.
#[must_use]
pub fn percent_label(value: BasisPoints) -> String {
    let value = value.get();
    let whole = value.checked_div(BASIS_POINTS_PER_PERCENT).unwrap_or(0);
    let tenth = value
        .checked_rem(BASIS_POINTS_PER_PERCENT)
        .and_then(|fraction| fraction.checked_div(BASIS_POINTS_PER_TENTH))
        .unwrap_or(0);
    format!("{whole}.{tenth}%")
}

/// Signed change in percentage points, truncated toward zero to one decimal.
#[must_use]
pub fn points_change(now: BasisPoints, before: BasisPoints) -> String {
    let (sign, magnitude) = if now.get() >= before.get() {
        ('+', now.get().saturating_sub(before.get()))
    } else {
        ('\u{2212}', before.get().saturating_sub(now.get()))
    };
    let whole = magnitude.checked_div(BASIS_POINTS_PER_PERCENT).unwrap_or(0);
    let tenth = magnitude
        .checked_rem(BASIS_POINTS_PER_PERCENT)
        .and_then(|fraction| fraction.checked_div(BASIS_POINTS_PER_TENTH))
        .unwrap_or(0);
    if whole == 0 && tenth == 0 {
        // A change below the displayed precision carries no sign.
        return "0.0 pts".to_string();
    }
    format!("{sign}{whole}.{tenth} pts")
}

/// Signed change of a rate, one decimal, for display only.
#[must_use]
pub fn rate_change(now: Ratio, before: Ratio) -> String {
    let change = now.as_f64() - before.as_f64();
    if format!("{:.1}", change.abs()) == "0.0" {
        return "0.0".to_string();
    }
    if change < 0.0 {
        format!("\u{2212}{:.1}", -change)
    } else {
        format!("+{change:.1}")
    }
}

/// First-pass accuracy of one track, truncated.
///
/// # Errors
/// Refuses an invalid ratio.
pub fn track_accuracy(report: &TrackProgress) -> Result<Option<BasisPoints>, CoreError> {
    if report.first_attempted == 0 {
        return Ok(None);
    }
    BasisPoints::from_ratio_truncating(report.first_correct, report.first_attempted).map(Some)
}

/// Correct pace of one track in its own unit (digits/min for the keypad).
#[must_use]
pub fn track_rate(report: &TrackProgress) -> Option<Ratio> {
    let active = Micros::from_us(report.active_micros);
    if report.track == Track::Numpad {
        digits_per_minute(report.correct_digits, active)
    } else {
        correct_wpm(report.correct_units, active)
    }
}
