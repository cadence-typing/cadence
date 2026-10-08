//! Deterministic session plan builder (P-02, L-05).
//!
//! `build_session_plan` composes the validated policy, the loaded word
//! pack, the embedded prose bank, the numeric generators and a seeded
//! PRNG into a fixed, replayable exercise schedule: exact phase
//! allocation, warm-up/adaptive word blocks through the bounded scan,
//! fresh numeric content, prose passages. The same
//! `(policy, corpus, prose, seed, budget, disabled)` inputs always
//! produce the same plan on supported builds (deterministic RNG:
//! ChaCha8; recorded in the plan for traces).
//!
//! Planning heuristics here are scheduling granularity only: exercises
//! end at the phase deadline regardless, and phase seconds are the
//! contract (P-02).

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::allocation::allocate;
use crate::candidate::{Candidate, UtilityInputs, scan_select};
use crate::corpus::{WordBand, WordPack};
use crate::lanes::{QuotaTracker, fallback_order};
use crate::numeric::{DigitSource, number_row_exercise, numpad_exercise};
use crate::policy::{Lane, Phase, TrainerPolicy};
use cadence_core::alphabet::TrainingChar;
use cadence_core::{BasisPoints, ContentDigest, CoreError, WordId};

/// Planning heuristic: nominal seconds per exercise (scheduling
/// granularity only; the phase deadline is the contract).
const NOMINAL_EXERCISE_SECONDS: u64 = 20;

/// Words per warm-up/adaptive exercise (one selection block = 10 slots
/// by policy; exercises bundle one block each).
const WORDS_PER_EXERCISE: usize = 10;

/// Rough prose exercise length in units.
const PROSE_EXERCISE_UNITS: usize = 120;

/// The seeded PRNG used for planning (L-05: established, pinned,
/// deterministic; algorithm recorded here for trace headers).
pub type PlanRng = ChaCha8Rng;

/// Numeric exercise generator signature.
type NumericGenerator = fn(
    &mut dyn DigitSource,
    &mut std::collections::HashSet<String>,
) -> Result<Vec<TrainingChar>, crate::numeric::NumericError>;

impl DigitSource for PlanRng {
    fn below(&mut self, bound: u64) -> u64 {
        rand::Rng::random_range(self, 0..bound.max(1))
    }
}

/// What kind of content an exercise carries (SkillKey track input,
/// L-01: numpad `12` and number-row `12` are different skills).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExerciseKind {
    /// Warm-up familiar words.
    WarmUp,
    /// Adaptive word practice (targeted/review/breadth lanes).
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
            Self::Words => "adaptive words",
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
    // Exact allocation over enabled phases only.
    let enabled: Vec<Phase> = policy
        .session
        .phase_order
        .iter()
        .copied()
        .filter(|phase| !disabled.contains(phase))
        .collect();
    let minimums: Vec<u64> = enabled
        .iter()
        .map(|phase| phase_minimum(policy, *phase))
        .collect();
    let weights: Vec<u64> = enabled
        .iter()
        .map(|phase| phase_weight(policy, *phase))
        .collect();
    let allocation =
        if disabled.is_empty() && budget_seconds == policy.session.default_budget_seconds {
            // The pinned default table is exact for the default budget
            // (validated against minimums by the policy parser).
            policy.session.phase_seconds.clone()
        } else {
            allocate(budget_seconds, &minimums, &weights)
                .map_err(|reason| PlanError::Infeasible { reason })?
        };

    let mut rng = PlanRng::seed_from_u64(seed);
    let mut used_digits = std::collections::HashSet::new();
    let mut phases = Vec::with_capacity(enabled.len());

    for (index, phase) in enabled.iter().copied().enumerate() {
        let seconds = allocation[index];
        let exercises = match phase {
            Phase::Review => Vec::new(),
            Phase::WarmUp => warmup_exercises(policy, pack, &mut rng, seconds)?,
            Phase::AdaptiveWords => adaptive_exercises(policy, pack, &mut rng, seconds)?,
            Phase::NumberRow => numeric_exercises(
                seconds,
                &mut rng,
                &mut used_digits,
                number_row_exercise,
                ExerciseKind::NumberRow,
            )?,
            Phase::Numpad => numeric_exercises(
                seconds,
                &mut rng,
                &mut used_digits,
                numpad_exercise,
                ExerciseKind::Numpad,
            )?,
            Phase::Prose => prose_exercises_for(seconds, prose_bank)?,
        };
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

fn phase_minimum(policy: &TrainerPolicy, phase: Phase) -> u64 {
    policy
        .session
        .phase_order
        .iter()
        .position(|candidate| *candidate == phase)
        .and_then(|index| policy.session.phase_minimum_seconds.get(index).copied())
        .unwrap_or(1)
}

fn phase_weight(policy: &TrainerPolicy, phase: Phase) -> u64 {
    policy
        .session
        .phase_order
        .iter()
        .position(|candidate| *candidate == phase)
        .and_then(|index| policy.session.phase_seconds.get(index).copied())
        .unwrap_or(1)
}

fn exercise_count(seconds: u64) -> usize {
    usize::try_from(seconds / NOMINAL_EXERCISE_SECONDS)
        .unwrap_or(1)
        .max(1)
}

fn warmup_exercises(
    policy: &TrainerPolicy,
    pack: &WordPack,
    rng: &mut PlanRng,
    seconds: u64,
) -> Result<Vec<PlannedExercise>, PlanError> {
    // Familiar words at an easy pace: core band, occurrence-capped.
    let count = exercise_count(seconds);
    let indices = pack.band(WordBand::Core).to_vec();
    let mut exercises = Vec::with_capacity(count);
    let mut used_words: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for _ in 0..count {
        let mut words = Vec::with_capacity(WORDS_PER_EXERCISE);
        for _ in 0..WORDS_PER_EXERCISE {
            let Some(word) = pick_core_word(pack, &indices, rng, &mut used_words) else {
                return Err(PlanError::NoEligibleCandidate { lane: Lane::Broad });
            };
            words.push(word);
        }
        exercises.push(PlannedExercise {
            units: join_words(&words),
            kind: ExerciseKind::WarmUp,
        });
    }
    let _ = policy;
    Ok(exercises)
}

fn pick_core_word(
    pack: &WordPack,
    indices: &[usize],
    rng: &mut PlanRng,
    used: &mut std::collections::HashSet<u64>,
) -> Option<Vec<TrainingChar>> {
    if indices.is_empty() {
        return None;
    }
    for _ in 0..indices.len() {
        let pick = usize::try_from(rng.below(indices.len() as u64)).unwrap_or(0);
        let entry_index = indices[pick];
        let entry = pack.words().get(entry_index)?;
        if used.insert(entry.id.get()) {
            return Some(entry.units.clone());
        }
    }
    // All used (tiny core band): allow reuse.
    let pick = usize::try_from(rng.below(indices.len() as u64)).unwrap_or(0);
    pack.words()
        .get(indices[pick])
        .map(|entry| entry.units.clone())
}

fn adaptive_exercises(
    policy: &TrainerPolicy,
    pack: &WordPack,
    rng: &mut PlanRng,
    seconds: u64,
) -> Result<Vec<PlannedExercise>, PlanError> {
    // One exercise per selection block: fixed interleaved lane order,
    // quota caps, bounded candidate scan with utility ranking and
    // seeded tie-breaks (L-05). Without a learner model yet, all
    // candidates are eligible and utility favours breadth mixing:
    // target lane samples the long tail (uncertainty), broad lane the
    // core (relevance).
    let count = exercise_count(seconds);
    let mut exercises = Vec::with_capacity(count);
    let mut pool: Vec<u64> = pack.words().iter().map(|entry| entry.id.get()).collect();
    // Deterministic pre-shuffle so scan tie-breaks vary by seed.
    shuffle(&mut pool, rng);

    for _ in 0..count {
        let mut tracker = QuotaTracker::new(policy.selection.clone());
        let mut words: Vec<Vec<TrainingChar>> = Vec::with_capacity(policy.selection.block_slots);
        for lane in policy.selection.lane_order.clone() {
            let served = serve_lane(policy, pack, &pool, rng, &mut tracker, lane)?;
            words.push(served);
        }
        exercises.push(PlannedExercise {
            units: join_words(&words),
            kind: ExerciseKind::Words,
        });
    }
    Ok(exercises)
}

fn serve_lane(
    policy: &TrainerPolicy,
    pack: &WordPack,
    pool: &[u64],
    rng: &mut PlanRng,
    tracker: &mut QuotaTracker,
    lane: Lane,
) -> Result<Vec<TrainingChar>, PlanError> {
    // Documented fallback order with caps preserved (L-05).
    for candidate_lane in fallback_order(lane) {
        if !tracker.lane_open(*candidate_lane) {
            continue;
        }
        let materialized = materialize(pack, pool, *candidate_lane, policy, rng);
        let Some(selected) = scan_select(
            &materialized,
            &policy.weights,
            policy.selection.maximum_candidates_per_slot,
        )
        .map_err(|error| PlanError::Content(error.to_string()))?
        else {
            continue;
        };
        if !tracker.word_admissible(selected, *candidate_lane, None) {
            continue;
        }
        let units = pack
            .word(selected)
            .map(|entry| entry.units.clone())
            .ok_or(PlanError::NoEligibleCandidate { lane })?;
        tracker
            .record(*candidate_lane, selected, None)
            .map_err(|error| PlanError::Content(error.to_string()))?;
        return Ok(units);
    }
    Err(PlanError::NoEligibleCandidate { lane })
}

fn materialize(
    pack: &WordPack,
    pool: &[u64],
    lane: Lane,
    policy: &TrainerPolicy,
    rng: &mut PlanRng,
) -> Vec<Candidate> {
    let bound = policy.selection.maximum_candidates_per_slot;
    let mut candidates = Vec::with_capacity(bound);
    for id in pool {
        if candidates.len() >= bound {
            break;
        }
        let Ok(owned) = WordId::new(*id) else {
            continue;
        };
        let Some(entry) = pack.word(owned) else {
            continue;
        };
        let pair = |value: u16| BasisPoints::new(value).unwrap_or(BasisPoints::ZERO);
        let (relevance, uncertainty) = match (lane, entry.band) {
            (Lane::Broad, WordBand::Core) => (pair(9_000), pair(500)),
            (Lane::Broad, _) => (pair(4_000), pair(1_000)),
            (Lane::Target, WordBand::Tail) => (pair(5_000), pair(9_000)),
            (Lane::Target, WordBand::Common) => (pair(6_000), pair(6_000)),
            (Lane::Target, WordBand::Core) => (pair(7_000), pair(2_000)),
            (Lane::Review, _) => (pair(6_000), pair(4_000)),
        };
        candidates.push(Candidate {
            id: entry.id,
            eligible: true,
            inputs: UtilityInputs {
                error: BasisPoints::ZERO,
                hesitation: BasisPoints::ZERO,
                relevance,
                uncertainty,
            },
            tie_break: rng.below(u64::MAX / 2),
        });
    }
    candidates
}

fn numeric_exercises(
    seconds: u64,
    rng: &mut PlanRng,
    used: &mut std::collections::HashSet<String>,
    generator: NumericGenerator,
    kind: ExerciseKind,
) -> Result<Vec<PlannedExercise>, PlanError> {
    let count = exercise_count(seconds);
    let mut exercises = Vec::with_capacity(count);
    for _ in 0..count {
        let units = generator(rng, used).map_err(|error| PlanError::Content(error.to_string()))?;
        exercises.push(PlannedExercise { units, kind });
    }
    Ok(exercises)
}

fn prose_exercises_for(seconds: u64, bank: &str) -> Result<Vec<PlannedExercise>, PlanError> {
    let count = exercise_count(seconds);
    let all = crate::prose::prose_exercises(bank, PROSE_EXERCISE_UNITS)
        .map_err(|error| PlanError::Content(error.to_string()))?;
    if all.is_empty() {
        return Err(PlanError::Content("prose bank empty".to_string()));
    }
    // Cycle deterministically when the bank is smaller than the plan.
    Ok((0..count)
        .map(|index| {
            let units = all[index % all.len()].clone();
            PlannedExercise {
                units,
                kind: ExerciseKind::Prose,
            }
        })
        .collect())
}

fn join_words(words: &[Vec<TrainingChar>]) -> Vec<TrainingChar> {
    let mut units = Vec::new();
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            units.push(
                TrainingChar::new(' ', cadence_core::LayoutSupportsPound::US).unwrap_or(
                    TrainingChar::new('~', cadence_core::LayoutSupportsPound::US)
                        .expect("tilde is in the alphabet"),
                ),
            );
        }
        units.extend(word.iter().copied());
    }
    units
}

/// Fisher–Yates shuffle driven by the seeded RNG.
fn shuffle(items: &mut [u64], rng: &mut PlanRng) {
    if items.len() < 2 {
        return;
    }
    for index in (1..items.len()).rev() {
        let pick = usize::try_from(rng.below((index as u64) + 1)).unwrap_or(0);
        items.swap(index, pick);
    }
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
