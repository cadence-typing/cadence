//! Benchmark probes (P-03, #63). The decisions are the proved
//! `cadence_kernel::benchmark::{due, comparable}` (Lean: a probe is due
//! exactly after the interval when a passage is available, and a missing
//! passage is said, never hidden; a paused, early-ended, pasted, cut-short
//! or synthetic probe is never comparable).
pub use cadence_kernel::benchmark::{Due, Facts, comparable, due};
