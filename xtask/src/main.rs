//! `xtask` — project gate commands (B-03).
//!
//! Wrappers (`./ci`) invoke the pinned Compose service; the logic lives
//! here. Commands are honest about state: a gate that has not been
//! implemented yet fails with `GateNotImplemented` naming the work
//! packet, never with fake success.

use std::process::ExitCode;

mod boundary;
mod contracts;
mod corpus;
mod dist;
mod gates;
mod notices;
mod release;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: cargo xtask <feedback|quality|boundary|verify-contracts|corpus-gate|dist|release-check --tag TAG|gate ID|gates>";
    let Some(command) = args.first() else {
        eprintln!("{usage}");
        return ExitCode::FAILURE;
    };
    let outcome = match command.as_str() {
        "feedback" => run_feedback(),
        "quality" => run_quality(),
        "boundary" => boundary::check(),
        "verify-contracts" => contracts::verify_manifest(),
        "gates" => gates::list(),
        "corpus-gate" => corpus::check(),
        "dist" => dist::build_dist(),
        "release-check" => release::check(args.get(1..).unwrap_or(&[])),
        "gate" => gates::run(args.get(1).map(String::as_str)),
        other => {
            eprintln!("unknown command {other:?}\n{usage}");
            return ExitCode::FAILURE;
        }
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Fast local feedback: format check plus targeted unit tests of the pure
/// crates. Deliberately cheaper than [`run_quality`]; superseded feedback
/// is not qualification evidence (B-03).
fn run_feedback() -> Result<(), String> {
    run("cargo", &["fmt", "--check"])?;
    run(
        "cargo",
        &["test", "-p", "cadence-core", "-p", "cadence-tui"],
    )?;
    Ok(())
}

/// The `quality` gate: fmt, Clippy with warnings denied, workspace tests,
/// architecture boundary check, handover-contract manifest verification
/// (B-05) and the TLC model gate (#28).
fn run_quality() -> Result<(), String> {
    run("cargo", &["fmt", "--check"])?;
    run(
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run("cargo", &["test", "--workspace"])?;
    boundary::check()?;
    contracts::verify_manifest()?;
    run_models()?;
    Ok(())
}

/// The TLC model gate (#28): the gate's own failure-mode tests, then every
/// model in `models/claims.toml` with the pinned TLC. A missing tool fails.
fn run_models() -> Result<(), String> {
    run(
        "python3",
        &[
            "-m",
            "unittest",
            "discover",
            "-s",
            "tools",
            "-p",
            "test_*.py",
        ],
    )?;
    run("python3", &["tools/tlc_gate.py"])
}

/// Runs a command, forwarding output; a nonzero exit is a gate failure.
fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new(program)
        .args(args)
        .status()
        .map_err(|error| format!("failed to spawn {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{program} {}` failed with {status}",
            args.join(" ")
        ))
    }
}
