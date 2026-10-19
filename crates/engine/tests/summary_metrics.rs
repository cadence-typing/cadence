//! Session headlines preserve weighting, keypad units, correction history and unknowns.

use cadence_core::progress::{Track, TrackProgress};
use cadence_core::{BasisPoints, CoreError};
use cadence_engine::summary::SummaryMetrics;
use cadence_engine::view::RateUnit;

fn report(track: Track, units: u64, first: (u64, u64), active_micros: u64) -> TrackProgress {
    let mut report = TrackProgress::empty(track);
    report.correct_units = units;
    report.correct_digits = if track == Track::Numpad { units } else { 0 };
    report.first_correct = first.0;
    report.first_attempted = first.1;
    report.correct_attempts = units;
    report.deletions = first.1.saturating_sub(first.0);
    report.content_attempts = units.saturating_add(report.deletions);
    report.active_micros = active_micros;
    report
}

#[test]
fn mixed_session_weights_counts_and_time_without_blending_keypad_speed() -> Result<(), CoreError> {
    let reports = [
        report(Track::WarmUp, 50, (50, 50), 10_000_000),
        report(Track::Words, 150, (100, 150), 90_000_000),
        report(Track::Numpad, 300, (300, 300), 60_000_000),
    ];
    let metrics = SummaryMetrics::from_reports(&reports, true)?;
    assert_eq!(metrics.speed_unit, RateUnit::WordsPerMinute);
    // 200 correct units / 100 typing seconds = 24 WPM, not mean(60,20)=40.
    assert_eq!(metrics.speed.map(|rate| rate.as_f64()), Some(24.0));
    // 450/500 across all first-attempt positions, not the mean of track percentages.
    assert_eq!(
        metrics.first_pass_accuracy.map(BasisPoints::get),
        Some(9000)
    );
    Ok(())
}

#[test]
fn keypad_headline_counts_digits_not_decimal_points_and_keeps_corrected_error()
-> Result<(), CoreError> {
    let mut keypad = report(Track::Numpad, 6, (5, 6), 60_000_000);
    keypad.correct_digits = 5;
    let metrics = SummaryMetrics::from_reports(&[keypad], true)?;
    assert_eq!(metrics.speed_unit, RateUnit::DigitsPerMinute);
    assert_eq!(metrics.speed.map(|rate| rate.as_f64()), Some(5.0));
    assert_eq!(
        metrics.first_pass_accuracy.map(BasisPoints::get),
        Some(8333)
    );
    Ok(())
}

#[test]
fn zero_correct_typing_still_contributes_its_hesitation_time() -> Result<(), CoreError> {
    let reports = [
        report(Track::Words, 50, (50, 50), 10_000_000),
        report(Track::Prose, 0, (0, 1), 50_000_000),
    ];
    let metrics = SummaryMetrics::from_reports(&reports, true)?;
    assert_eq!(metrics.speed.map(|rate| rate.as_f64()), Some(10.0));
    Ok(())
}

#[test]
fn invalid_and_empty_measurements_do_not_invent_rates_or_perfect_accuracy() -> Result<(), CoreError>
{
    let metrics =
        SummaryMetrics::from_reports(&[report(Track::Words, 4, (3, 4), 1_000_000)], false)?;
    assert_eq!(metrics.speed, None);
    assert_eq!(
        metrics.first_pass_accuracy.map(BasisPoints::get),
        Some(7500)
    );
    let empty = SummaryMetrics::from_reports(&[], true)?;
    assert_eq!(empty.speed, None);
    assert_eq!(empty.first_pass_accuracy, None);
    let rare_error = SummaryMetrics::from_reports(
        &[report(Track::Words, 10_000, (9999, 10_000), 60_000_000)],
        true,
    )?;
    assert_eq!(
        rare_error.first_pass_accuracy.map(BasisPoints::get),
        Some(9999)
    );
    Ok(())
}

#[test]
fn exhausted_totals_return_an_error_instead_of_wrapping() {
    let reports = [
        report(Track::Words, u64::MAX, (0, 0), 1),
        report(Track::Prose, 1, (0, 0), 1),
    ];
    assert!(matches!(
        SummaryMetrics::from_reports(&reports, true),
        Err(CoreError::IdExhaustion { .. })
    ));
}
