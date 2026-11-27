//! The `pty-linux` gate (H-05 "Real PTY smoke", #92): the PTY suite
//! (`crates/app/tests/pty.rs`) against the static release binary, then two
//! negative calibrations that must fail: a test-faults build told to leave
//! the terminal in raw mode, and a missing binary.

use std::path::{Path, PathBuf};
use std::process::Command;

const TARGET: &str = "x86_64-unknown-linux-musl";

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_unset| "cargo".into())
}

fn run(root: &Path, args: &[&str], env: &[(&str, &str)]) -> Result<bool, String> {
    let mut command = Command::new(cargo());
    command.args(args).current_dir(root);
    for (key, value) in env {
        command.env(key, value);
    }
    command
        .status()
        .map(|status| status.success())
        .map_err(|error| format!("cannot run cargo: {error}"))
}

/// Runs the gate.
///
/// # Errors
/// A failing suite, a calibration that passes, or a missing binary.
pub fn check(root: &Path) -> Result<(), String> {
    if !run(
        root,
        &[
            "build",
            "--locked",
            "--release",
            "--target",
            TARGET,
            "-p",
            "cadence",
        ],
        &[],
    )? {
        return Err("building the static cadence binary failed".into());
    }
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let binary = target.join(TARGET).join("release").join("cadence");
    if !binary.is_file() {
        return Err(format!(
            "the static binary is missing at {}",
            binary.display()
        ));
    }
    let binary = binary.to_string_lossy().into_owned();
    let suite = ["test", "--locked", "-p", "cadence", "--test", "pty"];
    if !run(root, &suite, &[("CADENCE_PTY_BINARY", &binary)])? {
        return Err("the PTY suite failed against the static binary".into());
    }
    println!("pty-linux: the suite passes against {binary}");
    let faulty = [
        "test",
        "--locked",
        "-p",
        "cadence",
        "--features",
        "test-faults",
        "--test",
        "pty",
    ];
    if run(root, &faulty, &[("CADENCE_FAULT_LEAVE_RAW", "1")])? {
        return Err("calibration: a binary that leaves raw mode passed the PTY suite".into());
    }
    println!("pty-linux: a binary that leaves raw mode fails the suite (calibration)");
    if run(
        root,
        &suite,
        &[("CADENCE_PTY_BINARY", "/nonexistent/cadence")],
    )? {
        return Err("calibration: the PTY suite passed with no binary".into());
    }
    println!("pty-linux: a missing binary fails the suite (calibration)");
    Ok(())
}
