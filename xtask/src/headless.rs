//! The `headless` gate (H-05, #73): builds the binary and replays every
//! committed journey (`fixtures/journeys/*.jsonl`, recorded traces) with
//! `cadence replay --read-only`. It fails when a journey's response
//! differs, when the binary is missing, or when no journey is found. Time
//! is virtual: nothing sleeps.

use std::path::PathBuf;
use std::process::Command;

fn root() -> Result<PathBuf, String> {
    let mut root = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").map_err(|_unset| "CARGO_MANIFEST_DIR not set")?,
    );
    root.pop();
    Ok(root)
}

/// Builds `cadence` and replays each journey.
///
/// # Errors
/// The first failing journey, a missing binary or no journeys.
pub fn check() -> Result<(), String> {
    let root = root()?;
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_unset| "cargo".into()))
        .args(["build", "--locked", "-p", "cadence"])
        .current_dir(&root)
        .status()
        .map_err(|error| format!("cannot build cadence: {error}"))?;
    if !status.success() {
        return Err("building cadence failed".into());
    }
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let binary = target.join("debug").join("cadence");
    if !binary.is_file() {
        return Err(format!(
            "the cadence binary is missing at {}",
            binary.display()
        ));
    }
    let journeys = root.join("fixtures/journeys");
    run_journeys(&binary, &journeys)?;
    let count = std::fs::read_dir(&journeys)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .count();
    if count != JOURNEYS {
        return Err(format!("{count} journeys, H-05 names {JOURNEYS}"));
    }
    tampered_traces_fail(&binary, &journeys.join("11-full-trace-replay.jsonl"))
}

/// Replays every `*.jsonl` journey in `dir` with `binary`.
///
/// # Errors
/// The first failing journey, or no journeys.
pub fn run_journeys(binary: &std::path::Path, dir: &std::path::Path) -> Result<(), String> {
    let mut journeys: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read {}: {error}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    journeys.sort();
    if journeys.is_empty() {
        return Err(format!("no journeys found in {}", dir.display()));
    }
    for journey in &journeys {
        let output = Command::new(binary)
            .arg("replay")
            .arg(journey)
            .arg("--read-only")
            .output()
            .map_err(|error| format!("cannot run {}: {error}", binary.display()))?;
        if !output.status.success() {
            return Err(format!(
                "journey {} diverged: {}",
                journey.display(),
                String::from_utf8_lossy(&output.stdout).trim()
            ));
        }
        println!(
            "headless journey ok: {}",
            journey
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        );
    }
    println!("headless gate: {} journeys replayed", journeys.len());
    Ok(())
}

/// The journeys H-05 names (`docs/06-headless-tests.md`).
pub const JOURNEYS: usize = 12;

/// Journey 11 (H-05): the full trace replays (above), and a copy with one
/// counter changed, or one request and its response missing, fails at its
/// first divergence.
///
/// # Errors
/// A tampered trace that replays, or one that fails at the wrong line.
pub fn tampered_traces_fail(
    binary: &std::path::Path,
    journey: &std::path::Path,
) -> Result<(), String> {
    let text = std::fs::read_to_string(journey)
        .map_err(|error| format!("cannot read {}: {error}", journey.display()))?;
    let mut lines: Vec<serde_json::Value> = text
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()
        .map_err(|error| format!("{}: {error}", journey.display()))?;
    // The first response reporting track counters.
    let index = lines
        .iter()
        .position(|record| {
            record
                .pointer("/line/result/tracks/0/first_pass/correct")
                .is_some_and(serde_json::Value::is_string)
        })
        .ok_or("journey 11 has no response with track counters")?;
    let dir = std::env::temp_dir().join(format!("cadence-tamper-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let replay = |name: &str, records: &[serde_json::Value]| -> Result<serde_json::Value, String> {
        let path = dir.join(name);
        let body: String = records.iter().map(|r| format!("{r}\n")).collect();
        std::fs::write(&path, body).map_err(|error| error.to_string())?;
        let output = Command::new(binary)
            .arg("replay")
            .arg(&path)
            .arg("--read-only")
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Err(format!(
                "the tampered trace {name} replayed without a divergence"
            ));
        }
        serde_json::from_slice(&output.stdout).map_err(|error| format!("{name}: {error}"))
    };
    let mut changed = lines.clone();
    if let Some(slot) = changed
        .get_mut(index)
        .and_then(|record| record.pointer_mut("/line/result/tracks/0/first_pass/correct"))
    {
        *slot = serde_json::json!("999999");
    }
    let report = replay("changed.jsonl", &changed)?;
    let expected = index.saturating_add(1);
    if report.get("diverged_at_line") != Some(&serde_json::json!(expected)) {
        return Err(format!(
            "a changed counter on line {expected} was reported as {report}"
        ));
    }
    // Remove the last typed input before the counters (a request that
    // changes state) and its response: the replay must diverge where the
    // missing input would have counted.
    let typed = lines
        .iter()
        .take(index)
        .rposition(|record| {
            record.get("type").and_then(serde_json::Value::as_str) == Some("request")
                && matches!(
                    record
                        .pointer("/line/op")
                        .and_then(serde_json::Value::as_str),
                    Some("text" | "key")
                )
        })
        .ok_or("journey 11 has no typed input before its counters")?;
    lines.drain(typed..=typed.saturating_add(1));
    let report = replay("missing.jsonl", &lines)?;
    let at = report
        .get("diverged_at_line")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let first = u64::try_from(typed).unwrap_or(u64::MAX);
    if at < first {
        return Err(format!(
            "a missing request was reported before it: {report}"
        ));
    }
    std::fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
    println!(
        "headless gate: tampered traces fail at their first divergence (lines {expected} and {at})"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::run_journeys;

    #[test]
    fn the_gate_fails_without_journeys_or_binary() -> Result<(), Box<dyn std::error::Error>> {
        let empty = tempfile::tempdir()?;
        let error = run_journeys(std::path::Path::new("/bin/true"), empty.path()).unwrap_err();
        assert!(error.contains("no journeys found"), "{error}");
        std::fs::write(empty.path().join("one.jsonl"), "{}\n")?;
        let error = run_journeys(&empty.path().join("missing-binary"), empty.path()).unwrap_err();
        assert!(error.contains("cannot run"), "{error}");
        // A journey whose replay fails is reported.
        let error = run_journeys(std::path::Path::new("/bin/false"), empty.path()).unwrap_err();
        assert!(error.contains("diverged"), "{error}");
        Ok(())
    }
}
