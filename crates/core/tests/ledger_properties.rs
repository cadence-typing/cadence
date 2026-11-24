//! Property tests over the ledger's write-once and boundedness
//! invariants (L-07 / VK-01 contract, executable form).
//!
//! Properties:
//! 1. First outcomes are write-once: once assigned, later events never
//!    change the vector (corrections cannot erase first errors).
//! 2. Counters are monotone and bounded by the number of applied events.
//! 3. The buffer never exceeds target length + allowance.
//! 4. Corrections alone cannot turn a first-incorrect position correct.

// Test assertions may use expect/unwrap/panic/indexing to fail loudly;
// the workspace denies them in production code only.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar, validate_target};
use cadence_core::ledger::{Applied, AttemptLedger, ExtraInputAllowance, LedgerEvent, Slot};
use proptest::prelude::*;

fn any_training_char() -> impl Strategy<Value = char> {
    proptest::char::range('a', 'z')
}

fn ledger_for(target: &str) -> AttemptLedger {
    AttemptLedger::new(
        validate_target(target, LayoutSupportsPound::US).unwrap(),
        ExtraInputAllowance::new(8).unwrap(),
    )
    .unwrap()
}

fn event_strategy() -> impl Strategy<Value = LedgerEvent> {
    prop_oneof![
        8 => any_training_char().prop_map(|c| LedgerEvent::Content(
            TrainingChar::new(c, LayoutSupportsPound::US).unwrap()
        )),
        2 => Just(LedgerEvent::Backspace),
    ]
}

proptest! {
    #[test]
    fn first_outcomes_are_write_once(
        target in "[a-z]{1,12}",
        events in prop::collection::vec(event_strategy(), 0..64),
    ) {
        let mut ledger = ledger_for(&target);
        let mut snapshots = Vec::new();
        for event in &events {
            ledger.apply(*event).unwrap();
            snapshots.push(ledger.first_outcomes().to_vec());
            // Invariant: previously ASSIGNED slots never change;
            // None -> Some is a legal first assignment.
            if snapshots.len() >= 2 {
                let previous = &snapshots[snapshots.len() - 2];
                for (index, slot) in previous.iter().enumerate() {
                    if *slot != Slot::Open {
                        prop_assert_eq!(ledger.first_outcomes()[index], *slot);
                    }
                }
            }
        }
    }

    #[test]
    fn counters_monotone_and_bounded(
        target in "[a-z]{1,12}",
        events in prop::collection::vec(event_strategy(), 0..64),
    ) {
        let mut ledger = ledger_for(&target);
        let mut previous = ledger.counters();
        for (step, event) in events.iter().enumerate() {
            ledger.apply(*event).unwrap();
            let current = ledger.counters();
            prop_assert!(current.content_attempts() >= previous.content_attempts());
            prop_assert!(current.correct_attempts() >= previous.correct_attempts());
            prop_assert!(current.deletions() >= previous.deletions());
            prop_assert!(current.overflow_attempts() >= previous.overflow_attempts());
            prop_assert!(current.content_attempts() <= (step as u64) + 1);
            prop_assert!(current.deletions() <= (step as u64) + 1);
            previous = current;
        }
    }

    #[test]
    fn buffer_respects_bound(
        target in "[a-z]{1,12}",
        events in prop::collection::vec(event_strategy(), 0..64),
    ) {
        let mut ledger = ledger_for(&target);
        let bound = ledger.target().len() + ledger.allowance().get() as usize;
        for event in &events {
            if let Ok(Applied::Content { .. }) | Ok(Applied::RejectedOverBound) = ledger.apply(*event) {
                prop_assert!(ledger.buffer().len() <= bound);
            }
        }
    }

    #[test]
    fn correction_cannot_erase_first_errors(
        target in "[a-z]{1,12}",
        wrong in any_training_char(),
    ) {
        // Type a deliberately wrong first character, then correct it.
        let wrong_unit = TrainingChar::new(wrong, LayoutSupportsPound::US).unwrap();
        let mut ledger = ledger_for(&target);
        let first = target.chars().next().unwrap();
        if wrong_unit.get() != first {
            ledger.apply(LedgerEvent::Content(wrong_unit)).unwrap();
            let before = ledger.first_outcomes().to_vec();
            ledger.apply(LedgerEvent::Backspace).unwrap();
            let after = ledger.first_outcomes();
            prop_assert_eq!(after[0], before[0]);
            prop_assert_eq!(after[0], Slot::Incorrect);
        }
    }

    #[test]
    fn first_correct_never_exceeds_first_attempted(
        target in "[a-z]{1,12}",
        events in prop::collection::vec(event_strategy(), 0..64),
    ) {
        let mut ledger = ledger_for(&target);
        for event in &events {
            ledger.apply(*event).unwrap();
        }
        prop_assert!(ledger.first_correct().get() <= ledger.first_attempted().get());
        prop_assert!(ledger.first_attempted().get() <= ledger.target().len() as u64);
        prop_assert!(ledger.correct_visible_units().get() <= ledger.target().len() as u64);
    }
}
