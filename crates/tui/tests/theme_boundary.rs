//! #82: colours live only in `theme.rs`; with high contrast (the terminal's
//! own colours) every signal is also a modifier, so errors stay visible.
#![cfg(test)]
#![expect(
    clippy::arithmetic_side_effects,
    reason = "a test driver counting a few events and cells"
)]

use cadence_core::{EventSeq, Layout, MonoTick, TrainingChar};
use cadence_engine::view::{DisplayView, LineRendition};
use cadence_engine::{Engine, EventEnvelope, EventKind, Key};
use cadence_tui::paint::paint;
use cadence_tui::theme::Palette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

#[test]
fn no_colour_literal_outside_the_theme_module() -> Result<(), Box<dyn std::error::Error>> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name().is_some_and(|n| n == "theme.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            for literal in ["Color::", "Rgb(", "Color {"] {
                assert!(!text.contains(literal), "{}: {literal}", path.display());
            }
        }
    }
    Ok(())
}

fn practice_with_a_slip() -> Result<Engine, Box<dyn std::error::Error>> {
    let units = cadence_core::validate_target("cats", Layout::Us)?;
    let mut engine = Engine::new_plan(cadence_trainer::SessionPlan {
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
            exercises: vec![cadence_trainer::PlannedExercise {
                units,
                kind: cadence_trainer::ExerciseKind::Words,
                words: Vec::new(),
                passage: None,
                caption: None,
                target: None,
                spans: Vec::new(),
            }],
        }],
    })?;
    let mut seq = 0;
    let mut send = |engine: &mut Engine, kind: EventKind| {
        seq += 1;
        engine.reduce(EventEnvelope {
            seq: EventSeq::from_raw(seq),
            at: MonoTick::from_us(seq * 300_000),
            kind,
        });
    };
    send(
        &mut engine,
        EventKind::Resized {
            columns: 100,
            rows: 32,
        },
    );
    send(&mut engine, EventKind::Key(Key::Enter));
    for c in ['c', 'x'] {
        send(
            &mut engine,
            EventKind::Key(Key::Content(TrainingChar::new(c, Layout::Us)?)),
        );
    }
    Ok(engine)
}

/// With every colour the terminal's default, correct, wrong, caret and
/// upcoming text still differ by modifier.
#[test]
fn errors_stay_distinguishable_without_colour() -> Result<(), Box<dyn std::error::Error>> {
    let engine = practice_with_a_slip()?;
    let view = engine.prepare(MonoTick::from_us(10_000_000));
    let display = DisplayView {
        high_contrast: true,
        ..view.display.clone()
    };
    let palette = Palette::for_display(&display, Palette::cadence_dark());
    assert!(palette.high_contrast());
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 32));
    let area = buffer.area;
    let mut rows = vec![LineRendition::Normal; 32];
    paint(&mut buffer, area, &view, palette, &mut rows);
    let mut modifiers = std::collections::BTreeMap::new();
    for y in 0..32 {
        let line: String = (0..100).map(|x| buffer[(x, y)].symbol()).collect();
        // The mistyped cell shows what was typed: c x t s.
        if let Some(start) = line.find("cxts") {
            for (offset, unit) in "cats".chars().enumerate() {
                let x = u16::try_from(start + offset)?;
                let cell = &buffer[(x, y)];
                assert_eq!(cell.fg, ratatui::style::Color::Reset, "no colour at all");
                modifiers.insert(unit, cell.modifier);
            }
            break;
        }
    }
    let correct = modifiers.get(&'c').copied().ok_or("typed c")?;
    let wrong = modifiers.get(&'a').copied().ok_or("mistyped a")?;
    let caret = modifiers.get(&'t').copied().ok_or("caret on t")?;
    let ahead = modifiers.get(&'s').copied().ok_or("upcoming s")?;
    assert!(correct.contains(Modifier::BOLD), "typed text is bold");
    assert!(
        wrong.contains(Modifier::UNDERLINED),
        "an error is underlined"
    );
    assert!(caret.contains(Modifier::REVERSED) && !caret.contains(Modifier::UNDERLINED));
    assert_eq!(ahead, Modifier::empty());
    let all = [correct, wrong, caret, ahead];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b, "every state differs without colour");
        }
    }
    Ok(())
}
