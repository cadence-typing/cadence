//! VR-01 (#91): live, headless and replay execute the same reducer. The
//! headless driver and the runtime reach scoring only through
//! `cadence-engine`: neither depends on the trainer or the kernel, and
//! neither names a scoring, ledger or credit routine of `cadence-core`.

use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn crate_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(name)
}

fn sources(dir: &Path, out: &mut Vec<(PathBuf, String)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push((path.clone(), std::fs::read_to_string(&path)?));
        }
    }
    Ok(())
}

/// Code, not comments: a doc comment may name the kernel it relies on.
fn code(text: &str) -> String {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn driver_and_runtime_reach_scoring_only_through_the_engine() -> TestResult {
    for name in ["driver", "runtime"] {
        let manifest = std::fs::read_to_string(crate_dir(name).join("Cargo.toml"))?;
        let dependencies = manifest
            .split("[dependencies]")
            .nth(1)
            .and_then(|rest| rest.split("\n[").next())
            .ok_or("no [dependencies]")?;
        for forbidden in ["cadence-trainer", "cadence-kernel"] {
            assert!(
                !dependencies.contains(forbidden),
                "cadence-{name} depends on {forbidden}"
            );
        }
        assert!(
            dependencies.contains("cadence-engine"),
            "cadence-{name} reaches the reducer through cadence-engine"
        );
        let mut files = Vec::new();
        sources(&crate_dir(name).join("src"), &mut files)?;
        for (path, text) in &files {
            let text = code(text);
            for forbidden in [
                "cadence_kernel",
                "cadence_trainer",
                "cadence_core::ledger",
                "cadence_core::metrics",
                "AttemptLedger",
                "credit_once",
                "first_outcomes",
            ] {
                assert!(
                    !text.contains(forbidden),
                    "{} names {forbidden}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}
