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
//!
//! All arithmetic is integer: evidence is fixed point (one undecayed
//! attempt weighs `focus::WEIGHT_ONE`) and every number that ranks a focus
//! comes from the proved `cadence_kernel::focus` functions (#38, #56).
use std::collections::{BTreeMap, HashMap};

use cadence_core::focus::{self, WEIGHT_ONE};
use cadence_core::learning::{
    HistoricSession, KeyLane, LearnerHistory, PlannerMode, Skill, SkillKey,
};
use cadence_core::{BasisPoints, CoreError, PassageId, TrainingChar, UtcDay, WordId};

use crate::policy::TrainerPolicy;
use crate::review::{DueQueue, ReviewState, is_due, ladder_step};

/// Timed samples a skill needs before its hesitation is compared.
const MINIMUM_TIMED_SAMPLES: u64 = 3;

/// Recency-weighted evidence in fixed point (`WEIGHT_ONE` per undecayed
/// sample). Sums saturate: they are far below `u64::MAX` for any history
/// the window admits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Evidence {
    attempts: u64,
    errors: u64,
    timed: u64,
    timed_micros: u64,
}

impl Evidence {
    fn add(&mut self, weight: u64, tally: cadence_core::learning::SkillTally) {
        let weighted = |value: u64| weight.saturating_mul(value);
        self.attempts = self.attempts.saturating_add(weighted(tally.attempts));
        self.errors = self.errors.saturating_add(weighted(tally.errors));
        self.timed = self.timed.saturating_add(weighted(tally.timed));
        self.timed_micros = self
            .timed_micros
            .saturating_add(weighted(tally.timed_micros));
    }
}

/// A kernel basis-point value as `BasisPoints` (the kernel keeps it within
/// `0..=10_000`).
fn bp(value: u64) -> BasisPoints {
    u16::try_from(value.min(focus::BASIS))
        .ok()
        .and_then(|v| BasisPoints::new(v).ok())
        .unwrap_or(BasisPoints::MAX)
}

/// A focus group (#46): each has its own error-rate and interval baseline
/// and its own focus list; L-01 keeps the number row and the numpad apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FocusGroup {
    /// Letter keys and transitions (the words phase).
    Letters,
    /// Number-row digits and digit pairs (lane `main`).
    Row,
    /// Numpad digits, the decimal point and their pairs (lane `numpad`).
    Numpad,
}

impl FocusGroup {
    /// Every group, in focus-list order.
    pub const ALL: [Self; 3] = [Self::Letters, Self::Row, Self::Numpad];
}

/// The group a skill belongs to, if any (other keys are recorded but never
/// a focus).
#[must_use]
pub fn group_of(key: SkillKey) -> Option<FocusGroup> {
    let digit = |unit: TrainingChar| unit.get().is_ascii_digit();
    let numpad = |unit: TrainingChar| unit.get().is_ascii_digit() || unit.get() == '.';
    let all = |test: &dyn Fn(TrainingChar) -> bool| match key.skill {
        Skill::Unit(unit) => test(unit),
        Skill::Bigram(first, second) => test(first) && test(second),
    };
    match key.lane {
        KeyLane::Main if is_word_skill(key.skill) => Some(FocusGroup::Letters),
        KeyLane::Main if all(&digit) => Some(FocusGroup::Row),
        KeyLane::Numpad if all(&numpad) => Some(FocusGroup::Numpad),
        KeyLane::Main | KeyLane::Numpad => None,
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
    /// Per focus group: weighted timed samples and weighted microseconds.
    group_timing: HashMap<FocusGroup, (u64, u64)>,
    /// Per focus group: observed first-attempt error rate (basis points).
    group_error_rate: HashMap<FocusGroup, u64>,
    prior_alpha: u64,
    prior_beta: u64,
    focus: Vec<SkillKey>,
    /// The focus split by group (letters, then row, then numpad).
    focus_groups: Vec<(FocusGroup, Vec<SkillKey>)>,
    due_words: Vec<WordId>,
    /// Each due word's ladder rung, parallel to `due_words` (#47).
    due_rungs: Vec<usize>,
    passage_age: HashMap<PassageId, usize>,
}

impl LearnerModel {
    /// A model with no evidence: cold start.
    #[must_use]
    pub fn cold(policy: &TrainerPolicy) -> Self {
        Self {
            skills: HashMap::new(),
            group_timing: HashMap::new(),
            group_error_rate: HashMap::new(),
            prior_alpha: policy.statistics.error_prior_alpha,
            prior_beta: policy.statistics.error_prior_beta,
            focus: Vec::new(),
            focus_groups: Vec::new(),
            due_words: Vec::new(),
            due_rungs: Vec::new(),
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
        let half_life = u64::from(learning.half_life_sessions);
        let mut group_totals: HashMap<FocusGroup, Evidence> = HashMap::new();
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
            let weight = focus::decay_weight(u64::try_from(age).unwrap_or(u64::MAX), half_life);
            for (key, tally) in &session.learning.skills {
                model.skills.entry(*key).or_default().add(weight, *tally);
                if let Some(group) = group_of(*key) {
                    group_totals.entry(group).or_default().add(weight, *tally);
                }
            }
        }
        for (group, total) in &group_totals {
            if total.timed > 0 {
                model
                    .group_timing
                    .insert(*group, (total.timed, total.timed_micros));
            }
            if let Some(rate) = total
                .errors
                .checked_mul(focus::BASIS)
                .and_then(|scaled| scaled.checked_div(total.attempts))
            {
                model.group_error_rate.insert(*group, rate);
            }
        }
        let learning = &policy.learning;
        for (group, count) in [
            (FocusGroup::Letters, learning.focus_skill_count),
            (FocusGroup::Row, learning.row_focus_skill_count),
            (FocusGroup::Numpad, learning.numpad_focus_skill_count),
        ] {
            let chosen = model.choose_focus(policy, group, count);
            model.focus.extend(chosen.iter().copied());
            model.focus_groups.push((group, chosen));
        }
        let due = due_words(&history.sessions, policy, today)?;
        model.due_words = due.iter().map(|(word, _)| *word).collect();
        model.due_rungs = due.iter().map(|(_, rung)| *rung).collect();
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

    /// One group's focus skills, strongest first (#46).
    #[must_use]
    pub fn focus_in(&self, group: FocusGroup) -> &[SkillKey] {
        self.focus_groups
            .iter()
            .find(|(g, _)| *g == group)
            .map_or(&[], |(_, keys)| keys.as_slice())
    }

    /// Words due for spaced review, FIFO by due day.
    #[must_use]
    pub fn due_words(&self) -> &[WordId] {
        &self.due_words
    }

    /// Due words with their ladder rung (0 = the 1-day rung), FIFO (#47).
    pub fn due_with_rungs(&self) -> impl Iterator<Item = (WordId, usize)> + '_ {
        self.due_words
            .iter()
            .copied()
            .zip(self.due_rungs.iter().copied())
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
        let hesitation = group_of(key)
            .and_then(|group| self.group_timing.get(&group))
            .map_or(0, |(group_timed, group_micros)| {
                focus::hesitation_bp(
                    evidence.timed,
                    evidence.timed_micros,
                    *group_timed,
                    *group_micros,
                    MINIMUM_TIMED_SAMPLES.saturating_mul(WEIGHT_ONE),
                )
            });
        SkillScore {
            error: bp(focus::posterior_bp(
                evidence.errors,
                evidence.attempts,
                self.prior_alpha,
                self.prior_beta,
            )),
            hesitation: bp(hesitation),
            uncertainty: bp(focus::uncertainty_bp(
                evidence.attempts,
                self.prior_alpha,
                self.prior_beta,
            )),
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
    /// never becomes a focus (the proved `focus::pick`, candidates in key
    /// order so map iteration order cannot matter). A key already explained
    /// by a chosen transition into it is skipped in favour of another skill.
    fn choose_focus(
        &self,
        policy: &TrainerPolicy,
        group: FocusGroup,
        count: usize,
    ) -> Vec<SkillKey> {
        let learning = &policy.learning;
        let thresholds = focus::Thresholds {
            attempts: u64::from(learning.minimum_focus_attempts).saturating_mul(WEIGHT_ONE),
            excess: u64::from(learning.focus_minimum_error_excess.get()),
            hesitation: u64::from(learning.focus_minimum_hesitation.get()),
            weight_error: u64::from(policy.weights.error.get()),
            weight_hesitation: u64::from(policy.weights.hesitation.get()),
        };
        let mut keys: Vec<SkillKey> = self
            .skills
            .keys()
            .filter(|key| group_of(**key) == Some(group))
            .copied()
            .collect();
        keys.sort();
        let candidates: Vec<focus::Candidate> = keys
            .iter()
            .map(|key| {
                let evidence = self.skills.get(key).copied().unwrap_or_default();
                let score = self.score(*key);
                focus::Candidate {
                    attempts: evidence.attempts,
                    errors: evidence.errors,
                    posterior: u64::from(score.error.get()),
                    group_rate: self.group_error_rate.get(&group).copied().unwrap_or(0),
                    hesitation: u64::from(score.hesitation.get()),
                }
            })
            .collect();
        let mut taken = vec![false; candidates.len()];
        let mut chosen: Vec<SkillKey> = Vec::with_capacity(count);
        while chosen.len() < count {
            let Some(index) = focus::pick(&candidates, &taken, thresholds) else {
                break;
            };
            let (Some(flag), Some(key)) = (taken.get_mut(index), keys.get(index)) else {
                break;
            };
            *flag = true;
            let explained = match key.skill {
                Skill::Unit(unit) => chosen
                    .iter()
                    .any(|other| matches!(other.skill, Skill::Bigram(_, second) if second == unit)),
                Skill::Bigram(..) => false,
            };
            if !explained {
                chosen.push(*key);
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
) -> Result<Vec<(WordId, usize)>, CoreError> {
    let ladder = &policy.review.interval_days;
    let mut states: BTreeMap<WordId, ReviewState> = BTreeMap::new();
    // Replay in (day, session) order: after a change of UTC offset a later
    // session's local day can be earlier than the previous one's (#52); the
    // ladder must still see non-decreasing days.
    let mut ordered: Vec<(usize, &HistoricSession)> = sessions.iter().enumerate().collect();
    ordered.sort_by_key(|(index, session)| (session.day, *index));
    for (_, session) in ordered {
        let day = UtcDay::from_days_since_epoch(session.day)?;
        for (word, tally) in &session.learning.words {
            let state = states.get(word).copied().unwrap_or_else(ReviewState::fresh);
            // The kernel decides: an error resets, clean typing before the
            // due day is practice (None), a due clean counts.
            if let Some(next) = ladder_step(state, tally.errors > 0, day, ladder)? {
                states.insert(*word, next);
            }
        }
    }
    let mut due: Vec<(UtcDay, WordId)> = states
        .iter()
        .map(|(word, state)| (*word, *state))
        .filter(|(_, state)| is_due(*state, today))
        .filter_map(|(word, state)| state.due.map(|due| (due, word)))
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
        let rung = states
            .get(&entry.word)
            .map_or(0, |state| state.interval_index);
        ordered.push((entry.word, rung));
    }
    Ok(ordered)
}

#[cfg(test)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "test fixtures use small bounded counts; overflow is impossible by construction"
)]
#[path = "learner_tests.rs"]
mod tests;
