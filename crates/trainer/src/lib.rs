//! `cadence-trainer` — learning policy and planning (A-01).
//!
//! Owns: the validated policy document, exact phase allocation,
//! selection lanes/quotas/caps, the bounded candidate scan and utility
//! ranking, the FIFO due-review queue, pace hysteresis, and the corpus
//! pack/index view used by the planner. Policy numbers arrive as
//! validated named data (`policies/trainer-v1.toml`); there are no
//! naked coefficients in code. Candidate utility arithmetic is bounded
//! fixed-point (`BasisPoints`).

pub mod allocation;
pub mod candidate;
pub mod corpus;
pub mod lanes;
pub mod pace;
pub mod policy;
pub mod review;

pub use allocation::allocate;
pub use candidate::{Candidate, CandidateError, UtilityInputs, scan_select};
pub use corpus::{CorpusError, WordBand, WordPack};
pub use lanes::{BlockPlan, LaneSlot, QuotaTracker};
pub use pace::{PaceDecision, pace_decision};
pub use policy::{
    Lane, PacePolicy, Phase, PolicyError, ReviewPolicy, SelectionPolicy, SessionPolicy,
    StatisticsPolicy, TrainerPolicy, UtilityWeights, parse_policy,
};
pub use review::{DueQueue, QueueEntry, ReviewOutcome};
