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
    /// A transfer probe on unseen text (#64).
    Probe,
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
            Self::Probe => "transfer probe",
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
    /// Pack words inside a prose exercise (#48): their outcomes feed the
    /// ladder and the queue. Word exercises use `words` instead.
    pub spans: Vec<WordSpan>,
}

/// A pack word inside a prose target (#48).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSpan {
    /// The pack word.
    pub id: WordId,
    /// Its index among the target's space-separated tokens.
    pub token: usize,
    /// First unit of the word in the target.
    pub start: usize,
    /// Units in the word.
    pub len: usize,
    /// The pack form (lowercase), served if the word returns.
    pub units: Vec<TrainingChar>,
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
            spans: Vec::new(),
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

/// The transfer probe a plan carries (#64).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedProbe {
    /// The probe passage.
    pub passage: PassageId,
    /// Its strata class.
    pub strata: u8,
    /// Whether the class matches the previous comparable probe's (or there
    /// is none).
    pub same_strata: bool,
}

/// Whether this session carries a transfer probe (#64).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProbePlan {
    /// No probe is due (or probes are off).
    #[default]
    NotDue,
    /// A probe is due but every probe passage was shown: none, and the
    /// start screen says so.
    NoPassage,
    /// This session's probe.
    Planned(PlannedProbe),
}

impl ProbePlan {
    /// The planned probe, if any.
    #[must_use]
    pub const fn planned(self) -> Option<PlannedProbe> {
        match self {
            Self::Planned(probe) => Some(probe),
            Self::NotDue | Self::NoPassage => None,
        }
    }
}

/// Probe content and history for the planner (#64).
#[derive(Debug, Clone, Copy)]
pub struct ProbeInputs<'a> {
    /// The probe bank.
    pub bank: &'a crate::probe::ProbeBank,
    /// What the profile's history says about probes.
    pub history: &'a cadence_core::learning::ProbeHistory,
}

/// Pace evidence for the planner (#62).
#[derive(Debug, Clone, Copy)]
pub struct PaceInputs<'a> {
    /// Recent sessions, newest first, as stored.
    pub recent: &'a [cadence_core::progress::SessionRecap],
    /// The latest suggested pace per typing track.
    pub targets: &'a [(cadence_core::progress::Track, u64)],
}

/// Separates the prose stream from the other phases' (#48).
const PROSE_STREAM: u64 = 0x5052_4f53_455f_5631;

/// Content and evidence a plan is built from.
#[derive(Debug, Clone, Copy)]
pub struct PlanInputs<'a> {
    /// Word pack.
    pub pack: &'a WordPack,
    /// Prose bank.
    pub prose: &'a ProseBank,
    /// Startup learner model.
    pub learner: &'a LearnerModel,
    /// Transfer probes; `None` disables them (tests, synthetic plans).
    pub probes: Option<ProbeInputs<'a>>,
    /// Pace evidence; `None` disables pace goals (tests, synthetic plans).
    pub pace: Option<PaceInputs<'a>>,
    /// How numpad entries are written (#83).
    pub numeric: crate::numeric::NumericOptions,
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
    /// The transfer probe decision for this session (#64).
    pub probe: ProbePlan,
    /// Each typing track's suggested pace and how it moved (#62).
    pub pace: Vec<cadence_core::learning::PaceRecord>,
    /// Due words no longer in the pack (retired by vetting, #8): dropped
    /// from planning.
    pub retired_due: usize,
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
    let pace = pace_goals(
        policy,
        inputs,
        &enabled.iter().map(|((p, _), _)| *p).collect::<Vec<_>>(),
    )?;
    // A lowered words pace draws a share of target slots from the most
    // familiar band next session (#62); otherwise none.
    let familiar = if pace.iter().any(|record| {
        record.track == cadence_core::progress::Track::Words
            && record.moved == cadence_core::learning::PaceMove::Lowered
    }) {
        policy.pace.familiar_target_share_when_lowered
    } else {
        (0, 1)
    };
    let mut rng = PlanRng::seed_from_u64(seed);
    let mut phases = Vec::with_capacity(enabled.len());
    // A due word is planned in one phase only (#47): warm-up first.
    let mut served_due = std::collections::HashSet::new();
    for (((phase, _), _), seconds) in enabled.into_iter().zip(allocation) {
        let exercises = crate::plan_content::exercises(
            policy,
            inputs,
            &mut rng,
            phase,
            seconds,
            &mut served_due,
            crate::plan_content::PhaseOptions {
                prose_seed: seed ^ PROSE_STREAM,
                familiar,
            },
        )?;
        phases.push(PlannedPhase {
            phase,
            seconds,
            exercises,
        });
    }

    let probe = split_for_probe(policy, inputs, &mut phases);
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
        probe,
        pace,
        retired_due: inputs
            .learner
            .due_words()
            .iter()
            .filter(|word| inputs.pack.word(**word).is_none())
            .count(),
    })
}

/// Correct units per minute of one stored block (digits on the numpad).
fn block_cpm(track: &cadence_core::progress::TrackProgress) -> Option<u64> {
    let units = if track.track == cadence_core::progress::Track::Numpad {
        track.correct_digits
    } else {
        track.correct_units
    };
    units
        .checked_mul(60_000_000)
        .and_then(|scaled| scaled.checked_div(track.active_micros))
        .filter(|cpm| *cpm > 0)
}

/// A track's baseline (#62): the lower median pace of its qualifying blocks
/// (the position floor, at or above the desired accuracy) in the history
/// window; `None` until one exists.
#[must_use]
pub fn pace_baseline(
    policy: &crate::policy::PacePolicy,
    track: cadence_core::progress::Track,
    recent: &[cadence_core::progress::SessionRecap],
) -> Option<u64> {
    let mut paces: Vec<u64> = recent
        .iter()
        .flat_map(|recap| recap.tracks.iter())
        .filter(|t| {
            t.track == track
                && t.first_attempted >= policy.minimum_first_attempt_positions_per_block
                && !cadence_core::pace::block_accuracy_below(
                    t.first_correct,
                    t.first_attempted,
                    u64::from(policy.desired_first_pass.get()),
                )
        })
        .filter_map(block_cpm)
        .collect();
    paces.sort_unstable();
    let middle = paces.len().checked_sub(1)?.checked_div(2)?;
    paces.get(middle).copied()
}

/// Each enabled typing track's goal for this session (#62): the stored
/// target, else the baseline, moved by the proved `pace::decide`.
fn pace_goals(
    policy: &TrainerPolicy,
    inputs: PlanInputs<'_>,
    enabled: &[Phase],
) -> Result<Vec<cadence_core::learning::PaceRecord>, PlanError> {
    use cadence_core::learning::{PaceMove, PaceRecord};
    use cadence_core::pace::Decision;
    use cadence_core::progress::Track;
    let Some(pace) = inputs.pace else {
        return Ok(Vec::new());
    };
    let mut goals = Vec::new();
    for (phase, track) in [
        (Phase::AdaptiveWords, Track::Words),
        (Phase::NumberRow, Track::NumberRow),
        (Phase::Numpad, Track::Numpad),
        (Phase::Prose, Track::Prose),
    ] {
        if !enabled.contains(&phase) {
            continue;
        }
        let previous = pace
            .targets
            .iter()
            .find(|(stored, _)| *stored == track)
            .map(|(_, cpm)| *cpm)
            .or_else(|| pace_baseline(&policy.pace, track, pace.recent));
        let (target_cpm, moved) = match pace_decision(&policy.pace, track, previous, pace.recent)? {
            Decision::NoBaseline => (None, PaceMove::NoBaseline),
            Decision::Steady => (previous, PaceMove::Steady),
            Decision::Raise(raised) => (Some(raised), PaceMove::Raised),
            Decision::Lower(lowered) => (Some(lowered), PaceMove::Lowered),
        };
        goals.push(PaceRecord {
            track,
            target_cpm,
            moved,
        });
    }
    Ok(goals)
}

/// On a due probe (`benchmark::due`), the prose phase becomes a probe phase
/// of `probe_seconds` (at most the prose allocation) followed by the
/// remaining prose; the phase seconds still sum to the allocation.
fn split_for_probe(
    policy: &TrainerPolicy,
    inputs: PlanInputs<'_>,
    phases: &mut Vec<PlannedPhase>,
) -> ProbePlan {
    let Some(probes) = inputs.probes else {
        return ProbePlan::NotDue;
    };
    let Some(index) = phases.iter().position(|p| p.phase == Phase::Prose) else {
        return ProbePlan::NotDue;
    };
    let chosen = probes.bank.choose(probes.history);
    match cadence_core::benchmark::due(
        probes.history.completed_with_prose_since,
        u64::from(policy.benchmark.every_completed_sessions),
        chosen.is_some(),
    ) {
        cadence_core::benchmark::Due::NotYet => return ProbePlan::NotDue,
        cadence_core::benchmark::Due::NoPassage => return ProbePlan::NoPassage,
        cadence_core::benchmark::Due::Probe => {}
    }
    let (Some((passage, same_strata)), Some(prose)) = (chosen, phases.get_mut(index)) else {
        return ProbePlan::NotDue;
    };
    let probe_seconds = policy.benchmark.probe_seconds.min(prose.seconds);
    let rest = prose.seconds.saturating_sub(probe_seconds);
    prose.seconds = rest;
    let probe_phase = PlannedPhase {
        phase: Phase::Probe,
        seconds: probe_seconds,
        exercises: vec![PlannedExercise {
            units: passage.units.clone(),
            kind: ExerciseKind::Probe,
            words: Vec::new(),
            passage: Some(passage.id),
            caption: Some("Transfer probe \u{b7} unseen text \u{b7} not practice".to_string()),
            target: None,
            spans: Vec::new(),
        }],
    };
    if rest == 0 {
        if let Some(slot) = phases.get_mut(index) {
            *slot = probe_phase;
        }
    } else {
        phases.insert(index, probe_phase);
    }
    ProbePlan::Planned(PlannedProbe {
        passage: passage.id,
        strata: passage.strata,
        same_strata,
    })
}

/// The pace decision for one typing track (L-06, #61): the proved
/// `cadence_kernel::pace::decide` over the track's recent blocks, one block
/// per session that typed it, oldest first. `target` is the track's last
/// suggested pace in correct units per minute (none yet: `NoBaseline`).
/// `recent` is newest first, as stored. Nothing is displayed or persisted
/// yet (#62).
///
/// # Errors
/// [`PlanError::Content`] when a stored track's counters are inconsistent.
pub fn pace_decision(
    policy: &crate::policy::PacePolicy,
    track: cadence_core::progress::Track,
    target: Option<u64>,
    recent: &[cadence_core::progress::SessionRecap],
) -> Result<cadence_core::pace::Decision, PlanError> {
    let mut blocks = Vec::new();
    for (session, recap) in recent.iter().rev().enumerate() {
        for progress in recap.tracks.iter().filter(|t| t.track == track) {
            if !progress.is_valid() {
                return Err(PlanError::Content(
                    "inconsistent track evidence".to_string(),
                ));
            }
            blocks.push(cadence_core::pace::Block {
                first_correct: progress.first_correct,
                first_attempted: progress.first_attempted,
                correct_attempts: progress.correct_attempts,
                content_attempts: progress.content_attempts,
                session: u64::try_from(session)
                    .map_err(|_unrepresentable| PlanError::Content("session index".to_string()))?,
            });
        }
    }
    Ok(cadence_core::pace::decide(
        target,
        u64::from(policy.desired_first_pass.get()),
        &blocks,
        cadence_core::pace::Policy {
            slow_down_below_bp: u64::from(policy.slow_down_below.get()),
            qualifying_blocks: u64::from(policy.qualifying_blocks_to_raise),
            minimum_sessions: u64::from(policy.minimum_sessions_to_raise),
            minimum_positions: policy.minimum_first_attempt_positions_per_block,
            raise_bp: u64::from(policy.maximum_raise.get()),
            lower_bp: u64::from(policy.maximum_lower.get()),
        },
    ))
}

/// Completed sessions that included prose since the last probe, newest
/// first as stored. Until probes are stored (#64) every such session in the
/// history counts.
#[must_use]
pub fn completed_with_prose(recent: &[cadence_core::progress::SessionRecap]) -> u64 {
    let count = recent
        .iter()
        .filter(|recap| {
            recap.completed
                && recap
                    .tracks
                    .iter()
                    .any(|t| t.track == cadence_core::progress::Track::Prose)
        })
        .count();
    u64::try_from(count).unwrap_or(u64::MAX)
}

/// Whether this session carries a transfer probe (P-03, #63): the proved
/// `cadence_kernel::benchmark::due` (#64 splits the prose phase on
/// `Probe`).
#[must_use]
pub fn probe_due(
    policy: &crate::policy::BenchmarkPolicy,
    recent: &[cadence_core::progress::SessionRecap],
    unseen_available: bool,
) -> cadence_core::benchmark::Due {
    cadence_core::benchmark::due(
        completed_with_prose(recent),
        u64::from(policy.every_completed_sessions),
        unseen_available,
    )
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "test fixtures use small bounded counts; overflow is impossible by construction"
)]
#[path = "plan_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "plan_pace_tests.rs"]
mod pace_tests;

#[cfg(test)]
mod probe_tests {
    use super::*;
    use cadence_core::benchmark::Due;
    use cadence_core::progress::{SessionRecap, Track, TrackProgress};

    fn policy() -> crate::policy::BenchmarkPolicy {
        crate::policy::parse_policy(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../policies/trainer-v3.toml"
            ))
            .unwrap()
            .as_slice(),
        )
        .unwrap()
        .benchmark
    }

    fn sessions(count: usize, completed: bool, track: Track) -> Vec<SessionRecap> {
        (0..count)
            .map(|_| SessionRecap {
                day: 1,
                completed,
                tracks: vec![TrackProgress::empty(track)],
                layout: None,
                content_class: cadence_core::progress::ContentClass::Standard,
            })
            .collect()
    }

    /// #63 table: 6 completed sessions is NotYet, 7 is Probe, 7 with no
    /// unseen passage is NoPassage; unfinished sessions and sessions
    /// without prose do not count.
    #[test]
    fn a_probe_is_due_after_seven_completed_prose_sessions() {
        let policy = policy();
        assert_eq!(policy.every_completed_sessions, 7);
        assert_eq!(
            probe_due(&policy, &sessions(6, true, Track::Prose), true),
            Due::NotYet
        );
        assert_eq!(
            probe_due(&policy, &sessions(7, true, Track::Prose), true),
            Due::Probe
        );
        assert_eq!(
            probe_due(&policy, &sessions(7, true, Track::Prose), false),
            Due::NoPassage
        );
        assert_eq!(
            probe_due(&policy, &sessions(7, false, Track::Prose), true),
            Due::NotYet
        );
        assert_eq!(
            probe_due(&policy, &sessions(7, true, Track::Words), true),
            Due::NotYet
        );
    }
}
