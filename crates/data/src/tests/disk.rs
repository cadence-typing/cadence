use cadence_core::progress::{ProgressStatus, SessionProgress};
use serde::Deserialize;

use super::{TestResult, checkpoint};
use crate::{
    ProgressStore, SUPPORTED_SCHEMA, StoreError,
    paths::{DATABASE_FILE, LOCK_FILE},
};

#[test]
fn lifetime_writer_lock_is_exclusive_and_released_on_drop() -> TestResult {
    let directory = tempfile::tempdir()?;
    let first = ProgressStore::open_in(directory.path())?;
    assert!(matches!(
        ProgressStore::open_in(directory.path()),
        Err(StoreError::WriterLockHeld { .. })
    ));
    drop(first);
    let reopened = ProgressStore::open_in(directory.path())?;
    assert_eq!(reopened.bootstrap()?.next_session.get(), 1);
    Ok(())
}

#[test]
fn reopened_history_counts_saved_interruptions_without_resume_claims() -> TestResult {
    let directory = tempfile::tempdir()?;
    {
        let mut store = ProgressStore::open_in(directory.path())?;
        for (session, status) in [
            (1, ProgressStatus::Completed),
            (2, ProgressStatus::Running),
            (3, ProgressStatus::Abandoned),
        ] {
            store.apply_checkpoint(&checkpoint(session, 1, status)?)?;
        }
    }
    let store = ProgressStore::open_in(directory.path())?;
    let bootstrap = store.bootstrap()?;
    assert_eq!(bootstrap.next_session.get(), 4);
    assert_eq!(bootstrap.history.completed_sessions, 1);
    assert_eq!(bootstrap.history.interrupted_sessions, 2);
    assert_eq!(bootstrap.history.latest_day, Some(20_000));
    Ok(())
}

#[test]
fn online_backup_preserves_live_wal_progress_and_receipts() -> TestResult {
    let source = tempfile::tempdir()?;
    let target = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(source.path())?;
    let original = checkpoint(7, 1, ProgressStatus::Running)?;
    let final_checkpoint = checkpoint(7, 2, ProgressStatus::Completed)?;
    store.apply_checkpoint(&original)?;
    store.apply_checkpoint(&final_checkpoint)?;
    let destination = target.path().join(DATABASE_FILE);
    store.backup_to(&destination)?;
    let mut restored = ProgressStore::open_in(target.path())?;
    assert_eq!(restored.session_history()?, store.session_history()?);
    assert_eq!(restored.bootstrap()?.history.completed_sessions, 1);
    let receipt = restored.apply_checkpoint(&original)?;
    assert_eq!(receipt.key, original.key);
    assert!(!receipt.created);
    assert!(matches!(
        store.backup_to(&destination),
        Err(StoreError::DestinationExists(_))
    ));
    Ok(())
}

#[derive(Deserialize)]
struct Export {
    schema_version: i64,
    sessions: Vec<ExportSession>,
}
#[derive(Deserialize)]
struct ExportSession {
    session_id: u64,
    sequence: u64,
    digest: String,
    progress: SessionProgress,
}

#[test]
fn portable_export_contains_real_exact_counters_and_never_overwrites() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let saved = checkpoint(2, 7, ProgressStatus::Abandoned)?;
    store.apply_checkpoint(&saved)?;
    let destination = directory.path().join("export.json");
    store.export_to(&destination)?;
    let bytes = std::fs::read(&destination)?;
    let export: Export = serde_json::from_slice(&bytes)?;
    assert_eq!(export.schema_version, SUPPORTED_SCHEMA);
    assert_eq!(export.sessions.len(), 1);
    let session = export.sessions.first().ok_or("missing exported session")?;
    assert_eq!(session.session_id, saved.key.session.get());
    assert_eq!(session.sequence, saved.key.sequence.get());
    assert_eq!(session.digest, saved.key.digest.to_hex());
    assert_eq!(session.progress, saved.progress);
    assert!(matches!(
        store.export_to(&destination),
        Err(StoreError::DestinationExists(_))
    ));
    assert_eq!(std::fs::read(&destination)?, bytes);
    assert!(matches!(
        store.backup_to(&destination),
        Err(StoreError::DestinationExists(_))
    ));
    assert_eq!(std::fs::read(&destination)?, bytes);
    Ok(())
}

#[test]
fn backup_and_export_report_unwritable_destination_without_success() -> TestResult {
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let destination = directory.path().join("missing-parent/output");
    assert!(store.backup_to(&destination).is_err());
    assert!(store.export_to(&destination).is_err());
    assert!(!destination.try_exists()?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn fresh_home_tree_database_lock_and_outputs_are_private() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir()?;
    let home = directory.path().join("fresh-home");
    let profile = crate::resolve_data_dir(
        None,
        Some(std::ffi::OsStr::new("relative-ignored")),
        Some(home.as_os_str()),
    )?;
    let mut store = ProgressStore::open_in(&profile.data)?;
    store.apply_checkpoint(&checkpoint(1, 1, ProgressStatus::Running)?)?;
    for path in [
        &home,
        &home.join(".local"),
        &home.join(".local/share"),
        &profile.data,
    ] {
        assert_eq!(std::fs::metadata(path)?.permissions().mode() & 0o077, 0);
    }
    let backup = profile.data.join("backup.sqlite3");
    let export = profile.data.join("export.json");
    store.backup_to(&backup)?;
    store.export_to(&export)?;
    for path in [
        profile.data.join(DATABASE_FILE),
        profile.data.join(LOCK_FILE),
        profile.data.join("progress.sqlite3-wal"),
        profile.data.join("progress.sqlite3-shm"),
        backup,
        export,
    ] {
        assert_eq!(std::fs::metadata(path)?.permissions().mode() & 0o077, 0);
    }
    Ok(())
}
