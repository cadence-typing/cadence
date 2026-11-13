use cadence_core::progress::ProgressStatus;
use cadence_core::{CommitSeq, ContentDigest, SessionId};

use super::{LEARNER_WINDOW, TestResult, checkpoint, rekey};
use crate::{ProgressStore, StoreError};

#[test]
fn lost_ack_retry_after_reopen_returns_exact_original_key() -> TestResult {
    let directory = tempfile::tempdir()?;
    let first = checkpoint(1, 4, ProgressStatus::Running)?;
    let latest = checkpoint(1, 8, ProgressStatus::Completed)?;
    {
        let mut store = ProgressStore::open_in(directory.path())?;
        assert!(store.apply_checkpoint(&first)?.created);
        assert!(store.apply_checkpoint(&latest)?.created);
    }
    let mut store = ProgressStore::open_in(directory.path())?;
    let receipt = store.apply_checkpoint(&first)?;
    assert_eq!(receipt.key, first.key);
    assert!(!receipt.created);
    let receipt = store.apply_checkpoint(&latest)?;
    assert_eq!(receipt.key, latest.key);
    assert!(!receipt.created);
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&latest)
    );
    assert_eq!(
        store.bootstrap(LEARNER_WINDOW)?.history.completed_sessions,
        1
    );
    Ok(())
}

#[test]
fn tampering_conflicts_and_unrecorded_stale_sequences_never_regress() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let original = checkpoint(1, 4, ProgressStatus::Running)?;
    store.apply_checkpoint(&original)?;
    let mut tampered = original.clone();
    tampered.key.digest = ContentDigest::from_bytes([0; 32]);
    assert!(matches!(
        store.apply_checkpoint(&tampered),
        Err(StoreError::InvalidCheckpoint(_))
    ));
    let mut progress = original.progress.clone();
    progress.day = 20_001;
    let conflict = rekey(&original, 4, progress)?;
    assert!(matches!(
        store.apply_checkpoint(&conflict),
        Err(StoreError::CommitConflict { .. })
    ));
    let stale = checkpoint(1, 3, ProgressStatus::Running)?;
    assert_eq!(
        store.apply_checkpoint(&stale),
        Err(StoreError::StaleSequence)
    );
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&original)
    );
    Ok(())
}

#[test]
fn frozen_identity_and_monotonic_counters_are_enforced() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let original = checkpoint(1, 1, ProgressStatus::Running)?;
    store.apply_checkpoint(&original)?;
    for field in [
        "profile",
        "day",
        "policy",
        "corpus",
        "count",
        "time",
        "missing_track",
    ] {
        let mut progress = original.progress.clone();
        match field {
            "profile" => progress.profile = "other".into(),
            "day" => progress.day = 20_001,
            "policy" => progress.policy_digest = "different".into(),
            "corpus" => progress.corpus_digest = "different".into(),
            "count" => {
                for track in &mut progress.tracks {
                    track.first_correct = 2;
                }
            }
            "time" => {
                progress.active_micros = 800_000;
                for track in &mut progress.tracks {
                    track.active_micros = 800_000;
                }
            }
            _ => {
                progress.tracks.clear();
                progress.active_micros = 0;
            }
        }
        let candidate = rekey(&original, 2, progress)?;
        assert!(
            matches!(
                store.apply_checkpoint(&candidate),
                Err(StoreError::Regression(_))
            ),
            "{field}"
        );
    }
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&original)
    );
    Ok(())
}

#[test]
fn legal_partial_buffer_deletion_preserves_exact_historical_counts() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let original = checkpoint(1, 1, ProgressStatus::Running)?;
    store.apply_checkpoint(&original)?;
    let mut progress = original.progress.clone();
    for track in &mut progress.tracks {
        track.correct_units = 1;
        track.correct_digits = 0;
        track.deletions = 3;
    }
    let deleted = rekey(&original, 2, progress)?;
    assert!(store.apply_checkpoint(&deleted)?.created);
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&deleted)
    );
    Ok(())
}

#[test]
fn terminal_sessions_cannot_change_status_or_measurements() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let completed = checkpoint(1, 1, ProgressStatus::Completed)?;
    store.apply_checkpoint(&completed)?;
    for status in [ProgressStatus::Running, ProgressStatus::Abandoned] {
        let candidate = checkpoint(1, 2, status)?;
        assert!(matches!(
            store.apply_checkpoint(&candidate),
            Err(StoreError::Regression(_))
        ));
    }
    Ok(())
}

#[test]
fn failed_receipt_insert_rolls_back_entire_cumulative_update() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let original = checkpoint(1, 1, ProgressStatus::Running)?;
    store.apply_checkpoint(&original)?;
    store.connection.execute_batch(
        "CREATE TRIGGER reject_receipt BEFORE INSERT ON commit_receipts BEGIN SELECT RAISE(ABORT,'injected receipt failure'); END;",
    )?;
    let final_checkpoint = checkpoint(1, 2, ProgressStatus::Completed)?;
    assert!(matches!(
        store.apply_checkpoint(&final_checkpoint),
        Err(StoreError::Sql(_))
    ));
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&original)
    );
    store
        .connection
        .execute_batch("DROP TRIGGER reject_receipt")?;
    assert!(store.apply_checkpoint(&final_checkpoint)?.created);
    assert!(!store.apply_checkpoint(&final_checkpoint)?.created);
    Ok(())
}

#[test]
fn sqlite_integer_boundaries_are_checked_not_clamped() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let mut candidate = checkpoint(1, 1, ProgressStatus::Running)?;
    candidate.key.session = SessionId::new(u64::MAX)?;
    assert!(matches!(
        store.apply_checkpoint(&candidate),
        Err(StoreError::IntegerRange { .. })
    ));
    candidate.key.session = SessionId::FIRST;
    candidate.key.sequence = CommitSeq::from_raw(u64::MAX);
    assert!(matches!(
        store.apply_checkpoint(&candidate),
        Err(StoreError::IntegerRange { .. })
    ));
    candidate.key.sequence = CommitSeq::INITIAL;
    candidate.progress.active_micros = u64::MAX;
    assert!(matches!(
        store.apply_checkpoint(&candidate),
        Err(StoreError::InvalidCheckpoint(_))
    ));
    assert!(store.session_history()?.is_empty());
    let boundary = u64::try_from(i64::MAX)?;
    let mut candidate = checkpoint(boundary, boundary, ProgressStatus::Running)?;
    candidate.progress.active_micros = boundary;
    for track in &mut candidate.progress.tracks {
        track.active_micros = boundary;
    }
    let candidate = rekey(&candidate, boundary, candidate.progress.clone())?;
    assert!(store.apply_checkpoint(&candidate)?.created);
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&candidate)
    );
    assert!(matches!(
        store.bootstrap(LEARNER_WINDOW),
        Err(StoreError::IntegerRange { .. })
    ));
    Ok(())
}

#[test]
fn forged_invalid_track_invariants_are_not_accepted() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let mut candidate = checkpoint(1, 1, ProgressStatus::Running)?;
    for track in &mut candidate.progress.tracks {
        track.first_correct = 100;
    }
    assert!(matches!(
        store.apply_checkpoint(&candidate),
        Err(StoreError::InvalidCheckpoint(_))
    ));
    assert!(store.session_history()?.is_empty());
    Ok(())
}
