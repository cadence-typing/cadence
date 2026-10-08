//! Shared-path render tests (VR-01 basis, U-02 totality).
//!
//! The same engine reducer and the same paint function feed a Ratatui
//! `TestBackend`; this is the path the live terminal and the future
//! headless driver share. Paint must be total for every nonzero
//! bounded viewport including 1×1 — no panic, no out-of-bounds.

// Test assertions may use expect/unwrap/panic/indexing to fail loudly;
// the workspace denies them in production code only.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cadence_core::{EventSeq, LayoutSupportsPound, MonoTick, TrainingChar};
use cadence_engine::{
    DomainOutcome, Engine, EventEnvelope, EventKind, InputOwner, Key, ScreenView, UnitSpan,
};
use cadence_tui::paint::paint;
use cadence_tui::theme::Palette;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

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

fn buffer_text(buffer: &Buffer) -> String {
    buffer
        .content()
        .iter()
        .map(|cell| cell.symbol().to_string())
        .collect()
}

#[test]
fn slice_journey_through_shared_reducer_and_paint() {
    let mut engine = Engine::new_slice(vec!["cat".to_string()]).expect("valid target");
    let palette = Palette::cadence_dark();

    // Today screen renders at the reference viewport.
    let view = engine.prepare(MonoTick::ORIGIN);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 32));
    let area = buffer.area;
    paint(&mut buffer, area, &view, palette);
    assert!(buffer_text(&buffer).contains("today"));

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
    // The phase-complete offer renders first; Enter opens the summary.
    let mut view = engine.prepare(MonoTick::from_us(tick));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 32));
    let mut area = buffer.area;
    paint(&mut buffer, area, &view, palette);
    assert!(buffer_text(&buffer).contains("complete"));

    send(&mut engine, EventKind::Key(Key::Enter), &mut seq, &mut tick);
    view = engine.prepare(MonoTick::from_us(tick));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 32));
    area = buffer.area;
    paint(&mut buffer, area, &view, palette);
    let text = buffer_text(&buffer);
    assert!(
        text.contains("first-pass 2/3"),
        "summary shows honest first-pass: {text}"
    );

    // Esc returns to Today; q quits (navigation owns input again).
    send(
        &mut engine,
        EventKind::Key(Key::Escape),
        &mut seq,
        &mut tick,
    );
    assert_eq!(engine.input_owner(), InputOwner::Navigation);
    let quit = send(&mut engine, content('q'), &mut seq, &mut tick);
    assert!(quit.quit);
}

#[test]
fn pause_resume_boundaries_visible_in_view() {
    let mut engine = Engine::new_slice(vec!["cat".to_string()]).unwrap();
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
}

#[test]
fn paint_is_total_for_every_nonzero_viewport() {
    let mut engine = Engine::new_slice(vec!["the quick brown fox".to_string()]).unwrap();
    engine.reduce(envelope(1, 1_000_000, EventKind::Key(Key::Enter)));
    engine.reduce(envelope(2, 1_500_000, content('t')));
    engine.reduce(envelope(3, 2_000_000, content('x')));
    let view = engine.prepare(MonoTick::from_us(3_000_000));
    let palette = Palette::cadence_light();

    for (width, height) in [
        (1u16, 1u16),
        (2, 1),
        (1, 2),
        (59, 17), // one below the minimum full experience 60×18
        (60, 18), // minimum
        (99, 31),
        (100, 32), // reference
        (120, 40), // wide
        (240, 80), // frame schema maximum
    ] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| {
                let area = frame.area();
                paint(frame.buffer_mut(), area, &view, palette);
            })
            .expect("draw");
    }
}

#[test]
fn mistyped_units_show_typed_with_expected_available() {
    let mut engine = Engine::new_slice(vec!["cat".to_string()]).unwrap();
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
}

#[test]
fn sequence_gaps_are_refused_without_mutation() {
    let mut engine = Engine::new_slice(vec!["cat".to_string()]).unwrap();
    engine.reduce(envelope(1, 1_000_000, EventKind::Key(Key::Enter)));
    // Gap: skip sequence 2, send 3.
    let refused = engine.reduce(envelope(3, 2_000_000, content('c')));
    assert!(!refused.dirty);
    assert_eq!(engine.consumed_events(), EventSeq::from_raw(1));
    // The drill did not record the ghost input.
    let view = engine.prepare(MonoTick::from_us(2_000_000));
    match &view.screen {
        ScreenView::Practice { spans, .. } => {
            assert_eq!(spans[0], UnitSpan::Cursor { expected: 'c' });
        }
        other => panic!("expected practice screen, got {other:?}"),
    }
}
