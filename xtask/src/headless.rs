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
    run_journeys(&binary, &root.join("fixtures/journeys"))
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
