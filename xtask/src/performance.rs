//! The `performance` gate (H-06, P6, #15): the harness (`cadence perf`,
//! feature `perf`) in a release build, with its receipt written under
//! `evidence-out/` (not committed), then its negative calibration, which
//! must fail: a 5 ms busy loop inside the measured reducer span.

use std::path::{Path, PathBuf};
use std::process::Command;

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_unset| "cargo".into())
}

fn harness(root: &Path, receipt: &Path, busy_us: Option<&str>) -> Result<bool, String> {
    let mut command = Command::new(cargo());
    command
        .args([
            "run",
            "--locked",
            "--release",
            "-q",
            "-p",
            "cadence",
            "--features",
            "perf",
            "--",
            "perf",
            "--output",
        ])
        .arg(receipt)
        .current_dir(root);
    if let Some(busy) = busy_us {
        command.args(["--calibrate-busy-us", busy]);
    }
    command
        .status()
        .map(|status| status.success())
        .map_err(|error| format!("cannot run cargo: {error}"))
}

/// Runs the gate.
///
/// # Errors
/// A budget missed, or a calibration that passes.
pub fn check(root: &Path) -> Result<(), String> {
    let out = std::env::var_os("CADENCE_EVIDENCE_OUT")
        .map_or_else(|| root.join("evidence-out"), PathBuf::from);
    let receipt = out.join("performance.json");
    if !harness(root, &receipt, None)? {
        return Err(format!(
            "the performance budgets were missed; see {}",
            receipt.display()
        ));
    }
    println!(
        "performance: every budget met; receipt {}",
        receipt.display()
    );
    let calibration = out.join("performance-calibration.json");
    if harness(root, &calibration, Some("5000"))? {
        return Err("calibration: a 5 ms busy loop in the reducer passed the budgets".into());
    }
    println!("performance: a 5 ms busy reducer fails the reducer budget (calibration)");
    Ok(())
}
