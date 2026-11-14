//! Session flow (plan 0004): pauses for external reasons, ending early into
//! the review, refused over-bound input, the interlude grace and comparison.
mod support;
use cadence_core::MonoTick;
use cadence_core::progress::{HistorySummary, SessionRecap, Track, TrackProgress};
use cadence_engine::view::ViewMode;
use cadence_engine::{DomainOutcome, EventKind, Key, PauseReason, ScreenView, UnitSpan};
use cadence_trainer::{ExerciseKind, Phase};
use support::*;

#[test]
fn paste_and_focus_loss_pause_without_invalidating() -> TestResult {
    for (event, reason) in [
        (EventKind::PasteRejected, PauseReason::PasteRejected),
        (EventKind::FocusLost, PauseReason::FocusLost),
    ] {
        let mut e = words()?;
        key(&mut e, 0, Key::Enter)?;
        content(&mut e, 1, 'c')?;
        assert_eq!(send(&mut e, 2, event)?.outcome, DomainOutcome::None);
        let view = e.prepare(MonoTick::from_us(2));
        assert_eq!(view.mode, ViewMode::Paused);
        assert_eq!(view.pause_reason, Some(reason));
        // Paused keys are not scored, and the line is kept.
        content(&mut e, 3, 'a')?;
        assert!(
            matches!(e.prepare(MonoTick::from_us(3)).screen,ScreenView::Practice {spans,paused:true,..} if spans.get(1)==Some(&UnitSpan::Cursor {expected:'a'}))
        );
        key(&mut e, 4, Key::Escape)?;
        content(&mut e, 5, 'a')?;
        let done = content(&mut e, 6, 't')?;
        assert!(matches!(
            done.outcome,
            DomainOutcome::ExerciseCompleted {
                first_pass: Some((3, 3)),
                ..
            }
        ));
        assert_eq!(e.prepare(MonoTick::from_us(6)).pause_reason, None);
    }
    Ok(())
}
#[test]
fn ending_early_opens_the_review_and_saves_as_unfinished() -> TestResult {
    for via_pause in [true, false] {
        let mut e = words()?;
        ready(&mut e)?;
        let mut at = 10;
        let t = key(&mut e, at, Key::Enter)?;
        settle(&mut e, &mut at, t)?;
        for c in ['c', 'x'] {
            at = at.saturating_add(500_000);
            let t = content(&mut e, at, c)?;
            settle(&mut e, &mut at, t)?;
        }
        at = at.saturating_add(500_000);
        let ended = if via_pause {
            let t = key(&mut e, at, Key::Escape)?;
            settle(&mut e, &mut at, t)?;
            assert_eq!(
                e.prepare(MonoTick::from_us(at)).pause_reason,
                Some(PauseReason::Requested)
            );
            content(&mut e, at, 'e')?
        } else {
            key(&mut e, at, Key::Interrupt)?
        };
        assert_eq!(ended.outcome, DomainOutcome::SessionAbandoned);
        assert!(!ended.quit);
        let (saved, quit) = settle(&mut e, &mut at, ended)?;
        assert!(!quit);
        assert_eq!(
            saved.ok_or("ending early saves")?.progress.status,
            cadence_core::progress::ProgressStatus::Abandoned
        );
        let review = e.prepare(MonoTick::from_us(at));
        assert_eq!(review.mode, ViewMode::Review);
        assert!(review.review.ended_early);
        // The mistake stays counted: 1 of 2 first attempts correct.
        let words = review
            .summary_reports
            .first()
            .ok_or("track evidence missing")?;
        assert_eq!((words.first_correct, words.first_attempted), (1, 2));
        // Enter on the review saves and exits after the final acknowledgement.
        let exit = key(&mut e, at, Key::Enter)?;
        assert!(settle(&mut e, &mut at, exit)?.1);
        assert!(e.recap().is_some_and(|line| line.contains("ended early")));
    }
    Ok(())
}
#[test]
fn input_past_the_extra_bound_is_refused_not_fatal() -> TestResult {
    let mut e = words()?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1, 'x')?;
    for at in 2..400 {
        content(&mut e, at, 'z')?;
    }
    let view = e.prepare(MonoTick::from_us(400));
    assert_eq!(view.mode, ViewMode::Typing);
    assert!(
        matches!(&view.screen, ScreenView::Practice { spans, .. } if spans.iter().any(|s| matches!(s, UnitSpan::Extra { .. })))
    );
    // Correcting back to the error and finishing still completes the exercise.
    for at in 400..800 {
        key(&mut e, at, Key::Backspace)?;
    }
    content(&mut e, 800, 'c')?;
    content(&mut e, 801, 'a')?;
    let done = content(&mut e, 802, 't')?;
    assert!(matches!(
        done.outcome,
        DomainOutcome::ExerciseCompleted { .. }
    ));
    Ok(())
}
#[test]
fn an_in_flight_key_cannot_skip_the_phase_complete_screen() -> TestResult {
    let mut e = engine(vec![
        phase(ExerciseKind::Words, Phase::AdaptiveWords, 1, &["cat"])?,
        phase(ExerciseKind::Numpad, Phase::Numpad, 20, &["12"])?,
    ])?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 10, 'c')?;
    assert!(matches!(
        send(&mut e, 1_000_100, EventKind::Tick)?.outcome,
        DomainOutcome::PhaseDeadline { .. }
    ));
    key(&mut e, 1_200_000, Key::Enter)?;
    assert_eq!(e.prepare(MonoTick::from_us(1_200_000)).mode, ViewMode::Next);
    assert!(matches!(
        key(&mut e, 1_900_000, Key::Enter)?.outcome,
        DomainOutcome::PhaseStarted { .. }
    ));
    Ok(())
}
#[test]
fn review_compares_only_with_the_same_track_set() -> TestResult {
    let words_only = TrackProgress {
        first_correct: 90,
        first_attempted: 100,
        correct_units: 300,
        active_micros: 60_000_000,
        ..TrackProgress::empty(Track::Words)
    };
    let numpad = TrackProgress::empty(Track::Numpad);
    let history = HistorySummary {
        completed_sessions: 2,
        recent: vec![
            SessionRecap {
                day: 122,
                completed: true,
                tracks: vec![words_only.clone(), numpad],
            },
            SessionRecap {
                day: 120,
                completed: false,
                tracks: vec![words_only],
            },
        ],
        ..HistorySummary::default()
    };
    let mut e = words()?;
    ready_with(&mut e, history)?;
    let today = e.prepare(MonoTick::from_us(1));
    assert_eq!(today.last_session.map(|b| b.days_ago), Some(1));
    let mut at = 10;
    let t = key(&mut e, at, Key::Enter)?;
    settle(&mut e, &mut at, t)?;
    for c in ['c', 'a', 't'] {
        at = at.saturating_add(300_000);
        let t = content(&mut e, at, c)?;
        settle(&mut e, &mut at, t)?;
    }
    let t = key(&mut e, at, Key::Interrupt)?;
    settle(&mut e, &mut at, t)?;
    let review = e.prepare(MonoTick::from_us(at)).review;
    // The words+numpad session is newer but not like-for-like.
    assert_eq!(review.session.map(|b| b.days_ago), Some(3));
    // Per track, the newest session that had the track is used.
    assert_eq!(
        review.tracks.first().map(|b| (b.days_ago, b.tracks.len())),
        Some((1, 1))
    );
    Ok(())
}
