//! `cadence-trainer` — learning policy and planning (A-01).
//!
//! Owns: learner observation summaries, the bounded candidate scan and
//! utility ranking, selection lanes and quota policy, the FIFO due-review
//! queue, pace hysteresis and phase-budget allocation.
//!
//! Must not own: terminal concerns, persistence implementations, file
//! reads. Policy numbers arrive as validated named data
//! (`policies/trainer-v1.toml`); there are no naked coefficients in code.
//! Candidate utility arithmetic is bounded fixed-point (`BasisPoints`).
//!
//! Status: foundations land with P2; typed IDs and checked arithmetic come
//! from `cadence-core`.
