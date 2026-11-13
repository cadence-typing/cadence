//! The `[learning]` policy section (plan 0003): history window, decay,
//! focus selection, spaced review volume, reinforcement and prose rotation.
use cadence_core::{BasisPoints, Micros};

use crate::corpus::WordBand;
use crate::policy::PolicyError;

/// Validated learner-model and content-rotation policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearningPolicy {
    /// Sessions of recorded evidence read at startup.
    pub history_sessions: usize,
    /// Evidence weight halves every this many sessions back.
    pub half_life_sessions: u32,
    /// Longest clean interval that still counts as a keying measurement.
    pub hesitation_ceiling: Micros,
    /// Focus skills chosen per plan.
    pub focus_skill_count: usize,
    /// Decayed first attempts before a skill's error rate can focus it.
    pub minimum_focus_attempts: u32,
    /// Error excess over the lane rate below which error is not a signal.
    pub focus_minimum_error_excess: BasisPoints,
    /// Relative slowness below which hesitation is not a signal.
    pub focus_minimum_hesitation: BasisPoints,
    /// Highest band target slots may draw from.
    pub target_maximum_band: WordBand,
    /// Relevance input for core-band targets.
    pub core_relevance: BasisPoints,
    /// Relevance input for common-band targets.
    pub common_relevance: BasisPoints,
    /// Per-session cap on one target word.
    pub maximum_target_word_occurrences_per_session: u32,
    /// Due review words served per session.
    pub maximum_due_reviews_per_session: usize,
    /// Missed words awaiting same-session reinforcement.
    pub reinforcement_queue_capacity: usize,
    /// Sessions within which a typed passage is not re-served.
    pub prose_no_repeat_sessions: usize,
    /// Prose exercise length bound in units.
    pub prose_exercise_units: usize,
    /// Prepared prose units per allocated second.
    pub prose_units_per_second: u64,
    /// Allocated seconds per prepared word exercise.
    pub seconds_per_word_exercise: u64,
}

/// Upper bounds keeping startup work and payloads bounded.
impl LearningPolicy {
    /// Sessions of evidence the store must supply: enough for both the
    /// learner window and the prose no-repeat window.
    #[must_use]
    pub fn history_window(&self) -> usize {
        self.history_sessions.max(self.prose_no_repeat_sessions)
    }
}

const MAXIMUM_HISTORY_SESSIONS: usize = 1_000;
const MAXIMUM_FOCUS_SKILLS: usize = cadence_core::learning::MAXIMUM_FOCUS_SKILLS;
const MAXIMUM_QUEUE: usize = 64;
const MAXIMUM_HESITATION_CEILING_MS: u64 = 10_000;

fn integer<T: TryFrom<i64>>(table: &toml::Table, key: &str) -> Result<T, PolicyError> {
    let raw = table
        .get(key)
        .ok_or_else(|| PolicyError::MissingKey(key.into()))?
        .as_integer()
        .ok_or_else(|| PolicyError::WrongType(key.into()))?;
    T::try_from(raw).map_err(|_| PolicyError::OutOfRange {
        key: key.into(),
        reason: "integer outside representable range",
    })
}

fn basis_points(table: &toml::Table, key: &str) -> Result<BasisPoints, PolicyError> {
    BasisPoints::new(integer(table, key)?).map_err(|_| PolicyError::OutOfRange {
        key: key.into(),
        reason: "basis points must be 0..=10000",
    })
}

/// Decodes `[learning]`.
///
/// # Errors
/// Missing, mistyped or out-of-range values.
pub(crate) fn parse_learning(document: &toml::Table) -> Result<LearningPolicy, PolicyError> {
    let t = document
        .get("learning")
        .and_then(toml::Value::as_table)
        .ok_or(PolicyError::MissingTable("learning"))?;
    let band = match t.get("target_maximum_band").and_then(toml::Value::as_str) {
        Some("core") => WordBand::Core,
        Some("common") => WordBand::Common,
        Some(_) => {
            return Err(PolicyError::OutOfRange {
                key: "target_maximum_band".into(),
                reason: "target bands are core or common; tail is unvetted",
            });
        }
        None => return Err(PolicyError::WrongType("target_maximum_band".into())),
    };
    let ceiling: u64 = integer(t, "hesitation_ceiling_milliseconds")?;
    let policy = LearningPolicy {
        history_sessions: integer(t, "history_sessions")?,
        half_life_sessions: integer(t, "half_life_sessions")?,
        hesitation_ceiling: Micros::from_millis(ceiling).map_err(|_| PolicyError::OutOfRange {
            key: "hesitation_ceiling_milliseconds".into(),
            reason: "duration overflow",
        })?,
        focus_skill_count: integer(t, "focus_skill_count")?,
        minimum_focus_attempts: integer(t, "minimum_focus_attempts")?,
        focus_minimum_error_excess: basis_points(t, "focus_minimum_error_excess_basis_points")?,
        focus_minimum_hesitation: basis_points(t, "focus_minimum_hesitation_basis_points")?,
        target_maximum_band: band,
        core_relevance: basis_points(t, "core_relevance_basis_points")?,
        common_relevance: basis_points(t, "common_relevance_basis_points")?,
        maximum_target_word_occurrences_per_session: integer(
            t,
            "maximum_target_word_occurrences_per_session",
        )?,
        maximum_due_reviews_per_session: integer(t, "maximum_due_reviews_per_session")?,
        reinforcement_queue_capacity: integer(t, "reinforcement_queue_capacity")?,
        prose_no_repeat_sessions: integer(t, "prose_no_repeat_sessions")?,
        prose_exercise_units: integer(t, "prose_exercise_units")?,
        prose_units_per_second: integer(t, "prose_units_per_second")?,
        seconds_per_word_exercise: integer(t, "seconds_per_word_exercise")?,
    };
    let checks = [
        (
            (1..=MAXIMUM_HISTORY_SESSIONS).contains(&policy.history_sessions),
            "history_sessions must be 1..=1000",
        ),
        (
            policy.half_life_sessions > 0,
            "half_life_sessions must be positive",
        ),
        (
            (1..=MAXIMUM_HESITATION_CEILING_MS).contains(&ceiling),
            "hesitation ceiling must be 1..=10000 ms",
        ),
        (
            (1..=MAXIMUM_FOCUS_SKILLS).contains(&policy.focus_skill_count),
            "focus_skill_count must fit the persisted focus bound",
        ),
        (
            policy.minimum_focus_attempts > 0,
            "minimum_focus_attempts must be positive",
        ),
        (
            policy.maximum_target_word_occurrences_per_session > 0,
            "target word session cap must be positive",
        ),
        (
            policy.maximum_due_reviews_per_session <= cadence_core::learning::MAXIMUM_SESSION_WORDS,
            "due review volume exceeds the session word bound",
        ),
        (
            (1..=MAXIMUM_QUEUE).contains(&policy.reinforcement_queue_capacity),
            "reinforcement queue must be 1..=64",
        ),
        (
            policy.prose_no_repeat_sessions <= MAXIMUM_HISTORY_SESSIONS,
            "prose no-repeat window exceeds the history bound",
        ),
        (
            (1..=usize::from(u16::MAX)).contains(&policy.prose_exercise_units),
            "prose exercise bound must fit a target",
        ),
        (
            (1..=1_000).contains(&policy.prose_units_per_second),
            "prose planning rate must be 1..=1000 units/s",
        ),
        (
            (1..=600).contains(&policy.seconds_per_word_exercise),
            "seconds_per_word_exercise must be 1..=600",
        ),
    ];
    for (ok, what) in checks {
        if !ok {
            return Err(PolicyError::Inconsistent { what });
        }
    }
    Ok(policy)
}
