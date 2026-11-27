//! #15 (AGENTS.md rule 3): the key path allocates nothing in proportion to
//! the target text and serializes no whole state per key. Every key of an
//! exercise ten times as long allocates no more than a short one's, apart
//! from a fixed slack, checkpoints included.

mod support;

use std::alloc::System;

use cadence_trainer::{ExerciseKind, Phase};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use support::{TestResult, content, engine, phase, ready, send};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

/// Bytes a key may allocate beyond the short exercise's worst key.
const SLACK: usize = 1_024;

/// The largest allocation of any one key while typing `units` units of a
/// target of `length` units (with a 100 ms pace, so checkpoints fall due).
fn worst_key(length: usize, units: usize) -> Result<usize, Box<dyn std::error::Error>> {
    let target: String = "the quick brown fox jumps over a lazy dog "
        .chars()
        .cycle()
        .take(length)
        .collect::<String>()
        .trim_end()
        .to_string();
    let mut e = engine(vec![phase(
        ExerciseKind::Words,
        Phase::AdaptiveWords,
        600,
        &[&target],
    )?])?;
    ready(&mut e)?;
    send(
        &mut e,
        1,
        cadence_engine::EventKind::Command(cadence_engine::commands::Action::Begin),
    )?;
    let mut worst = 0;
    let mut at = 1_000_u64;
    for c in target.chars().take(units) {
        at = at.saturating_add(100_000);
        let region = Region::new(GLOBAL);
        let transition = content(&mut e, at, c)?;
        let bytes = region.change().bytes_allocated;
        drop(transition);
        worst = worst.max(bytes);
    }
    Ok(worst)
}

#[test]
fn a_key_allocates_nothing_in_proportion_to_the_target() -> TestResult {
    let short = worst_key(100, 90)?;
    let long = worst_key(2_000, 90)?;
    eprintln!("worst key: short {short} bytes, long {long} bytes");
    assert!(
        long <= short.saturating_add(SLACK),
        "a key of a 2000-unit target allocated {long} bytes; of a 100-unit one {short}"
    );
    Ok(())
}
