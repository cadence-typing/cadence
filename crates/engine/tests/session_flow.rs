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
                layout: None,
                content_class: cadence_core::progress::ContentClass::Standard,
            },
            SessionRecap {
                day: 120,
                completed: false,
                tracks: vec![words_only],
                layout: None,
                content_class: cadence_core::progress::ContentClass::Standard,
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

/// #77: results never show a delta between sessions typed on different
/// layouts, and the start-screen recap names only this layout's sessions.
#[test]
fn review_never_compares_across_layouts() -> TestResult {
    let words_only = TrackProgress {
        first_correct: 90,
        first_attempted: 100,
        correct_units: 300,
        active_micros: 60_000_000,
        ..TrackProgress::empty(Track::Words)
    };
    let on = |layout, day| SessionRecap {
        day,
        completed: true,
        tracks: vec![words_only.clone()],
        layout: Some(layout),
        content_class: cadence_core::progress::ContentClass::Standard,
    };
    for (history, expected) in [
        (vec![on(cadence_core::Layout::Us, 122)], None),
        (
            vec![
                on(cadence_core::Layout::Us, 122),
                on(cadence_core::Layout::Uk, 120),
            ],
            Some(3),
        ),
    ] {
        let history = HistorySummary {
            completed_sessions: 1,
            recent: history,
            ..HistorySummary::default()
        };
        let mut e = words()?.with_layout(
            cadence_core::Layout::Uk,
            cadence_core::progress::ContentClass::Standard,
        );
        ready_with(&mut e, history)?;
        let today = e.prepare(MonoTick::from_us(1));
        assert_eq!(today.last_session.map(|b| b.days_ago), expected);
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
        assert_eq!(review.session.map(|b| b.days_ago), expected);
        assert_eq!(review.tracks.first().map(|b| b.days_ago), expected);
    }
    Ok(())
}

/// #60: a review row never makes sessions incomparable or enters the
/// comparison: an earlier session with a closing review compares with
/// today's on the shared tracks, and the start-screen recap omits it.
#[test]
fn a_closing_review_row_does_not_break_like_for_like() -> TestResult {
    let words_only = TrackProgress {
        first_correct: 90,
        first_attempted: 100,
        correct_units: 300,
        active_micros: 60_000_000,
        ..TrackProgress::empty(Track::Words)
    };
    let review = TrackProgress {
        first_correct: 10,
        first_attempted: 10,
        correct_units: 40,
        active_micros: 5_000_000,
        ..TrackProgress::empty(Track::Review)
    };
    let history = HistorySummary {
        completed_sessions: 1,
        recent: vec![SessionRecap {
            day: 122,
            completed: true,
            tracks: vec![words_only, review],
            layout: None,
            content_class: cadence_core::progress::ContentClass::Standard,
        }],
        ..HistorySummary::default()
    };
    let mut e = words()?;
    ready_with(&mut e, history)?;
    let today = e.prepare(MonoTick::from_us(1));
    let recap = today.last_session.ok_or("recap expected")?;
    assert!(recap.tracks.iter().all(|t| t.track != Track::Review));
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
    let context = e.prepare(MonoTick::from_us(at)).review;
    let session = context.session.ok_or("like-for-like expected")?;
    assert_eq!(session.days_ago, 1);
    assert!(session.tracks.iter().all(|t| t.track != Track::Review));
    Ok(())
}

/// #8: retired due words are named once, on the first session planned with
/// a changed corpus.
#[test]
fn retired_due_words_are_named_once() -> TestResult {
    for (latest, shown) in [(Some("an older corpus".to_string()), true), (None, false)] {
        let mut plan_phases = vec![phase(
            cadence_trainer::ExerciseKind::Words,
            cadence_trainer::Phase::AdaptiveWords,
            10,
            &["cat dog"],
        )?];
        let mut e = cadence_engine::Engine::new_plan(cadence_trainer::SessionPlan {
            budget_seconds: 30,
            policy_digest: cadence_core::ContentDigest::from_bytes([1; 32]),
            corpus_digest: cadence_core::ContentDigest::from_bytes([2; 32]),
            record: cadence_core::learning::PlanRecord {
                seed: 7,
                mode: cadence_core::learning::PlannerMode::Adaptive,
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
            retired_due: 3,
            phases: std::mem::take(&mut plan_phases),
        })?;
        ready_with(
            &mut e,
            HistorySummary {
                latest_corpus_digest: latest,
                ..HistorySummary::default()
            },
        )?;
        let view = e.prepare(MonoTick::from_us(1));
        let named = format!("{view:?}").contains("3 due words left the vetted word pack");
        assert_eq!(named, shown);
    }
    Ok(())
}

/// #85: a pack session's recap never gives a standard session a baseline
/// (headline comparison), and a pack session shows its caption.
#[test]
fn pack_sessions_are_never_compared_with_standard_practice() -> TestResult {
    use cadence_core::progress::ContentClass;
    let words_only = TrackProgress {
        first_correct: 90,
        first_attempted: 100,
        correct_units: 300,
        active_micros: 60_000_000,
        ..TrackProgress::empty(Track::Words)
    };
    let recap = |class| SessionRecap {
        day: 122,
        completed: true,
        tracks: vec![words_only.clone()],
        layout: None,
        content_class: class,
    };
    let history = HistorySummary {
        completed_sessions: 1,
        recent: vec![recap(ContentClass::Imported), recap(ContentClass::Code)],
        ..HistorySummary::default()
    };
    let mut e = words()?;
    ready_with(&mut e, history)?;
    assert_eq!(e.prepare(MonoTick::from_us(1)).last_session, None);
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
    let view = e.prepare(MonoTick::from_us(at));
    assert_eq!(view.review.session, None);
    assert!(view.review.tracks.is_empty());
    assert_eq!(view.content_note, None, "a standard session has no caption");
    Ok(())
}

/// #85: in a code exercise the newline is a unit typed with Enter: the
/// exercise completes only when Enter is typed at that position; Enter in a
/// word exercise is never content, so standard scores are unchanged.
#[test]
fn a_code_newline_is_typed_with_enter() -> TestResult {
    let mut e = engine(vec![phase(
        ExerciseKind::Prose,
        Phase::Prose,
        60,
        &["a\nb"],
    )?])?;
    ready(&mut e)?;
    key(&mut e, 1, Key::Enter)?;
    content(&mut e, 2, 'a')?;
    assert!(
        matches!(content(&mut e, 3, 'b')?.outcome, DomainOutcome::None),
        "b where the newline is expected is a slip"
    );
    key(&mut e, 4, Key::Backspace)?;
    key(&mut e, 5, Key::Enter)?;
    let done = content(&mut e, 6, 'b')?;
    assert!(
        matches!(
            done.outcome,
            DomainOutcome::ExerciseCompleted {
                first_pass: Some((2, 3)),
                ..
            }
        ),
        "{:?}",
        done.outcome
    );
    // Words: Enter changes nothing.
    let mut w = words()?;
    ready(&mut w)?;
    key(&mut w, 1, Key::Enter)?;
    content(&mut w, 2, 'c')?;
    key(&mut w, 3, Key::Enter)?;
    content(&mut w, 4, 'a')?;
    let done = content(&mut w, 5, 't')?;
    assert!(matches!(
        done.outcome,
        DomainOutcome::ExerciseCompleted {
            first_pass: Some((3, 3)),
            ..
        }
    ));
    Ok(())
}
