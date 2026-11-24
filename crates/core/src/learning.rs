//! Per-session learning evidence (plan 0003; L-02 first attempts, L-03 timing).
//!
//! The reducer folds first-attempt facts into a bounded [`SessionLearning`]
//! value that travels with the cumulative checkpoint. Nothing here changes a
//! displayed score: a corrected mistake stays an error, and the hesitation
//! ceiling only excludes long intervals from the *learner model*, never from
//! active time or speed.
use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use crate::alphabet::{LayoutSupportsPound, TrainingChar};
use crate::{CoreError, Micros, PassageId, SessionId, WordId};

#[cfg(test)]
#[path = "learning_tests.rs"]
mod tests;

/// V1 training alphabet size: printable ASCII (95), newline and `£`.
pub const TRAINING_ALPHABET_SIZE: usize = 97;
/// Structural bound on distinct skills in one session: two key lanes, each
/// with every unit and every ordered unit pair of the training alphabet.
pub const MAXIMUM_SESSION_SKILLS: usize =
    2 * (TRAINING_ALPHABET_SIZE + TRAINING_ALPHABET_SIZE * TRAINING_ALPHABET_SIZE);
/// Distinct planned words in one session. The longest legal session is
/// 1,800 s; even 4 words/s (≈240 WPM) stays below this bound.
pub const MAXIMUM_SESSION_WORDS: usize = 8_192;
/// Distinct prose passages served in one session (≥ one per 2 s of the
/// longest legal session).
pub const MAXIMUM_SESSION_PASSAGES: usize = 1_024;
/// Focus skills recorded for one plan.
pub const MAXIMUM_FOCUS_SKILLS: usize = 16;

/// Physically distinct key groups keep separate histories (L-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyLane {
    /// Main keyboard: words, prose, number row.
    Main,
    /// Numeric keypad.
    Numpad,
}

impl KeyLane {
    /// Stable byte in the learning digest (declaration order; never reuse).
    #[must_use]
    pub const fn digest_tag(self) -> u8 {
        match self {
            Self::Main => 0,
            Self::Numpad => 1,
        }
    }

    /// Stable persisted name.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Numpad => "numpad",
        }
    }

    /// Parses a persisted name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "main" => Some(Self::Main),
            "numpad" => Some(Self::Numpad),
            _ => None,
        }
    }
}

/// A trainable skill: one key, or one ordered key transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Skill {
    /// A single expected unit.
    Unit(TrainingChar),
    /// The transition into the second unit from the first.
    Bigram(TrainingChar, TrainingChar),
}

impl Skill {
    /// Stable persisted kind name.
    #[must_use]
    pub const fn kind_key(self) -> &'static str {
        match self {
            Self::Unit(_) => "unit",
            Self::Bigram(..) => "bigram",
        }
    }

    /// The skill's characters, as typed.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Unit(unit) => unit.get().to_string(),
            Self::Bigram(first, second) => [first.get(), second.get()].iter().collect(),
        }
    }

    /// Parses a persisted kind and text pair.
    ///
    /// # Errors
    /// Rejects unknown kinds, wrong lengths and characters outside the
    /// training alphabet.
    pub fn parse(kind: &str, text: &str) -> Result<Self, CoreError> {
        let mut units = text
            .chars()
            .map(|character| TrainingChar::new(character, LayoutSupportsPound::UK));
        let mut next = || -> Result<Option<TrainingChar>, CoreError> {
            units.next().transpose().map_err(|_rejected| invalid())
        };
        let skill = match (kind, next()?, next()?) {
            ("unit", Some(unit), None) => Self::Unit(unit),
            ("bigram", Some(first), Some(second)) => Self::Bigram(first, second),
            _ => return Err(invalid()),
        };
        if next()?.is_some() {
            return Err(invalid());
        }
        Ok(skill)
    }
}

/// A skill within its key lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkillKey {
    /// Key group.
    pub lane: KeyLane,
    /// Key or transition.
    pub skill: Skill,
}

/// First-attempt counts and clean inter-key timing for one skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkillTally {
    /// First attempts at this skill.
    pub attempts: u64,
    /// First attempts that were wrong (later corrections do not erase them).
    pub errors: u64,
    /// Clean intervals measured (subset of attempts).
    pub timed: u64,
    /// Sum of the measured clean intervals.
    pub timed_micros: u64,
}

impl SkillTally {
    fn valid(self) -> bool {
        self.errors <= self.attempts && self.timed <= self.attempts
    }
    fn covers(self, old: Self) -> bool {
        self.attempts >= old.attempts
            && self.errors >= old.errors
            && self.timed >= old.timed
            && self.timed_micros >= old.timed_micros
    }
}

/// Completed occurrences of a planned word and how many were not clean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WordTally {
    /// Completed occurrences.
    pub attempts: u64,
    /// Occurrences with any wrong insertion, corrected or not.
    pub errors: u64,
}

/// Whether a plan used learner evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlannerMode {
    /// No usable evidence: seeded broad sampling.
    ColdStart,
    /// Targets weighted by recorded weaknesses and due reviews.
    Adaptive,
}

impl PlannerMode {
    /// Stable byte in the learning digest (declaration order; never reuse).
    #[must_use]
    pub const fn digest_tag(self) -> u8 {
        match self {
            Self::ColdStart => 0,
            Self::Adaptive => 1,
        }
    }

    /// Stable persisted name.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::ColdStart => "cold_start",
            Self::Adaptive => "adaptive",
        }
    }

    /// Parses a persisted name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "cold_start" => Some(Self::ColdStart),
            "adaptive" => Some(Self::Adaptive),
            _ => None,
        }
    }
}

/// The replay identity of a session plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRecord {
    /// The ChaCha8 seed the plan was built from.
    pub seed: u64,
    /// Cold start or adaptive.
    pub mode: PlannerMode,
    /// Focus skills chosen at planning time, strongest first.
    pub focus: Vec<SkillKey>,
}

/// Cumulative learning evidence for one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLearning {
    /// How the plan was built.
    pub plan: PlanRecord,
    /// Per-skill first-attempt and timing tallies.
    pub skills: BTreeMap<SkillKey, SkillTally>,
    /// Per-word completion outcomes for planned word exercises.
    pub words: BTreeMap<WordId, WordTally>,
    /// Prose passages actually typed.
    pub passages: BTreeSet<PassageId>,
    /// Missed words re-served later in the same session.
    pub reinforced: u64,
}

impl SessionLearning {
    /// Empty evidence for a plan.
    #[must_use]
    pub const fn new(plan: PlanRecord) -> Self {
        Self {
            plan,
            skills: BTreeMap::new(),
            words: BTreeMap::new(),
            passages: BTreeSet::new(),
            reinforced: 0,
        }
    }

    /// Records one first attempt at `key`, with an optional clean interval.
    ///
    /// # Errors
    /// Refuses counter overflow and the structural skill bound.
    pub fn observe_skill(
        &mut self,
        key: SkillKey,
        error: bool,
        interval: Option<Micros>,
    ) -> Result<(), CoreError> {
        if !self.skills.contains_key(&key) && self.skills.len() >= MAXIMUM_SESSION_SKILLS {
            return Err(CoreError::CapacityExhausted {
                limit: "session skill observations",
            });
        }
        let mut tally = self.skills.get(&key).copied().unwrap_or_default();
        tally.attempts = increment(tally.attempts)?;
        if error {
            tally.errors = increment(tally.errors)?;
        }
        if let Some(interval) = interval {
            tally.timed = increment(tally.timed)?;
            tally.timed_micros = tally.timed_micros.checked_add(interval.as_us()).ok_or(
                CoreError::ArithmeticOverflow {
                    operation: "skill timing accumulation",
                },
            )?;
        }
        self.skills.insert(key, tally);
        Ok(())
    }

    /// Records one completed occurrence of a planned word.
    ///
    /// # Errors
    /// Refuses counter overflow and the per-session word bound.
    pub fn observe_word(&mut self, word: WordId, error: bool) -> Result<(), CoreError> {
        if !self.words.contains_key(&word) && self.words.len() >= MAXIMUM_SESSION_WORDS {
            return Err(CoreError::CapacityExhausted {
                limit: "session word outcomes",
            });
        }
        let mut tally = self.words.get(&word).copied().unwrap_or_default();
        tally.attempts = increment(tally.attempts)?;
        if error {
            tally.errors = increment(tally.errors)?;
        }
        self.words.insert(word, tally);
        Ok(())
    }

    /// Records that a prose passage was typed.
    ///
    /// # Errors
    /// Refuses the per-session passage bound.
    pub fn observe_passage(&mut self, passage: PassageId) -> Result<(), CoreError> {
        if !self.passages.contains(&passage) && self.passages.len() >= MAXIMUM_SESSION_PASSAGES {
            return Err(CoreError::CapacityExhausted {
                limit: "session prose passages",
            });
        }
        self.passages.insert(passage);
        Ok(())
    }

    /// Counts one same-session reinforcement substitution.
    ///
    /// # Errors
    /// Refuses counter overflow.
    pub fn note_reinforcement(&mut self) -> Result<(), CoreError> {
        self.reinforced = increment(self.reinforced)?;
        Ok(())
    }

    /// Validates structural bounds and tally invariants.
    ///
    /// # Errors
    /// Returns an identity error for malformed evidence.
    pub fn validate(&self) -> Result<(), CoreError> {
        let mut values = self
            .skills
            .values()
            .flat_map(|t| [t.attempts, t.errors, t.timed, t.timed_micros])
            .chain(self.words.values().flat_map(|t| [t.attempts, t.errors]))
            .chain([self.reinforced]);
        if self.skills.len() > MAXIMUM_SESSION_SKILLS
            || self.words.len() > MAXIMUM_SESSION_WORDS
            || self.passages.len() > MAXIMUM_SESSION_PASSAGES
            || self.plan.focus.len() > MAXIMUM_FOCUS_SKILLS
            || !self.skills.values().all(|tally| tally.valid())
            || !self
                .words
                .values()
                .all(|tally| tally.errors <= tally.attempts)
            || values.any(|value| value > i64::MAX.unsigned_abs())
        {
            return Err(invalid());
        }
        Ok(())
    }

    /// Whether `self` is a cumulative advance of `old` within one session.
    #[must_use]
    pub fn advances_from(&self, old: &Self) -> bool {
        self.plan == old.plan
            && self.reinforced >= old.reinforced
            && old.passages.is_subset(&self.passages)
            && old.skills.iter().all(|(key, tally)| {
                self.skills
                    .get(key)
                    .is_some_and(|current| current.covers(*tally))
            })
            && old.words.iter().all(|(word, tally)| {
                self.words.get(word).is_some_and(|current| {
                    current.attempts >= tally.attempts && current.errors >= tally.errors
                })
            })
    }

    /// Appends the canonical, length-prefixed encoding to a checkpoint hash.
    pub(crate) fn digest_into(&self, hash: &mut Sha256) {
        hash.update(b"cadence-learning-v1");
        hash.update(self.plan.seed.to_le_bytes());
        hash.update([self.plan.mode.digest_tag()]);
        hash.update(length(self.plan.focus.len()));
        for key in &self.plan.focus {
            skill_bytes(hash, *key);
        }
        hash.update(length(self.skills.len()));
        for (key, tally) in &self.skills {
            skill_bytes(hash, *key);
            for value in [
                tally.attempts,
                tally.errors,
                tally.timed,
                tally.timed_micros,
            ] {
                hash.update(value.to_le_bytes());
            }
        }
        hash.update(length(self.words.len()));
        for (word, tally) in &self.words {
            for value in [word.get(), tally.attempts, tally.errors] {
                hash.update(value.to_le_bytes());
            }
        }
        hash.update(length(self.passages.len()));
        for passage in &self.passages {
            hash.update(passage.get().to_le_bytes());
        }
        hash.update(self.reinforced.to_le_bytes());
    }
}

/// One past session's evidence, as read back at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricSession {
    /// Session identity.
    pub session: SessionId,
    /// Calendar day of the session.
    pub day: i64,
    /// Its recorded evidence.
    pub learning: SessionLearning,
}

/// Recent learning evidence, oldest first, bounded by the caller's window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LearnerHistory {
    /// Sessions with recorded evidence.
    pub sessions: Vec<HistoricSession>,
}

fn skill_bytes(hash: &mut Sha256, key: SkillKey) {
    hash.update([key.lane.digest_tag()]);
    match key.skill {
        Skill::Unit(unit) => {
            hash.update([0]);
            hash.update(u32::from(unit.get()).to_le_bytes());
        }
        Skill::Bigram(first, second) => {
            hash.update([1]);
            hash.update(u32::from(first.get()).to_le_bytes());
            hash.update(u32::from(second.get()).to_le_bytes());
        }
    }
}

fn length(count: usize) -> [u8; 8] {
    // usize is at most 64 bits on every supported target.
    u64::try_from(count).unwrap_or(u64::MAX).to_le_bytes()
}

fn increment(value: u64) -> Result<u64, CoreError> {
    value.checked_add(1).ok_or(CoreError::ArithmeticOverflow {
        operation: "learning tally",
    })
}

fn invalid() -> CoreError {
    CoreError::InvalidIdentity {
        kind: "SessionLearning",
        reason: "malformed or out-of-bounds learning evidence",
    }
}
