//! `cadence-trainer` — learning policy and planning (A-01).
//!
//! Owns: the validated policy document, exact phase allocation,
//! selection lanes/quotas/caps, the bounded candidate scan and utility
//! ranking, the FIFO due-review queue, pace hysteresis, and the corpus
//! pack/index view used by the planner. Policy numbers arrive as
//! validated named data (`policies/trainer-v2.toml`); there are no
//! naked coefficients in code. Candidate utility arithmetic is bounded
//! fixed-point (`BasisPoints`).

pub mod candidate;
pub mod corpus;
pub mod lanes;
pub mod learner;
pub mod learning_policy;
pub mod numeric;
pub mod plan;
mod plan_content;
mod plan_words;
pub mod policy;
mod policy_parse;
mod policy_validation;
pub mod probe;
pub mod prose;
pub mod review;

pub use cadence_core::plan::allocate;
pub use candidate::{Candidate, CandidateError, UtilityInputs, scan_select};
pub use corpus::{CorpusError, WordBand, WordPack};
pub use lanes::{BlockPlan, LaneSlot, QuotaTracker};
pub use learner::{FocusGroup, LearnerModel, SkillScore, group_of};
pub use learning_policy::LearningPolicy;
pub use numeric::{DigitSource, NumericError, number_row_exercise, numpad_exercise};
pub use plan::{
    Adaptation, ExerciseKind, PaceInputs, PlanError, PlanInputs, PlanRng, PlannedExercise,
    PlannedPhase, PlannedProbe, PlannedWord, ProbeInputs, ProbePlan, SessionPlan, WordSpan,
    build_session_plan, completed_with_prose, pace_baseline, pace_decision, probe_due,
};
pub use policy::{
    Lane, PacePolicy, Phase, PolicyError, ReviewPolicy, SelectionPolicy, SessionPolicy,
    StatisticsPolicy, TrainerPolicy, UtilityWeights, parse_policy,
};
pub use prose::{ProseBank, ProseError, ProsePassage, ProseSource, chunk_passage};
pub use review::{DueQueue, QueueEntry, ReviewState, is_due, ladder_step};
