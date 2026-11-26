//! #80: one command table for dispatch, hints, help and headless actions.
mod support;
use cadence_core::MonoTick;
use cadence_engine::commands::{
    Action, CommandState, Destination, KeyMatch, bindings_for, resolve,
};
use cadence_engine::view::{PauseReason, ViewMode};
use cadence_engine::{Direction, DomainOutcome, EventKind, Key, ScreenView, UnitSpan};
use cadence_trainer::Phase;
use support::*;

fn key_for(m: KeyMatch) -> Option<Key> {
    Some(match m {
        KeyMatch::Enter => Key::Enter,
        KeyMatch::Escape => Key::Escape,
        KeyMatch::Tab => Key::Tab,
        KeyMatch::BackTab => Key::BackTab,
        KeyMatch::Arrow(d) => Key::Arrow(d),
        KeyMatch::Char(c) => {
            Key::Content(cadence_core::TrainingChar::new(c, cadence_core::Layout::Us).ok()?)
        }
        KeyMatch::Interrupt | KeyMatch::F2 => return None,
    })
}

/// Every binding in every state is reachable by each of its keys, and the
/// first binding a key reaches is the one listed for it (no shadowing).
#[test]
fn every_command_is_reachable_by_its_keys() {
    let modes = [
        ViewMode::Ready,
        ViewMode::Typing,
        ViewMode::Paused,
        ViewMode::Next,
        ViewMode::Review,
        ViewMode::Saving,
        ViewMode::SaveError,
        ViewMode::Invalid,
    ];
    let reasons = [
        None,
        Some(PauseReason::Requested),
        Some(PauseReason::SavingSlow),
    ];
    for mode in modes {
        for destination in Destination::ALL {
            for reason in reasons {
                for (enter_submits, help) in [(false, false), (true, false), (false, true)] {
                    let state = CommandState {
                        mode,
                        enter_submits,
                        pause_reason: reason,
                        destination,
                        help,
                    };
                    for binding in bindings_for(state) {
                        for m in binding.matches {
                            if let Some(key) = key_for(*m) {
                                assert_eq!(
                                    resolve(bindings_for(state), key),
                                    Some(binding.action),
                                    "{state:?}: {} ({m:?})",
                                    binding.keys
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn help_lists_the_table_of_the_state_beneath() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    let today = e.prepare(MonoTick::from_us(1)).commands;
    content(&mut e, 1, '?')?;
    let view = e.prepare(MonoTick::from_us(1));
    assert_eq!(view.help, Some(today));
    assert!(view.commands.iter().all(|b| b.action == Action::Help));
    key(&mut e, 2, Key::Escape)?;
    assert_eq!(e.prepare(MonoTick::from_us(2)).help, None);
    Ok(())
}

#[test]
fn q_and_question_mark_are_content_while_typing() -> TestResult {
    let mut e = engine(vec![phase(
        cadence_trainer::ExerciseKind::Words,
        Phase::AdaptiveWords,
        60,
        &["q?a"],
    )?])?;
    ready(&mut e)?;
    key(&mut e, 1, Key::Enter)?;
    content(&mut e, 2, 'q')?;
    let t = content(&mut e, 3, '?')?;
    assert!(!t.quit);
    let view = e.prepare(MonoTick::from_us(3));
    assert!(view.help.is_none());
    assert!(matches!(
        view.screen,
        ScreenView::Practice { ref spans, .. } if spans.get(2) == Some(&UnitSpan::Cursor { expected: 'a' })
    ));
    Ok(())
}

#[test]
fn navigation_is_refused_while_running_and_paused() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    key(&mut e, 1, Key::Enter)?;
    key(&mut e, 2, Key::Tab)?;
    assert_eq!(
        e.prepare(MonoTick::from_us(2)).destination.selected,
        Destination::Today
    );
    assert!(!e.offers(Action::NextDestination));
    key(&mut e, 3, Key::Escape)?;
    key(&mut e, 4, Key::Tab)?;
    assert!(
        !e.offers(Action::NextDestination),
        "paused: only end-early leaves"
    );
    assert!(matches!(
        e.prepare(MonoTick::from_us(4)).screen,
        ScreenView::Practice { .. }
    ));
    assert_eq!(
        send(&mut e, 5, EventKind::Command(Action::NextDestination))?.outcome,
        DomainOutcome::None,
        "an unoffered command changes nothing"
    );
    assert!(matches!(
        content(&mut e, 6, 'e')?.outcome,
        DomainOutcome::SessionAbandoned
    ));
    Ok(())
}

#[test]
fn train_and_numpad_request_an_ordinary_single_track_plan() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    key(&mut e, 1, Key::Tab)?;
    assert_eq!(
        e.prepare(MonoTick::from_us(1)).destination.selected,
        Destination::Train
    );
    key(&mut e, 2, Key::Arrow(Direction::Down))?;
    key(&mut e, 3, Key::Arrow(Direction::Down))?;
    content(&mut e, 4, '+')?;
    let t = key(&mut e, 5, Key::Enter)?;
    assert_eq!(
        t.outcome,
        DomainOutcome::PlanRequested {
            phase: Phase::NumberRow,
            minutes: 10
        }
    );
    // Numpad is the next destination; Shift-Tab and Left go back.
    key(&mut e, 6, Key::Arrow(Direction::Right))?;
    assert_eq!(
        e.prepare(MonoTick::from_us(6)).destination.selected,
        Destination::Numpad
    );
    assert_eq!(
        key(&mut e, 7, Key::Enter)?.outcome,
        DomainOutcome::PlanRequested {
            phase: Phase::Numpad,
            minutes: 10
        }
    );
    key(&mut e, 8, Key::BackTab)?;
    key(&mut e, 9, Key::Arrow(Direction::Left))?;
    assert_eq!(
        e.prepare(MonoTick::from_us(9)).destination.selected,
        Destination::Today
    );
    Ok(())
}
