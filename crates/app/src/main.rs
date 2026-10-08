//! `cadence` — CLI composition root (A-01).
//!
//! Composes the runtime, driver, TUI and data adapters into the user-facing
//! binary. User-facing errors are plain and actionable; `anyhow`-style
//! aggregation is acceptable only here and in xtask, never in the reusable
//! crates.
//!
//! Status: P0 baseline. The interactive TUI, `doctor`, `config check`,
//! `headless`, `replay`, `export` and `backup` subcommands land with their
//! work packets (P1, P3, P4); see STATUS.md.

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    // Honest baseline behaviour: report identity and pending surface
    // instead of implying an interactive trainer exists before P1.
    println!("cadence {VERSION}");
    println!(
        "offline typing trainer — interactive mode not yet implemented (work packet P1; see STATUS.md)"
    );
    std::process::exit(0);
}
