//! Focus selection in integers (L-03/L-05, #38, #56). The decisions are the
//! proved `cadence_kernel::focus` functions (Lean: the decay weight is one
//! at age zero, halves exactly every half-life and never grows with age;
//! the posterior is within 0..=10,000, the prior mean without evidence and
//! monotone in errors; hesitation needs its sample minimum; a sparse or
//! quiet skill never qualifies; `pick` returns a qualifying unchosen skill
//! with the greatest signal).
pub use cadence_kernel::focus::{
    BASIS, Candidate, Thresholds, WEIGHT_ONE, decay_weight, hesitation_bp, pick, posterior_bp,
    qualifies, signal, uncertainty_bp,
};
