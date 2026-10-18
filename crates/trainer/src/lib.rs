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
pub mod numeric;
pub mod pace;
pub mod plan;
mod plan_content;
pub mod policy;
mod policy_parse;
mod policy_validation;
pub mod prose;
pub mod review;

pub use allocation::allocate;
pub use candidate::{Candidate, CandidateError, UtilityInputs, scan_select};
pub use corpus::{CorpusError, WordBand, WordPack};
pub use lanes::{BlockPlan, LaneSlot, QuotaTracker};
pub use numeric::{DigitSource, NumericError, number_row_exercise, numpad_exercise};
pub use pace::{PaceDecision, pace_decision};
pub use plan::{
    ExerciseKind, PlanError, PlanRng, PlannedExercise, PlannedPhase, SessionPlan,
    build_session_plan,
};
pub use policy::{
    Lane, PacePolicy, Phase, PolicyError, ReviewPolicy, SelectionPolicy, SessionPolicy,
    StatisticsPolicy, TrainerPolicy, UtilityWeights, parse_policy,
};
pub use prose::prose_exercises;
pub use review::{DueQueue, QueueEntry, ReviewOutcome};
