//! Worker result admission (VP-01, VP-02, #75). The decision is the proved
//! `cadence_kernel::ticket::admit` (Lean: accepted exactly for the current
//! epoch and the pending request; never twice; a stale failure never
//! replaces anything).
pub use cadence_kernel::ticket::{Admit, admit};
