//! Learner evidence attribution (L-02, L-03). The decisions are the proved
//! `cadence_kernel::evidence` functions (Lean: a transition is first-correct
//! exactly when both of its keys were; an interval is a timed sample exactly
//! when it is adjacent, matched, within one timing segment, positive and
//! within the ceiling).
pub use cadence_kernel::evidence::{Interval, Mark, clean_interval, pair_first_correct};
