//! Pace hysteresis (L-06, #61). The decision is the proved
//! `cadence_kernel::pace::decide` (Lean: no baseline without a target; a
//! raise is bounded and needs enough qualifying blocks from enough sessions
//! with no drop in attempt accuracy; a lowering is bounded and never below
//! 1; a latest block below the slow-down band lowers; between the bands the
//! pace stays; the accuracy comparison is exact).
pub use cadence_kernel::pace::{Block, Decision, Policy, block_accuracy_below, decide};
