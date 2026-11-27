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
use cadence_core::{ContentDigest, Layout, Micros, SessionId};
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
        "scoring-cat" | "save-error" | "commented-config" | "config-conflict"
        | "config-changes" => (1, 60, vec!["cat".to_string()]),
        "contamination" => (5, 60, vec!["cat".to_string()]),
        "numpad-small" => (3, 60, vec!["0123".to_string(), "4.56".to_string()]),
        "row-and-pad" => (4, 30, vec!["room 1234".to_string()]),
        "daily-small" => (
            42,
            120,
            DAILY_SMALL.chunks(4).map(|words| words.join(" ")).collect(),
        ),
        other => return Err(format!("unknown scenario {other}")),
    };
    let (kind, phase) = match name {
        "numpad-small" => (ExerciseKind::Numpad, Phase::Numpad),
        "row-and-pad" => (ExerciseKind::NumberRow, Phase::NumberRow),
        _ => (ExerciseKind::Words, Phase::AdaptiveWords),
    };
    let exercises = targets
        .iter()
        .map(|text| {
            cadence_core::validate_target(text, Layout::Us)
                .map(|units| PlannedExercise::plain(units, kind))
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
        probe: if name == "contamination" {
            ProbePlan::Planned(cadence_trainer::PlannedProbe {
                passage: cadence_core::PassageId::new(77).map_err(|e| e.to_string())?,
                strata: 0b0101,
                same_strata: true,
            })
        } else {
            ProbePlan::NotDue
        },
        pace: Vec::new(),
        retired_due: 0,
        phases: if name == "contamination" {
            // A transfer probe first (#86 journey 12), then words.
            let probe =
                cadence_core::validate_target("Probe text for measuring transfer.", Layout::Us)
                    .map_err(|error| error.to_string())?;
            vec![
                PlannedPhase {
                    phase: Phase::Probe,
                    seconds: 30,
                    exercises: vec![PlannedExercise {
                        units: probe,
                        kind: ExerciseKind::Probe,
                        words: Vec::new(),
                        passage: Some(cadence_core::PassageId::new(77).map_err(|e| e.to_string())?),
                        caption: Some("Transfer probe".to_string()),
                        target: None,
                        spans: Vec::new(),
                    }],
                },
                PlannedPhase {
                    phase,
                    seconds,
                    exercises,
                },
            ]
        } else if name == "row-and-pad" {
            // The same digits on the number row, then on the numpad (L-01).
            let pad = cadence_core::validate_target("1234", Layout::Us)
                .map_err(|error| error.to_string())?;
            vec![
                PlannedPhase {
                    phase,
                    seconds,
                    exercises,
                },
                PlannedPhase {
                    phase: Phase::Numpad,
                    seconds,
                    exercises: vec![PlannedExercise::plain(pad, ExerciseKind::Numpad)],
                },
            ]
        } else {
            vec![PlannedPhase {
                phase,
                seconds,
                exercises,
            }]
        },
    })
}

/// The commented config the `commented-config` and `config-conflict`
/// scenarios start from (#86).
const COMMENTED_CONFIG: &str = "# Cadence settings, edited by hand\n[display]\n# the colours\ntheme = \"dark\"   # dark, light or terminal\n\n[session]\nminutes = 15\n";

/// The profile store and settings behind a headless session. The settings
/// file is the profile's own `config.toml`, never the owner's.
struct StoreServices {
    store: ProgressStore,
    day: i64,
    /// `<profile>/config.toml`.
    config: PathBuf,
    /// The `save-error` scenario: every save fails (a fixture fault).
    failing: bool,
    /// The `config-conflict` scenario: the next write meets an external
    /// edit (a fixture of the file system, not product logic).
    edit_before_write: bool,
    /// The `config-changes` scenario: the next reload meets an external,
    /// invalid edit.
    invalid_before_reload: bool,
}

impl StoreServices {
    /// The settings now in the profile's file, applied to `engine`.
    fn with_settings(&self, engine: Engine) -> Result<Engine, String> {
        let loaded = cadence_data::config_write::reload(&self.config).map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        Ok(engine.with_settings(
            &cadence_runtime::config_worker::values(&loaded.config),
            loaded.digest,
        ))
    }

    /// The daily plan for the profile's settings, or a Train or Numpad plan.
    fn daily(&self, track: Option<(Phase, u64)>) -> Result<Engine, String> {
        let policy = cadence_trainer::parse_policy(crate::POLICY.as_bytes())
            .map_err(|error| error.to_string())?;
        let bootstrap = self
            .store
            .bootstrap(policy.learning.history_window())
            .map_err(|error| error.to_string())?;
        let loaded = cadence_data::config_write::reload(&self.config).map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        let mut options = crate::apply_config(crate::cli::Options::defaults(), &loaded.config);
        if let Some((phase, minutes)) = track {
            options.track = Some(phase);
            options.minutes = Some(minutes);
        }
        let engine = crate::build_engine(&options, &policy, &bootstrap, self.day)
            .map_err(|error| error.to_string())?;
        self.with_settings(engine)
    }
}

/// Seeds the `contamination` scenario's history (#86): two standard live
/// sessions, a synthetic one, an imported-text one and a standard one,
/// oldest first.
fn seed_contamination(store: &mut ProgressStore) -> Result<(), String> {
    use cadence_core::progress::{
        ContentClass, ProgressStatus, SessionProgress, Track, TrackProgress,
    };
    let session = |id: u64, class: ContentClass| -> Result<Checkpoint, String> {
        Checkpoint::new(
            SessionId::new(id).map_err(|e| e.to_string())?,
            cadence_core::CommitSeq::from_raw(1),
            SessionProgress {
                profile: "default".into(),
                day: 20_000_i64.saturating_add(i64::try_from(id).unwrap_or(0)),
                utc_offset_minutes: None,
                layout: Some(Layout::Us),
                content_class: class,
                pack_digest: (class == ContentClass::Imported).then(|| "ab".repeat(32)),
                policy_digest: "policy-fixture".into(),
                corpus_digest: "corpus-fixture".into(),
                status: ProgressStatus::Completed,
                active_micros: 60_000_000,
                tracks: vec![TrackProgress {
                    first_correct: 90,
                    first_attempted: 100,
                    correct_attempts: 100,
                    content_attempts: 110,
                    correct_units: 90,
                    active_micros: 60_000_000,
                    ..TrackProgress::empty(Track::Words)
                }],
            },
            cadence_core::progress::Evidence::None,
        )
        .map_err(|e| e.to_string())
    };
    for (id, class, synthetic) in [
        (1, ContentClass::Standard, false),
        (2, ContentClass::Standard, false),
        (3, ContentClass::Standard, true),
        (4, ContentClass::Imported, false),
        (5, ContentClass::Standard, false),
    ] {
        let checkpoint = session(id, class)?;
        store
            .apply_checkpoint(&checkpoint)
            .map_err(|e| e.to_string())?;
        if synthetic {
            store
                .mark_synthetic(checkpoint.key.session)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

impl Services for StoreServices {
    fn engine(&mut self, scenario: Option<&str>) -> Result<Engine, String> {
        self.failing = scenario == Some("save-error");
        self.edit_before_write = scenario == Some("config-conflict");
        self.invalid_before_reload = scenario == Some("config-changes");
        if matches!(
            scenario,
            Some("commented-config" | "config-conflict" | "config-changes")
        ) {
            std::fs::write(&self.config, COMMENTED_CONFIG).map_err(|error| error.to_string())?;
        }
        if scenario == Some("contamination") {
            seed_contamination(&mut self.store)?;
        }
        match scenario {
            Some(name) => {
                let engine =
                    Engine::new_plan(scenario_plan(name)?).map_err(|error| error.to_string())?;
                self.with_settings(engine)
            }
            None => {
                // The daily plan on a profile with no settings file is a
                // first run (#78), exactly as live.
                let first_run = !self.config.exists();
                let engine = self.daily(None)?;
                Ok(if first_run {
                    engine.with_first_run()
                } else {
                    engine
                })
            }
        }
    }

    fn storage(&mut self) -> Result<(SessionId, i64, HistorySummary), String> {
        let bootstrap = self.store.bootstrap(1).map_err(|error| error.to_string())?;
        Ok((bootstrap.next_session, self.day, bootstrap.history))
    }

    fn save(&mut self, checkpoint: &Checkpoint) -> Result<(), String> {
        if self.failing {
            return Err("fixture fault: the save service refuses".to_string());
        }
        self.store
            .apply_checkpoint(checkpoint)
            .map_err(|error| error.to_string())?;
        self.store
            .mark_synthetic(checkpoint.key.session)
            .map_err(|error| error.to_string())
    }

    fn settings(
        &mut self,
        request: &cadence_engine::settings::SettingsRequest,
    ) -> cadence_engine::jobs::JobOutcome {
        if self.edit_before_write
            && matches!(
                request,
                cadence_engine::settings::SettingsRequest::Write { .. }
            )
        {
            self.edit_before_write = false;
            let edited = format!("{COMMENTED_CONFIG}# edited in another window\n");
            if let Err(error) = std::fs::write(&self.config, edited) {
                return cadence_engine::jobs::JobOutcome::Failed(error.to_string());
            }
        }
        if self.invalid_before_reload
            && matches!(request, cadence_engine::settings::SettingsRequest::Reload)
        {
            self.invalid_before_reload = false;
            if let Err(error) = std::fs::write(&self.config, "[session]\nminutes = 7\n") {
                return cadence_engine::jobs::JobOutcome::Failed(error.to_string());
            }
        }
        // The profile directory is host-specific: diagnostics name the file
        // relative to it, so a journey replays identically anywhere.
        match cadence_runtime::config_worker::answer(Some(&self.config), request.clone()) {
            cadence_engine::jobs::JobOutcome::Failed(reason) => {
                let profile = self
                    .config
                    .parent()
                    .map(|dir| format!("{}/", dir.display()))
                    .unwrap_or_default();
                cadence_engine::jobs::JobOutcome::Failed(reason.replace(&profile, "<profile>/"))
            }
            other => other,
        }
    }

    fn history(
        &mut self,
        limit: usize,
    ) -> Result<Vec<cadence_core::progress::HistoryPoint>, String> {
        self.store
            .history_points(limit)
            .map_err(|error| error.to_string())
    }

    fn rebuild(&mut self, track: Option<(Phase, u64)>) -> Result<Engine, String> {
        self.daily(track)
    }

    fn config_text(&self) -> Option<String> {
        std::fs::read_to_string(&self.config).ok()
    }
}

/// The headless driver's calendar day: virtual, like its clock, so a
/// journey (and the daily plan's seed) replays identically on any date.
const VIRTUAL_DAY: i64 = 20_500;

fn utc_day() -> Result<i64, Box<dyn Error>> {
    Ok(VIRTUAL_DAY)
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
    record: Option<(&Path, bool)>,
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
    let result = serve(directory, script, record);
    if let Some(dir) = temporary {
        std::fs::remove_dir_all(dir)?;
    }
    result
}

fn serve(
    directory: &Path,
    script: Option<&Path>,
    record: Option<(&Path, bool)>,
) -> Result<(), Box<dyn Error>> {
    use cadence_driver::trace::{Header, Recorder};
    let mut recorder = record.map(|(_, content)| {
        Recorder::new(&Header {
            version: env!("CARGO_PKG_VERSION").to_string(),
            record_content: content,
        })
    });
    let store = ProgressStore::open_in(directory)?;
    let mut host = Headless::new(StoreServices {
        store,
        day: utc_day()?,
        config: directory.join("config.toml"),
        failing: false,
        edit_before_write: false,
        invalid_before_reload: false,
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
        if let Some(recorder) = recorder.as_mut() {
            recorder.record(&line, &response);
        }
        writeln!(out, "{response}")?;
        out.flush()?;
        if host.finished() {
            break;
        }
    }
    // End of input: finish and flush as `quit` would.
    if host.engine().is_some() && !host.finished() {
        let quit = br#"{"v":1,"id":"18446744073709551615","op":"quit","args":{"flush":true}}"#;
        let response = protocol.handle(quit, &mut host);
        if let Some(recorder) = recorder.as_mut() {
            recorder.record(quit, &response);
        }
        writeln!(out, "{response}")?;
    }
    if let (Some(recorder), Some((path, _))) = (recorder, record) {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(recorder.text().as_bytes())?;
        file.sync_all()?;
    }
    Ok(())
}

/// The captures of a matching replay, or the divergence report.
type Replayed = Result<Vec<serde_json::Map<String, serde_json::Value>>, String>;

/// `cadence replay FILE --read-only`: re-executes a trace on a fresh
/// temporary profile (never one it was not given) and compares every
/// response; prints the earliest divergence and fails on one.
///
/// # Errors
/// I/O failures and any divergence.
pub fn replay(trace: &Path, capture_dir: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let text = std::fs::read_to_string(trace)?;
    let dir = ephemeral_dir()?;
    let outcome = (|| -> Result<Replayed, Box<dyn Error>> {
        let store = ProgressStore::open_in(&dir)?;
        let mut host = Headless::new(StoreServices {
            store,
            day: utc_day()?,
            config: dir.join("config.toml"),
            failing: false,
            edit_before_write: false,
            invalid_before_reload: false,
        });
        Ok(cadence_driver::trace::replay(&text, &mut host).map_err(|d| d.report()))
    })();
    std::fs::remove_dir_all(&dir)?;
    match outcome? {
        Ok(captures) => {
            if let Some(out) = capture_dir {
                std::fs::create_dir_all(out)?;
                for (index, capture) in captures.iter().enumerate() {
                    std::fs::write(
                        out.join(format!("capture-{index:03}.json")),
                        serde_json::to_vec_pretty(capture)?,
                    )?;
                }
            }
            println!(
                "{}",
                serde_json::json!({"replayed": true, "captures": captures.len()})
            );
            Ok(())
        }
        Err(report) => {
            println!("{report}");
            Err("the replay diverged from the trace".into())
        }
    }
}

/// `cadence doctor --json`: version, schema, paths, terminal facts, disk use
/// and each session's chunk and derived watermarks (#11), as one JSON object
/// on stdout. `--rebuild-derived` first recomputes every chunked session's
/// derived rows from its chunks and lists the sessions that differed.
///
/// # Errors
/// Path resolution or encoding failures, and a failed rebuild.
pub fn doctor(profile: Option<&Path>, rebuild_derived: bool) -> Result<(), Box<dyn Error>> {
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
    // The store is opened only when a database exists: doctor never creates
    // one.
    let mut rebuilt = None;
    let store = match &paths {
        Ok(paths) if bytes.is_some() => match cadence_data::ProgressStore::open_in(&paths.data) {
            Ok(mut store) => {
                if rebuild_derived {
                    rebuilt = Some(store.rebuild_derived()?);
                }
                serde_json::json!({
                    "sessions": store.watermarks()?.iter().map(|mark| serde_json::json!({
                        "session": mark.session,
                        "chunk_watermark": mark.chunks,
                        "derived_watermark": mark.derived,
                        "legacy": mark.legacy,
                    })).collect::<Vec<_>>(),
                })
            }
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        },
        Ok(_) | Err(_) => {
            if rebuild_derived {
                return Err("no progress database to rebuild".into());
            }
            serde_json::Value::Null
        }
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
        "store": store,
        "rebuilt_sessions": rebuilt,
        "terminal": {
            "term": std::env::var("TERM").ok(),
            "stdin_is_terminal": std::io::stdin().is_terminal(),
            "stdout_is_terminal": std::io::stdout().is_terminal(),
        },
    });
    println!("{report}");
    Ok(())
}
