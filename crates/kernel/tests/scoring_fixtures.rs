//! `fixtures/scoring.json` (hand-calculated expected results) replayed
//! through the kernel's own functions: the same arithmetic the Lean
//! theorems are about, checked against numbers computed independently.

use cadence_kernel::checkpoint::{Admission, Receipt, admit, latest_after};
use cadence_kernel::counters::{Counters, Step, content_step, deletion_step, matched};
use cadence_kernel::first::{Slot, count_first, settle};
use cadence_kernel::ratio::{BasisPoints, Pace, basis_points, wpm};
use cadence_kernel::time::{Active, active_time};
use serde_json::Value;

fn fixture() -> Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/scoring.json"
    ))
    .expect("fixture file present");
    serde_json::from_str(&raw).expect("fixture parses")
}

/// Replays events with the minimal buffer model: a content event is
/// compared with the target at the buffer length, a backspace pops.
fn replay(target: &str, events: &[Value]) -> (Vec<Slot>, Counters, usize) {
    let target: Vec<char> = target.chars().collect();
    let mut slots = vec![Slot::Open; target.len()];
    let mut counters = Counters::default();
    let mut buffer: Vec<char> = Vec::new();
    for event in events {
        let text = event.as_str().expect("string event");
        if text == "Backspace" {
            let Step::Counted(next) = deletion_step(counters, 1) else {
                panic!("deletion overflow");
            };
            counters = next;
            buffer.pop();
            continue;
        }
        let typed = text.chars().next().expect("non-empty event");
        let position = buffer.len();
        let expected = target.get(position).copied();
        let same = expected == Some(typed);
        let Step::Counted(next) = content_step(counters, expected.is_some(), same) else {
            panic!("content overflow");
        };
        counters = next;
        if let Some(slot) = slots.get_mut(position) {
            *slot = settle(*slot, matched(expected.is_some(), same));
        }
        buffer.push(typed);
    }
    let visible = buffer
        .iter()
        .zip(target.iter())
        .filter(|(typed, wanted)| typed == wanted)
        .count();
    (slots, counters, visible)
}

#[test]
fn every_scoring_case_matches_the_independent_expectation() {
    let fixture = fixture();
    let cases = fixture["cases"].as_array().expect("cases");
    assert!(!cases.is_empty());
    for case in cases {
        let id = case["id"].as_str().expect("id");
        let events = case["events"].as_array().expect("events");
        let expected = &case["expected"];
        let (slots, counters, visible) = replay(case["target"].as_str().expect("target"), events);
        let counts = count_first(&slots);
        let get = |field: &str| expected[field].as_u64().expect(field);
        assert_eq!(counts.correct as u64, get("first_correct"), "{id}");
        assert_eq!(counts.attempted as u64, get("first_attempted"), "{id}");
        assert_eq!(counters.correct, get("correct_attempts"), "{id}");
        assert_eq!(counters.content, get("content_attempts"), "{id}");
        assert_eq!(counters.deletions, get("deletions"), "{id}");
        assert_eq!(counters.overflow, get("overflow_attempts"), "{id}");
        assert_eq!(visible as u64, get("correct_visible_units"), "{id}");
        let active = case["active_microseconds"].as_u64().expect("active");
        match (wpm(visible as u64, active), expected["correct_wpm_fraction"].as_array()) {
            (Pace::Rate(rate), Some(pair)) => {
                assert_eq!(rate.num, u128::from(pair[0].as_u64().expect("num")), "{id}");
                assert_eq!(rate.den, u128::from(pair[1].as_u64().expect("den")), "{id}");
            }
            (Pace::Empty, None) => {}
            (got, want) => panic!("{id}: pace {got:?} against {want:?}"),
        }
    }
}

#[test]
fn a_corrected_mistake_stays_a_first_attempt_error() {
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["id"] == "corrected-error")
        .expect("corrected-error case");
    let (slots, _, visible) = replay("cat", case["events"].as_array().expect("events"));
    // c x Backspace a t: the final text is right, the second position's
    // first outcome stays incorrect.
    assert_eq!(visible, 3);
    assert_eq!(slots, vec![Slot::Correct, Slot::Incorrect, Slot::Correct]);
    assert_eq!(count_first(&slots).correct, 2);
}

#[test]
fn timing_case_excludes_only_explicit_pauses() {
    let fixture = fixture();
    let timing = &fixture["timing_case"];
    let elapsed = timing["elapsed_microseconds"].as_u64().expect("elapsed");
    let pauses: Vec<u64> = timing["pauses"]
        .as_array()
        .expect("pauses")
        .iter()
        .map(|pair| pair[1].as_u64().expect("end") - pair[0].as_u64().expect("start"))
        .collect();
    let want = timing["expected_active_microseconds"].as_u64().expect("active");
    assert_eq!(active_time(elapsed, &pauses), Active::Time(want));
}

#[test]
fn accuracy_truncates_and_stays_within_full() {
    assert_eq!(basis_points(2, 3), BasisPoints::Value(6_666));
    assert_eq!(basis_points(3, 3), BasisPoints::Value(10_000));
    assert_eq!(basis_points(0, 5), BasisPoints::Value(0));
    assert_eq!(basis_points(1, 0), BasisPoints::ZeroDenominator);
    assert_eq!(basis_points(4, 3), BasisPoints::OverFull);
}

#[test]
fn checkpoint_credit_is_applied_once() {
    assert_eq!(admit(None, Receipt::Absent, 1), Admission::Apply);
    let held = latest_after(None, Admission::Apply, 1);
    assert_eq!(held, Some(1));
    // The same sequence again, with or without its receipt, is not applied.
    assert_eq!(admit(held, Receipt::Same, 1), Admission::Replay);
    assert_eq!(admit(held, Receipt::Absent, 1), Admission::Stale);
    assert_eq!(admit(held, Receipt::Different, 1), Admission::Conflict);
    assert_eq!(admit(held, Receipt::Absent, 2), Admission::Apply);
}
