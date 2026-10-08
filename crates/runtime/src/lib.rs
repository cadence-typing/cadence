//! `cadence-runtime` — composition and transport (A-01, A-04).
//!
//! P1 scope: live terminal lifecycle and crossterm ingress with
//! ingress-time stamping. Workers, the bounded event transport, the
//! reserved-slot `JobRegistry` and effect execution land with P3; this
//! crate deliberately contains no UI dependency (the composition root
//! wires it to the shared paint path).

pub mod live;

pub use live::{TerminalError, TerminalGuard, elapsed_since};
