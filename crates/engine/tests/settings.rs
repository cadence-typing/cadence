//! #79: the settings overlay. Nothing reads as saved before the worker's
//! acknowledgement; display settings apply at once and leave the target,
//! ledger and clock untouched; score-affecting settings wait for the next
//! session; a failed or conflicting write or an invalid reload keeps the
//! previous settings with one diagnostic.
mod support;
use cadence_core::MonoTick;
use cadence_core::settings::{ConfigDigest, KEYS, Setting};
use cadence_engine::jobs::{JobKind, JobOutcome, JobTicket};
use cadence_engine::settings::SettingsRequest;
use cadence_engine::{Direction, Engine, EventKind, Key};
use support::*;

fn row(name: &str) -> usize {
    KEYS.iter()
        .position(|key| key.name == name)
        .unwrap_or(usize::MAX)
}

/// Paused mid-exercise with the overlay open on `name`.
fn paused_in_settings(name: &str) -> Result<Engine, Box<dyn std::error::Error>> {
    let mut e = words()?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1, 'c')?;
    key(&mut e, 2, Key::Escape)?;
    content(&mut e, 3, 's')?;
    for _ in 0..row(name) {
        key(&mut e, 4, Key::Arrow(Direction::Down))?;
    }
    Ok(e)
}

fn save(e: &mut Engine, at: u64) -> Result<JobTicket, Box<dyn std::error::Error>> {
    let t = key(e, at, Key::Enter)?;
    let ticket = t.job.ok_or("Enter starts a write")?;
    assert_eq!(ticket.kind, JobKind::ConfigWrite);
    Ok(ticket)
}

fn answer(
    e: &mut Engine,
    at: u64,
    ticket: JobTicket,
    outcome: JobOutcome,
) -> Result<(), Box<dyn std::error::Error>> {
    send(e, at, EventKind::JobResult { ticket, outcome })?;
    Ok(())
}

fn written(values: &[(&'static str, Setting)]) -> JobOutcome {
    JobOutcome::Settings {
        values: values.to_vec(),
        digest: Some(ConfigDigest::of(b"x")),
        theme: None,
    }
}

#[test]
fn a_theme_change_applies_at_once_and_touches_nothing_scored() -> TestResult {
    let mut e = paused_in_settings("display.theme")?;
    let before = e.prepare(MonoTick::from_us(5));
    key(&mut e, 5, Key::Arrow(Direction::Right))?;
    let ticket = save(&mut e, 6)?;
    assert_eq!(
        e.settings_request(ticket),
        Some(SettingsRequest::Write {
            based_on: None,
            edits: vec![("display.theme", Setting::Text("light".into()))],
        })
    );
    // Not saved until acknowledged.
    let waiting = e.prepare(MonoTick::from_us(6));
    assert_eq!(waiting.display.theme, "dark");
    assert!(waiting.settings.as_ref().is_some_and(|s| s.saving));
    answer(
        &mut e,
        7,
        ticket,
        written(&[("display.theme", Setting::Text("light".into()))]),
    )?;
    let after = e.prepare(MonoTick::from_us(7));
    assert_eq!(after.display.theme, "light");
    assert!(after.settings.is_none(), "the overlay closes on save");
    assert_eq!(after.notice.as_deref(), Some("Settings saved"));
    assert_eq!(after.screen, before.screen, "target and ledger untouched");
    assert_eq!(after.active, before.active, "clock untouched");
    assert_eq!(after.phases, before.phases);
    Ok(())
}

#[test]
fn a_minutes_change_waits_for_the_next_session() -> TestResult {
    let mut e = paused_in_settings("session.minutes")?;
    let plan = e.prepare(MonoTick::from_us(5)).phases;
    key(&mut e, 5, Key::Arrow(Direction::Right))?;
    let ticket = save(&mut e, 6)?;
    answer(
        &mut e,
        7,
        ticket,
        written(&[("session.minutes", Setting::Integer(20))]),
    )?;
    let view = e.prepare(MonoTick::from_us(7));
    assert_eq!(view.phases, plan, "the running plan is unchanged");
    assert_eq!(
        view.notice.as_deref(),
        Some("Settings saved · minutes applies next session")
    );
    Ok(())
}

#[test]
fn a_conflicting_write_keeps_the_previous_settings() -> TestResult {
    let mut e = paused_in_settings("display.theme")?;
    key(&mut e, 5, Key::Arrow(Direction::Right))?;
    let ticket = save(&mut e, 6)?;
    answer(
        &mut e,
        7,
        ticket,
        JobOutcome::Failed("the settings file changed on disk".into()),
    )?;
    let view = e.prepare(MonoTick::from_us(7));
    assert_eq!(view.display.theme, "dark");
    assert_eq!(
        view.notice.as_deref(),
        Some("Settings not changed: the settings file changed on disk")
    );
    let overlay = view.settings.ok_or("still open")?;
    assert!(overlay.rows.iter().all(|row| !row.changed), "draft reset");
    Ok(())
}

#[test]
fn an_invalid_reload_keeps_the_last_valid_settings() -> TestResult {
    let mut e = paused_in_settings("display.theme")?;
    let t = content(&mut e, 5, 'r')?;
    let ticket = t.job.ok_or("r reloads")?;
    assert_eq!(e.settings_request(ticket), Some(SettingsRequest::Reload));
    answer(
        &mut e,
        6,
        ticket,
        JobOutcome::Failed(
            "config.toml:2: session.minutes: expected one of 5, 10, 15, 20, 30".into(),
        ),
    )?;
    let view = e.prepare(MonoTick::from_us(6));
    assert_eq!(view.display.theme, "dark");
    assert!(view.notice.is_some_and(|n| n.contains("session.minutes")));
    Ok(())
}

#[test]
fn a_stale_result_changes_nothing() -> TestResult {
    let mut e = paused_in_settings("display.theme")?;
    key(&mut e, 5, Key::Arrow(Direction::Right))?;
    let ticket = save(&mut e, 6)?;
    send(&mut e, 7, EventKind::RestartJobs)?;
    answer(
        &mut e,
        8,
        ticket,
        written(&[("display.theme", Setting::Text("light".into()))]),
    )?;
    assert_eq!(e.prepare(MonoTick::from_us(8)).display.theme, "dark");
    Ok(())
}

#[test]
fn settings_never_open_while_typing() -> TestResult {
    let mut e = words()?;
    key(&mut e, 0, Key::Enter)?;
    content(&mut e, 1, 's')?;
    assert!(e.prepare(MonoTick::from_us(1)).settings.is_none());
    Ok(())
}

/// #82: a theme file is used whole: an invalid one keeps the current
/// palette and theme with one diagnostic; a valid one becomes the palette.
#[test]
fn an_invalid_theme_file_keeps_the_current_palette() -> TestResult {
    let mut e = words()?;
    ready(&mut e)?;
    content(&mut e, 1, 's')?;
    let ticket = content(&mut e, 2, 'r')?.job.ok_or("reload")?;
    answer(
        &mut e,
        3,
        ticket,
        JobOutcome::Settings {
            values: vec![("display.theme", Setting::Text("ocean.toml".into()))],
            digest: Some(ConfigDigest::of(b"x")),
            theme: Some(Err("/cfg/ocean.toml: missing role warning".into())),
        },
    )?;
    let view = e.prepare(MonoTick::from_us(3));
    assert_eq!(view.display.theme, "dark");
    assert_eq!(view.display.theme_colors, None);
    assert_eq!(
        view.notice.as_deref(),
        Some("Settings not changed: /cfg/ocean.toml: missing role warning")
    );
    let ticket = content(&mut e, 4, 'r')?.job.ok_or("reload")?;
    let colours = [cadence_core::settings::ThemeColor::Rgb(1, 2, 3); 11];
    answer(
        &mut e,
        5,
        ticket,
        JobOutcome::Settings {
            values: vec![("display.theme", Setting::Text("ocean.toml".into()))],
            digest: Some(ConfigDigest::of(b"y")),
            theme: Some(Ok(colours)),
        },
    )?;
    let view = e.prepare(MonoTick::from_us(5));
    assert_eq!(view.display.theme, "ocean.toml");
    assert_eq!(view.display.theme_colors, Some(colours));
    Ok(())
}

/// #82: the key guide names the expected next key on numeric tracks:
/// always with `always`, never with `off`, and with `auto` only for the
/// first numeric sessions.
#[test]
fn the_key_guide_follows_its_setting() -> TestResult {
    use cadence_core::progress::{HistorySummary, SessionRecap, Track, TrackProgress};
    use cadence_trainer::{ExerciseKind, Phase};
    let numeric_session = SessionRecap {
        day: 1,
        completed: true,
        tracks: vec![TrackProgress {
            first_attempted: 10,
            ..TrackProgress::empty(Track::Numpad)
        }],
        layout: None,
        content_class: cadence_core::progress::ContentClass::Standard,
    };
    for (mode, past_numeric, expected) in [
        ("auto", 0, Some('1')),
        ("auto", 3, None),
        ("always", 3, Some('1')),
        ("off", 0, None),
    ] {
        let mut e = engine(vec![phase(
            ExerciseKind::Numpad,
            Phase::Numpad,
            20,
            &["12"],
        )?])?
        .with_settings(&[("display.key_guide", Setting::Text(mode.into()))], None);
        ready_with(
            &mut e,
            HistorySummary {
                recent: vec![numeric_session.clone(); past_numeric],
                ..HistorySummary::default()
            },
        )?;
        key(&mut e, 1, Key::Enter)?;
        assert_eq!(
            e.prepare(MonoTick::from_us(1)).key_guide,
            expected,
            "{mode}, {past_numeric}"
        );
    }
    // Words are never guided.
    let mut e = words()?.with_settings(
        &[("display.key_guide", Setting::Text("always".into()))],
        None,
    );
    ready(&mut e)?;
    key(&mut e, 1, Key::Enter)?;
    assert_eq!(e.prepare(MonoTick::from_us(1)).key_guide, None);
    Ok(())
}
