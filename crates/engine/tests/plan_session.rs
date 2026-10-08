//! Plan-driven session behaviour tests (A-05, P-02).
//!
//! Phase deadlines end the scored slice at the event boundary with the
//! partial exercise retained; numpad Enter submits only; the summary
//! aggregates numerators/denominators across tracks.

// Test assertions may use expect/unwrap/panic/indexing to fail loudly;
// the workspace denies them in production code only.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use cadence_core::alphabet::validate_target;
use cadence_core::{ContentDigest, EventSeq, LayoutSupportsPound, MonoTick, TrainingChar};
use cadence_engine::{
    DomainOutcome, Engine, EventEnvelope, EventKind, InputOwner, Key, ScreenView,
};
use cadence_trainer::{ExerciseKind, PlannedExercise, PlannedPhase, SessionPlan};

fn units(text: &str) -> Vec<TrainingChar> {
    validate_target(text, LayoutSupportsPound::US).unwrap()
}

fn plan(phases: Vec<PlannedPhase>, budget: u64) -> SessionPlan {
    SessionPlan {
        budget_seconds: budget,
        policy_digest: ContentDigest::from_bytes([1; 32]),
        corpus_digest: ContentDigest::from_bytes([2; 32]),
        seed: 7,
        phases,
    }
}

fn envelope(seq: u64, at_us: u64, kind: EventKind) -> EventEnvelope {
    EventEnvelope {
        seq: EventSeq::from_raw(seq),
        at: MonoTick::from_us(at_us),
        kind,
    }
}

fn content(character: char) -> EventKind {
    EventKind::Key(Key::Content(
        TrainingChar::new(character, LayoutSupportsPound::US).unwrap(),
    ))
}

/// A two-phase plan: a 10-second words phase then a 20-second numpad
/// phase, then review.
fn small_plan() -> SessionPlan {
    plan(
        vec![
            PlannedPhase {
                phase: cadence_trainer::Phase::AdaptiveWords,
                seconds: 10,
                exercises: vec![
                    PlannedExercise {
                        units: units("cat"),
                        kind: ExerciseKind::Words,
                    },
                    PlannedExercise {
                        units: units("dog"),
                        kind: ExerciseKind::Words,
                    },
                ],
            },
            PlannedPhase {
                phase: cadence_trainer::Phase::Numpad,
                seconds: 20,
                exercises: vec![PlannedExercise {
                    units: units("12 34"),
                    kind: ExerciseKind::Numpad,
                }],
            },
            PlannedPhase {
                phase: cadence_trainer::Phase::Review,
                seconds: 5,
                exercises: vec![],
            },
        ],
        35,
    )
}

#[test]
fn phase_deadline_ends_scored_slice_at_boundary() {
    let mut engine = Engine::new_plan(small_plan()).unwrap();
    engine.reduce(envelope(1, 0, EventKind::Key(Key::Enter)));

    // First content key starts the clock at 1s; keys at/after the 10s
    // active-time boundary are outside the scored slice.
    engine.reduce(envelope(2, 1_000_000, content('c')));
    engine.reduce(envelope(3, 2_000_000, content('a')));
    let boundary = engine.reduce(envelope(4, 11_000_000, content('t')));

    match boundary.outcome {
        DomainOutcome::PhaseDeadline { phase, partial } => {
            assert_eq!(phase, cadence_trainer::Phase::AdaptiveWords);
            assert!(partial, "the unfinished 'cat' is retained as partial");
        }
        other => panic!("expected phase deadline, got {other:?}"),
    }
    // The next phase is offered, not forced.
    assert_eq!(engine.input_owner(), InputOwner::Navigation);
    let view = engine.prepare(MonoTick::from_us(11_000_000));
    match &view.screen {
        ScreenView::PhaseComplete { partial, next, .. } => {
            assert!(*partial);
            assert_eq!(next.as_deref(), Some("numpad"));
        }
        other => panic!("expected phase-complete offer, got {other:?}"),
    }
}

#[test]
fn phases_advance_and_summary_aggregates_honestly() {
    let mut engine = Engine::new_plan(small_plan()).unwrap();
    engine.reduce(envelope(1, 0, EventKind::Key(Key::Enter)));
    // cat clean (1s..4s), dog with a correction (4s..9s).
    for (seq, at, kind) in [
        (2u64, 1_000_000u64, content('c')),
        (3, 1_500_000, content('a')),
        (4, 2_000_000, content('t')),
        (5, 3_000_000, content('d')),
        (6, 3_500_000, content('x')),
        (7, 4_000_000, EventKind::Key(Key::Backspace)),
        (8, 4_500_000, content('o')),
        (9, 5_000_000, content('g')),
    ] {
        let transition = engine.reduce(envelope(seq, at, kind));
        assert!(!transition.quit);
    }
    // Words phase exhausted its exercises → phase-complete → Enter.
    let offer = engine.prepare(MonoTick::from_us(5_000_000));
    assert!(matches!(offer.screen, ScreenView::PhaseComplete { .. }));
    engine.reduce(envelope(10, 5_000_000, EventKind::Key(Key::Enter)));

    // Numpad phase: digits then Enter submits (L-02).
    for (seq, at, character) in [
        (11u64, 6_000_000u64, '1'),
        (12, 6_200_000, '2'),
        (13, 6_400_000, ' '),
        (14, 6_600_000, '3'),
        (15, 6_800_000, '4'),
    ] {
        engine.reduce(envelope(seq, at, content(character)));
    }
    let submitted = engine.reduce(envelope(16, 7_000_000, EventKind::Key(Key::Enter)));
    match submitted.outcome {
        DomainOutcome::ExerciseCompleted { first_pass, .. } => {
            assert_eq!(first_pass, Some((5, 5)));
        }
        other => panic!("expected numpad submit completion, got {other:?}"),
    }

    // Numpad phase complete → review phase is non-typing → summary.
    engine.reduce(envelope(17, 7_000_000, EventKind::Key(Key::Enter)));
    let summary = engine.reduce(envelope(18, 7_000_000, EventKind::Key(Key::Enter)));
    assert!(matches!(summary.outcome, DomainOutcome::SessionSummary));
    let view = engine.prepare(MonoTick::from_us(7_000_000));
    match &view.screen {
        ScreenView::Summary { lines } => {
            let words_line = lines
                .iter()
                .find(|line| line.starts_with("adaptive words:"))
                .expect("words track aggregated");
            // cat 3/3 + dog 2/3 first-attempt = 5/6 aggregated.
            assert!(words_line.contains("first-pass 5/6"), "{words_line}");
            assert!(words_line.contains("attempt 6/7"), "{words_line}");
            let numpad_line = lines
                .iter()
                .find(|line| line.starts_with("numpad:"))
                .expect("numpad track separate (numeric history)");
            assert!(numpad_line.contains("first-pass 5/5"));
        }
        other => panic!("expected summary, got {other:?}"),
    }
}

#[test]
fn numpad_enter_does_not_advance_wrong_value() {
    let mut engine = Engine::new_plan(small_plan()).unwrap();
    engine.reduce(envelope(1, 0, EventKind::Key(Key::Enter)));
    // Finish the words phase quickly.
    for (seq, at, kind) in [
        (2u64, 1_000_000u64, content('c')),
        (3, 1_200_000, content('a')),
        (4, 1_400_000, content('t')),
        (5, 1_500_000, content('d')),
        (6, 1_700_000, content('o')),
        (7, 1_900_000, content('g')),
    ] {
        engine.reduce(envelope(seq, at, kind));
    }
    engine.reduce(envelope(8, 2_000_000, EventKind::Key(Key::Enter)));

    // Type a wrong first digit, then Enter: stays on the exercise.
    engine.reduce(envelope(9, 3_000_000, content('9')));
    let wrong_submit = engine.reduce(envelope(10, 3_500_000, EventKind::Key(Key::Enter)));
    assert!(matches!(wrong_submit.outcome, DomainOutcome::None));
    let view = engine.prepare(MonoTick::from_us(3_500_000));
    match &view.screen {
        ScreenView::Practice { status, .. } => {
            assert_eq!(status.phase_label, "numpad");
            assert_eq!(status.exercise_index, 1);
        }
        other => panic!("expected practice, got {other:?}"),
    }
}

#[test]
fn pause_excludes_time_from_phase_budget() {
    let mut engine = Engine::new_plan(small_plan()).unwrap();
    engine.reduce(envelope(1, 0, EventKind::Key(Key::Enter)));
    engine.reduce(envelope(2, 1_000_000, content('c')));
    // Pause 1s..30s, resume: the phase budget counts active time only.
    engine.reduce(envelope(3, 1_500_000, EventKind::Key(Key::Escape)));
    engine.reduce(envelope(4, 30_000_000, EventKind::Key(Key::Escape)));
    let still_in = engine.reduce(envelope(5, 31_000_000, content('a')));
    assert!(matches!(still_in.outcome, DomainOutcome::None));
    let view = engine.prepare(MonoTick::from_us(31_000_000));
    match &view.screen {
        ScreenView::Practice { status, .. } => {
            assert_eq!(status.phase_label, "adaptive words");
        }
        other => panic!("expected practice, got {other:?}"),
    }
}
