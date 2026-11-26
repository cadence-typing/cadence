//! CLI composition: offline content, reducer, retained writer and shared paint.

mod cli;
mod practice;

use std::error::Error;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use cadence_core::calendar::{Day, local_day};
use cadence_core::{LayoutSupportsPound, UtcDay};
use cadence_data::{ProgressStore, StoreBootstrap};
use cadence_engine::Engine;
use cadence_runtime::SaveWorker;
use cadence_trainer::{
    LearnerModel, Phase, PlanInputs, ProseBank, TrainerPolicy, WordPack, build_session_plan,
    parse_policy,
};

/// The complete distributed word data, not a demo-only practice path.
const WORDS: &str = include_str!("../../../corpus/words-en-v1.tsv");
const PROSE: &str = include_str!("../../../corpus/prose-en-v2.tsv");
const PROSE_SOURCES: &str = include_str!("../../../corpus/prose-sources-v2.tsv");
const PROBES: &str = include_str!("../../../corpus/benchmark-en-v1.tsv");
const POLICY: &str = include_str!("../../../policies/trainer-v3.toml");
/// Shipped prose lower bound (plan 0003): enough for a long no-repeat window.
const MINIMUM_PASSAGES: usize = 2_000;
/// Domain separation for the per-session plan seed.
const SEED_DOMAIN: &[u8] = b"cadence-plan-seed-v1";
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
            // The wall clock and the local offset are read once, here, at
            // session start; the proved calendar::local_day decides the day.
            let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
            let (offset, detected) = match options.utc_offset {
                Some(seconds) => (seconds, true),
                None => local_offset(),
            };
            let day = match local_day(now, offset) {
                Day::Number(day) => day,
                Day::OffsetOutOfRange | Day::Overflow => {
                    return Err("the local date is outside the supported range".into());
                }
            };
            let policy = parse_policy(POLICY.as_bytes())?;
            // Store first: the plan is built from its evidence, before the
            // terminal and the input/reduce/render loop start.
            let (worker, bootstrap) =
                SaveWorker::open(options.profile.clone(), policy.learning.history_window())?;
            let engine = build_engine(&options, &policy, &bootstrap, day)?
                .with_utc_offset(Some(offset.checked_div(60).ok_or("offset")?), detected);
            practice::run(
                engine,
                worker,
                bootstrap,
                day,
                options.text_size.unwrap_or_default(),
                options.theme.palette(),
            )
        }
    }
}

/// The local UTC offset (seconds east) at session start, and whether a
/// local time zone was found; without one, days are counted in UTC.
fn local_offset() -> (i32, bool) {
    let detected = std::env::var_os("TZ").is_some_and(|tz| !tz.is_empty())
        || std::path::Path::new("/etc/localtime").exists();
    if !detected {
        return (0, false);
    }
    (chrono::Local::now().offset().local_minus_utc(), true)
}

fn build_engine(
    options: &cli::Options,
    policy: &TrainerPolicy,
    bootstrap: &StoreBootstrap,
    day: i64,
) -> Result<Engine, Box<dyn Error>> {
    // Content and learner work finish before the hot input/reducer/render path.
    let pack = WordPack::from_tsv(WORDS, MINIMUM_WORDS, LayoutSupportsPound::US)?;
    let prose = ProseBank::from_tsv(PROSE, PROSE_SOURCES, MINIMUM_PASSAGES)?;
    let probes = cadence_trainer::probe::ProbeBank::from_tsv(
        PROBES,
        &pack,
        cadence_trainer::probe::MINIMUM_PROBES,
    )?;
    let learner = LearnerModel::from_history(
        &bootstrap.learner,
        policy,
        UtcDay::from_days_since_epoch(day)?,
    )?;
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
    let seed = options
        .seed
        .unwrap_or_else(|| session_seed(bootstrap.next_session.get(), day));
    let inputs = PlanInputs {
        pack: &pack,
        prose: &prose,
        learner: &learner,
        probes: Some(cadence_trainer::ProbeInputs {
            bank: &probes,
            history: &bootstrap.probes,
        }),
    };
    let plan = build_session_plan(policy, inputs, seed, &disabled, budget)?;
    Ok(Engine::new_plan(plan)?)
}

/// A distinct, replayable seed per session (persisted with the plan), so
/// two sessions on one day no longer repeat the same content.
fn session_seed(session: u64, day: i64) -> u64 {
    let mut bytes = SEED_DOMAIN.to_vec();
    bytes.extend_from_slice(&session.to_le_bytes());
    bytes.extend_from_slice(&day.to_le_bytes());
    let digest = cadence_trainer::corpus::digest_of(&bytes);
    let mut seed = [0_u8; 8];
    seed.copy_from_slice(digest.as_bytes().get(..8).unwrap_or(&[0; 8]));
    u64::from_le_bytes(seed)
}
