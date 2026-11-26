//! #78: first-run setup. A fresh profile shows setup, and practice cannot
//! start until the written choice is acknowledged; an existing profile's
//! history is kept; Enter writes every answer, defaults included.
mod support;
use cadence_core::MonoTick;
use cadence_core::progress::HistorySummary;
use cadence_core::settings::{ConfigDigest, Setting};
use cadence_engine::jobs::JobOutcome;
use cadence_engine::settings::SettingsRequest;
use cadence_engine::{Direction, DomainOutcome, EventKind, Key, ScreenView};
use support::*;

fn written(values: Vec<(&'static str, Setting)>) -> JobOutcome {
    JobOutcome::Settings {
        values,
        digest: Some(ConfigDigest::of(b"setup")),
    }
}

#[test]
fn a_fresh_profile_sets_up_before_practice() -> TestResult {
    let mut e = words()?.with_first_run();
    ready(&mut e)?;
    let view = e.prepare(MonoTick::from_us(1));
    let setup = view.setup.ok_or("setup is shown")?;
    let keys: Vec<&str> = setup.rows.iter().map(|row| row.key).collect();
    assert_eq!(
        keys,
        ["keyboard.layout", "keyboard.numpad", "session.minutes"]
    );
    // Change the layout to UK; Enter writes, it does not start practice.
    key(&mut e, 2, Key::Arrow(Direction::Right))?;
    let t = key(&mut e, 3, Key::Enter)?;
    assert_ne!(t.outcome, DomainOutcome::SessionStarted);
    assert!(matches!(
        e.prepare(MonoTick::from_us(3)).screen,
        ScreenView::Today { .. }
    ));
    let ticket = t.job.ok_or("Enter writes the setup")?;
    assert_eq!(
        e.settings_request(ticket),
        Some(SettingsRequest::Write {
            based_on: None,
            edits: vec![
                ("keyboard.layout", Setting::Text("uk".into())),
                ("keyboard.numpad", Setting::Boolean(true)),
                ("session.minutes", Setting::Integer(15)),
            ],
        })
    );
    // Typing before the acknowledgement does nothing.
    content(&mut e, 4, 'c')?;
    assert!(matches!(
        e.prepare(MonoTick::from_us(4)).screen,
        ScreenView::Today { .. }
    ));
    let done = send(
        &mut e,
        5,
        EventKind::JobResult {
            ticket,
            outcome: written(vec![("keyboard.layout", Setting::Text("uk".into()))]),
        },
    )?;
    assert_eq!(done.outcome, DomainOutcome::SetupComplete);
    let view = e.prepare(MonoTick::from_us(5));
    assert!(view.setup.is_none());
    // The start screen's figures are unknown: no history.
    assert_eq!(view.last_session, None);
    assert_eq!(view.history.completed_sessions, 0);
    Ok(())
}

#[test]
fn a_failed_setup_write_keeps_setup_open() -> TestResult {
    let mut e = words()?.with_first_run();
    ready(&mut e)?;
    let ticket = key(&mut e, 1, Key::Enter)?.job.ok_or("write")?;
    let t = send(
        &mut e,
        2,
        EventKind::JobResult {
            ticket,
            outcome: JobOutcome::Failed("cannot write the settings file: read-only".into()),
        },
    )?;
    assert_ne!(t.outcome, DomainOutcome::SetupComplete);
    let setup = e.prepare(MonoTick::from_us(2)).setup.ok_or("still shown")?;
    assert!(setup.notice.is_some_and(|n| n.contains("read-only")));
    Ok(())
}

#[test]
fn an_existing_profile_keeps_its_history_through_setup() -> TestResult {
    let mut e = words()?.with_first_run();
    let history = HistorySummary {
        completed_sessions: 12,
        interrupted_sessions: 2,
        ..HistorySummary::default()
    };
    ready_with(&mut e, history.clone())?;
    let ticket = key(&mut e, 1, Key::Enter)?.job.ok_or("write")?;
    let t = send(
        &mut e,
        2,
        EventKind::JobResult {
            ticket,
            outcome: written(Vec::new()),
        },
    )?;
    assert_eq!(t.outcome, DomainOutcome::SetupComplete);
    assert_eq!(e.prepare(MonoTick::from_us(2)).history, history);
    Ok(())
}

#[test]
fn q_and_ctrl_c_still_quit_from_setup() -> TestResult {
    for quit in [
        Key::Interrupt,
        Key::Content(cadence_core::TrainingChar::new(
            'q',
            cadence_core::Layout::Us,
        )?),
    ] {
        let mut e = words()?.with_first_run();
        ready(&mut e)?;
        assert!(key(&mut e, 1, quit)?.quit, "{quit:?}");
    }
    Ok(())
}
