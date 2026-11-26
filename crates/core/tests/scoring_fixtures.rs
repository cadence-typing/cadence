//! Hand-calculated scoring fixtures (fixtures/scoring.json) executed
//! against the real ledger and metric kernels.
//!
//! These are the P1 exit criterion: the `cat` correction fixture,
//! partial-exercise, overflow and pause cases give exact documented
//! ratios. The fixture file is pinned by `MANIFEST.sha256`.
// Test assertions may use expect/unwrap/panic/indexing to fail loudly;
// the workspace denies them in production code only.
#![cfg(test)]

use cadence_core::alphabet::{Layout, TrainingChar, validate_target};
use cadence_core::ledger::{AttemptLedger, ExtraInputAllowance, LedgerEvent};
use cadence_core::metrics::{exercise_metrics, numeric_metrics};
use cadence_core::session::TimingSegments;
use cadence_core::tick::{Micros, MonoTick};
use serde_json::Value;

fn run_case(target: &str, events: &[Value], active: Micros) -> AttemptLedger {
    let units = validate_target(target, Layout::Us)
        .unwrap_or_else(|error| panic!("target {target:?} rejected: {error}"));
    let mut ledger =
        AttemptLedger::new(units, ExtraInputAllowance::new(128).unwrap()).expect("bounded target");
    for event in events {
        let text = event.as_str().unwrap_or_else(|| panic!("non-string event"));
        let applied = if text == "Backspace" {
            ledger.apply(LedgerEvent::Backspace)
        } else {
            let character = TrainingChar::new(
                text.chars().next().unwrap_or_else(|| panic!("empty event")),
                Layout::Us,
            )
            .unwrap_or_else(|error| panic!("event {text:?} rejected: {error}"));
            ledger.apply(LedgerEvent::Content(character))
        };
        applied.unwrap_or_else(|error| panic!("apply {text:?} failed: {error}"));
    }
    let _ = active;
    ledger
}

#[test]
fn all_scoring_fixture_cases_match() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/scoring.json"
    ))
    .expect("fixture file present");
    let fixture: Value = serde_json::from_str(&raw).expect("fixture parses");
    let cases = fixture["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty(), "fixture must contain cases");

    for case in cases {
        let id = case["id"].as_str().unwrap();
        let target = case["target"].as_str().unwrap();
        let events = case["events"].as_array().unwrap();
        let active = Micros::from_us(case["active_microseconds"].as_u64().unwrap());
        let interrupted = case["interrupted"].as_bool().unwrap();
        let expected = &case["expected"];

        let mut ledger = run_case(target, events, active);
        if interrupted {
            ledger.mark_interrupted();
        }
        let metrics = exercise_metrics(&ledger, active);

        let get = |field: &str| {
            expected[field]
                .as_u64()
                .unwrap_or_else(|| panic!("{id}.{field}"))
        };
        assert_eq!(
            metrics.first_correct,
            get("first_correct"),
            "{id}.first_correct"
        );
        assert_eq!(
            metrics.first_attempted,
            get("first_attempted"),
            "{id}.first_attempted"
        );
        assert_eq!(
            metrics.correct_attempts,
            get("correct_attempts"),
            "{id}.correct_attempts"
        );
        assert_eq!(
            metrics.content_attempts,
            get("content_attempts"),
            "{id}.content_attempts"
        );
        assert_eq!(metrics.deletions, get("deletions"), "{id}.deletions");
        assert_eq!(
            metrics.overflow_attempts,
            get("overflow_attempts"),
            "{id}.overflow_attempts"
        );
        assert_eq!(
            metrics.clean_completion,
            expected["clean_completion"].as_bool().unwrap(),
            "{id}.clean_completion"
        );
        assert_eq!(
            metrics.measurement_uninterrupted,
            expected["measurement_uninterrupted"].as_bool().unwrap(),
            "{id}.measurement_uninterrupted"
        );
        match expected["correct_wpm_fraction"].as_array() {
            Some(fraction) => {
                let ratio = metrics
                    .correct_wpm
                    .unwrap_or_else(|| panic!("{id}: expected a WPM fraction, got None"));
                assert_eq!(
                    ratio.numerator(),
                    u128::from(fraction[0].as_u64().unwrap()),
                    "{id}.wpm numerator"
                );
                assert_eq!(
                    ratio.denominator(),
                    u128::from(fraction[1].as_u64().unwrap()),
                    "{id}.wpm denominator"
                );
            }
            None => assert!(
                metrics.correct_wpm.is_none(),
                "{id}: expected no WPM fraction"
            ),
        }

        if let Some(numeric) = case["numeric_expected"].as_object() {
            let numpad = numeric_metrics(&ledger, active);
            assert_eq!(
                numpad.correct_digits,
                numeric["correct_digits"].as_u64().unwrap(),
                "{id}.correct_digits"
            );
            let fraction = numeric["digits_per_minute_fraction"].as_array().unwrap();
            let ratio = numpad.digits_per_minute.unwrap();
            assert_eq!(ratio.numerator(), u128::from(fraction[0].as_u64().unwrap()));
            assert_eq!(
                ratio.denominator(),
                u128::from(fraction[1].as_u64().unwrap())
            );
            assert_eq!(
                numpad.submitted_exact,
                numeric["submitted_exact"].as_bool().unwrap()
            );
            assert_eq!(
                numpad.clean_entry,
                numeric["clean_entry"].as_bool().unwrap()
            );
        }
    }
}

#[test]
fn timing_fixture_case_matches() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/scoring.json"
    ))
    .expect("fixture file present");
    let fixture: Value = serde_json::from_str(&raw).expect("fixture parses");
    let timing = &fixture["timing_case"];
    // Drive the production timing segments: content at 0, each explicit
    // pause [start, end) closes and reopens the segment.
    let mut segments = TimingSegments::new();
    segments.first_content(MonoTick::ORIGIN).unwrap();
    for pair in timing["pauses"].as_array().unwrap() {
        let pair = pair.as_array().unwrap();
        segments
            .pause(MonoTick::from_us(pair[0].as_u64().unwrap()))
            .unwrap();
        segments
            .resume(MonoTick::from_us(pair[1].as_u64().unwrap()))
            .unwrap();
    }
    let elapsed = MonoTick::from_us(timing["elapsed_microseconds"].as_u64().unwrap());
    let active: Micros = segments
        .active_through(elapsed)
        .expect("checked active time");
    assert_eq!(
        active.as_us(),
        timing["expected_active_microseconds"].as_u64().unwrap()
    );
    assert_eq!(
        timing["benchmark_eligible"].as_bool(),
        Some(false),
        "paused runs are never benchmark eligible (L-03)"
    );
}
