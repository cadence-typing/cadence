//! Learner model (plan 0003): built once at startup from recent evidence,
//! off the input/reduce/render path.
//!
//! * Error: Beta(`error_prior_alpha`, `error_prior_beta`) posterior over
//!   recency-weighted first attempts. A corrected mistake stays an error.
//! * Hesitation: a skill's mean clean interval relative to its key lane's
//!   mean; only skills with enough timed samples are compared.
//! * Uncertainty: the prior's share of the posterior; sparse evidence is
//!   uncertain, never "mastered".
//! * Due words: the spaced-review ladder replayed over recorded word
//!   outcomes, served FIFO by due day.
use std::collections::{BTreeMap, HashMap};

use cadence_core::learning::{
    HistoricSession, KeyLane, LearnerHistory, PlannerMode, Skill, SkillKey,
};
use cadence_core::{BasisPoints, CoreError, PassageId, TrainingChar, UtcDay, WordId};

use crate::policy::TrainerPolicy;
use crate::review::{DueQueue, ReviewOutcome, ReviewState};

/// Timed samples a skill needs before its hesitation is compared.
const MINIMUM_TIMED_SAMPLES: f64 = 3.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Evidence {
    attempts: f64,
    errors: f64,
    timed: f64,
    timed_micros: f64,
}

impl Evidence {
    fn add(&mut self, weight: f64, tally: cadence_core::learning::SkillTally) {
        self.attempts += weight * as_f64(tally.attempts);
        self.errors += weight * as_f64(tally.errors);
        self.timed += weight * as_f64(tally.timed);
        self.timed_micros += weight * as_f64(tally.timed_micros);
    }
}

/// Utility inputs derived for one skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkillScore {
    /// Posterior first-attempt error rate.
    pub error: BasisPoints,
    /// Slowness relative to the lane mean (0 = at or faster than mean;
    /// 10,000 = twice the mean or slower).
    pub hesitation: BasisPoints,
    /// Prior share of the posterior.
    pub uncertainty: BasisPoints,
}

/// The startup learner model.
#[derive(Debug, Clone, PartialEq)]
pub struct LearnerModel {
    skills: HashMap<SkillKey, Evidence>,
    lane_mean_interval: HashMap<KeyLane, f64>,
    lane_error_rate: HashMap<KeyLane, f64>,
    prior_alpha: f64,
    prior_beta: f64,
    focus: Vec<SkillKey>,
    due_words: Vec<WordId>,
    passage_age: HashMap<PassageId, usize>,
}

impl LearnerModel {
    /// A model with no evidence: cold start.
    #[must_use]
    pub fn cold(policy: &TrainerPolicy) -> Self {
        Self {
            skills: HashMap::new(),
            lane_mean_interval: HashMap::new(),
            lane_error_rate: HashMap::new(),
            prior_alpha: as_f64(policy.statistics.error_prior_alpha),
            prior_beta: as_f64(policy.statistics.error_prior_beta),
            focus: Vec::new(),
            due_words: Vec::new(),
            passage_age: HashMap::new(),
        }
    }

    /// Builds the model from history ordered oldest first.
    ///
    /// # Errors
    /// Malformed history days or an inconsistent review ladder.
    pub fn from_history(
        history: &LearnerHistory,
        policy: &TrainerPolicy,
        today: UtcDay,
    ) -> Result<Self, CoreError> {
        let mut model = Self::cold(policy);
        let learning = &policy.learning;
        let count = history.sessions.len();
        let half_life = f64::from(learning.half_life_sessions);
        let mut lane_totals: HashMap<KeyLane, Evidence> = HashMap::new();
        for (index, session) in history.sessions.iter().enumerate() {
            let age = count.saturating_sub(index).saturating_sub(1);
            if age < learning.prose_no_repeat_sessions {
                for passage in &session.learning.passages {
                    model
                        .passage_age
                        .entry(*passage)
                        .and_modify(|existing| *existing = (*existing).min(age))
                        .or_insert(age);
                }
            }
            if age >= learning.history_sessions {
                continue;
            }
            let weight = 0.5_f64.powf(as_f64_usize(age) / half_life);
            for (key, tally) in &session.learning.skills {
                model.skills.entry(*key).or_default().add(weight, *tally);
                lane_totals.entry(key.lane).or_default().add(weight, *tally);
            }
        }
        for (lane, total) in &lane_totals {
            if total.timed > 0.0 {
                model
                    .lane_mean_interval
                    .insert(*lane, total.timed_micros / total.timed);
            }
            if total.attempts > 0.0 {
                model
                    .lane_error_rate
                    .insert(*lane, total.errors / total.attempts);
            }
        }
        model.focus = model.choose_focus(policy);
        model.due_words = due_words(&history.sessions, policy, today)?;
        Ok(model)
    }

    /// Cold start or adaptive.
    #[must_use]
    pub fn mode(&self) -> PlannerMode {
        if self.focus.is_empty() && self.due_words.is_empty() {
            PlannerMode::ColdStart
        } else {
            PlannerMode::Adaptive
        }
    }

    /// Focus skills, strongest first.
    #[must_use]
    pub fn focus(&self) -> &[SkillKey] {
        &self.focus
    }

    /// Words due for spaced review, FIFO by due day.
    #[must_use]
    pub fn due_words(&self) -> &[WordId] {
        &self.due_words
    }

    /// Sessions since a passage was last typed, within the no-repeat window.
    #[must_use]
    pub fn passage_age(&self, passage: PassageId) -> Option<usize> {
        self.passage_age.get(&passage).copied()
    }

    /// Utility inputs for one skill; unseen skills are fully uncertain.
    #[must_use]
    pub fn score(&self, key: SkillKey) -> SkillScore {
        let evidence = self.skills.get(&key).copied().unwrap_or_default();
        let prior = self.prior_alpha + self.prior_beta;
        let posterior = (evidence.errors + self.prior_alpha) / (evidence.attempts + prior);
        let hesitation = match self.lane_mean_interval.get(&key.lane) {
            Some(mean) if evidence.timed >= MINIMUM_TIMED_SAMPLES && *mean > 0.0 => {
                evidence.timed_micros / evidence.timed / mean - 1.0
            }
            _ => 0.0,
        };
        SkillScore {
            error: basis_points(posterior),
            hesitation: basis_points(hesitation),
            uncertainty: basis_points(prior / (evidence.attempts + prior)),
        }
    }

    /// Word inputs: its weakest main-lane transition and unit dominate.
    #[must_use]
    pub fn word_score(&self, units: &[TrainingChar]) -> SkillScore {
        let mut best = SkillScore::default();
        let main = |skill| SkillKey {
            lane: KeyLane::Main,
            skill,
        };
        let units_iter = units.iter().map(|unit| main(Skill::Unit(*unit)));
        let pairs = units.windows(2).filter_map(|pair| match pair {
            [first, second] => Some(main(Skill::Bigram(*first, *second))),
            _ => None,
        });
        let mut uncertainty_sum = 0_u64;
        let mut count = 0_u64;
        for key in units_iter.chain(pairs) {
            let score = self.score(key);
            best.error = best.error.max(score.error);
            best.hesitation = best.hesitation.max(score.hesitation);
            uncertainty_sum = uncertainty_sum.saturating_add(u64::from(score.uncertainty.get()));
            count = count.saturating_add(1);
        }
        best.uncertainty = uncertainty_sum
            .checked_div(count)
            .and_then(|mean| u16::try_from(mean).ok())
            .and_then(|mean| BasisPoints::new(mean).ok())
            .unwrap_or(BasisPoints::MAX);
        best
    }

    /// Ranks letter skills by error excess over the lane rate and relative
    /// hesitation, each counted only above its policy threshold, so noise
    /// never becomes a focus. A key already explained by a chosen transition
    /// into it is skipped in favour of another skill.
    fn choose_focus(&self, policy: &TrainerPolicy) -> Vec<SkillKey> {
        let learning = &policy.learning;
        let minimum = f64::from(learning.minimum_focus_attempts);
        let weights = &policy.weights;
        let above = |value: BasisPoints, threshold: BasisPoints| {
            if value >= threshold {
                u64::from(value.get())
            } else {
                0
            }
        };
        let mut ranked: Vec<(u64, SkillKey)> = self
            .skills
            .iter()
            .filter(|(key, evidence)| {
                key.lane == KeyLane::Main
                    && is_word_skill(key.skill)
                    && evidence.attempts >= minimum
            })
            .filter_map(|(key, evidence)| {
                let score = self.score(*key);
                let lane_rate = self.lane_error_rate.get(&key.lane).copied().unwrap_or(0.0);
                let excess = if evidence.errors > 0.0 {
                    basis_points(f64::from(score.error.get()) / 10_000.0 - lane_rate)
                } else {
                    BasisPoints::ZERO
                };
                let value = u64::from(weights.error.get())
                    .saturating_mul(above(excess, learning.focus_minimum_error_excess))
                    .saturating_add(u64::from(weights.hesitation.get()).saturating_mul(above(
                        score.hesitation,
                        learning.focus_minimum_hesitation,
                    )));
                (value > 0).then_some((value, *key))
            })
            .collect();
        ranked.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
        let mut chosen: Vec<SkillKey> = Vec::with_capacity(learning.focus_skill_count);
        for (_, key) in ranked {
            if chosen.len() >= learning.focus_skill_count {
                break;
            }
            let explained = match key.skill {
                Skill::Unit(unit) => chosen
                    .iter()
                    .any(|other| matches!(other.skill, Skill::Bigram(_, second) if second == unit)),
                Skill::Bigram(..) => false,
            };
            if !explained {
                chosen.push(key);
            }
        }
        chosen
    }
}

/// Word targets use lowercase letters and the apostrophe only.
fn is_word_skill(skill: Skill) -> bool {
    let letter = |unit: TrainingChar| unit.get().is_ascii_lowercase() || unit.get() == '\'';
    match skill {
        Skill::Unit(unit) => letter(unit),
        Skill::Bigram(first, second) => letter(first) && letter(second),
    }
}

/// Replays the review ladder over recorded outcomes: an error schedules the
/// 1-day rung; a clean completion on or after the due day counts toward
/// promotion; clean typing before the due day is ordinary practice.
fn due_words(
    sessions: &[HistoricSession],
    policy: &TrainerPolicy,
    today: UtcDay,
) -> Result<Vec<WordId>, CoreError> {
    let queue = DueQueue::new();
    let ladder = &policy.review.interval_days;
    let mut states: BTreeMap<WordId, ReviewState> = BTreeMap::new();
    for session in sessions {
        let day = UtcDay::from_days_since_epoch(session.day)?;
        for (word, tally) in &session.learning.words {
            let state = states.get(word).copied();
            let outcome = if tally.errors > 0 {
                ReviewOutcome::Error { on: day }
            } else {
                match state.and_then(|state| state.due) {
                    Some(due) if day >= due => ReviewOutcome::Clean { on: day },
                    _ => continue,
                }
            };
            let next = queue.next_state(
                state.unwrap_or_else(ReviewState::fresh),
                outcome,
                ladder,
                day,
            )?;
            states.insert(*word, next);
        }
    }
    let mut due: Vec<(UtcDay, WordId)> = states
        .into_iter()
        .filter_map(|(word, state)| state.due.filter(|due| *due <= today).map(|due| (due, word)))
        .collect();
    due.sort();
    let mut fifo = DueQueue::new();
    for (since, word) in due
        .into_iter()
        .take(policy.learning.maximum_due_reviews_per_session)
    {
        fifo.admit(word, since)?;
    }
    let mut ordered = Vec::with_capacity(fifo.len());
    while let Some(entry) = fifo.serve_head() {
        ordered.push(entry.word);
    }
    Ok(ordered)
}

/// Converts a clamped ratio in `0.0..=1.0` to basis points.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0..=10000 and rounded before the cast"
)]
fn basis_points(ratio: f64) -> BasisPoints {
    if !ratio.is_finite() {
        return BasisPoints::ZERO;
    }
    let scaled = (ratio.clamp(0.0, 1.0) * 10_000.0).round() as u16;
    BasisPoints::new(scaled).unwrap_or(BasisPoints::MAX)
}

/// Counts are far below 2^52; model arithmetic is approximate by design.
#[expect(
    clippy::as_conversions,
    reason = "no lossless integer to f64 conversion exists; counts are far below 2^52"
)]
const fn as_f64(value: u64) -> f64 {
    value as f64
}

/// Session ages are bounded by the history window.
#[expect(
    clippy::as_conversions,
    reason = "no lossless integer to f64 conversion exists; ages are bounded by the history window"
)]
const fn as_f64_usize(value: usize) -> f64 {
    value as f64
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "test fixtures use small bounded counts; overflow is impossible by construction"
)]
#[path = "learner_tests.rs"]
mod tests;
