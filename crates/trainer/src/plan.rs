//! Deterministic session plan builder (P-02, L-05).
//!
//! Builds a replayable schedule from validated policy, corpus, prose bank,
//! the startup learner model and pinned ChaCha8 randomness (plan 0003).
//! With no evidence the plan is a cold start; otherwise target slots favour
//! recorded weak keys/transitions and review slots serve due missed words.
//! Phase deadlines, not target counts, determine elapsed practice time.
//! Prepared content is finite; the engine records repeated-target cycling.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::corpus::{WordPack, digest_of};
use crate::learner::LearnerModel;
use crate::numeric::DigitSource;
use crate::policy::{Lane, Phase, TrainerPolicy};
use crate::prose::ProseBank;
use cadence_core::alphabet::TrainingChar;
use cadence_core::learning::{PlanRecord, SkillKey};
use cadence_core::plan::allocate;
use cadence_core::{ContentDigest, CoreError, Micros, PassageId, WordId};

/// The seeded PRNG used for planning (L-05: established, pinned,
/// deterministic; algorithm recorded here for trace headers).
pub type PlanRng = ChaCha8Rng;

impl DigitSource for PlanRng {
    fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        rand::Rng::random_range(self, 0..bound)
    }
}

/// What kind of content an exercise carries (SkillKey track input,
/// L-01: numpad `12` and number-row `12` are different skills).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExerciseKind {
    /// Cold-start word practice.
    WarmUp,
    /// Seeded word practice; no learner observations are assumed.
    Words,
    /// Number-row digits in text contexts.
    NumberRow,
    /// Numpad entry drill; Enter submits.
    Numpad,
    /// Natural prose transfer.
    Prose,
}

impl ExerciseKind {
    /// The track label shown in the UI.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::WarmUp => "warm-up",
            Self::Words => "words",
            Self::NumberRow => "number row",
            Self::Numpad => "numpad",
            Self::Prose => "prose",
        }
    }
}

/// Why a word occupies its slot (L-05 slot reason).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedWord {
    /// Corpus identity.
    pub id: WordId,
    /// Lane quota the slot counts against.
    pub lane: Lane,
    /// The focus skill that selected it, if any.
    pub focus: Option<SkillKey>,
    /// Whether it is a due spaced review.
    pub due_review: bool,
}

/// One planned exercise: an immutable target plus its track kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedExercise {
    /// The exercise's immutable target units.
    pub units: Vec<TrainingChar>,
    /// The content track.
    pub kind: ExerciseKind,
    /// Word slots in target order (word exercises only).
    pub words: Vec<PlannedWord>,
    /// The prose passage this exercise is part of.
    pub passage: Option<PassageId>,
    /// Attribution or repeat notice shown with the target.
    pub caption: Option<String>,
    /// The focus skill a numeric entry was built around (#5), for the
    /// caption (#50) and replay; `None` for uniform entries.
    pub target: Option<cadence_core::learning::SkillKey>,
}

impl PlannedExercise {
    /// An exercise with no word slots, passage or caption.
    #[must_use]
    pub const fn plain(units: Vec<TrainingChar>, kind: ExerciseKind) -> Self {
        Self {
            units,
            kind,
            words: Vec::new(),
            passage: None,
            caption: None,
            target: None,
        }
    }
}

/// Reducer-side adaptation bounds carried with the plan, so the engine
/// stays policy-free and replay uses the bounds the plan was built with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adaptation {
    /// Clean intervals above this are pauses for the learner model only.
    pub hesitation_ceiling: Micros,
    /// Missed words waiting for same-session reinforcement.
    pub reinforcement_capacity: usize,
    /// Times one word may be reinforced in a session.
    pub reinforcements_per_word: u32,
    /// Entries before a missed numeric entry returns (#59; 0 disables).
    pub numeric_gap_entries: u32,
}

/// Content and evidence a plan is built from.
#[derive(Debug, Clone, Copy)]
pub struct PlanInputs<'a> {
    /// Word pack.
    pub pack: &'a WordPack,
    /// Prose bank.
    pub prose: &'a ProseBank,
    /// Startup learner model.
    pub learner: &'a LearnerModel,
}

/// One planned phase with its exact seconds and exercise queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPhase {
    /// Which phase this is.
    pub phase: Phase,
    /// Exact allocated seconds (typing phases) or review seconds.
    pub seconds: u64,
    /// Exercises in service order (empty for review).
    pub exercises: Vec<PlannedExercise>,
}

/// A complete, replayable session plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlan {
    /// Total budget in seconds.
    pub budget_seconds: u64,
    /// Policy digest the plan was built under.
    pub policy_digest: ContentDigest,
    /// Corpus digest (words and prose) the plan was built under.
    pub corpus_digest: ContentDigest,
    /// Seed, planner mode and focus skills (persisted for replay).
    pub record: PlanRecord,
    /// Reducer-side adaptation bounds.
    pub adaptation: Adaptation,
    /// Phases in policy order (disabled phases omitted).
    pub phases: Vec<PlannedPhase>,
}

/// Plan construction failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The budget cannot satisfy the enabled minimums (explicit, P-02:
    /// reject rather than quietly drop a selected track).
    Infeasible {
        /// The allocation diagnostic.
        reason: CoreError,
    },
    /// A lane found no eligible candidate in the pack (tiny corpus).
    NoEligibleCandidate {
        /// Which lane starved.
        lane: Lane,
    },
    /// Numeric/prose generation failed (authoring bug).
    Content(String),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Infeasible { reason } => write!(f, "session budget infeasible: {reason}"),
            Self::NoEligibleCandidate { lane } => write!(
                f,
                "no eligible candidate for the {} lane; the corpus is too small — broaden content",
                lane.key()
            ),
            Self::Content(message) => write!(f, "content generation failed: {message}"),
        }
    }
}

impl std::error::Error for PlanError {}

/// Builds the deterministic session plan.
///
/// # Errors
/// - [`PlanError::Infeasible`] when enabled-phase minimums exceed the
///   budget.
/// - [`PlanError::NoEligibleCandidate`] when a lane starves in a tiny
///   corpus (explicit; no unbounded resampling).
/// - [`PlanError::Content`] on generator bugs.
pub fn build_session_plan(
    policy: &TrainerPolicy,
    inputs: PlanInputs<'_>,
    seed: u64,
    disabled: &[Phase],
    budget_seconds: u64,
) -> Result<SessionPlan, PlanError> {
    policy
        .validate()
        .map_err(|error| PlanError::Content(error.to_string()))?;
    if !(policy.session.minimum_budget_seconds..=policy.session.maximum_budget_seconds)
        .contains(&budget_seconds)
    {
        return Err(PlanError::Infeasible {
            reason: CoreError::CapacityExhausted {
                limit: "session budget outside policy bounds",
            },
        });
    }
    let weights = policy
        .session
        .allocation_weights()
        .map_err(|error| PlanError::Content(error.to_string()))?;
    let enabled: Vec<_> = policy
        .session
        .phase_order
        .iter()
        .copied()
        .zip(policy.session.phase_minimum_seconds.iter().copied())
        .zip(weights)
        .filter(|((phase, _), _)| !disabled.contains(phase))
        .collect();
    let minimums: Vec<_> = enabled.iter().map(|((_, minimum), _)| *minimum).collect();
    let weights: Vec<_> = enabled.iter().map(|(_, weight)| *weight).collect();
    let allocation = allocate(budget_seconds, &minimums, &weights)
        .map_err(|reason| PlanError::Infeasible { reason })?;
    let mut rng = PlanRng::seed_from_u64(seed);
    let mut phases = Vec::with_capacity(enabled.len());
    for (((phase, _), _), seconds) in enabled.into_iter().zip(allocation) {
        let exercises = crate::plan_content::exercises(policy, inputs, &mut rng, phase, seconds)?;
        phases.push(PlannedPhase {
            phase,
            seconds,
            exercises,
        });
    }

    let mut corpus = inputs.pack.digest().as_bytes().to_vec();
    corpus.extend_from_slice(inputs.prose.digest().as_bytes());
    Ok(SessionPlan {
        budget_seconds,
        policy_digest: policy.source_digest,
        corpus_digest: digest_of(&corpus),
        record: PlanRecord {
            seed,
            mode: inputs.learner.mode(),
            focus: inputs.learner.focus().to_vec(),
        },
        adaptation: Adaptation {
            hesitation_ceiling: policy.learning.hesitation_ceiling,
            reinforcement_capacity: policy.learning.reinforcement_queue_capacity,
            reinforcements_per_word: policy.review.maximum_same_session_reinforcements,
            numeric_gap_entries: policy.learning.numeric_reinforcement_gap_entries,
        },
        phases,
    })
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "test fixtures use small bounded counts; overflow is impossible by construction"
)]
#[path = "plan_tests.rs"]
mod tests;
