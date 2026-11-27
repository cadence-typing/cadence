//! #89: the save protocol's event tape. `golden/save-tape.txt` records
//! events and the commands the proved `cadence_kernel::save::step` answered;
//! it replays exactly, and re-running its events reproduces it. Rewrite it
//! with `CADENCE_RECORD_TAPE=1`.
//!
//! `replay` and `run` have `tauten::machine`'s semantics; that crate needs
//! rustc 1.98 and the workspace builds with 1.97.1, so they are stated here
//! until the toolchain moves.

use cadence_core::save::{Command, Event, State, initial, step};

/// Where a replay first disagreed with its tape (`tauten::machine::Divergence`).
#[derive(Debug)]
struct Divergence {
    at: usize,
    recorded: Command,
    produced: Command,
}

/// `tauten::machine::replay` for the save machine.
fn replay(
    initial: State,
    tape: impl IntoIterator<Item = (Event, Command)>,
) -> Result<State, Divergence> {
    let mut state = initial;
    for (at, (event, recorded)) in tape.into_iter().enumerate() {
        let (next, produced) = step(state, event);
        if produced != recorded {
            return Err(Divergence {
                at,
                recorded,
                produced,
            });
        }
        state = next;
    }
    Ok(state)
}

/// `tauten::machine::run` for the save machine.
fn run(initial: State, events: impl IntoIterator<Item = Event>) -> (State, Vec<Command>) {
    let mut state = initial;
    let mut commands = Vec::new();
    for event in events {
        let (next, command) = step(state, event);
        commands.push(command);
        state = next;
    }
    (state, commands)
}

const TAPE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/save-tape.txt");

fn event_text(event: Event) -> String {
    match event {
        Event::Due => "due".into(),
        Event::Flush => "flush".into(),
        Event::Acked { seq } => format!("acked {seq}"),
        Event::AckFailed { seq } => format!("ack-failed {seq}"),
        Event::Retry => "retry".into(),
        Event::ExitRequested => "exit".into(),
        Event::FinalTimeout => "final-timeout".into(),
        Event::Stalled => "stalled".into(),
    }
}

fn command_text(command: Command) -> String {
    match command {
        Command::None => "none".into(),
        Command::Dispatch { seq } => format!("dispatch {seq}"),
        Command::Pause => "pause".into(),
        Command::Resume => "resume".into(),
        Command::Quit { success } => format!("quit {}", if success { "ok" } else { "failed" }),
    }
}

fn parse_event(text: &str) -> Option<Event> {
    let mut words = text.split_whitespace();
    let name = words.next()?;
    let seq = || -> Option<u64> { text.split_whitespace().nth(1)?.parse().ok() };
    Some(match name {
        "due" => Event::Due,
        "flush" => Event::Flush,
        "acked" => Event::Acked { seq: seq()? },
        "ack-failed" => Event::AckFailed { seq: seq()? },
        "retry" => Event::Retry,
        "exit" => Event::ExitRequested,
        "final-timeout" => Event::FinalTimeout,
        "stalled" => Event::Stalled,
        _ => return None,
    })
}

fn parse_command(text: &str) -> Option<Command> {
    let mut words = text.split_whitespace();
    Some(match (words.next()?, words.next()) {
        ("none", None) => Command::None,
        ("dispatch", Some(seq)) => Command::Dispatch {
            seq: seq.parse().ok()?,
        },
        ("pause", None) => Command::Pause,
        ("resume", None) => Command::Resume,
        ("quit", Some("ok")) => Command::Quit { success: true },
        ("quit", Some("failed")) => Command::Quit { success: false },
        _ => return None,
    })
}

/// A session's save traffic: routine saves, a stale and a duplicate
/// acknowledgement, a stall that catches up, a failure and its retry, an
/// exit with a save in flight, and the final acknowledgement.
fn events() -> Vec<Event> {
    vec![
        Event::Flush,
        Event::Due,
        Event::Flush,
        Event::Due,
        Event::Flush,
        Event::Acked { seq: 1 },
        Event::Flush,
        Event::Acked { seq: 1 },
        Event::Stalled,
        Event::Stalled,
        Event::Due,
        Event::Acked { seq: 2 },
        Event::Flush,
        Event::AckFailed { seq: 3 },
        Event::Due,
        Event::Flush,
        Event::Retry,
        Event::Flush,
        Event::ExitRequested,
        Event::FinalTimeout,
        Event::Acked { seq: 4 },
        Event::Flush,
        Event::ExitRequested,
        Event::Acked { seq: 5 },
    ]
}

#[test]
fn the_recorded_tape_replays_to_the_same_commands() -> Result<(), Box<dyn std::error::Error>> {
    let (end, commands) = run(initial(), events());
    let recorded: String = events()
        .into_iter()
        .zip(&commands)
        .map(|(event, command)| format!("{} -> {}\n", event_text(event), command_text(*command)))
        .collect();
    if std::env::var_os("CADENCE_RECORD_TAPE").is_some() {
        std::fs::write(TAPE, &recorded)?;
    }
    let text = std::fs::read_to_string(TAPE)?;
    let mut tape = Vec::new();
    for (line, entry) in text.lines().enumerate() {
        let (event, command) = entry
            .split_once(" -> ")
            .and_then(|(e, c)| Some((parse_event(e)?, parse_command(c)?)))
            .ok_or_else(|| format!("{TAPE}:{}: malformed {entry:?}", line.saturating_add(1)))?;
        tape.push((event, command));
    }
    let replayed = replay(initial(), tape).map_err(|d| {
        format!(
            "diverged at line {}: recorded {:?}, produced {:?}",
            d.at.saturating_add(1),
            d.recorded,
            d.produced
        )
    })?;
    assert_eq!(replayed, end);
    assert_eq!(text, recorded, "the events reproduce the tape");
    // The session ended cleanly with everything acknowledged.
    assert_eq!(commands.last(), Some(&Command::Quit { success: true }));
    assert_eq!((end.sequence, end.last_ack), (5, 5));
    Ok(())
}
