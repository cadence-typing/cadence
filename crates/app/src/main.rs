//! `cadence` — CLI composition root (A-01, H-01, B-02).
//!
//! Composes the runtime (terminal lifecycle/ingress), the engine
//! (reducer) and the shared paint path (TUI) into the user-facing
//! binary. The base word corpus and prose bank are embedded so the
//! statically linked bundle works fully offline. P3 adds XDG-backed
//! persistence; today every run builds a fresh deterministic session
//! (seeded from the calendar day, so a day's plan is stable across
//! restarts) and progress does not yet survive exit — stated honestly
//! on the Today screen.

use std::process::ExitCode;
use std::time::Instant;

use cadence_core::{EventSeq, WordId};
use cadence_engine::{Engine, EventEnvelope, EventKind};
use cadence_runtime::{TerminalGuard, elapsed_since};
use cadence_trainer::{WordBand, WordPack, build_session_plan, parse_policy};
use cadence_tui::paint::paint;
use cadence_tui::theme::Palette;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The embedded base corpus (decision 0002; CC-BY-SA 4.0 notice in
/// corpus/NOTICE, digest recorded in corpus/manifest.json).
const WORDS_TSV: &str = include_str!("../../../corpus/words-en-v1.tsv");

/// The embedded prose bank (Project Gutenberg #1342, public domain).
const PROSE_BANK: &str = include_str!("../../../corpus/prose-en-v1.txt");

/// The pinned trainer policy.
const POLICY_TOML: &str = include_str!("../../../policies/trainer-v1.toml");

/// Daily-plan seed basis: the calendar day keeps one day's plan stable
/// across restarts (P2 determinism; a real profile RNG stream arrives
/// with P3 persistence).
fn daily_seed() -> u64 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    // Days since epoch, folded into a u64 seed.
    seconds / 86_400
}

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
                "usage: cadence [--version]   (settings/headless subcommands arrive with P3/P4)"
            );
            ExitCode::from(USAGE_ERROR)
        }
    }
}

/// The guided daily session (P2 scope): a deterministic 15-minute plan
/// from the embedded corpus, run through the shared engine and paint
/// path.
fn interactive() -> ExitCode {
    let Some(engine) = build_engine() else {
        return ExitCode::FAILURE;
    };
    match owner_loop(engine) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cadence: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Loads the embedded corpus through the production pack type and
/// builds the deterministic session plan.
fn build_engine() -> Option<Engine> {
    let policy = match parse_policy(POLICY_TOML.as_bytes()) {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("cadence: embedded policy invalid: {error}");
            return None;
        }
    };
    let pack = match load_embedded_words() {
        Ok(pack) => pack,
        Err(error) => {
            eprintln!("cadence: embedded corpus invalid: {error}");
            return None;
        }
    };
    let seed = daily_seed();
    let plan = match build_session_plan(&policy, &pack, PROSE_BANK, seed, &[], 900) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("cadence: could not plan the session: {error}");
            return None;
        }
    };
    match Engine::new_plan(plan) {
        Ok(engine) => Some(engine),
        Err(error) => {
            eprintln!("cadence: {error}");
            None
        }
    }
}

/// Parses the embedded TSV into the production `WordPack` (the same
/// loader the corpus gate exercises).
fn load_embedded_words() -> Result<WordPack, String> {
    let mut entries = Vec::new();
    for (index, line) in WORDS_TSV.lines().enumerate() {
        let line_number = index.saturating_add(1);
        let fields: Vec<&str> = line.split('\t').collect();
        let (Some(rank_text), Some(word), Some(band_text)) = (
            fields.first().copied(),
            fields.get(1).copied(),
            fields.get(2).copied(),
        ) else {
            return Err(format!("line {line_number}: expected rank\\tword\\tband"));
        };
        let rank: u64 = rank_text
            .parse()
            .map_err(|_| format!("line {line_number}: bad rank"))?;
        let band =
            WordBand::parse(band_text).map_err(|error| format!("line {line_number}: {error}"))?;
        let _ = WordId::new(rank).map_err(|_| "zero rank")?;
        entries.push((rank, word.to_string(), band));
    }
    let digest = cadence_trainer::corpus::digest_of(WORDS_TSV.as_bytes());
    WordPack::new(
        entries,
        digest,
        // The embedded base pack is a curated subset for bundle size;
        // the release corpus gate still verifies the full pack on disk.
        1_000,
        cadence_core::LayoutSupportsPound::US,
    )
    .map_err(|error| error.to_string())
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
