//! Deterministic session plan builder (P-02, L-05).
//!
//! Builds a replayable cold-start schedule from validated policy, corpus,
//! original prose and pinned ChaCha8 randomness. No learner observations
//! are supplied here: word slots are practice samples, not due reviews,
//! weakness adaptation, baseline measurements or benchmark results.
//! Phase deadlines, not target counts, determine elapsed practice time.
//! Prepared content is finite; the engine records repeated-target cycling.

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::allocation::allocate;
use crate::corpus::WordPack;
use crate::numeric::DigitSource;
use crate::policy::{Lane, Phase, TrainerPolicy};
use cadence_core::alphabet::TrainingChar;
use cadence_core::{ContentDigest, CoreError};

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

/// One planned exercise: an immutable target plus its track kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedExercise {
    /// The exercise's immutable target units.
    pub units: Vec<TrainingChar>,
    /// The content track.
    pub kind: ExerciseKind,
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
    /// Corpus digest the plan was built under.
    pub corpus_digest: ContentDigest,
    /// The RNG seed (recorded for replay).
    pub seed: u64,
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
    pack: &WordPack,
    prose_bank: &str,
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
        let exercises =
            crate::plan_content::exercises(policy, pack, prose_bank, &mut rng, phase, seconds)?;
        phases.push(PlannedPhase {
            phase,
            seconds,
            exercises,
        });
    }

    Ok(SessionPlan {
        budget_seconds,
        policy_digest: policy.source_digest,
        corpus_digest: pack.digest(),
        seed,
        phases,
    })
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )]

    use super::*;
    use crate::corpus::WordBand;

    fn policy() -> TrainerPolicy {
        crate::policy::parse_policy(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../policies/trainer-v1.toml"
            ))
            .unwrap()
            .as_slice(),
        )
        .unwrap()
    }

    fn pack(word_count: u64) -> WordPack {
        let mut entries = Vec::new();
        for n in 1..=word_count {
            let band = if n <= 2_000 {
                WordBand::Core
            } else if n <= 10_000 {
                WordBand::Common
            } else {
                WordBand::Tail
            };
            entries.push((n, format!("word{n}"), band));
        }
        WordPack::new(
            entries,
            ContentDigest::from_bytes([9; 32]),
            1,
            cadence_core::LayoutSupportsPound::US,
        )
        .unwrap()
    }

    const PROSE: &str =
        "Alpha beta gamma delta. Epsilon zeta eta theta.\n\nIota kappa lambda mu nu.";

    #[test]
    fn default_plan_is_deterministic_and_replayable() {
        let policy = policy();
        let pack = pack(1_000);
        let first = build_session_plan(&policy, &pack, PROSE, 42, &[], 900).unwrap();
        let second = build_session_plan(&policy, &pack, PROSE, 42, &[], 900).unwrap();
        assert_eq!(first, second, "same inputs produce the same plan");
        assert_eq!(
            first.phases.iter().map(|phase| phase.seconds).sum::<u64>(),
            900
        );
        // The default split is the pinned table.
        let seconds: Vec<u64> = first.phases.iter().map(|phase| phase.seconds).collect();
        assert_eq!(seconds, vec![60, 300, 120, 180, 180, 60]);
    }

    #[test]
    fn different_seeds_differ() {
        let policy = policy();
        let pack = pack(1_000);
        let first = build_session_plan(&policy, &pack, PROSE, 1, &[], 900).unwrap();
        let second = build_session_plan(&policy, &pack, PROSE, 2, &[], 900).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn disabled_tracks_are_omitted_and_budget_stays_exact() {
        let policy = policy();
        let pack = pack(1_000);
        let disabled = vec![Phase::Numpad, Phase::NumberRow];
        let plan = build_session_plan(&policy, &pack, PROSE, 7, &disabled, 900).unwrap();
        assert_eq!(
            plan.phases
                .iter()
                .map(|phase| phase.phase)
                .collect::<Vec<_>>(),
            vec![
                Phase::WarmUp,
                Phase::AdaptiveWords,
                Phase::Prose,
                Phase::Review
            ]
        );
        assert_eq!(
            plan.phases.iter().map(|phase| phase.seconds).sum::<u64>(),
            900
        );
    }

    #[test]
    fn shorter_budget_still_serves_every_enabled_track() {
        let policy = policy();
        let pack = pack(1_000);
        let plan = build_session_plan(&policy, &pack, PROSE, 9, &[], 300).unwrap();
        assert_eq!(plan.phases.len(), 6);
        assert_eq!(plan.phases.iter().map(|p| p.seconds).sum::<u64>(), 300);
        assert!(plan.phases.iter().all(|phase| phase.seconds > 0));
    }

    #[test]
    fn infeasible_budget_fails_explicitly() {
        let policy = policy();
        let pack = pack(100);
        match build_session_plan(&policy, &pack, PROSE, 1, &[], 100) {
            Err(PlanError::Infeasible { .. }) => {}
            other => panic!("expected infeasible, got {other:?}"),
        }
    }

    #[test]
    fn tiny_corpus_fails_with_no_eligible_candidate() {
        let policy = policy();
        let pack = pack(4); // far below lane capacity
        match build_session_plan(&policy, &pack, PROSE, 1, &[], 900) {
            Err(PlanError::NoEligibleCandidate { .. }) | Err(PlanError::Infeasible { .. }) => {}
            other => panic!("expected explicit failure, got {other:?}"),
        }
    }

    #[test]
    fn phases_carry_their_track_content() {
        let policy = policy();
        let pack = pack(1_000);
        let plan = build_session_plan(&policy, &pack, PROSE, 5, &[], 900).unwrap();
        for phase in &plan.phases {
            match phase.phase {
                Phase::Review => assert!(phase.exercises.is_empty()),
                Phase::WarmUp | Phase::AdaptiveWords => {
                    assert!(!phase.exercises.is_empty());
                    assert!(
                        phase.exercises[0].kind == ExerciseKind::WarmUp
                            || phase.exercises[0].kind == ExerciseKind::Words
                    );
                }
                Phase::NumberRow => assert_eq!(phase.exercises[0].kind, ExerciseKind::NumberRow),
                Phase::Numpad => assert_eq!(phase.exercises[0].kind, ExerciseKind::Numpad),
                Phase::Prose => assert_eq!(phase.exercises[0].kind, ExerciseKind::Prose),
            }
        }
    }
}
