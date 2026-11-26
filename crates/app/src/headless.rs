//! `cadence headless` and `cadence doctor --json` (#69). The headless
//! driver runs the same engine and renderer as live mode; this composition
//! root supplies its services: fixture scenarios or the real daily plan,
//! and a profile store whose sessions are marked `synthetic` and never read
//! back as history.

use std::error::Error;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cadence_core::progress::{Checkpoint, HistorySummary};
use cadence_core::{ContentDigest, LayoutSupportsPound, Micros, SessionId};
use cadence_data::{ProgressStore, SUPPORTED_SCHEMA};
use cadence_driver::headless::{Headless, Services};
use cadence_driver::protocol::Protocol;
use cadence_engine::Engine;
use cadence_trainer::{
    Adaptation, ExerciseKind, Phase, PlannedExercise, PlannedPhase, ProbePlan, SessionPlan,
};

/// Words of the `daily-small` scenario (`fixtures/scenarios.json`).
const DAILY_SMALL: &[&str] = &[
    "street",
    "strong",
    "almost",
    "string",
    "first",
    "stream",
    "understand",
    "structure",
    "clear",
    "purpose",
    "system",
    "simple",
    "practice",
    "result",
    "number",
    "precision",
    "daily",
    "pattern",
    "steady",
    "thought",
];

fn scenario_plan(name: &str) -> Result<SessionPlan, String> {
    let (seed, seconds, targets): (u64, u64, Vec<String>) = match name {
        "scoring-cat" => (1, 60, vec!["cat".to_string()]),
        "daily-small" => (
            42,
            120,
            DAILY_SMALL.chunks(4).map(|words| words.join(" ")).collect(),
        ),
        other => return Err(format!("unknown scenario {other}")),
    };
    let exercises = targets
        .iter()
        .map(|text| {
            cadence_core::validate_target(text, LayoutSupportsPound::US)
                .map(|units| PlannedExercise::plain(units, ExerciseKind::Words))
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SessionPlan {
        budget_seconds: seconds,
        policy_digest: ContentDigest::from_bytes([0; 32]),
        corpus_digest: ContentDigest::from_bytes([0; 32]),
        record: cadence_core::learning::PlanRecord {
            seed,
            mode: cadence_core::learning::PlannerMode::ColdStart,
            focus: Vec::new(),
        },
        adaptation: Adaptation {
            hesitation_ceiling: Micros::from_us(1_500_000),
            reinforcement_capacity: 8,
            reinforcements_per_word: 1,
            numeric_gap_entries: 0,
        },
        probe: ProbePlan::NotDue,
        pace: Vec::new(),
        retired_due: 0,
        phases: vec![PlannedPhase {
            phase: Phase::AdaptiveWords,
            seconds,
            exercises,
        }],
    })
}

/// The profile store behind a headless session.
struct StoreServices {
    store: ProgressStore,
    day: i64,
}

impl Services for StoreServices {
    fn engine(&mut self, scenario: Option<&str>) -> Result<Engine, String> {
        match scenario {
            Some(name) => Engine::new_plan(scenario_plan(name)?).map_err(|error| error.to_string()),
            None => {
                let policy = cadence_trainer::parse_policy(crate::POLICY.as_bytes())
                    .map_err(|error| error.to_string())?;
                let bootstrap = self
                    .store
                    .bootstrap(policy.learning.history_window())
                    .map_err(|error| error.to_string())?;
                let options = crate::cli::Options::defaults();
                crate::build_engine(&options, &policy, &bootstrap, self.day)
                    .map_err(|error| error.to_string())
            }
        }
    }

    fn storage(&mut self) -> Result<(SessionId, i64, HistorySummary), String> {
        let bootstrap = self.store.bootstrap(1).map_err(|error| error.to_string())?;
        Ok((bootstrap.next_session, self.day, bootstrap.history))
    }

    fn save(&mut self, checkpoint: &Checkpoint) -> Result<(), String> {
        self.store
            .apply_checkpoint(checkpoint)
            .map_err(|error| error.to_string())?;
        self.store
            .mark_synthetic(checkpoint.key.session)
            .map_err(|error| error.to_string())
    }
}

fn utc_day() -> Result<i64, Box<dyn Error>> {
    let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    match cadence_core::calendar::local_day(now, 0) {
        cadence_core::calendar::Day::Number(day) => Ok(day),
        cadence_core::calendar::Day::OffsetOutOfRange | cadence_core::calendar::Day::Overflow => {
            Err("the date is outside the supported range".into())
        }
    }
}

/// A fresh profile directory under the system temporary directory.
fn ephemeral_dir() -> Result<PathBuf, Box<dyn Error>> {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!("cadence-headless-{}-{nanos}", std::process::id()));
    std::fs::create_dir(&dir)?;
    Ok(dir)
}

/// Runs the headless driver until `quit` or the end of input (which
/// finishes the session the same way).
///
/// # Errors
/// Profile, input or output failures.
pub fn run(
    profile: Option<&Path>,
    script: Option<&Path>,
    ephemeral: bool,
) -> Result<(), Box<dyn Error>> {
    let temporary = if ephemeral {
        Some(ephemeral_dir()?)
    } else {
        None
    };
    let directory = temporary
        .as_deref()
        .or(profile)
        .ok_or("headless requires --profile-dir or --ephemeral-profile")?;
    let result = serve(directory, script);
    if let Some(dir) = temporary {
        std::fs::remove_dir_all(dir)?;
    }
    result
}

fn serve(directory: &Path, script: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let store = ProgressStore::open_in(directory)?;
    let mut host = Headless::new(StoreServices {
        store,
        day: utc_day()?,
    });
    let mut protocol = Protocol::new();
    let input: Box<dyn BufRead> = match script {
        Some(path) => Box::new(std::io::BufReader::new(std::fs::File::open(path)?)),
        None => Box::new(std::io::stdin().lock()),
    };
    let mut out = std::io::stdout().lock();
    for line in input.split(b'\n') {
        let line = line?;
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let response = protocol.handle(&line, &mut host);
        writeln!(out, "{response}")?;
        out.flush()?;
        if host.finished() {
            return Ok(());
        }
    }
    // End of input: finish and flush as `quit` would.
    if host.engine().is_some() {
        let quit = br#"{"v":1,"id":"18446744073709551615","op":"quit","args":{"flush":true}}"#;
        let response = protocol.handle(quit, &mut host);
        writeln!(out, "{response}")?;
    }
    Ok(())
}

/// `cadence doctor --json`: version, schema, paths, terminal facts and disk
/// use, as one JSON object on stdout.
///
/// # Errors
/// Path resolution or encoding failures.
pub fn doctor(profile: Option<&Path>) -> Result<(), Box<dyn Error>> {
    use std::io::IsTerminal;
    let paths = cadence_data::data_dir(profile);
    let (data, exists, bytes) = match &paths {
        Ok(paths) => {
            let database = paths.data.join(cadence_data::paths::DATABASE_FILE);
            let bytes = std::fs::metadata(&database).map(|m| m.len()).ok();
            (
                Some(paths.data.display().to_string()),
                paths.data.is_dir(),
                bytes,
            )
        }
        Err(_) => (None, false, None),
    };
    let report = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "schema": SUPPORTED_SCHEMA,
        "profile": {
            "path": data,
            "exists": exists,
            "database_bytes": bytes,
            "error": paths.as_ref().err().map(ToString::to_string),
        },
        "terminal": {
            "term": std::env::var("TERM").ok(),
            "stdin_is_terminal": std::io::stdin().is_terminal(),
            "stdout_is_terminal": std::io::stdout().is_terminal(),
        },
    });
    println!("{report}");
    Ok(())
}
