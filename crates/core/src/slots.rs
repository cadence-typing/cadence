//! Re-serving slots (#43). The decision is the proved
//! `cadence_kernel::slots::substitute` (Lean: a due review or a target slot
//! is never replaced, and a word already in the exercise is never placed
//! again).
pub use cadence_kernel::slots::{SlotLane, substitute};
