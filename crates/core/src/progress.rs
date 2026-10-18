//! Portable, validated cumulative progress and exact checkpoint identities.
use crate::{CommitSeq, ContentDigest, CoreError, SessionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;

/// Stable practice track identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Track {
    /// Warm-up practice.
    WarmUp,
    /// Word practice.
    Words,
    /// Number-row practice.
    NumberRow,
    /// Numeric keypad practice.
    Numpad,
    /// Prose practice.
    Prose,
}
impl Track {
    /// Human-readable label.
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
/// Persisted session disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStatus {
    /// Session still in progress (not a crash-resume claim).
    Running,
    /// All practice phases finished.
    Completed,
    /// User stopped early.
    Abandoned,
}
/// Exact cumulative counters for a track. Units may decrease on deletion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackProgress {
    /// Track identity.
    pub track: Track,
    /// First-correct positions.
    pub first_correct: u64,
    /// First-attempted positions.
    pub first_attempted: u64,
    /// Correct content insertions.
    pub correct_attempts: u64,
    /// All content insertions.
    pub content_attempts: u64,
    /// Deletion actions.
    pub deletions: u64,
    /// Insertions beyond target.
    pub overflow_attempts: u64,
    /// Positionally correct units.
    pub correct_units: u64,
    /// Positionally correct digits.
    pub correct_digits: u64,
    /// Track-active duration.
    pub active_micros: u64,
}
impl TrackProgress {
    /// Empty track accumulator.
    #[must_use]
    pub const fn empty(track: Track) -> Self {
        Self {
            track,
            first_correct: 0,
            first_attempted: 0,
            correct_attempts: 0,
            content_attempts: 0,
            deletions: 0,
            overflow_attempts: 0,
            correct_units: 0,
            correct_digits: 0,
            active_micros: 0,
        }
    }
    /// Atomically combines counters, refusing overflow.
    /// # Errors
    /// Returns an arithmetic error without modifying the accumulator.
    pub fn add(&mut self, other: &Self) -> Result<(), CoreError> {
        if self.track != other.track {
            return Err(invalid());
        }
        let a = self.values();
        let b = other.values();
        let mut sums = [0; 9];
        for ((out, left), right) in sums.iter_mut().zip(a).zip(b) {
            *out = left
                .checked_add(right)
                .ok_or(CoreError::ArithmeticOverflow {
                    operation: "track accumulation",
                })?;
        }
        let [
            first_correct,
            first_attempted,
            correct_attempts,
            content_attempts,
            deletions,
            overflow_attempts,
            correct_units,
            correct_digits,
            active_micros,
        ] = sums;
        *self = Self {
            track: self.track,
            first_correct,
            first_attempted,
            correct_attempts,
            content_attempts,
            deletions,
            overflow_attempts,
            correct_units,
            correct_digits,
            active_micros,
        };
        Ok(())
    }
    fn values(&self) -> [u64; 9] {
        [
            self.first_correct,
            self.first_attempted,
            self.correct_attempts,
            self.content_attempts,
            self.deletions,
            self.overflow_attempts,
            self.correct_units,
            self.correct_digits,
            self.active_micros,
        ]
    }
}
/// A cumulative session snapshot, independent of storage representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProgress {
    /// Bounded profile identity.
    pub profile: String,
    /// Local calendar day supplied at startup.
    pub day: i64,
    /// Frozen policy digest.
    pub policy_digest: String,
    /// Frozen corpus digest.
    pub corpus_digest: String,
    /// Session disposition.
    pub status: ProgressStatus,
    /// At most five distinct tracks.
    pub tracks: Vec<TrackProgress>,
    /// Total session-active duration.
    pub active_micros: u64,
}
impl SessionProgress {
    /// Validates identity, numeric bounds, and exact invariants.
    /// # Errors
    /// Refuses malformed or unrepresentable persisted progress.
    pub fn validate(&self) -> Result<(), CoreError> {
        if [&self.profile, &self.policy_digest, &self.corpus_digest]
            .iter()
            .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
            || self.tracks.len() > 5
            || self.active_micros > i64::MAX as u64
        {
            return Err(invalid());
        }
        let mut active = 0_u64;
        for (index, t) in self.tracks.iter().enumerate() {
            if self
                .tracks
                .iter()
                .take(index)
                .any(|previous| previous.track == t.track)
                || t.values().iter().any(|v| *v > i64::MAX as u64)
                || t.first_correct > t.first_attempted
                || t.first_attempted > t.content_attempts
                || t.correct_attempts > t.content_attempts
                || t.first_correct > t.correct_attempts
                || t.overflow_attempts > t.content_attempts
                || t.correct_units > t.correct_attempts
                || t.correct_digits > t.correct_units
                || t.correct_attempts
                    .checked_add(t.overflow_attempts)
                    .is_none_or(|v| v > t.content_attempts)
            {
                return Err(invalid());
            }
            active = active.checked_add(t.active_micros).ok_or_else(invalid)?;
        }
        if active != self.active_micros {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Full freshness identity for both success and failure acknowledgements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveKey {
    /// Session identity.
    pub session: SessionId,
    /// Monotonic commit sequence.
    pub sequence: CommitSeq,
    /// Canonical content digest.
    pub digest: ContentDigest,
}
/// Validated cumulative durable-save request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// Exact request identity.
    pub key: SaveKey,
    /// Full replacement progress.
    pub progress: SessionProgress,
}
impl Checkpoint {
    /// Validates and hashes a canonical, unambiguous binary encoding.
    /// # Errors
    /// Returns validation errors before a request can be dispatched.
    pub fn new(
        session: SessionId,
        sequence: CommitSeq,
        mut progress: SessionProgress,
    ) -> Result<Self, CoreError> {
        progress.validate()?;
        progress.tracks.sort_by_key(|t| t.track);
        let mut h = Sha256::new();
        h.update(b"cadence-progress-v1");
        for s in [
            &progress.profile,
            &progress.policy_digest,
            &progress.corpus_digest,
        ] {
            h.update(u64::try_from(s.len()).map_err(|_| invalid())?.to_le_bytes());
            h.update(s.as_bytes());
        }
        h.update(progress.day.to_le_bytes());
        h.update([match progress.status {
            ProgressStatus::Running => 0,
            ProgressStatus::Completed => 1,
            ProgressStatus::Abandoned => 2,
        }]);
        h.update(progress.active_micros.to_le_bytes());
        for t in &progress.tracks {
            h.update([t.track as u8]);
            for v in t.values() {
                h.update(v.to_le_bytes());
            }
        }
        Ok(Self {
            key: SaveKey {
                session,
                sequence,
                digest: ContentDigest::from_bytes(h.finalize().into()),
            },
            progress,
        })
    }
}
/// Storage's terminal response; errors carry the same exact key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveResult {
    /// Request identity.
    pub key: SaveKey,
    /// Durable success or visible failure.
    pub result: Result<(), String>,
}
/// Startup history counts; does not imply mid-word recovery.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistorySummary {
    /// Completed sessions.
    pub completed_sessions: u64,
    /// Saved but unfinished sessions.
    pub interrupted_sessions: u64,
    /// Most recent local calendar day.
    pub latest_day: Option<i64>,
}
fn invalid() -> CoreError {
    CoreError::InvalidIdentity {
        kind: "SessionProgress",
        reason: "invalid bounded identity or cumulative counters",
    }
}
