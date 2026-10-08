//! `cadence-runtime` — composition and transport (A-01, A-04).
//!
//! Owns: the bounded event transport and its named limits, the reserved
//! result-slot `JobRegistry`, the small bounded worker pool, effect
//! execution (SQL, corpus, model rebuilds), the OS clock and calendar
//! ports, crossterm ingress with ingress-time stamping, and terminal
//! lifecycle (raw mode, alternate screen, restoration).
//!
//! Must not own: independent scoring or planner rules — those live in
//! core/trainer and the runtime only moves their typed facts.
//!
//! Status: transport and worker lifecycle land with P3; terminal ingress
//! with P1's live slice.
