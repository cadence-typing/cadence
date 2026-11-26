//! Shared-path render tests (VR-01 basis, U-02 totality).
//!
//! The same engine reducer and painter produce cells and native row metadata
//! for the live presenter and future headless driver. Paint remains total
//! for every nonzero bounded viewport, including 1×1.

// Test assertions may use expect/unwrap/panic/indexing to fail loudly;
// the workspace denies them in production code only.
#![cfg(test)]

use cadence_core::{EventSeq, LayoutSupportsPound, MonoTick, TrainingChar};
use cadence_engine::view::LineRendition;
use cadence_engine::{
    DomainOutcome, Engine, EventEnvelope, EventKind, InputOwner, Key, ScreenView, UnitSpan,
};
use cadence_tui::paint::paint;
use cadence_tui::theme::Palette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn planned_engine(text: &str) -> Result<Engine, Box<dyn std::error::Error>> {
    let units = cadence_core::validate_target(text, LayoutSupportsPound::US)?;
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
        },
        phases: vec![cadence_trainer::PlannedPhase {
            phase: cadence_trainer::Phase::AdaptiveWords,
            seconds: 60,
            exercises: vec![cadence_trainer::PlannedExercise {
                units,
                kind: cadence_trainer::ExerciseKind::Words,
                words: Vec::new(),
                passage: None,
                caption: None,
                target: None,
            }],
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
        TrainingChar::new(character, LayoutSupportsPound::US)
            .expect("fixture characters are in the alphabet"),
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
