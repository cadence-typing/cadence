//! Engine fixtures shared by the session test crates.
#![allow(
    dead_code,
    reason = "each integration-test crate uses a different subset of these helpers"
)]
use cadence_core::progress::{Checkpoint, HistorySummary, SaveResult};
use cadence_core::{
    ContentDigest, CoreError, LayoutSupportsPound, MonoTick, SessionId, TrainingChar,
};
use cadence_engine::{Engine, EventEnvelope, EventKind, Key, Transition};
use cadence_trainer::{ExerciseKind, Phase, PlannedExercise, PlannedPhase, SessionPlan};

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

pub fn phase(
    kind: ExerciseKind,
    phase: Phase,
    seconds: u64,
    targets: &[&str],
) -> Result<PlannedPhase, Box<dyn std::error::Error>> {
    let mut exercises = Vec::new();
    for text in targets {
        exercises.push(PlannedExercise {
            kind,
            units: cadence_core::validate_target(text, LayoutSupportsPound::US)?,
            words: Vec::new(),
            passage: None,
            caption: None,
            target: None,
            spans: Vec::new(),
        });
    }
    Ok(PlannedPhase {
        phase,
        seconds,
        exercises,
    })
}
pub fn engine(phases: Vec<PlannedPhase>) -> Result<Engine, CoreError> {
    Engine::new_plan(SessionPlan {
        budget_seconds: 30,
        policy_digest: ContentDigest::from_bytes([1; 32]),
        corpus_digest: ContentDigest::from_bytes([2; 32]),
        record: cadence_core::learning::PlanRecord {
            seed: 7,
            mode: cadence_core::learning::PlannerMode::ColdStart,
            focus: Vec::new(),
        },
        adaptation: cadence_trainer::Adaptation {
            hesitation_ceiling: cadence_core::Micros::from_us(1_500_000),
            reinforcement_capacity: 8,
            reinforcements_per_word: 1,
            numeric_gap_entries: 0,
        },
        probe: cadence_trainer::ProbePlan::NotDue,
        pace: Vec::new(),
        phases,
    })
}
pub fn words() -> Result<Engine, Box<dyn std::error::Error>> {
    Ok(engine(vec![phase(
        ExerciseKind::Words,
        Phase::AdaptiveWords,
        10,
        &["cat", "dog"],
    )?])?)
}
pub fn send(e: &mut Engine, at: u64, kind: EventKind) -> Result<Transition, CoreError> {
    let seq = e.consumed_events().next()?;
    Ok(e.reduce(EventEnvelope {
        seq,
        at: MonoTick::from_us(at),
        kind,
    }))
}
pub fn key(e: &mut Engine, at: u64, key: Key) -> Result<Transition, CoreError> {
    send(e, at, EventKind::Key(key))
}
pub fn content(e: &mut Engine, at: u64, c: char) -> Result<Transition, Box<dyn std::error::Error>> {
    Ok(key(
        e,
        at,
        Key::Content(TrainingChar::new(c, LayoutSupportsPound::US)?),
    )?)
}
pub fn ready(e: &mut Engine) -> Result<(), CoreError> {
    send(
        e,
        0,
        EventKind::StorageReady {
            next_session: SessionId::new(1)?,
            day: 123,
            history: HistorySummary::default(),
        },
    )?;
    Ok(())
}
pub fn ack(e: &mut Engine, at: u64, checkpoint: &Checkpoint) -> Result<Transition, CoreError> {
    send(
        e,
        at,
        EventKind::SaveResult(SaveResult {
            key: checkpoint.key,
            result: Ok(()),
        }),
    )
}
pub fn checkpoint(t: Transition) -> Result<Checkpoint, Box<dyn std::error::Error>> {
    t.checkpoint.ok_or_else(|| "checkpoint expected".into())
}

/// Admits storage with the given history before the session starts.
pub fn ready_with(e: &mut Engine, history: HistorySummary) -> Result<(), CoreError> {
    send(
        e,
        0,
        EventKind::StorageReady {
            next_session: SessionId::new(1)?,
            day: 123,
            history,
        },
    )?;
    Ok(())
}
/// Acknowledges every checkpoint a transition dispatches; returns the last
/// acknowledged checkpoint and whether the final acknowledgement released quit.
pub fn settle(
    e: &mut Engine,
    at: &mut u64,
    mut t: Transition,
) -> Result<(Option<Checkpoint>, bool), CoreError> {
    let mut last = None;
    let mut quit = t.quit;
    while let Some(saved) = t.checkpoint.take() {
        *at = at.saturating_add(1);
        t = ack(e, *at, &saved)?;
        quit = t.quit;
        last = Some(saved);
    }
    Ok((last, quit))
}
