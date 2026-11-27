//! Portable, validated cumulative progress and exact checkpoint identities.
use crate::learning::SessionLearning;
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
    /// The typed closing review (schema 4, #45; recorded from #49). Kept
    /// out of the session headline (#60).
    Review,
    /// A transfer probe on unseen text (schema 5, #64). Kept out of the
    /// session headline like the review.
    Benchmark,
}
impl Track {
    /// Stable byte in the checkpoint digest (declaration order of the v1
    /// encoding; never reuse a value).
    #[must_use]
    pub const fn digest_tag(self) -> u8 {
        match self {
            Self::WarmUp => 0,
            Self::Words => 1,
            Self::NumberRow => 2,
            Self::Numpad => 3,
            Self::Prose => 4,
            Self::Review => 5,
            Self::Benchmark => 6,
        }
    }

    /// Human-readable label.
    #[must_use]
    pub const fn key(self) -> &'static str {
        // The persisted name (the store's and serde's snake_case).
        match self {
            Self::WarmUp => "warm_up",
            Self::Words => "words",
            Self::NumberRow => "number_row",
            Self::Numpad => "numpad",
            Self::Prose => "prose",
            Self::Review => "review",
            Self::Benchmark => "benchmark",
        }
    }

    /// Parses a persisted name ([`Track::key`]).
    #[must_use]
    pub fn parse_key(key: &str) -> Option<Self> {
        [
            Self::WarmUp,
            Self::Words,
            Self::NumberRow,
            Self::Numpad,
            Self::Prose,
            Self::Review,
            Self::Benchmark,
        ]
        .into_iter()
        .find(|track| track.key() == key)
    }

    /// The display label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::WarmUp => "warm-up",
            Self::Words => "adaptive words",
            Self::NumberRow => "number row",
            Self::Numpad => "numpad",
            Self::Prose => "prose",
            Self::Review => "review",
            Self::Benchmark => "transfer probe",
        }
    }

    /// Whether the track's figures enter the session headline and
    /// like-for-like comparisons: the closing review and the transfer probe
    /// do not (#60, #64).
    #[must_use]
    pub const fn in_headline(self) -> bool {
        !matches!(self, Self::Review | Self::Benchmark)
    }
}
/// What kind of content a session practised (#77): only sessions of the
/// same class are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentClass {
    /// The shipped packs.
    #[default]
    Standard,
    /// Text the learner imported (#84).
    Imported,
    /// Code practice packs (#85).
    Code,
    /// Numpad drills with operators between operands (#83): never compared
    /// with plain numpad drills.
    Operators,
}

impl ContentClass {
    /// The persisted name.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Imported => "imported",
            Self::Code => "code",
            Self::Operators => "operators",
        }
    }

    /// Parses a persisted name.
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        [Self::Standard, Self::Imported, Self::Code, Self::Operators]
            .into_iter()
            .find(|c| c.key() == key)
    }

    /// A stable code for comparisons and digests.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Standard => 0,
            Self::Imported => 1,
            Self::Code => 2,
            Self::Operators => 3,
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
        // The kernel decides the merge (Lean: each counter is the exact sum,
        // or nothing is merged; validity is preserved).
        match cadence_kernel::progress::merge(self.counters(), other.counters()) {
            cadence_kernel::progress::Merge::Overflow => Err(CoreError::ArithmeticOverflow {
                operation: "track accumulation",
            }),
            cadence_kernel::progress::Merge::Merged(t) => {
                *self = Self {
                    track: self.track,
                    first_correct: t.first_correct,
                    first_attempted: t.first_attempted,
                    correct_attempts: t.correct_attempts,
                    content_attempts: t.content_attempts,
                    deletions: t.deletions,
                    overflow_attempts: t.overflow_attempts,
                    correct_units: t.correct_units,
                    correct_digits: t.correct_digits,
                    active_micros: t.active_micros,
                };
                Ok(())
            }
        }
    }
    /// Whether the counters are consistent: the proved
    /// `cadence_kernel::progress::track_valid`.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        cadence_kernel::progress::track_valid(self.counters())
    }
    /// The kernel's view of the counters, for its validity decision.
    fn counters(&self) -> cadence_kernel::progress::TrackCounters {
        cadence_kernel::progress::TrackCounters {
            first_correct: self.first_correct,
            first_attempted: self.first_attempted,
            correct_attempts: self.correct_attempts,
            content_attempts: self.content_attempts,
            deletions: self.deletions,
            overflow_attempts: self.overflow_attempts,
            correct_units: self.correct_units,
            correct_digits: self.correct_digits,
            active_micros: self.active_micros,
        }
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
/// Whether a closing exercise may be credited: the proved
/// [`cadence_kernel::progress::credit_once`].
pub use cadence_kernel::progress::credit_once;
/// A cumulative session snapshot, independent of storage representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProgress {
    /// Bounded profile identity.
    pub profile: String,
    /// Local calendar day at session start (`calendar::local_day`, #52);
    /// a UTC day for sessions recorded before schema 4.
    pub day: i64,
    /// The UTC offset in minutes the day was counted in; `None` for
    /// sessions recorded before schema 4 (#52). In the digest only when
    /// present, so earlier receipts keep their digests.
    #[serde(default)]
    pub utc_offset_minutes: Option<i32>,
    /// The keyboard layout the session was typed on (#77); `None` for
    /// sessions recorded before schema 6. In the digest only when present.
    #[serde(default)]
    pub layout: Option<crate::alphabet::Layout>,
    /// The content class (#77). In the digest only when not standard.
    #[serde(default)]
    pub content_class: ContentClass,
    /// For a pack session (#85): the pack's passage digest (hex). In the
    /// digest only when present.
    #[serde(default)]
    pub pack_digest: Option<String>,
    /// Frozen policy digest.
    pub policy_digest: String,
    /// Frozen corpus digest.
    pub corpus_digest: String,
    /// Session disposition.
    pub status: ProgressStatus,
    /// At most seven distinct tracks.
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
            || self.tracks.len() > 7
            || self
                .utc_offset_minutes
                .is_some_and(|m| m.unsigned_abs() > 18 * 60)
            || self.active_micros > cadence_kernel::progress::COUNTER_MAXIMUM
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
                || !cadence_kernel::progress::track_valid(t.counters())
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
    /// Full replacement learning evidence. Sessions saved before schema 3
    /// have none; their digest is exactly the original v1 encoding.
    pub learning: Option<SessionLearning>,
}
/// Tag separating the v1 progress encoding from the learning section. Track
/// bytes are 0..=5, so this byte can never begin another track record.
const LEARNING_SECTION_TAG: u8 = 0xFF;
/// Tag of the optional UTC-offset section (#52), after the tracks.
const OFFSET_SECTION_TAG: u8 = 0xFE;
impl Checkpoint {
    /// Validates and hashes a canonical, unambiguous binary encoding.
    /// # Errors
    /// Returns validation errors before a request can be dispatched.
    pub fn new(
        session: SessionId,
        sequence: CommitSeq,
        mut progress: SessionProgress,
        learning: Option<SessionLearning>,
    ) -> Result<Self, CoreError> {
        progress.validate()?;
        if let Some(learning) = &learning {
            learning.validate()?;
        }
        progress.tracks.sort_by_key(|t| t.track);
        let mut h = Sha256::new();
        h.update(b"cadence-progress-v1");
        for s in [
            &progress.profile,
            &progress.policy_digest,
            &progress.corpus_digest,
        ] {
            h.update(
                u64::try_from(s.len())
                    .map_err(|_too_long| invalid())?
                    .to_le_bytes(),
            );
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
            h.update([t.track.digest_tag()]);
            for v in t.values() {
                h.update(v.to_le_bytes());
            }
        }
        if let Some(offset) = progress.utc_offset_minutes {
            h.update([OFFSET_SECTION_TAG]);
            h.update(offset.to_le_bytes());
        }
        if let Some(layout) = progress.layout {
            h.update([0xFB]);
            h.update(layout.key().as_bytes());
        }
        if progress.content_class != ContentClass::Standard {
            h.update([0xFA, progress.content_class.code()]);
        }
        if let Some(pack) = &progress.pack_digest {
            h.update([0xF9]);
            h.update(pack.as_bytes());
        }
        if let Some(learning) = &learning {
            h.update([LEARNING_SECTION_TAG]);
            learning.digest_into(&mut h);
        }
        Ok(Self {
            key: SaveKey {
                session,
                sequence,
                digest: ContentDigest::from_bytes(h.finalize().into()),
            },
            progress,
            learning,
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
    /// Exact track totals of recent sessions with typed evidence, newest
    /// first, bounded by [`RECENT_SESSION_CAPACITY`]. Used only for display
    /// and like-for-like comparison.
    pub recent: Vec<SessionRecap>,
    /// Comparable transfer probes recorded so far (#65).
    pub comparable_probes: u64,
    /// The latest comparable probe's track totals (#65): the only probe a
    /// new one is compared with.
    pub last_comparable_probe: Option<TrackProgress>,
    /// The corpus digest the latest session was planned with (#8).
    pub latest_corpus_digest: Option<String>,
}
/// Earlier sessions kept for comparison: enough to find the last
/// session with the same track set after a few single-track runs.
pub const RECENT_SESSION_CAPACITY: usize = 30;
/// One session as the Progress destination reads it (#81): its basis and
/// its exact per-track totals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPoint {
    /// The session.
    pub session: crate::SessionId,
    /// Its local calendar day.
    pub day: i64,
    /// The layout it was typed on, `unspecified` read as its adopter.
    pub layout: Option<crate::alphabet::Layout>,
    /// Its content class.
    pub content_class: ContentClass,
    /// Recorded by the headless driver.
    pub synthetic: bool,
    /// For a session with a transfer probe: whether that probe is
    /// comparable.
    pub probe_comparable: Option<bool>,
    /// Exact cumulative track totals.
    pub tracks: Vec<TrackProgress>,
}

/// One earlier session's exact per-track totals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecap {
    /// Local calendar day the session was recorded on.
    pub day: i64,
    /// Whether every planned phase finished; otherwise it ended early.
    pub completed: bool,
    /// Exact cumulative track totals, in stored order.
    pub tracks: Vec<TrackProgress>,
    /// The layout it was typed on, `unspecified` read as its adopter (#77).
    pub layout: Option<crate::alphabet::Layout>,
    /// Its content class (#77).
    pub content_class: ContentClass,
}
fn invalid() -> CoreError {
    CoreError::InvalidIdentity {
        kind: "SessionProgress",
        reason: "invalid bounded identity or cumulative counters",
    }
}
