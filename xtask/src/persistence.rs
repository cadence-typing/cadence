//! The `persistence-linux` gate (H-05 "Native persistence integration",
//! #88): the campaign suite (`crates/app/tests/persistence.rs`, built with
//! the store's test-only fault hooks), then its negative calibration, which
//! must fail: every run folds its chunks twice.

use std::path::Path;
use std::process::Command;

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_unset| "cargo".into())
}

fn suite(root: &Path, calibrate: bool) -> Result<bool, String> {
    let mut command = Command::new(cargo());
    command
        .args([
            "test",
            "--locked",
            "-p",
            "cadence",
            "--features",
            "test-faults",
            "--test",
            "persistence",
        ])
        .current_dir(root);
    if calibrate {
        command.env("CADENCE_CALIBRATE_DOUBLE_APPLY", "1");
    } else {
        command.env_remove("CADENCE_CALIBRATE_DOUBLE_APPLY");
    }
    command
        .status()
        .map(|status| status.success())
        .map_err(|error| format!("cannot run cargo: {error}"))
}

/// Runs the gate.
///
/// # Errors
/// A failing campaign, or a calibration that passes.
pub fn check(root: &Path) -> Result<(), String> {
    if !suite(root, false)? {
        return Err("the persistence campaigns failed".into());
    }
    println!(
        "persistence-linux: the kill, migration, bad-state, full-disk and restore campaigns pass"
    );
    if suite(root, true)? {
        return Err("calibration: the campaigns passed with every chunk folded twice".into());
    }
    println!(
        "persistence-linux: a store that folds chunks twice fails the campaigns (calibration)"
    );
    Ok(())
}
