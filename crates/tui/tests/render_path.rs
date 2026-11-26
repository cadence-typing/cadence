//! Shared-path render tests (VR-01 basis, U-02 totality).
//!
//! The same engine reducer and painter produce cells and native row metadata
//! for the live presenter and future headless driver. Paint remains total
//! for every nonzero bounded viewport, including 1×1.

// Test assertions may use expect/unwrap/panic/indexing to fail loudly;
// the workspace denies them in production code only.
#![cfg(test)]

use cadence_core::{EventSeq, Layout, MonoTick, TrainingChar};
use cadence_engine::view::LineRendition;
use cadence_engine::{
    DomainOutcome, Engine, EventEnvelope, EventKind, InputOwner, Key, ScreenView, UnitSpan,
};
use cadence_tui::paint::paint;
use cadence_tui::theme::Palette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn planned_engine(text: &str) -> Result<Engine, Box<dyn std::error::Error>> {
    let units = cadence_core::validate_target(text, Layout::Us)?;
    words_engine(vec![cadence_trainer::PlannedExercise {
        units,
        kind: cadence_trainer::ExerciseKind::Words,
        words: Vec::new(),
        passage: None,
        caption: None,
        target: None,
        spans: Vec::new(),
    }])
}

fn words_engine(
    exercises: Vec<cadence_trainer::PlannedExercise>,
) -> Result<Engine, Box<dyn std::error::Error>> {
    Ok(Engine::new_plan(cadence_trainer::SessionPlan {
        budget_seconds: 60,
        policy_digest: cadence_core::ContentDigest::from_bytes([1; 32]),
        corpus_digest: cadence_core::ContentDigest::from_bytes([2; 32]),
        record: cadence_core::learning::PlanRecord {
            seed: 0,
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
        retired_due: 0,
        phases: vec![cadence_trainer::PlannedPhase {
            phase: cadence_trainer::Phase::AdaptiveWords,
            seconds: 60,
            exercises,
        }],
    })?)
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
        TrainingChar::new(character, Layout::Us).expect("fixture characters are in the alphabet"),
    ))
}

#[test]
fn plan_journey_through_shared_reducer_and_paint() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = planned_engine("cat")?;
    let palette = Palette::cadence_dark();

    // Today screen renders at the reference viewport.
    let view = engine.prepare(MonoTick::ORIGIN);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 32));
    let area = buffer.area;
    let mut rows = vec![LineRendition::Normal; usize::from(area.height)];
    paint(&mut buffer, area, &view, palette, &mut rows);

    // Enter starts the drill; type c x Backspace a t (handover example).
    let mut seq = 0u64;
    let mut tick = 1_000_000u64;
    let send = |engine: &mut Engine, kind: EventKind, seq: &mut u64, tick: &mut u64| {
        *seq += 1;
        *tick += 500_000;
        engine.reduce(envelope(*seq, *tick, kind))
    };

    let transition = send(&mut engine, EventKind::Key(Key::Enter), &mut seq, &mut tick);
    assert!(transition.dirty);
    assert!(matches!(transition.outcome, DomainOutcome::SessionStarted));
    send(&mut engine, content('c'), &mut seq, &mut tick);
    send(&mut engine, content('x'), &mut seq, &mut tick);
    send(
        &mut engine,
        EventKind::Key(Key::Backspace),
        &mut seq,
        &mut tick,
    );
    send(&mut engine, content('a'), &mut seq, &mut tick);
    let done = send(&mut engine, content('t'), &mut seq, &mut tick);
    assert!(matches!(
        done.outcome,
        DomainOutcome::ExerciseCompleted {
            first_pass: Some((2, 3)),
            ..
        }
    ));
    // Prepared-list exhaustion cycles; only the exact deadline ends practice.
    assert!(matches!(
        engine.prepare(MonoTick::from_us(tick)).screen,
        ScreenView::Practice { .. }
    ));
    tick = 62_000_000;
    seq += 1;
    engine.reduce(envelope(seq, tick, EventKind::Tick));
    // The phase-complete offer renders first; Enter opens the summary.
    let mut view = engine.prepare(MonoTick::from_us(tick));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 32));
    let mut area = buffer.area;
    paint(&mut buffer, area, &view, palette, &mut rows);

    // A deliberate Enter, after the interlude's in-flight-key grace.
    tick += 1_000_000;
    send(&mut engine, EventKind::Key(Key::Enter), &mut seq, &mut tick);
    view = engine.prepare(MonoTick::from_us(tick));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 32));
    area = buffer.area;
    paint(&mut buffer, area, &view, palette, &mut rows);
    let report = view
        .summary_reports
        .first()
        .ok_or("summary evidence missing")?;
    assert_eq!((report.first_correct, report.first_attempted), (2, 3));

    // Summary owns navigation; Ctrl-C follows the explicit finish path.
    assert_eq!(engine.input_owner(), InputOwner::Navigation);
    let quit = send(
        &mut engine,
        EventKind::Key(Key::Interrupt),
        &mut seq,
        &mut tick,
    );
    assert!(quit.quit);
    Ok(())
}

#[test]
fn pause_resume_boundaries_visible_in_view() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = planned_engine("cat")?;
    engine.reduce(envelope(1, 1_000_000, EventKind::Key(Key::Enter)));
    engine.reduce(envelope(2, 2_000_000, content('c')));

    // Esc pauses at 3s; the prepared view says paused.
    engine.reduce(envelope(3, 3_000_000, EventKind::Key(Key::Escape)));
    let paused = engine.prepare(MonoTick::from_us(3_000_000));
    match &paused.screen {
        ScreenView::Practice { paused, status, .. } => {
            assert!(*paused);
            assert!(status.paused);
        }
        other => panic!("expected practice screen, got {other:?}"),
    }

    // Esc resumes at 7s: active = typing 2s..3s = 1s; the 4s pause is
    // excluded from active time (L-03).
    engine.reduce(envelope(4, 7_000_000, EventKind::Key(Key::Escape)));
    let resumed = engine.prepare(MonoTick::from_us(7_000_000));
    match &resumed.screen {
        ScreenView::Practice { status, .. } => {
            assert!(!status.paused);
            assert_eq!(status.active_seconds, 1);
        }
        other => panic!("expected practice screen, got {other:?}"),
    }
    Ok(())
}

#[test]
fn paint_respects_its_viewport_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = planned_engine("the quick brown fox")?;
    engine.reduce(envelope(1, 1_000_000, EventKind::Key(Key::Enter)));
    engine.reduce(envelope(2, 1_500_000, content('t')));
    engine.reduce(envelope(3, 2_000_000, content('x')));
    let view = engine.prepare(MonoTick::from_us(2_000_000));
    for (width, height) in [
        (1_u16, 1_u16),
        (59, 18),
        (60, 17),
        (60, 18),
        (100, 32),
        (120, 40),
    ] {
        let surrounding = Rect::new(0, 0, width.saturating_add(2), height.saturating_add(2));
        let viewport = Rect::new(1, 1, width, height);
        let mut buffer = Buffer::empty(surrounding);
        for cell in &mut buffer.content {
            cell.set_char('#');
        }
        let mut rows = vec![LineRendition::Normal; usize::from(surrounding.height)];
        paint(
            &mut buffer,
            viewport,
            &view,
            Palette::cadence_dark(),
            &mut rows,
        );
        for y in surrounding.top()..surrounding.bottom() {
            for x in surrounding.left()..surrounding.right() {
                if !viewport.contains((x, y).into()) {
                    assert_eq!(buffer.cell((x, y)).map(|c| c.symbol()), Some("#"));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn mistyped_units_show_typed_with_expected_available() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = planned_engine("cat")?;
    engine.reduce(envelope(1, 1_000_000, EventKind::Key(Key::Enter)));
    engine.reduce(envelope(2, 1_500_000, content('x')));
    let view = engine.prepare(MonoTick::from_us(2_000_000));
    match &view.screen {
        ScreenView::Practice { spans, .. } => {
            assert_eq!(
                spans[0],
                UnitSpan::Wrong {
                    typed: 'x',
                    expected: 'c'
                }
            );
            assert_eq!(spans[1], UnitSpan::Cursor { expected: 'a' });
            assert_eq!(spans[2], UnitSpan::Upcoming { expected: 't' });
        }
        other => panic!("expected practice screen, got {other:?}"),
    }
    Ok(())
}

#[test]
fn sequence_gaps_invalidate_without_scoring_input() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = planned_engine("cat")?;
    engine.reduce(envelope(1, 1_000_000, EventKind::Key(Key::Enter)));
    // Gap: skip sequence 2, send 3.
    let refused = engine.reduce(envelope(3, 2_000_000, content('c')));
    assert!(matches!(
        refused.outcome,
        DomainOutcome::MeasurementInvalid { .. }
    ));
    assert_eq!(engine.consumed_events(), EventSeq::from_raw(1));
    // The drill did not record the ghost input.
    let view = engine.prepare(MonoTick::from_us(2_000_000));
    match &view.screen {
        ScreenView::Practice { spans, .. } => {
            assert_eq!(spans[0], UnitSpan::Cursor { expected: 'c' });
        }
        other => panic!("expected practice screen, got {other:?}"),
    }
    Ok(())
}

fn screen_text(buffer: &Buffer) -> String {
    let area = buffer.area;
    (area.y..area.bottom())
        .map(|y| {
            (area.x..area.right())
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render(engine: &Engine, at: u64, columns: u16, rows: u16) -> String {
    let view = engine.prepare(MonoTick::from_us(at));
    let mut buffer = Buffer::empty(Rect::new(0, 0, columns, rows));
    let area = buffer.area;
    let mut renditions = vec![LineRendition::Normal; usize::from(area.height)];
    paint(
        &mut buffer,
        area,
        &view,
        Palette::cadence_dark(),
        &mut renditions,
    );
    screen_text(&buffer)
}

#[test]
fn pause_controls_hide_the_target_and_name_the_reason() -> Result<(), Box<dyn std::error::Error>> {
    for (columns, rows) in [(100, 30), (80, 24), (60, 18)] {
        let mut engine = planned_engine("quiet harbour")?;
        engine.reduce(envelope(1, 1_000, EventKind::Key(Key::Enter)));
        engine.reduce(envelope(2, 2_000, content('q')));
        engine.reduce(envelope(3, 3_000, EventKind::FocusLost));
        let screen = render(&engine, 3_000, columns, rows);
        assert!(screen.contains("Paused"), "{columns}x{rows}:\n{screen}");
        assert!(screen.contains("lost focus"), "{columns}x{rows}:\n{screen}");
        assert!(screen.contains("end session & review"), "{screen}");
        assert!(
            !screen.contains("harbour"),
            "target readable while paused:\n{screen}"
        );
        assert!(!screen.contains("INVALID"));
    }
    Ok(())
}

#[test]
fn review_names_slips_and_end_state_at_every_size() -> Result<(), Box<dyn std::error::Error>> {
    for (columns, rows) in [(100, 30), (80, 24), (60, 18)] {
        let mut engine = planned_engine("the theme then")?;
        let mut seq = 0;
        let mut at = 1_000;
        let mut send = |engine: &mut Engine, kind: EventKind| {
            seq += 1;
            at += 200_000;
            engine.reduce(envelope(seq, at, kind));
        };
        send(&mut engine, EventKind::Key(Key::Enter));
        for c in ['t', 'g'] {
            send(&mut engine, content(c));
        }
        send(&mut engine, EventKind::Key(Key::Interrupt));
        let screen = render(&engine, at, columns, rows);
        assert!(screen.contains("Session review"), "{screen}");
        assert!(screen.contains("Save & exit"), "{screen}");
        if rows >= 24 {
            assert!(
                screen.contains("ended early"),
                "{columns}x{rows}:\n{screen}"
            );
            assert!(
                screen.contains("Most slips: th 1/1"),
                "{columns}x{rows}:\n{screen}"
            );
            assert!(screen.contains("first time"), "{columns}x{rows}:\n{screen}");
        }
    }
    Ok(())
}

/// #60: the closing review has its own row with its own accuracy and pace
/// and the note "not in the headline"; the headline tiles do not move.
#[test]
fn the_closing_review_row_is_not_in_the_headline() -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::progress::{Track, TrackProgress};
    for (columns, rows) in [(60, 18), (100, 32)] {
        let mut engine = planned_engine("the theme then")?;
        let mut seq = 0;
        let mut at = 1_000;
        let mut send = |engine: &mut Engine, kind: EventKind| {
            seq += 1;
            at += 200_000;
            engine.reduce(envelope(seq, at, kind));
        };
        send(&mut engine, EventKind::Key(Key::Enter));
        for c in "the the".chars() {
            send(&mut engine, content(c));
        }
        send(&mut engine, EventKind::Key(Key::Interrupt));
        let paint_view = |view: &cadence_engine::PreparedView| {
            let mut buffer = Buffer::empty(Rect::new(0, 0, columns, rows));
            let area = buffer.area;
            let mut renditions = vec![LineRendition::Normal; usize::from(area.height)];
            paint(
                &mut buffer,
                area,
                view,
                Palette::cadence_dark(),
                &mut renditions,
            );
            screen_text(&buffer)
        };
        let view = engine.prepare(MonoTick::from_us(at));
        let without = paint_view(&view);
        let mut with = view.clone();
        with.summary_reports.push(TrackProgress {
            first_correct: 3,
            first_attempted: 3,
            correct_attempts: 3,
            content_attempts: 3,
            correct_units: 3,
            active_micros: 1_000_000,
            ..TrackProgress::empty(Track::Review)
        });
        let screen = paint_view(&with);
        assert!(screen.contains("review"), "{columns}x{rows}:\n{screen}");
        assert!(
            screen.contains("not in the headline"),
            "{columns}x{rows}:\n{screen}"
        );
        assert!(screen.contains("100.0%"), "{columns}x{rows}:\n{screen}");
        // The headline tiles (above the table) are unchanged.
        let head = |s: &str| {
            s.lines()
                .take_while(|l| !l.contains("Track"))
                .filter(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(head(&without), head(&screen), "{columns}x{rows}");
    }
    Ok(())
}

fn slotted(
    slots: &[(&str, u64, cadence_trainer::Lane)],
) -> Result<cadence_trainer::PlannedExercise, Box<dyn std::error::Error>> {
    let text: Vec<&str> = slots.iter().map(|(word, _, _)| *word).collect();
    Ok(cadence_trainer::PlannedExercise {
        units: cadence_core::validate_target(&text.join(" "), Layout::Us)?,
        kind: cadence_trainer::ExerciseKind::Words,
        words: slots
            .iter()
            .map(|(_, id, lane)| {
                Ok(cadence_trainer::PlannedWord {
                    id: cadence_core::WordId::new(*id)?,
                    lane: *lane,
                    focus: None,
                    due_review: false,
                })
            })
            .collect::<Result<_, cadence_core::CoreError>>()?,
        passage: None,
        caption: None,
        target: None,
        spans: Vec::new(),
    })
}

/// #7: each label ("again: cat", "repeat: dog", "repeat · list
/// restarted") shows whole at 60x18.
#[test]
fn repeat_labels_show_whole_at_the_smallest_size() -> Result<(), Box<dyn std::error::Error>> {
    use cadence_trainer::Lane::{Broad, Review, Target};
    let mut engine = words_engine(vec![
        slotted(&[("cat", 1, Target), ("dog", 2, Broad)])?,
        slotted(&[("sun", 3, Target), ("hat", 4, Review)])?,
        slotted(&[("map", 5, Target), ("dog", 2, Broad)])?,
    ])?;
    let mut seq = 0;
    let mut at = 1_000;
    let mut send = |engine: &mut Engine, kind: EventKind| -> u64 {
        seq += 1;
        at += 100_000;
        engine.reduce(envelope(seq, at, kind));
        at
    };
    send(&mut engine, EventKind::Key(Key::Enter));
    let mut keys = |engine: &mut Engine, text: &str| -> u64 {
        let mut last = 0;
        for c in text.chars() {
            last = if c == '#' {
                send(engine, content('z'));
                send(engine, EventKind::Key(Key::Backspace))
            } else {
                send(engine, content(c))
            };
        }
        last
    };
    let mut captions = Vec::new();
    for (typed, wanted) in [
        ("c#at dog", "again: cat"),
        ("sun cat", "repeat: dog"),
        ("map dog", "repeat · list restarted"),
    ] {
        let now = keys(&mut engine, typed);
        let screen = render(&engine, now, 60, 18);
        assert!(screen.contains(wanted), "60x18 lacks {wanted:?}:\n{screen}");
        captions.push(wanted);
    }
    assert_eq!(captions.len(), 3);
    Ok(())
}

fn plain(
    text: &str,
    kind: cadence_trainer::ExerciseKind,
) -> Result<cadence_trainer::PlannedExercise, Box<dyn std::error::Error>> {
    Ok(cadence_trainer::PlannedExercise::plain(
        cadence_core::validate_target(text, Layout::Us)?,
        kind,
    ))
}

/// A plan with the given phases and focus skills (adaptive when any).
fn targeted_engine(
    phases: &[cadence_trainer::Phase],
    focus: &[(cadence_core::learning::KeyLane, &str)],
) -> Result<Engine, Box<dyn std::error::Error>> {
    use cadence_core::learning::{PlannerMode, Skill, SkillKey};
    use cadence_trainer::{ExerciseKind, Phase};
    let focus = focus
        .iter()
        .map(|(lane, text)| {
            let units: Vec<TrainingChar> = text
                .chars()
                .map(|c| TrainingChar::new(c, Layout::Us))
                .collect::<Result<_, _>>()?;
            let skill = match units.as_slice() {
                [one] => Skill::Unit(*one),
                [first, second] => Skill::Bigram(*first, *second),
                _ => return Err("one or two units".into()),
            };
            Ok(SkillKey { lane: *lane, skill })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let planned = phases
        .iter()
        .map(|phase| {
            let exercise = match phase {
                Phase::NumberRow => plain("room 42", ExerciseKind::NumberRow)?,
                Phase::Numpad => plain("12", ExerciseKind::Numpad)?,
                _ => plain("the then", ExerciseKind::Words)?,
            };
            Ok(cadence_trainer::PlannedPhase {
                phase: *phase,
                seconds: 60,
                exercises: vec![exercise],
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    Ok(Engine::new_plan(cadence_trainer::SessionPlan {
        budget_seconds: 180,
        policy_digest: cadence_core::ContentDigest::from_bytes([1; 32]),
        corpus_digest: cadence_core::ContentDigest::from_bytes([2; 32]),
        record: cadence_core::learning::PlanRecord {
            seed: 0,
            mode: if focus.is_empty() {
                PlannerMode::ColdStart
            } else {
                PlannerMode::Adaptive
            },
            focus,
        },
        adaptation: cadence_trainer::Adaptation {
            hesitation_ceiling: cadence_core::Micros::from_us(1_500_000),
            reinforcement_capacity: 8,
            reinforcements_per_word: 1,
            numeric_gap_entries: 0,
        },
        probe: cadence_trainer::ProbePlan::NotDue,
        pace: Vec::new(),
        retired_due: 0,
        phases: planned,
    })?)
}

fn line_with<'a>(screen: &'a str, needle: &str) -> Option<&'a str> {
    screen
        .lines()
        .find(|line| line.contains(needle))
        .map(str::trim)
}

/// #50: the start screen names targets per group, from the plan record;
/// nothing is cut mid-token at 60 columns: whole groups drop from the
/// right with "+N more".
#[test]
fn the_start_screen_names_targets_per_group() -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::learning::KeyLane::{Main, Numpad};
    use cadence_trainer::Phase::{AdaptiveWords, NumberRow, Numpad as NumpadPhase};
    let all = [AdaptiveWords, NumberRow, NumpadPhase];
    for (columns, rows) in [(60, 18), (100, 32)] {
        let cold = render(&targeted_engine(&all, &[])?, 0, columns, rows);
        assert!(
            cold.contains("No per-key history yet"),
            "{columns}x{rows}:\n{cold}"
        );

        let letters = render(
            &targeted_engine(&[AdaptiveWords], &[(Main, "th"), (Main, "ou")])?,
            0,
            columns,
            rows,
        );
        assert_eq!(
            line_with(&letters, "Today targets"),
            Some("Today targets  th · ou"),
            "{letters}"
        );

        let every = targeted_engine(
            &all,
            &[
                (Main, "th"),
                (Main, "ou"),
                (Main, "w"),
                (Main, "7"),
                (Main, "48"),
                (Numpad, "0."),
                (Numpad, "55"),
            ],
        )?;
        let screen = render(&every, 0, columns, rows);
        let line = line_with(&screen, "Today targets")
            .ok_or_else(|| format!("{columns}x{rows}:\n{screen}"))?;
        let full = "Today targets  th · ou · w   number row 7 · 48   numpad 0. · 55";
        if columns >= 100 {
            assert_eq!(line, full);
        } else {
            assert!(line.ends_with(" more"), "{line}");
            let shown = line.split("   +").next().unwrap_or_default();
            assert!(full.starts_with(shown), "cut mid-group: {line}");
        }
    }
    Ok(())
}

/// #50: a numpad-only session reports its numpad slips, never "letter
/// pairs"; the numpad purpose names its targets.
#[test]
fn a_numpad_only_session_reports_numpad_slips() -> Result<(), Box<dyn std::error::Error>> {
    use cadence_core::learning::KeyLane::Numpad;
    for (columns, rows) in [(60, 18), (100, 32)] {
        let mut engine = targeted_engine(&[cadence_trainer::Phase::Numpad], &[(Numpad, "12")])?;
        let today = render(&engine, 0, columns, rows);
        assert!(
            today.contains("Exact entries targeting 12"),
            "{columns}x{rows}:\n{today}"
        );
        let mut seq = 0;
        let mut at = 1_000;
        let mut send = |engine: &mut Engine, kind: EventKind| {
            seq += 1;
            at += 200_000;
            engine.reduce(envelope(seq, at, kind));
        };
        send(&mut engine, EventKind::Key(Key::Enter));
        send(&mut engine, content('1'));
        send(&mut engine, content('3'));
        send(&mut engine, EventKind::Key(Key::Backspace));
        send(&mut engine, content('2'));
        send(&mut engine, EventKind::Key(Key::Interrupt));
        let screen = render(&engine, at, columns, rows);
        assert!(!screen.contains("letter pairs"), "{screen}");
        if rows >= 24 {
            assert!(
                screen.contains("Most slips: numpad 12 1/1"),
                "{columns}x{rows}:\n{screen}"
            );
        }
    }
    Ok(())
}

fn goal_engine(
    moved: cadence_core::learning::PaceMove,
    target_cpm: Option<u64>,
) -> Result<Engine, Box<dyn std::error::Error>> {
    let units = cadence_core::validate_target("the theme then", Layout::Us)?;
    Ok(Engine::new_plan(cadence_trainer::SessionPlan {
        budget_seconds: 60,
        policy_digest: cadence_core::ContentDigest::from_bytes([1; 32]),
        corpus_digest: cadence_core::ContentDigest::from_bytes([2; 32]),
        record: cadence_core::learning::PlanRecord {
            seed: 0,
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
        pace: vec![cadence_core::learning::PaceRecord {
            track: cadence_core::progress::Track::Words,
            target_cpm,
            moved,
        }],
        retired_due: 0,
        phases: vec![cadence_trainer::PlannedPhase {
            phase: cadence_trainer::Phase::AdaptiveWords,
            seconds: 60,
            exercises: vec![cadence_trainer::PlannedExercise::plain(
                units,
                cadence_trainer::ExerciseKind::Words,
            )],
        }],
    })?)
}

/// #62: the start screen and the results name the words goal and how it
/// moved, whole at 60x18 and 100x32; a lowering explains itself in the
/// results; with no baseline the goal says so.
#[test]
fn the_pace_goal_is_named_on_the_start_screen_and_results() -> Result<(), Box<dyn std::error::Error>>
{
    use cadence_core::learning::PaceMove;
    for (columns, rows) in [(60, 18), (100, 32)] {
        let raised = render(&goal_engine(PaceMove::Raised, Some(260))?, 0, columns, rows);
        assert!(
            raised.contains("Words goal 52 WPM · raised"),
            "{columns}x{rows}:\n{raised}"
        );
        let none = render(&goal_engine(PaceMove::NoBaseline, None)?, 0, columns, rows);
        assert!(
            none.contains("Words goal: building your baseline"),
            "{columns}x{rows}:\n{none}"
        );
        let mut engine = goal_engine(PaceMove::Lowered, Some(190))?;
        let start = render(&engine, 0, columns, rows);
        assert!(
            start.contains("Words goal 38 WPM · lowered"),
            "{columns}x{rows}:\n{start}"
        );
        engine.reduce(envelope(1, 1_000, EventKind::Key(Key::Enter)));
        engine.reduce(envelope(2, 200_000, content('t')));
        engine.reduce(envelope(3, 400_000, EventKind::Key(Key::Interrupt)));
        let results = render(&engine, 400_000, columns, rows);
        if rows >= 24 {
            assert!(
                results.contains("Words goal 38 WPM · lowered: more familiar words next time"),
                "{columns}x{rows}:\n{results}"
            );
        }
        assert!(!results.contains("Words goal 38 WPM · lowered: more familiar wor\n"));
    }
    Ok(())
}

fn probe_engine() -> Result<Engine, Box<dyn std::error::Error>> {
    let units = cadence_core::validate_target("Probe text for measuring transfer.", Layout::Us)?;
    let passage = cadence_core::PassageId::new(77)?;
    let mut exercise =
        cadence_trainer::PlannedExercise::plain(units, cadence_trainer::ExerciseKind::Probe);
    exercise.passage = Some(passage);
    exercise.caption = Some("Transfer probe · unseen text · not practice".to_string());
    Ok(Engine::new_plan(cadence_trainer::SessionPlan {
        budget_seconds: 60,
        policy_digest: cadence_core::ContentDigest::from_bytes([1; 32]),
        corpus_digest: cadence_core::ContentDigest::from_bytes([2; 32]),
        record: cadence_core::learning::PlanRecord {
            seed: 0,
            mode: cadence_core::learning::PlannerMode::Adaptive,
            focus: Vec::new(),
        },
        adaptation: cadence_trainer::Adaptation {
            hesitation_ceiling: cadence_core::Micros::from_us(1_500_000),
            reinforcement_capacity: 8,
            reinforcements_per_word: 1,
            numeric_gap_entries: 0,
        },
        probe: cadence_trainer::ProbePlan::Planned(cadence_trainer::PlannedProbe {
            passage,
            strata: 1,
            same_strata: true,
        }),
        pace: Vec::new(),
        retired_due: 0,
        phases: vec![cadence_trainer::PlannedPhase {
            phase: cadence_trainer::Phase::Probe,
            seconds: 2,
            exercises: vec![exercise],
        }],
    })?)
}

/// #65: the start screen says today includes a probe; the probe is
/// captioned; the results name the probe and, with no earlier comparable
/// probe, say so; whole at 60x18 and 100x32; no efficacy claim.
#[test]
fn the_transfer_probe_is_named_and_measured() -> Result<(), Box<dyn std::error::Error>> {
    for (columns, rows) in [(60, 18), (100, 32)] {
        let mut engine = probe_engine()?;
        let start = render(&engine, 0, columns, rows);
        assert!(
            start.contains("Transfer probe today: unseen text, not practice"),
            "{columns}x{rows}:\n{start}"
        );
        engine.reduce(envelope(1, 1_000, EventKind::Key(Key::Enter)));
        let mut at = 1_000;
        for (seq, c) in "Probe text".chars().enumerate() {
            at += 100_000;
            engine.reduce(envelope(u64::try_from(seq)? + 2, at, content(c)));
        }
        let practice = render(&engine, at, columns, rows);
        assert!(
            practice.contains("Transfer probe · unseen text · not practice"),
            "{columns}x{rows}:\n{practice}"
        );
        engine.reduce(envelope(12, at + 3_000_000, EventKind::Tick));
        engine.reduce(envelope(13, at + 4_500_000, EventKind::Key(Key::Enter)));
        let results = render(&engine, at + 4_500_000, columns, rows);
        if rows >= 24 {
            assert!(
                results.contains("not enough comparable probes yet")
                    || results.contains("first comparable probe"),
                "{columns}x{rows}:\n{results}"
            );
        }
        for word in ["improve", "better", "faster", "gain"] {
            assert!(
                !results.to_lowercase().contains(word),
                "efficacy wording: {word}"
            );
        }
    }
    Ok(())
}

/// #79: the settings overlay renders at three sizes with its controls, and
/// the selected row stays visible when the list scrolls.
#[test]
fn the_settings_overlay_renders_at_three_sizes() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = planned_engine("cat")?;
    engine.reduce(envelope(1, 1, content('s')));
    let last = cadence_core::settings::KEYS.len();
    for seq in 2..=u64::try_from(last)? {
        engine.reduce(envelope(
            seq,
            seq,
            EventKind::Key(Key::Arrow(cadence_engine::Direction::Down)),
        ));
    }
    let view = engine.prepare(MonoTick::from_us(100));
    assert_eq!(view.settings.as_ref().map(|s| s.cursor), Some(last - 1));
    for (columns, rows) in [(60_u16, 18_u16), (80, 24), (100, 32)] {
        let mut buffer = Buffer::empty(Rect::new(0, 0, columns, rows));
        let area = buffer.area;
        let mut renditions = vec![LineRendition::Normal; usize::from(rows)];
        paint(
            &mut buffer,
            area,
            &view,
            Palette::cadence_dark(),
            &mut renditions,
        );
        let text = screen_text(&buffer);
        assert!(text.contains(" Settings "), "{columns}x{rows}\n{text}");
        assert!(text.contains("› high_contrast"), "{columns}x{rows}\n{text}");
        assert!(text.contains("applies now"), "{columns}x{rows}\n{text}");
        assert!(text.contains("Enter save"), "{columns}x{rows}\n{text}");
        assert!(
            renditions.iter().all(|r| *r == LineRendition::Normal),
            "no double-height rows under the overlay"
        );
    }
    Ok(())
}
