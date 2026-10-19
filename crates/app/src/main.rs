//! CLI composition: offline content, reducer, retained writer and shared paint.

mod cli;
mod practice;

use std::error::Error;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use cadence_core::LayoutSupportsPound;
use cadence_data::ProgressStore;
use cadence_engine::Engine;
use cadence_trainer::{Phase, WordPack, build_session_plan, parse_policy};

/// The complete distributed word data, not a demo-only practice path.
const WORDS: &str = include_str!("../../../corpus/words-en-v1.tsv");
const PROSE: &str = include_str!("../../../corpus/prose-en-v1.txt");
const POLICY: &str = include_str!("../../../policies/trainer-v1.toml");
/// UTC day partition; not used to measure intra-run cadence.
const SECONDS_PER_DAY: u64 = 86_400;
/// Minutes accepted at the CLI convert exactly into policy seconds.
const SECONDS_PER_MINUTE: u64 = 60;
/// Shipped vocabulary count lower bound from P-04, checked on normal startup.
const MINIMUM_WORDS: usize = 20_000;

fn main() -> ExitCode {
    let options = match cli::parse() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("cadence: {error}");
            return ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cadence: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: cli::Options) -> Result<(), Box<dyn Error>> {
    match &options.command {
        cli::Command::Version => {
            println!(
                "cadence {} ({}, development preview)",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::ARCH
            );
            Ok(())
        }
        cli::Command::Help => {
            cli::help();
            Ok(())
        }
        cli::Command::Backup(destination) => {
            let mut store = ProgressStore::open(options.profile.as_deref())?;
            store.backup_to(destination)?;
            println!("Backup created: {}", destination.display());
            Ok(())
        }
        cli::Command::Export(destination) => {
            let store = ProgressStore::open(options.profile.as_deref())?;
            store.export_to(destination)?;
            println!("Progress exported: {}", destination.display());
            Ok(())
        }
        cli::Command::Practice => {
            let day = SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_secs()
                .checked_div(SECONDS_PER_DAY)
                .ok_or("invalid day conversion")?;
            let engine = build_engine(&options, day)?;
            practice::run(
                engine,
                options.profile,
                i64::try_from(day)?,
                options.text_size.unwrap_or_default(),
            )
        }
    }
}

fn build_engine(options: &cli::Options, day: u64) -> Result<Engine, Box<dyn Error>> {
    // Content work finishes before entering the hot input/reducer/render path.
    let policy = parse_policy(POLICY.as_bytes())?;
    let pack = WordPack::from_tsv(WORDS, MINIMUM_WORDS, LayoutSupportsPound::US)?;
    let budget = match options.minutes {
        Some(minutes) => minutes
            .checked_mul(SECONDS_PER_MINUTE)
            .ok_or("duration overflow")?,
        None => policy.session.default_budget_seconds,
    };
    let disabled: Vec<Phase> = match options.track {
        Some(selected) => policy
            .session
            .phase_order
            .iter()
            .copied()
            .filter(|phase| *phase != selected && *phase != Phase::Review)
            .collect(),
        None => Vec::new(),
    };
    let seed = options.seed.unwrap_or(day);
    let plan = build_session_plan(&policy, &pack, PROSE, seed, &disabled, budget)?;
    Ok(Engine::new_plan(plan)?)
}
