//! `cadence` — CLI composition root (A-01, H-01).
//!
//! Composes the runtime (terminal lifecycle/ingress), the engine
//! (reducer) and the shared paint path (TUI) into the user-facing
//! binary. P1 surface: `cadence` (experimental interactive slice),
//! `cadence --version`. `doctor`, `config check`, `headless`, `replay`,
//! `export`, `backup` land with their work packets (P3/P4); see
//! STATUS.md for the honest gate inventory.

use std::process::ExitCode;
use std::time::Instant;

use cadence_core::EventSeq;
use cadence_engine::{Engine, EventEnvelope, EventKind};
use cadence_runtime::{TerminalGuard, elapsed_since};
use cadence_tui::paint::paint;
use cadence_tui::theme::Palette;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Exit code for argument errors (1 is reserved for runtime failures).
const USAGE_ERROR: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => interactive(),
        Some("--version") | Some("-V") => {
            println!("cadence {VERSION}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("cadence: unknown argument {other:?}");
            eprintln!(
                "usage: cadence [--version]   (interactive slice; more subcommands arrive with P3/P4)"
            );
            ExitCode::from(USAGE_ERROR)
        }
    }
}

/// The P1 experimental vertical slice, labelled as such on screen: one
/// practice exercise with error/cursor feedback, pause/resume, result
/// and normal quit.
fn interactive() -> ExitCode {
    // Slice targets are ordinary practice text (P2 replaces them with
    // the adaptive planner and its committed observations).
    let targets = vec![
        "the quick brown fox jumps over the lazy dog".to_string(),
        "steady daily practice builds precision".to_string(),
        "cat".to_string(),
        "005.20".to_string(),
    ];
    let engine = match Engine::new_slice(targets) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("cadence: {error}");
            return ExitCode::FAILURE;
        }
    };
    match owner_loop(engine) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cadence: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The single owner loop (A-02): ingress → admission → reduce → prepare
/// → paint, synchronous and bounded per iteration.
fn owner_loop(mut engine: Engine) -> Result<(), Box<dyn std::error::Error>> {
    let mut session = TerminalGuard::enter()?;
    let palette = Palette::cadence_dark();
    let origin = Instant::now();
    let mut seq: u64 = 0;

    draw(&engine, &mut session, palette, origin)?;

    loop {
        let kind = session.next_event()?.unwrap_or(EventKind::Tick);
        seq = seq.saturating_add(1);
        let transition = engine.reduce(EventEnvelope {
            seq: EventSeq::from_raw(seq),
            at: elapsed_since(origin),
            kind,
        });
        if transition.quit {
            session.restore()?;
            return Ok(());
        }
        if transition.dirty {
            draw(&engine, &mut session, palette, origin)?;
        }
    }
}

fn draw(
    engine: &Engine,
    session: &mut TerminalGuard,
    palette: Palette,
    origin: Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    let view = engine.prepare(elapsed_since(origin));
    session.draw(|frame| {
        let area = frame.area();
        paint(frame.buffer_mut(), area, &view, palette);
    })?;
    Ok(())
}
