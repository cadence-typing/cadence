//! Display controls must preserve scoring facts and obey resize pause boundaries.

use cadence_core::{ContentDigest, LayoutSupportsPound, MonoTick, TrainingChar};
use cadence_engine::view::{PhaseState, TextSize, TextSupport, ViewMode};
use cadence_engine::{
    DomainOutcome, Engine, EventEnvelope, EventKind, Key, ScreenView, Transition,
};
use cadence_trainer::{ExerciseKind, Phase, PlannedExercise, PlannedPhase, SessionPlan};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn engine() -> Result<Engine, Box<dyn std::error::Error>> {
    Ok(Engine::new_plan(SessionPlan {
        budget_seconds: 31,
        policy_digest: ContentDigest::from_bytes([1; 32]),
        corpus_digest: ContentDigest::from_bytes([2; 32]),
        record: cadence_core::learning::PlanRecord {
            seed: 42,
            mode: cadence_core::learning::PlannerMode::ColdStart,
            focus: Vec::new(),
        },
        adaptation: cadence_trainer::Adaptation {
            hesitation_ceiling: cadence_core::Micros::from_us(1_500_000),
            reinforcement_capacity: 8,
            reinforcements_per_word: 1,
            numeric_gap_entries: 0,
        },
        phases: vec![
            PlannedPhase {
                phase: Phase::AdaptiveWords,
                seconds: 10,
                exercises: vec![PlannedExercise {
                    units: cadence_core::validate_target("cat", LayoutSupportsPound::US)?,
                    kind: ExerciseKind::Words,
                    words: Vec::new(),
                    passage: None,
                    caption: None,
                    target: None,
                }],
            },
            PlannedPhase {
                phase: Phase::Numpad,
                seconds: 20,
                exercises: vec![PlannedExercise {
                    units: cadence_core::validate_target("005.20", LayoutSupportsPound::US)?,
                    kind: ExerciseKind::Numpad,
                    words: Vec::new(),
                    passage: None,
                    caption: None,
                    target: None,
                }],
            },
            PlannedPhase {
                phase: Phase::Review,
                seconds: 1,
                exercises: Vec::new(),
            },
        ],
    })?)
}

fn send(
    engine: &mut Engine,
    at: u64,
    kind: EventKind,
) -> Result<Transition, Box<dyn std::error::Error>> {
    Ok(engine.reduce(EventEnvelope {
        seq: engine.consumed_events().next()?,
        at: MonoTick::from_us(at),
        kind,
    }))
}

fn content(
    engine: &mut Engine,
    at: u64,
    value: char,
) -> Result<Transition, Box<dyn std::error::Error>> {
    send(
        engine,
        at,
        EventKind::Key(Key::Content(TrainingChar::new(
            value,
            LayoutSupportsPound::US,
        )?)),
    )
}

#[test]
fn display_changes_never_rewrite_input_or_first_outcomes() -> TestResult {
    let mut engine = engine()?;
    send(&mut engine, 0, EventKind::Key(Key::Enter))?;
    content(&mut engine, 1_000_000, 'c')?;
    content(&mut engine, 2_000_000, 'x')?;
    let before = engine.prepare(MonoTick::from_us(2_000_000));
    assert_eq!(before.text_support, TextSupport::NormalOnly);
    for support in [TextSupport::DoubleHeight, TextSupport::NormalOnly] {
        send(&mut engine, 2_000_000, EventKind::SetTextSupport(support))?;
        let changed = engine.prepare(MonoTick::from_us(2_000_000));
        assert_eq!(changed.text_support, support);
        assert_eq!(changed.text_size, before.text_size);
        assert_eq!(changed.screen, before.screen);
        assert_eq!(changed.active, before.active);
    }
    send(&mut engine, 2_000_000, EventKind::Key(Key::ToggleTextSize))?;
    let large = engine.prepare(MonoTick::from_us(2_000_000));
    assert_eq!(large.text_size, TextSize::Large);
    assert_eq!(before.screen, large.screen);
    assert_eq!(before.active, large.active);
    send(
        &mut engine,
        2_000_000,
        EventKind::SetTextSize(TextSize::Normal),
    )?;
    assert_eq!(
        engine.prepare(MonoTick::from_us(2_000_000)).screen,
        before.screen
    );
    send(&mut engine, 2_100_000, EventKind::Key(Key::Backspace))?;
    content(&mut engine, 2_200_000, 'a')?;
    let done = content(&mut engine, 2_300_000, 't')?;
    assert!(matches!(
        done.outcome,
        DomainOutcome::ExerciseCompleted {
            first_pass: Some((2, 3)),
            ..
        }
    ));
    Ok(())
}

#[test]
fn tiny_resize_pauses_and_cannot_resume_until_a_usable_viewport() -> TestResult {
    let mut engine = engine()?;
    send(&mut engine, 0, EventKind::Key(Key::Enter))?;
    content(&mut engine, 1_000_000, 'c')?;
    send(
        &mut engine,
        2_000_000,
        EventKind::Resized {
            columns: 59,
            rows: 18,
        },
    )?;
    let paused = engine.prepare(MonoTick::from_us(2_000_000));
    assert_eq!(paused.mode, ViewMode::Paused);
    assert_eq!(paused.active.as_us(), 1_000_000);
    send(&mut engine, 3_000_000, EventKind::Key(Key::Escape))?;
    content(&mut engine, 4_000_000, 'x')?;
    send(
        &mut engine,
        5_000_000,
        EventKind::Resized {
            columns: 100,
            rows: 32,
        },
    )?;
    let restored = engine.prepare(MonoTick::from_us(5_000_000));
    assert_eq!(restored.mode, ViewMode::Paused);
    assert_eq!(restored.screen, paused.screen);
    assert_eq!(restored.active, paused.active);
    send(&mut engine, 6_000_000, EventKind::Key(Key::Escape))?;
    content(&mut engine, 7_000_000, 'a')?;
    let done = content(&mut engine, 8_000_000, 't')?;
    assert!(matches!(
        done.outcome,
        DomainOutcome::ExerciseCompleted {
            first_pass: Some((3, 3)),
            ..
        }
    ));
    assert_eq!(
        engine.prepare(MonoTick::from_us(8_000_000)).active.as_us(),
        3_000_000
    );
    Ok(())
}

#[test]
fn interlude_reports_closed_phase_without_claiming_future_progress() -> TestResult {
    let mut engine = engine()?;
    send(&mut engine, 0, EventKind::Key(Key::Enter))?;
    content(&mut engine, 1_000_000, 'c')?;
    content(&mut engine, 1_500_000, 'x')?;
    send(&mut engine, 20_000_000, EventKind::Tick)?;
    let view = engine.prepare(MonoTick::from_us(20_000_000));
    assert_eq!(view.mode, ViewMode::Next);
    assert!(matches!(
        view.screen,
        ScreenView::PhaseComplete { partial: true, .. }
    ));
    let report = view
        .phase_report
        .ok_or("interlude must retain partial evidence")?;
    assert_eq!(
        (
            report.first_correct,
            report.first_attempted,
            report.active_micros
        ),
        (1, 2, 10_000_000)
    );
    assert_eq!(
        view.phases
            .iter()
            .map(|phase| phase.state)
            .collect::<Vec<_>>(),
        vec![
            PhaseState::Complete,
            PhaseState::Upcoming,
            PhaseState::Upcoming
        ]
    );
    send(&mut engine, 30_000_000, EventKind::Key(Key::Enter))?;
    let next = engine.prepare(MonoTick::from_us(30_000_000));
    assert_eq!(next.mode, ViewMode::Typing);
    assert_eq!(next.active.as_us(), 10_000_000);
    assert_eq!(
        next.phases
            .iter()
            .map(|phase| phase.state)
            .collect::<Vec<_>>(),
        vec![
            PhaseState::Complete,
            PhaseState::Active,
            PhaseState::Upcoming
        ]
    );
    Ok(())
}
