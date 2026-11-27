//! CLI composition: offline content, reducer, retained writer and shared paint.

mod cli;
mod headless;
mod import;
mod practice;
#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "a test driver over small bounded virtual times and counts"
)]
mod scripted_sessions;
mod session;

use std::error::Error;
use std::process::ExitCode;

use cadence_core::{Layout, UtcDay};
use cadence_data::{ProgressStore, StoreBootstrap};
use cadence_engine::Engine;
use cadence_trainer::{
    LearnerModel, Phase, PlanInputs, ProseBank, TrainerPolicy, WordPack, build_session_plan,
    parse_policy,
};

/// The complete distributed word data, not a demo-only practice path.
const WORDS: &str = include_str!("../../../corpus/words-en-v2.tsv");
const PROSE: &str = include_str!("../../../corpus/prose-en-v2.tsv");
const PROSE_SOURCES: &str = include_str!("../../../corpus/prose-sources-v2.tsv");
const PROBES: &str = include_str!("../../../corpus/benchmark-en-v1.tsv");
pub(crate) const POLICY: &str = include_str!("../../../policies/trainer-v4.toml");
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
        cli::Command::Restore(backup) => {
            let paths = cadence_data::data_dir(options.profile.as_deref())?;
            let restored = ProgressStore::restore(&paths.data, backup)?;
            println!(
                "Restored {} session(s) from {} (schema {}).",
                restored.sessions,
                backup.display(),
                restored.schema
            );
            if let Some(previous) = restored.previous {
                println!("The replaced database is kept as {}.", previous.display());
            }
            Ok(())
        }
        cli::Command::Export(destination) => {
            let store = ProgressStore::open(options.profile.as_deref())?;
            store.export_to(destination)?;
            println!("Progress exported: {}", destination.display());
            Ok(())
        }
        cli::Command::Headless {
            script,
            ephemeral,
            record,
            record_content,
        } => headless::run(
            options.profile.as_deref(),
            script.as_deref(),
            *ephemeral,
            record.as_deref().map(|path| (path, *record_content)),
        ),
        cli::Command::Replay { trace, capture_dir } => {
            headless::replay(trace, capture_dir.as_deref())
        }
        cli::Command::Doctor { rebuild_derived } => {
            headless::doctor(options.profile.as_deref(), *rebuild_derived)
        }
        cli::Command::ConfigCheck(file) => config_check(file.as_deref()),
        cli::Command::Import(request) => {
            let config = load_config(None)?.config;
            let layout = config
                .text("keyboard.layout")
                .and_then(cadence_core::Layout::parse)
                .unwrap_or_default();
            import::run(request, options.profile.as_deref(), layout)
        }
        cli::Command::Packs => import::list(options.profile.as_deref()),
        cli::Command::Practice => session::run(&options),
    }
}

/// The settings a session's scores depend on, for the audit (#79): those
/// the proved `settings::takes_effect` holds for the next session.
pub(crate) fn score_affecting(
    config: &cadence_data::config::Config,
) -> Vec<(&'static str, String)> {
    config
        .effective()
        .into_iter()
        .filter(|(key, _, _)| {
            cadence_core::settings::effect(key) == Some(cadence_core::settings::Effect::NextSession)
        })
        .map(|(key, value, _)| (key, value.to_string()))
        .collect()
}

/// The config file: the given path, else the XDG location.
pub(crate) fn config_path(file: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    file.map(std::path::Path::to_path_buf).or_else(|| {
        cadence_data::config::path(
            std::env::var_os("XDG_CONFIG_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    })
}

pub(crate) fn load_config(
    file: Option<&std::path::Path>,
) -> Result<cadence_data::config_write::Loaded, Box<dyn Error>> {
    let Some(path) = config_path(file) else {
        return Ok(cadence_data::config_write::Loaded {
            config: cadence_data::config::Config::default(),
            digest: None,
        });
    };
    cadence_data::config_write::reload(&path).map_err(|errors| {
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            .into()
    })
}

/// Defaults, then the file, then flags: a flag keeps its meaning and wins.
pub(crate) fn apply_config(
    mut options: cli::Options,
    config: &cadence_data::config::Config,
) -> cli::Options {
    if options.minutes.is_none() {
        options.minutes = config
            .integer("session.minutes")
            .filter(|_| {
                config.source("session.minutes") == Some(cadence_data::config::Source::File)
            })
            .and_then(|m| u64::try_from(m).ok());
    }
    if options.text_size.is_none()
        && config.source("display.text_size") == Some(cadence_data::config::Source::File)
    {
        options.text_size = match config.text("display.text_size") {
            Some("large") => Some(cadence_engine::view::TextSize::Large),
            Some("normal") => Some(cadence_engine::view::TextSize::Normal),
            _ => Some(cadence_engine::view::TextSize::Auto),
        };
    }
    options.numpad = config.boolean("keyboard.numpad").unwrap_or(true);
    options.numeric = cadence_trainer::numeric::NumericOptions {
        operators: config.boolean("content.numpad_operators").unwrap_or(false),
        separator: if config.text("keyboard.decimal_separator") == Some(",") {
            cadence_trainer::numeric::Separator::Comma
        } else {
            cadence_trainer::numeric::Separator::Point
        },
    };
    options.layout = config
        .text("keyboard.layout")
        .and_then(cadence_core::Layout::parse)
        .unwrap_or_default();
    if !options.theme_set
        && config.source("display.theme") == Some(cadence_data::config::Source::File)
    {
        options.theme = match config.text("display.theme") {
            Some("light") => cli::Theme::Light,
            Some("terminal") => cli::Theme::Terminal,
            _ => cli::Theme::Dark,
        };
    }
    options
}

/// `cadence config check [FILE]`: effective values with their source, or
/// the diagnostics (non-zero exit).
fn config_check(file: Option<&std::path::Path>) -> Result<(), Box<dyn Error>> {
    let path = config_path(file).ok_or("no config location: set HOME or XDG_CONFIG_HOME")?;
    match cadence_data::config::Config::load(&path) {
        Ok(config) => {
            println!("# {}", path.display());
            for (key, value, source) in config.effective() {
                println!("{key} = {value}  # {source}");
            }
            Ok(())
        }
        Err(errors) => {
            for error in &errors {
                println!("{error}");
            }
            Err(format!("{} problem(s) in {}", errors.len(), path.display()).into())
        }
    }
}

/// The local UTC offset (seconds east) at session start, and whether a
/// local time zone was found; without one, days are counted in UTC.
pub(crate) fn local_offset() -> (i32, bool) {
    let detected = std::env::var_os("TZ").is_some_and(|tz| !tz.is_empty())
        || std::path::Path::new("/etc/localtime").exists();
    if !detected {
        return (0, false);
    }
    (jiff::Zoned::now().offset().seconds(), true)
}

pub(crate) fn build_engine(
    options: &cli::Options,
    policy: &TrainerPolicy,
    bootstrap: &StoreBootstrap,
    day: i64,
) -> Result<Engine, Box<dyn Error>> {
    // Content and learner work finish before the hot input/reducer/render path.
    let pack = WordPack::from_tsv(WORDS, MINIMUM_WORDS, Layout::Us)?;
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
        // No numpad (#78): its phase is disabled and plan::allocate gives its
        // seconds to the others.
        None if !options.numpad => vec![Phase::Numpad],
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
        pace: Some(cadence_trainer::PaceInputs {
            recent: &bootstrap.history.recent,
            targets: &bootstrap.pace_targets,
        }),
        numeric: options.numeric,
    };
    let plan = build_session_plan(policy, inputs, seed, &disabled, budget)?;
    // A pack session (#85): the same single-track plan, over the pack.
    let Some(name) = &options.pack else {
        return Ok(Engine::new_plan(plan)?);
    };
    let pack = load_pack(name, options)?;
    let mut rng = <cadence_trainer::PlanRng as rand::SeedableRng>::seed_from_u64(seed);
    let plan = cadence_trainer::packs::pack_session(
        plan,
        &pack,
        &mut rng,
        policy.learning.prose_units_per_second,
    )?;
    Ok(Engine::new_plan(plan)?.with_pack(pack.digest))
}

/// The shipped code packs (#85).
const CODE_PACKS: [(&str, &str); 2] = [
    (
        "code-rust",
        include_str!("../../../corpus/code-rust-v1.tsv"),
    ),
    (
        "code-shell",
        include_str!("../../../corpus/code-shell-v1.tsv"),
    ),
];

/// Whether `name` is a shipped code pack.
pub(crate) fn is_code_pack(name: &str) -> bool {
    CODE_PACKS.iter().any(|(code, _)| *code == name)
}

/// A shipped code pack, or an imported pack from the profile's packs.
fn load_pack(
    name: &str,
    options: &cli::Options,
) -> Result<cadence_trainer::packs::Pack, Box<dyn Error>> {
    if let Some((_, tsv)) = CODE_PACKS.iter().find(|(code, _)| *code == name) {
        return Ok(cadence_trainer::packs::code_pack(name, tsv)?);
    }
    let packs = cadence_data::packs::directory(
        &cadence_data::paths::data_dir(options.profile.as_deref())?.data,
    );
    let text = std::fs::read_to_string(packs.join(name).join("passages.txt"))
        .map_err(|error| format!("no pack named {name} ({error}); see `cadence packs`"))?;
    Ok(cadence_trainer::packs::imported_pack(
        name,
        &text,
        options.layout,
    )?)
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

#[cfg(test)]
mod config_tests {
    use super::{apply_config, cli};
    use cadence_data::config::Config;
    use std::path::Path;

    /// #77: changing only the theme changes no partition; the layout comes
    /// from `keyboard.layout` alone.
    #[test]
    fn changing_only_the_theme_changes_no_partition() -> Result<(), String> {
        let parse = |text: &str| {
            Config::parse(text, Path::new("/cfg/config.toml")).map_err(|e| format!("{e:?}"))
        };
        let dark = apply_config(
            cli::Options::defaults(),
            &parse("[keyboard]\nlayout = \"uk\"\n")?,
        );
        let light = apply_config(
            cli::Options::defaults(),
            &parse("[keyboard]\nlayout = \"uk\"\n[display]\ntheme = \"light\"\n")?,
        );
        assert_eq!(dark.layout, cadence_core::Layout::Uk);
        assert_eq!(dark.layout, light.layout);
        assert!(matches!(light.theme, cli::Theme::Light));
        Ok(())
    }
}

#[cfg(test)]
mod offset_tests {
    /// #95: the local offset (now from jiff) is the system's, as `date +%z`
    /// reports it for the same time zone.
    #[test]
    fn the_local_offset_matches_the_system() -> Result<(), Box<dyn std::error::Error>> {
        let (offset, detected) = super::local_offset();
        if !detected {
            assert_eq!(offset, 0);
            return Ok(());
        }
        let output = std::process::Command::new("date").arg("+%z").output()?;
        let text = String::from_utf8(output.stdout)?;
        let text = text.trim();
        let sign = if text.starts_with('-') { -1 } else { 1 };
        let hours: i32 = text.get(1..3).ok_or("hours")?.parse()?;
        let minutes: i32 = text.get(3..5).ok_or("minutes")?.parse()?;
        assert_eq!(
            offset,
            sign * (hours * 3600 + minutes * 60),
            "date +%z says {text}"
        );
        Ok(())
    }
}
