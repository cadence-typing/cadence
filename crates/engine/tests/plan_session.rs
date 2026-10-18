//! Behavioral regressions for exact clocks, scoring and durable quit.
use cadence_core::progress::{Checkpoint, HistorySummary, SaveResult};
use cadence_core::{
    ContentDigest, CoreError, EventSeq, LayoutSupportsPound, MonoTick, SessionId, TrainingChar,
};
use cadence_engine::{
    DomainOutcome, Engine, EventEnvelope, EventKind, InputOwner, Key, ScreenView, Transition,
    UnitSpan,
};
use cadence_trainer::{ExerciseKind, Phase, PlannedExercise, PlannedPhase, SessionPlan};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn phase(
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
        });
    }
    Ok(PlannedPhase {
        phase,
        seconds,
        exercises,
    })
}
fn engine(phases: Vec<PlannedPhase>) -> Result<Engine, CoreError> {
    Engine::new_plan(SessionPlan {
        budget_seconds: 30,
        policy_digest: ContentDigest::from_bytes([1; 32]),
        corpus_digest: ContentDigest::from_bytes([2; 32]),
        seed: 7,
        phases,
    })
}
fn words() -> Result<Engine, Box<dyn std::error::Error>> {
    Ok(engine(vec![phase(
        ExerciseKind::Words,
        Phase::AdaptiveWords,
        10,
        &["cat", "dog"],
    )?])?)
}
fn send(e: &mut Engine, at: u64, kind: EventKind) -> Result<Transition, CoreError> {
    let seq = e.consumed_events().next()?;
    Ok(e.reduce(EventEnvelope {
        seq,
        at: MonoTick::from_us(at),
        kind,
    }))
}
fn key(e: &mut Engine, at: u64, key: Key) -> Result<Transition, CoreError> {
    send(e, at, EventKind::Key(key))
}
fn content(e: &mut Engine, at: u64, c: char) -> Result<Transition, Box<dyn std::error::Error>> {
    Ok(key(
        e,
        at,
        Key::Content(TrainingChar::new(c, LayoutSupportsPound::US)?),
    )?)
}
fn ready(e: &mut Engine) -> Result<(), CoreError> {
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
fn ack(e: &mut Engine, at: u64, checkpoint: &Checkpoint) -> Result<Transition, CoreError> {
    send(
        e,
        at,
        EventKind::SaveResult(SaveResult {
            key: checkpoint.key,
            result: Ok(()),
        }),
    )
}
fn checkpoint(t: Transition) -> Result<Checkpoint, Box<dyn std::error::Error>> {
    t.checkpoint.ok_or_else(|| "checkpoint expected".into())
}

#[test]
fn correction_preserves_first_pass_and_cycles_without_early_end() -> TestResult {
    let mut e = words()?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1_000_000, 'c')?;
    content(&mut e, 2_000_000, 'x')?;
    key(&mut e, 2_500_000, Key::Backspace)?;
    content(&mut e, 3_000_000, 'a')?;
    let done = content(&mut e, 4_000_000, 't')?;
    assert!(matches!(
        done.outcome,
        DomainOutcome::ExerciseCompleted {
            first_pass: Some((2, 3)),
            ..
        }
    ));
    content(&mut e, 5_000_000, 'd')?;
    content(&mut e, 6_000_000, 'o')?;
    content(&mut e, 7_000_000, 'g')?;
    assert_eq!(e.input_owner(), InputOwner::Practice);
    assert!(
        matches!(e.prepare(MonoTick::from_us(7_000_000)).screen,ScreenView::Practice {status,..} if status.exercise_index==1)
    );
    Ok(())
}
#[test]
fn late_tick_clamps_exact_deadline_and_excludes_transition_screens() -> TestResult {
    let mut e = engine(vec![
        phase(ExerciseKind::Words, Phase::AdaptiveWords, 10, &["cat"])?,
        phase(ExerciseKind::Numpad, Phase::Numpad, 20, &["12"])?,
    ])?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1_000_000, 'c')?;
    let late = send(&mut e, 40_000_000, EventKind::Tick)?;
    assert!(matches!(
        late.outcome,
        DomainOutcome::PhaseDeadline { partial: true, .. }
    ));
    assert_eq!(
        e.prepare(MonoTick::from_us(40_000_000)).active.as_us(),
        10_000_000
    );
    key(&mut e, 90_000_000, Key::Enter)?;
    send(&mut e, 100_000_000, EventKind::Tick)?;
    assert_eq!(
        e.prepare(MonoTick::from_us(100_000_000)).active.as_us(),
        10_000_000
    );
    content(&mut e, 101_000_000, '1')?;
    content(&mut e, 102_000_000, '2')?;
    assert!(
        matches!(e.prepare(MonoTick::from_us(102_000_000)).screen,ScreenView::Practice {status,..} if status.enter_submits)
    );
    let submit = key(&mut e, 103_000_000, Key::Enter)?;
    assert!(matches!(
        submit.outcome,
        DomainOutcome::ExerciseCompleted {
            first_pass: Some((2, 2)),
            ..
        }
    ));
    send(&mut e, 140_000_000, EventKind::Tick)?;
    assert_eq!(
        e.prepare(MonoTick::from_us(140_000_000)).active.as_us(),
        30_000_000
    );
    Ok(())
}
#[test]
fn exact_boundary_key_is_not_scored() -> TestResult {
    let mut e = words()?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1_000_000, 'c')?;
    content(&mut e, 2_000_000, 'a')?;
    let boundary = content(&mut e, 11_000_000, 't')?;
    assert!(matches!(
        boundary.outcome,
        DomainOutcome::PhaseDeadline { partial: true, .. }
    ));
    key(&mut e, 12_000_000, Key::Enter)?;
    assert!(
        matches!(e.prepare(MonoTick::from_us(12_000_000)).screen,ScreenView::Summary {lines} if lines.iter().any(|s|s.contains("first-pass 2/2")))
    );
    Ok(())
}
#[test]
fn numpad_enter_does_not_advance_wrong_value() -> TestResult {
    let mut e = engine(vec![phase(
        ExerciseKind::Numpad,
        Phase::Numpad,
        20,
        &["12"],
    )?])?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1_000_000, '9')?;
    assert!(matches!(
        key(&mut e, 2_000_000, Key::Enter)?.outcome,
        DomainOutcome::None
    ));
    assert!(
        matches!(e.prepare(MonoTick::from_us(2_000_000)).screen,ScreenView::Practice {spans,..} if spans.first()==Some(&UnitSpan::Wrong {typed:'9',expected:'1'}))
    );
    Ok(())
}
#[test]
fn pause_before_first_input_and_paused_controls_do_not_score() -> TestResult {
    let mut e = words()?;
    key(&mut e, 0, Key::Enter)?;
    key(&mut e, 1_000_000, Key::Escape)?;
    assert_eq!(e.input_owner(), InputOwner::Navigation);
    content(&mut e, 2_000_000, 'c')?;
    content(&mut e, 3_000_000, 'r')?;
    key(&mut e, 30_000_000, Key::Escape)?;
    content(&mut e, 40_000_000, 'c')?;
    key(&mut e, 41_000_000, Key::Escape)?;
    key(&mut e, 80_000_000, Key::Escape)?;
    content(&mut e, 81_000_000, 'a')?;
    assert_eq!(
        e.prepare(MonoTick::from_us(81_000_000)).active.as_us(),
        2_000_000
    );
    assert!(
        matches!(e.prepare(MonoTick::from_us(81_000_000)).screen,ScreenView::Practice {status,..} if status.first_pass_percent==Some(100))
    );
    Ok(())
}
#[test]
fn backward_time_and_bad_sequences_visibly_invalidate() -> TestResult {
    let mut e = words()?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 2_000_000, 'c')?;
    let bad = content(&mut e, 1_000_000, 'a')?;
    assert!(matches!(
        bad.outcome,
        DomainOutcome::MeasurementInvalid { .. }
    ));
    assert!(e.prepare(MonoTick::from_us(2_000_000)).notice.is_some());
    let mut e = words()?;
    key(&mut e, 0, Key::Enter)?;
    let bad = e.reduce(EventEnvelope {
        seq: EventSeq::from_raw(3),
        at: MonoTick::from_us(1),
        kind: EventKind::Tick,
    });
    assert!(matches!(
        bad.outcome,
        DomainOutcome::MeasurementInvalid { .. }
    ));
    assert_eq!(e.consumed_events(), EventSeq::from_raw(1));
    Ok(())
}
#[test]
fn paste_and_focus_loss_stop_scoring() -> TestResult {
    for event in [EventKind::PasteRejected, EventKind::FocusLost] {
        let mut e = words()?;
        key(&mut e, 0, Key::Enter)?;
        assert!(matches!(
            send(&mut e, 1, event)?.outcome,
            DomainOutcome::MeasurementInvalid { .. }
        ));
        content(&mut e, 2, 'c')?;
        assert!(
            matches!(e.prepare(MonoTick::from_us(2)).screen,ScreenView::Practice {spans,paused:true,..} if spans.first()==Some(&UnitSpan::Cursor {expected:'c'}))
        );
    }
    Ok(())
}
#[test]
fn per_exercise_and_track_durations_are_not_session_denominators() -> TestResult {
    let mut e = engine(vec![
        phase(
            ExerciseKind::Words,
            Phase::AdaptiveWords,
            10,
            &["cat", "dog"],
        )?,
        phase(ExerciseKind::Numpad, Phase::Numpad, 20, &["12"])?,
    ])?;
    key(&mut e, 0, Key::Enter)?;
    for (at, c) in [
        (1_000_000, 'c'),
        (2_000_000, 'a'),
        (3_000_000, 't'),
        (4_000_000, 'd'),
        (5_000_000, 'o'),
    ] {
        content(&mut e, at, c)?;
    }
    assert!(
        matches!(e.prepare(MonoTick::from_us(5_000_000)).screen,ScreenView::Practice {status,..} if status.rate.as_deref()==Some("12.0"))
    );
    content(&mut e, 6_000_000, 'g')?;
    send(&mut e, 11_000_000, EventKind::Tick)?;
    key(&mut e, 30_000_000, Key::Enter)?;
    content(&mut e, 40_000_000, '1')?;
    content(&mut e, 41_000_000, '2')?;
    key(&mut e, 42_000_000, Key::Enter)?;
    send(&mut e, 60_000_000, EventKind::Tick)?;
    key(&mut e, 61_000_000, Key::Enter)?;
    let ScreenView::Summary { lines } = e.prepare(MonoTick::from_us(61_000_000)).screen else {
        return Err("summary expected".into());
    };
    assert!(lines.iter().any(|s| s.contains("correct wpm 7.2")));
    assert!(lines.iter().any(|s| s.contains("correct digits/min 6.0")));
    Ok(())
}
#[test]
fn stale_success_and_failure_keys_cannot_release_final_quit() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    let initial = checkpoint(key(&mut e, 0, Key::Enter)?)?;
    ack(&mut e, 1, &initial)?;
    content(&mut e, 2, 'c')?;
    let finish = key(&mut e, 3, Key::Interrupt)?;
    assert!(!finish.quit);
    let final_save = checkpoint(finish)?;
    for failure in [false, true] {
        let mut wrong = final_save.key;
        wrong.digest = ContentDigest::from_bytes([9; 32]);
        let t = send(
            &mut e,
            4,
            EventKind::SaveResult(SaveResult {
                key: wrong,
                result: if failure {
                    Err("stale".to_string())
                } else {
                    Ok(())
                },
            }),
        )?;
        assert!(!t.quit);
        assert!(e.prepare(MonoTick::from_us(4)).saving);
        assert!(!e.prepare(MonoTick::from_us(4)).save_failed);
    }
    assert!(ack(&mut e, 5, &final_save)?.quit);
    Ok(())
}
#[test]
fn pending_checkpoint_retains_dirty_tail_and_retry_uses_fresh_full_key() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    let first = checkpoint(key(&mut e, 0, Key::Enter)?)?;
    content(&mut e, 1, 'c')?;
    content(&mut e, 2, 'x')?;
    key(&mut e, 3, Key::Backspace)?;
    content(&mut e, 4, 'a')?;
    content(&mut e, 5, 't')?;
    let finish = key(&mut e, 6, Key::Interrupt)?;
    assert!(!finish.quit);
    assert!(finish.checkpoint.is_none());
    let final_save = checkpoint(ack(&mut e, 7, &first)?)?;
    let track = final_save.progress.tracks.first().ok_or("track expected")?;
    assert_eq!(
        (
            track.content_attempts,
            track.correct_attempts,
            track.first_correct,
            track.first_attempted,
            track.deletions
        ),
        (4, 3, 2, 3, 1)
    );
    send(
        &mut e,
        8,
        EventKind::SaveResult(SaveResult {
            key: final_save.key,
            result: Err("disk full".to_string()),
        }),
    )?;
    assert!(e.prepare(MonoTick::from_us(8)).save_failed);
    let retry = checkpoint(content(&mut e, 9, 'r')?)?;
    assert_eq!(retry.progress, final_save.progress);
    assert_ne!(retry.key.sequence, final_save.key.sequence);
    assert!(!ack(&mut e, 10, &final_save)?.quit);
    assert!(ack(&mut e, 11, &retry)?.quit);
    Ok(())
}
#[test]
fn explicit_second_interrupt_after_failure_is_unsuccessful_exit() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    let first = checkpoint(key(&mut e, 0, Key::Enter)?)?;
    ack(&mut e, 1, &first)?;
    let final_save = checkpoint(key(&mut e, 2, Key::Interrupt)?)?;
    send(
        &mut e,
        3,
        EventKind::SaveResult(SaveResult {
            key: final_save.key,
            result: Err("disk full".to_string()),
        }),
    )?;
    let exit = key(&mut e, 4, Key::Interrupt)?;
    assert!(exit.quit && exit.exit_failed);
    Ok(())
}
#[test]
fn overdue_acknowledgement_invalidates_instead_of_invisibly_scoring() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1, 'c')?;
    assert!(matches!(
        send(&mut e, 2_000_001, EventKind::Tick)?.outcome,
        DomainOutcome::MeasurementInvalid { .. }
    ));
    assert_eq!(e.input_owner(), InputOwner::Navigation);
    Ok(())
}

#[test]
fn repeated_typing_and_checkpoint_ack_do_not_duplicate_counts() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    let first = checkpoint(key(&mut e, 0, Key::Enter)?)?;
    ack(&mut e, 1, &first)?;
    for _ in 0..100 {
        content(&mut e, 2, 'c')?;
        key(&mut e, 2, Key::Backspace)?;
    }
    content(&mut e, 3, 'c')?;
    let paused = checkpoint(key(&mut e, 4, Key::Escape)?)?;
    let track = paused.progress.tracks.first().ok_or("track expected")?;
    assert_eq!(
        (
            track.content_attempts,
            track.correct_attempts,
            track.deletions,
            track.first_attempted,
            track.correct_units
        ),
        (101, 101, 100, 1, 1)
    );
    ack(&mut e, 5, &paused)?;
    let final_save = checkpoint(key(&mut e, 6, Key::Interrupt)?)?;
    assert_eq!(final_save.progress.tracks, paused.progress.tracks);
    Ok(())
}

#[test]
fn storage_ready_and_resize_do_not_start_or_invent_a_session() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    let resized = send(
        &mut e,
        1,
        EventKind::Resized {
            columns: 80,
            rows: 24,
        },
    )?;
    assert!(resized.checkpoint.is_none());
    let exit = key(&mut e, 2, Key::Interrupt)?;
    assert!(exit.quit);
    assert!(exit.checkpoint.is_none());
    assert_eq!(e.prepare(MonoTick::from_us(2)).active.as_us(), 0);
    Ok(())
}

#[test]
fn late_success_ack_is_consumed_even_when_watchdog_invalidates() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    let first = checkpoint(key(&mut e, 0, Key::Enter)?)?;
    content(&mut e, 1, 'c')?;
    let late = ack(&mut e, 3_000_000, &first)?;
    assert!(matches!(
        late.outcome,
        DomainOutcome::MeasurementInvalid { .. }
    ));
    let replacement = checkpoint(late)?;
    assert_ne!(replacement.key, first.key);
    ack(&mut e, 3_000_001, &replacement)?;
    let final_save = checkpoint(key(&mut e, 3_000_002, Key::Interrupt)?)?;
    assert!(ack(&mut e, 3_000_003, &final_save)?.quit);
    Ok(())
}

#[test]
fn live_numeric_rate_excludes_decimal_units() -> TestResult {
    let mut e = engine(vec![phase(
        ExerciseKind::Numpad,
        Phase::Numpad,
        30,
        &["12.34"],
    )?])?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1_000_000, '1')?;
    content(&mut e, 2_000_000, '2')?;
    content(&mut e, 3_000_000, '.')?;
    let ScreenView::Practice { status, .. } = e.prepare(MonoTick::from_us(3_000_000)).screen else {
        return Err("numpad must still await submission".into());
    };
    assert_eq!(
        status.rate_unit,
        cadence_engine::view::RateUnit::DigitsPerMinute
    );
    assert_eq!(status.rate.as_deref(), Some("60.0"));
    Ok(())
}
