//! Hermetic persistence regressions; all filesystem tests use owned temporary profiles.

mod checkpoints;
mod config;
mod config_write;
mod disk;
mod learning;
mod schema;

use cadence_core::progress::{Checkpoint, ProgressStatus, SessionProgress, Track, TrackProgress};
use cadence_core::{CommitSeq, SessionId};

/// Sessions of evidence requested by tests (any positive window).
const LEARNER_WINDOW: usize = 60;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn checkpoint(
    session: u64,
    sequence: u64,
    status: ProgressStatus,
) -> Result<Checkpoint, cadence_core::CoreError> {
    Checkpoint::new(
        SessionId::new(session)?,
        CommitSeq::from_raw(sequence),
        SessionProgress {
            profile: "default".into(),
            day: 20_000,
            utc_offset_minutes: None,
            layout: None,
            content_class: cadence_core::progress::ContentClass::Standard,
            pack_digest: None,
            policy_digest: "policy-v1".into(),
            corpus_digest: "corpus-v1".into(),
            status,
            active_micros: 900_000,
            tracks: vec![TrackProgress {
                track: Track::NumberRow,
                first_correct: 3,
                first_attempted: 4,
                correct_attempts: 5,
                content_attempts: 8,
                deletions: 2,
                overflow_attempts: 1,
                correct_units: 3,
                correct_digits: 2,
                active_micros: 900_000,
            }],
        },
        None,
    )
}

fn rekey(
    checkpoint: &Checkpoint,
    sequence: u64,
    progress: SessionProgress,
) -> Result<Checkpoint, cadence_core::CoreError> {
    Checkpoint::new(
        checkpoint.key.session,
        CommitSeq::from_raw(sequence),
        progress,
        checkpoint.learning.clone(),
    )
}
