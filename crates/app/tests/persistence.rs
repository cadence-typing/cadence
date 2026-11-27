//! The persistence campaigns (H-05 "Native persistence integration", D-02,
//! #88): the real binary on temporary profiles with real SQLite. Kills are
//! the store's test-only fault hooks (`CADENCE_FAULT_STORE`, compiled only
//! with `test-faults`): the process aborts at a named point, as a kill
//! would. After every run the profile is reopened and must hold its earlier
//! acknowledged history unchanged, report the committed watermark, hold no
//! fact twice (receipts once per sequence, derived rows equal to their
//! chunks) and never be reset; the bad states end in a visible diagnostic
//! that changes nothing. These are process-crash tests, not proof of
//! power-loss safety.
//!
//! `CADENCE_CALIBRATE_DOUBLE_APPLY=1` makes every run fold its chunks twice:
//! the campaign must then fail (`cargo xtask gate persistence-linux` checks
//! both).
#![cfg(feature = "test-faults")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use cadence_data::ProgressStore;
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn cadence() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cadence"))
}

fn calibration() -> Option<&'static str> {
    std::env::var_os("CADENCE_CALIBRATE_DOUBLE_APPLY").map(|_| "double-apply")
}

/// A short daily-small session with many checkpoints and two closed
/// exercises (chunks).
fn script(dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    script_named(dir, "campaign.jsonl", false)
}

/// The campaign script, optionally capturing the screen before it quits.
fn script_named(
    dir: &Path,
    name: &str,
    capture: bool,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let words = [
        "street",
        "strong",
        "almost",
        "string",
        "first",
        "stream",
        "understand",
    ];
    let mut requests = vec![
        serde_json::json!({"op": "hello", "args": {"scenario": "daily-small"}}),
        serde_json::json!({"op": "action", "args": {"name": "Begin"}}),
    ];
    for round in 0..6 {
        let text: String = words
            .iter()
            .cycle()
            .skip(round)
            .take(4)
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        requests.push(serde_json::json!({"op": "text", "args": {"text": text, "inter_key_microseconds": 200_000}}));
        requests.push(serde_json::json!({"op": "advance", "args": {"microseconds": 2_500_000}}));
    }
    if capture {
        requests.push(serde_json::json!({"op": "capture", "args": {"format": "text"}}));
    }
    requests.push(serde_json::json!({"op": "quit", "args": {"flush": true}}));
    let mut lines = String::new();
    for (id, mut request) in (1_u64..).zip(requests) {
        if let Some(object) = request.as_object_mut() {
            object.insert("v".into(), 1.into());
            object.insert("id".into(), id.to_string().into());
        }
        lines.push_str(&request.to_string());
        lines.push('\n');
    }
    let path = dir.join(name);
    std::fs::write(&path, lines)?;
    Ok(path)
}

/// Runs the session on `profile`, with `fault` (or the calibration) armed.
fn session(profile: &Path, script: &Path, fault: Option<&str>) -> std::io::Result<Output> {
    let mut command = cadence();
    command
        .args(["headless", "--script"])
        .arg(script)
        .arg("--profile-dir")
        .arg(profile);
    command.env_remove("CADENCE_FAULT_STORE");
    if let Some(fault) = fault
        .map(str::to_string)
        .or_else(|| calibration().map(str::to_string))
    {
        command.env("CADENCE_FAULT_STORE", fault);
    }
    command.output()
}

fn doctor(profile: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let output = cadence()
        .args(["doctor", "--json", "--profile-dir"])
        .arg(profile)
        .env_remove("CADENCE_FAULT_STORE")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

/// The newest acknowledged sequence the run reported before it ended.
fn acknowledged(output: &Output) -> u64 {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|response| {
            response
                .pointer("/result/durable_commit_seq")
                .and_then(Value::as_str)
                .and_then(|seq| seq.parse().ok())
        })
        .max()
        .unwrap_or(0)
}

/// What a reopened profile holds, after checking it holds nothing twice.
#[derive(Debug, PartialEq, Eq)]
struct Facts {
    /// Each session's (commit, chunk, derived) watermarks, by session.
    sessions: Vec<(u64, u64, u64, u64)>,
    /// The checkpoints themselves, for "unchanged" comparisons.
    history: Vec<cadence_data::StoredSession>,
}

fn facts(profile: &Path) -> Result<Facts, Box<dyn std::error::Error>> {
    let mut store = ProgressStore::open_in(profile)?;
    let history = store.session_history()?;
    // Derived rows equal the fold of their chunks (EvidenceCountedOnce).
    let differed = store.rebuild_derived()?;
    assert!(
        differed.is_empty(),
        "derived rows differ from their chunks in sessions {differed:?}"
    );
    let sessions = store
        .watermarks()?
        .into_iter()
        .map(|mark| (mark.session, mark.commit, mark.chunks, mark.derived))
        .collect::<Vec<_>>();
    for (session, commit, chunks, derived) in &sessions {
        assert_eq!(chunks, derived, "session {session}: derived watermark");
        let stored = history
            .iter()
            .find(|s| s.checkpoint.key.session.get() == *session)
            .ok_or("watermark without a session")?;
        assert_eq!(
            stored.checkpoint.key.sequence.get(),
            *commit,
            "session {session}: commit"
        );
    }
    Ok(Facts { sessions, history })
}

fn copy_profile(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// A profile holding one earlier, fully acknowledged session.
fn seeded(dir: &Path, script: &Path) -> Result<(PathBuf, Facts), Box<dyn std::error::Error>> {
    let profile = dir.join("seed");
    std::fs::create_dir_all(&profile)?;
    let output = session(&profile, script, None)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let facts = facts(&profile)?;
    Ok((profile, facts))
}

/// The kill campaign: a kill before every statement of every checkpoint
/// transaction, before each commit, and after each commit before its
/// acknowledgement.
#[test]
fn kills_around_every_checkpoint_lose_nothing_acknowledged() -> TestResult {
    let dir = tempfile::tempdir()?;
    let script = script(dir.path())?;
    let (seed, before) = seeded(dir.path(), &script)?;
    let earlier = before.history.first().ok_or("seed session")?.clone();
    let mut runs = 0;
    for point in [
        "checkpoint-commit",
        "checkpoint-acknowledge",
        "checkpoint-statement",
    ] {
        for nth in 1_u64.. {
            let profile = dir.path().join(format!("{point}-{nth}"));
            std::fs::create_dir_all(&profile)?;
            copy_profile(&seed, &profile)?;
            let output = session(&profile, &script, Some(&format!("{point}:{nth}")))?;
            runs += 1;
            let after = facts(&profile)?;
            // The earlier session is untouched by anything this run did.
            assert_eq!(
                after.history.first(),
                Some(&earlier),
                "{point}:{nth}: earlier history"
            );
            if output.status.success() {
                // The fault never fired: the point was past the session's
                // last checkpoint. Every point was reached at least once.
                assert!(nth > 1, "{point} was never reached");
                break;
            }
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("test fault: abort"),
                "{point}:{nth}: the run ended for another reason: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let commit = after.sessions.get(1).map_or(0, |(_, commit, _, _)| *commit);
            // Nothing acknowledged to the engine is lost.
            assert!(
                commit >= acknowledged(&output),
                "{point}:{nth}: lost an acknowledgement"
            );
            match point {
                // Killed before the n-th commit: n - 1 are durable.
                "checkpoint-commit" => assert_eq!(commit, nth - 1, "{point}:{nth}"),
                // Killed after the n-th commit, before its acknowledgement.
                "checkpoint-acknowledge" => assert_eq!(commit, nth, "{point}:{nth}"),
                _ => {}
            }
            // The reported watermark is the committed one.
            let report = doctor(&profile)?;
            let reported = report
                .pointer("/store/sessions/1/commit_watermark")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            assert_eq!(reported, commit, "{point}:{nth}: reported watermark");
        }
    }
    assert!(runs > 40, "a campaign of {runs} kills");
    Ok(())
}

/// A kill at every step of every migration path (3 to 7 through 6 to 7),
/// and before its commit: the file keeps its schema and every session, and
/// the next open migrates it.
#[test]
fn interrupted_migrations_keep_the_old_schema_and_then_migrate() -> TestResult {
    let dir = tempfile::tempdir()?;
    for schema in 3..cadence_data::SUPPORTED_SCHEMA {
        let steps = cadence_data::SUPPORTED_SCHEMA - schema;
        let points = (1..=steps)
            .map(|nth| format!("migration:{nth}"))
            .chain(["migration-commit".to_string()]);
        for fault in points {
            let profile = dir
                .path()
                .join(format!("v{schema}-{}", fault.replace(':', "-")));
            cadence_data::fixtures::populated_profile(&profile, schema)?;
            let output = cadence()
                .args(["doctor", "--json", "--profile-dir"])
                .arg(&profile)
                .env("CADENCE_FAULT_STORE", &fault)
                .output()?;
            assert!(
                !output.status.success(),
                "schema {schema} {fault}: not interrupted"
            );
            assert_eq!(
                cadence_data::fixtures::schema_version(&profile)?,
                schema,
                "schema {schema} {fault}: half-migrated"
            );
            let reopened = facts(&profile)?;
            assert_eq!(
                reopened.history.len(),
                2,
                "schema {schema} {fault}: sessions"
            );
            assert_eq!(
                cadence_data::fixtures::schema_version(&profile)?,
                cadence_data::SUPPORTED_SCHEMA
            );
        }
    }
    Ok(())
}

fn bytes(profile: &Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(profile.join(cadence_data::paths::DATABASE_FILE))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A newer schema, a held writer lock, an unwritable directory and a
/// truncated file each end in a diagnostic and change nothing.
#[test]
fn bad_states_are_named_and_change_nothing() -> TestResult {
    let dir = tempfile::tempdir()?;
    let script = script(dir.path())?;
    let (seed, before) = seeded(dir.path(), &script)?;

    // A newer schema is refused before any write.
    let newer = dir.path().join("newer");
    std::fs::create_dir_all(&newer)?;
    copy_profile(&seed, &newer)?;
    cadence_data::fixtures::set_schema_version(&newer, cadence_data::SUPPORTED_SCHEMA + 1)?;
    let file = bytes(&newer)?;
    let output = session(&newer, &script, None)?;
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("newer than supported"),
        "{}",
        stderr(&output)
    );
    assert_eq!(bytes(&newer)?, file, "a newer file is never changed");

    // A held writer lock: the second writer is refused.
    let locked = dir.path().join("locked");
    std::fs::create_dir_all(&locked)?;
    copy_profile(&seed, &locked)?;
    {
        let _writer = ProgressStore::open_in(&locked)?;
        let output = session(&locked, &script, None)?;
        assert!(!output.status.success());
        assert!(
            stderr(&output).contains("another cadence writer"),
            "{}",
            stderr(&output)
        );
    }
    assert_eq!(facts(&locked)?.history, before.history);

    // An unwritable profile directory.
    let readonly = dir.path().join("readonly");
    std::fs::create_dir_all(&readonly)?;
    copy_profile(&seed, &readonly)?;
    set_mode(&readonly, 0o500)?;
    let output = session(&readonly, &script, None);
    set_mode(&readonly, 0o700)?;
    let output = output?;
    assert!(!output.status.success(), "a read-only profile was written");
    assert!(!stderr(&output).is_empty(), "no diagnostic");
    assert_eq!(facts(&readonly)?.history, before.history);

    // A truncated database is reported as damaged, never reset.
    let truncated = dir.path().join("truncated");
    std::fs::create_dir_all(&truncated)?;
    copy_profile(&seed, &truncated)?;
    let whole = bytes(&truncated)?;
    let half = whole.get(..whole.len().div_ceil(2)).ok_or("half")?.to_vec();
    std::fs::write(truncated.join(cadence_data::paths::DATABASE_FILE), &half)?;
    let output = session(&truncated, &script, None)?;
    assert!(!output.status.success(), "a truncated database was used");
    let report = doctor(&truncated)?;
    assert!(
        report.pointer("/store/error").is_some(),
        "doctor names it: {report}"
    );
    assert_eq!(
        bytes(&truncated)?,
        half,
        "the damaged file is kept as it was"
    );
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

/// A full disk: writes beyond a file-size limit fail (SIGXFSZ ignored, so
/// SQLite sees the error). The failed save is named, and the earlier
/// history survives.
#[test]
fn a_full_disk_fails_the_save_visibly_and_keeps_the_history() -> TestResult {
    let dir = tempfile::tempdir()?;
    let script = script(dir.path())?;
    let shown = script_named(dir.path(), "shown.jsonl", true)?;
    let (seed, before) = seeded(dir.path(), &script)?;
    let full = dir.path().join("full");
    std::fs::create_dir_all(&full)?;
    copy_profile(&seed, &full)?;
    let limit = bytes(&full)?.len() + 4_096;
    let output = Command::new("sh")
        .arg("-c")
        .arg(r#"trap "" XFSZ; exec prlimit --fsize="$1" "$2" headless --script "$3" --profile-dir "$4""#)
        .arg("sh")
        .arg(limit.to_string())
        .arg(env!("CARGO_BIN_EXE_cadence"))
        .arg(&shown)
        .arg(&full)
        .env_remove("CADENCE_FAULT_STORE")
        .output()?;
    let screen: String = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|r| {
            r.pointer("/result/text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect();
    assert!(
        screen.contains("Save failed"),
        "the failed save is not on screen: {screen}"
    );
    let after = facts(&full)?;
    // What was acknowledged before the disk filled is what is stored.
    let commit = after.sessions.get(1).map_or(0, |(_, commit, _, _)| *commit);
    assert_eq!(commit, acknowledged(&output), "the stored watermark");
    assert_eq!(
        after.history.first(),
        before.history.first(),
        "the earlier session"
    );
    Ok(())
}

/// A backup taken of a profile killed during practice holds exactly what
/// was committed; restore validates the backup before replacing anything,
/// keeps the replaced database, and refuses a damaged backup or a running
/// writer without changing the profile.
#[test]
fn backup_during_practice_and_validated_restore() -> TestResult {
    let dir = tempfile::tempdir()?;
    let script = script(dir.path())?;
    let (seed, _) = seeded(dir.path(), &script)?;
    let practice = dir.path().join("practice");
    std::fs::create_dir_all(&practice)?;
    copy_profile(&seed, &practice)?;
    let output = session(&practice, &script, Some("checkpoint-commit:5"))?;
    assert!(!output.status.success());
    let backup = dir.path().join("backup.sqlite3");
    let made = cadence()
        .args(["backup", "--output"])
        .arg(&backup)
        .arg("--profile-dir")
        .arg(&practice)
        .output()?;
    assert!(made.status.success(), "{}", stderr(&made));
    let committed = facts(&practice)?;
    assert_eq!(committed.sessions.get(1).map(|s| s.1), Some(4));

    // Restore into another profile: the replaced database is kept.
    let target = dir.path().join("target");
    std::fs::create_dir_all(&target)?;
    copy_profile(&seed, &target)?;
    let replaced = bytes(&target)?;
    let restore = |profile: &Path, from: &Path| {
        cadence()
            .args(["restore", "--from"])
            .arg(from)
            .arg("--profile-dir")
            .arg(profile)
            .env_remove("CADENCE_FAULT_STORE")
            .output()
    };
    let restored = restore(&target, &backup)?;
    assert!(restored.status.success(), "{}", stderr(&restored));
    assert_eq!(facts(&target)?.history, committed.history);
    assert_eq!(
        std::fs::read(target.join(cadence_data::PREVIOUS_DATABASE_FILE))?,
        replaced,
        "the replaced database is kept"
    );

    // A damaged backup is refused before anything is replaced.
    let damaged = dir.path().join("damaged.sqlite3");
    let whole = std::fs::read(&backup)?;
    std::fs::write(
        &damaged,
        whole.get(..whole.len().div_ceil(2)).ok_or("half")?,
    )?;
    let other = dir.path().join("other");
    std::fs::create_dir_all(&other)?;
    copy_profile(&seed, &other)?;
    let kept = bytes(&other)?;
    let refused = restore(&other, &damaged)?;
    assert!(!refused.status.success(), "a damaged backup was restored");
    assert_eq!(bytes(&other)?, kept);

    // A running writer: restore waits for none and replaces nothing.
    {
        let _writer = ProgressStore::open_in(&other)?;
        let refused = restore(&other, &backup)?;
        assert!(!refused.status.success());
        assert!(
            stderr(&refused).contains("another cadence writer"),
            "{}",
            stderr(&refused)
        );
    }
    assert_eq!(bytes(&other)?, kept);

    // An interrupted restore leaves a working database either way.
    for point in ["restore-staged", "restore-swap"] {
        let profile = dir.path().join(point);
        std::fs::create_dir_all(&profile)?;
        copy_profile(&seed, &profile)?;
        let output = cadence()
            .args(["restore", "--from"])
            .arg(&backup)
            .arg("--profile-dir")
            .arg(&profile)
            .env("CADENCE_FAULT_STORE", point)
            .output()?;
        assert!(!output.status.success(), "{point}: not interrupted");
        // The staged copy is left beside it; the database opens as it was.
        let after = facts(&profile)?;
        assert_eq!(
            after.history.len(),
            1,
            "{point}: the current database survives"
        );
    }
    Ok(())
}

/// The negative calibration in-process: with chunks folded twice, the
/// campaign's own check finds the double count.
#[test]
fn the_calibration_fault_is_detected() -> TestResult {
    let dir = tempfile::tempdir()?;
    let script = script(dir.path())?;
    let profile = dir.path().join("double");
    std::fs::create_dir_all(&profile)?;
    let output = session(&profile, &script, Some("double-apply"))?;
    assert!(output.status.success(), "{}", stderr(&output));
    let mut store = ProgressStore::open_in(&profile)?;
    assert_eq!(store.rebuild_derived()?, vec![1], "a double fold is found");
    Ok(())
}
