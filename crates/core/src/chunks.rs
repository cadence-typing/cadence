//! Observation chunks (L-01, D-02, #11): the evidence of one closed
//! exercise, immutable once stored. A session's chunks are numbered densely
//! from 1; the learner's per-session totals are their fold, and a
//! checkpoint names them by their count and a hash chain over their
//! digests, so a stored session's identity is recomputed from the stored
//! chunks alone. Which chunk is appended, replayed or refused is the proved
//! `cadence_kernel::chunks::admit`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::learning::{KeyLane, SessionLearning, Skill, SkillKey, SkillTally, WordTally};
use crate::progress::Track;
use crate::{ContentDigest, CoreError, PassageId, WordId};

pub use cadence_kernel::chunks::{Admit, admit, watermark_ok};

/// The chunk encoding's version.
const ENCODING: u64 = 1;

/// One closed exercise's evidence: what it added to the session's totals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationChunk {
    /// Its position in the session, from 1.
    pub seq: u64,
    /// The track the exercise belonged to.
    pub track: Track,
    /// First-attempt tallies the exercise added, per skill.
    pub skills: BTreeMap<SkillKey, SkillTally>,
    /// Word outcomes the exercise added.
    pub words: BTreeMap<WordId, WordTally>,
    /// Prose passages first typed in the exercise.
    pub passages: BTreeSet<PassageId>,
    /// Reinforcement substitutions the exercise added.
    pub reinforced: u64,
}

fn invalid(reason: &'static str) -> CoreError {
    CoreError::InvalidIdentity {
        kind: "ObservationChunk",
        reason,
    }
}

fn sub(now: u64, before: u64) -> Result<u64, CoreError> {
    now.checked_sub(before)
        .ok_or_else(|| invalid("evidence went backwards between chunks"))
}

fn add(left: u64, right: u64) -> Result<u64, CoreError> {
    left.checked_add(right)
        .ok_or(CoreError::ArithmeticOverflow {
            operation: "folding observation chunks",
        })
}

impl ObservationChunk {
    /// The evidence `now` holds beyond `before`, as chunk `seq` of `track`.
    ///
    /// # Errors
    /// Evidence that went backwards (never, for one session's snapshots).
    pub fn between(
        seq: u64,
        track: Track,
        now: &SessionLearning,
        before: &SessionLearning,
    ) -> Result<Self, CoreError> {
        let mut skills = BTreeMap::new();
        for (key, tally) in &now.skills {
            let old = before.skills.get(key).copied().unwrap_or_default();
            let delta = SkillTally {
                attempts: sub(tally.attempts, old.attempts)?,
                errors: sub(tally.errors, old.errors)?,
                timed: sub(tally.timed, old.timed)?,
                timed_micros: sub(tally.timed_micros, old.timed_micros)?,
            };
            if delta != SkillTally::default() {
                skills.insert(*key, delta);
            }
        }
        let mut words = BTreeMap::new();
        for (word, tally) in &now.words {
            let old = before.words.get(word).copied().unwrap_or_default();
            let delta = WordTally {
                attempts: sub(tally.attempts, old.attempts)?,
                errors: sub(tally.errors, old.errors)?,
            };
            if delta != WordTally::default() {
                words.insert(*word, delta);
            }
        }
        Ok(Self {
            seq,
            track,
            skills,
            words,
            passages: now.passages.difference(&before.passages).copied().collect(),
            reinforced: sub(now.reinforced, before.reinforced)?,
        })
    }

    /// Adds this chunk's evidence to `totals`.
    ///
    /// # Errors
    /// Counter overflow.
    pub fn fold_into(&self, totals: &mut SessionLearning) -> Result<(), CoreError> {
        for (key, delta) in &self.skills {
            let mut tally = totals.skills.get(key).copied().unwrap_or_default();
            tally.attempts = add(tally.attempts, delta.attempts)?;
            tally.errors = add(tally.errors, delta.errors)?;
            tally.timed = add(tally.timed, delta.timed)?;
            tally.timed_micros = add(tally.timed_micros, delta.timed_micros)?;
            totals.skills.insert(*key, tally);
        }
        for (word, delta) in &self.words {
            let mut tally = totals.words.get(word).copied().unwrap_or_default();
            tally.attempts = add(tally.attempts, delta.attempts)?;
            tally.errors = add(tally.errors, delta.errors)?;
            totals.words.insert(*word, tally);
        }
        totals.passages.extend(self.passages.iter().copied());
        totals.reinforced = add(totals.reinforced, self.reinforced)?;
        Ok(())
    }

    /// The canonical encoding: JSON with sorted keys and entries in key
    /// order, so equal evidence always encodes to equal bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let skills: Vec<Value> = self
            .skills
            .iter()
            .map(|(key, t)| {
                json!([
                    key.lane.key(),
                    key.skill.kind_key(),
                    key.skill.text(),
                    t.attempts,
                    t.errors,
                    t.timed,
                    t.timed_micros
                ])
            })
            .collect();
        let words: Vec<Value> = self
            .words
            .iter()
            .map(|(word, t)| json!([word.get(), t.attempts, t.errors]))
            .collect();
        let passages: Vec<u64> = self.passages.iter().map(|p| p.get()).collect();
        let value = json!({
            "encoding": ENCODING,
            "seq": self.seq,
            "track": self.track.key(),
            "skills": skills,
            "words": words,
            "passages": passages,
            "reinforced": self.reinforced,
        });
        serde_json::to_vec(&value).unwrap_or_default()
    }

    /// The digest of the canonical encoding.
    #[must_use]
    pub fn digest(&self) -> ContentDigest {
        ContentDigest::from_bytes(Sha256::digest(self.encode()).into())
    }

    /// Decodes a stored encoding, checking that it re-encodes to the same
    /// bytes (so a stored payload has exactly one meaning).
    ///
    /// # Errors
    /// A malformed or non-canonical payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, CoreError> {
        let bad = || invalid("malformed observation chunk");
        let value: Value = serde_json::from_slice(bytes).map_err(|_json| bad())?;
        let number = |v: &Value| v.as_u64().ok_or_else(bad);
        if value.get("encoding").map(number).transpose()? != Some(ENCODING) {
            return Err(invalid("unknown observation chunk encoding"));
        }
        let track = value
            .get("track")
            .and_then(Value::as_str)
            .and_then(Track::parse_key)
            .ok_or_else(bad)?;
        let mut skills = BTreeMap::new();
        for row in value
            .get("skills")
            .and_then(Value::as_array)
            .ok_or_else(bad)?
        {
            let row = row.as_array().ok_or_else(bad)?;
            let text = |i: usize| row.get(i).and_then(Value::as_str).ok_or_else(bad);
            let n = |i: usize| row.get(i).ok_or_else(bad).and_then(number);
            let key = SkillKey {
                lane: KeyLane::parse(text(0)?).ok_or_else(bad)?,
                skill: Skill::parse(text(1)?, text(2)?)?,
            };
            skills.insert(
                key,
                SkillTally {
                    attempts: n(3)?,
                    errors: n(4)?,
                    timed: n(5)?,
                    timed_micros: n(6)?,
                },
            );
        }
        let mut words = BTreeMap::new();
        for row in value
            .get("words")
            .and_then(Value::as_array)
            .ok_or_else(bad)?
        {
            let row = row.as_array().ok_or_else(bad)?;
            let n = |i: usize| row.get(i).ok_or_else(bad).and_then(number);
            words.insert(
                WordId::new(n(0)?)?,
                WordTally {
                    attempts: n(1)?,
                    errors: n(2)?,
                },
            );
        }
        let mut passages = BTreeSet::new();
        for id in value
            .get("passages")
            .and_then(Value::as_array)
            .ok_or_else(bad)?
        {
            passages.insert(PassageId::new(number(id)?)?);
        }
        let chunk = Self {
            seq: value.get("seq").ok_or_else(bad).and_then(number)?,
            track,
            skills,
            words,
            passages,
            reinforced: value.get("reinforced").ok_or_else(bad).and_then(number)?,
        };
        if chunk.encode() != bytes {
            return Err(invalid("non-canonical observation chunk"));
        }
        Ok(chunk)
    }
}

/// The chain over a session's chunk digests: `chain(previous, digest)`,
/// starting from [`GENESIS`]. A checkpoint names its chunks by their count
/// and this chain.
#[must_use]
pub fn chain(previous: ContentDigest, digest: ContentDigest) -> ContentDigest {
    let mut hash = Sha256::new();
    hash.update(b"cadence-chunk-chain-v1");
    hash.update(previous.as_bytes());
    hash.update(digest.as_bytes());
    ContentDigest::from_bytes(hash.finalize().into())
}

/// The chain before any chunk.
pub const GENESIS: ContentDigest = ContentDigest::from_bytes([0; 32]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphabet::{Layout, TrainingChar};
    use crate::learning::{PlanRecord, PlannerMode};

    fn learning() -> SessionLearning {
        SessionLearning::new(PlanRecord {
            seed: 1,
            mode: PlannerMode::Adaptive,
            focus: Vec::new(),
        })
    }

    #[test]
    fn a_chunk_is_the_difference_and_its_fold_restores_the_totals()
    -> Result<(), Box<dyn std::error::Error>> {
        let key = SkillKey {
            lane: KeyLane::Main,
            skill: Skill::Unit(TrainingChar::new('t', Layout::Us)?),
        };
        let mut before = learning();
        before.observe_skill(key, false, None)?;
        let mut now = before.clone();
        now.observe_skill(key, true, None)?;
        now.observe_word(WordId::new(7)?, true)?;
        now.observe_passage(PassageId::new(9)?)?;
        now.note_reinforcement()?;
        let chunk = ObservationChunk::between(2, Track::Words, &now, &before)?;
        assert_eq!(
            chunk.skills.get(&key).map(|t| (t.attempts, t.errors)),
            Some((1, 1))
        );
        let mut folded = before.clone();
        chunk.fold_into(&mut folded)?;
        assert_eq!(folded, now);
        // The encoding round-trips exactly; a changed byte does not decode
        // to the same chunk.
        let bytes = chunk.encode();
        assert_eq!(ObservationChunk::decode(&bytes)?, chunk);
        let mut other = chunk.clone();
        other.reinforced = 2;
        assert_ne!(other.digest(), chunk.digest());
        assert!(ObservationChunk::decode(b"{\"encoding\":1}").is_err());
        // The chain depends on every digest and their order.
        let a = chain(chain(GENESIS, chunk.digest()), other.digest());
        let b = chain(chain(GENESIS, other.digest()), chunk.digest());
        assert_ne!(a, b);
        Ok(())
    }
}
