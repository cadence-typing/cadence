//! Test-only fixtures for the persistence campaigns (#88), compiled with the
//! `test-faults` feature: a populated profile in an earlier schema, built by
//! writing it in the current one and taking the later migrations back off.

use std::path::Path;

use cadence_core::progress::{
    Checkpoint, ContentClass, Evidence, ProgressStatus, SessionProgress, Track, TrackProgress,
};
use cadence_core::{CommitSeq, SessionId};

use crate::{ProgressStore, StoreError};

/// What each migration added, newest first, as the statements that remove it.
const DOWNGRADES: [(i64, &str); 4] = [
    (
        7,
        "DROP TABLE derived_state; DROP TABLE observation_chunks;",
    ),
    (
        6,
        "DROP TABLE settings_audit; ALTER TABLE sessions DROP COLUMN pack_digest;
         ALTER TABLE sessions DROP COLUMN content_class; ALTER TABLE sessions DROP COLUMN layout;",
    ),
    (
        5,
        "DROP TABLE pace_targets; DROP TABLE benchmark_probes;
         ALTER TABLE sessions DROP COLUMN provenance;",
    ),
    (4, "ALTER TABLE sessions DROP COLUMN utc_offset_minutes;"),
];

fn session(id: u64, sequence: u64) -> Result<Checkpoint, cadence_core::CoreError> {
    Checkpoint::new(
        SessionId::new(id)?,
        CommitSeq::from_raw(sequence),
        SessionProgress {
            profile: "default".into(),
            day: 20_000,
            utc_offset_minutes: None,
            layout: None,
            content_class: ContentClass::Standard,
            pack_digest: None,
            policy_digest: "policy-v1".into(),
            corpus_digest: "corpus-v1".into(),
            status: ProgressStatus::Completed,
            active_micros: 900_000,
            tracks: vec![TrackProgress {
                track: Track::Words,
                first_correct: 3,
                first_attempted: 4,
                correct_attempts: 5,
                content_attempts: 8,
                deletions: 2,
                overflow_attempts: 1,
                correct_units: 3,
                correct_digits: 0,
                active_micros: 900_000,
            }],
        },
        Evidence::None,
    )
}

/// Writes a profile in `directory` at `schema` (3 to the current) holding
/// two completed sessions with their receipts.
///
/// # Errors
/// An unsupported schema, or a store failure.
pub fn populated_profile(directory: &Path, schema: i64) -> Result<(), StoreError> {
    if !(3..=crate::SUPPORTED_SCHEMA).contains(&schema) {
        return Err(StoreError::Open(format!("no fixture for schema {schema}")));
    }
    let mut store = ProgressStore::open_in(directory)?;
    for id in 1..=2 {
        let checkpoint =
            session(id, 1).map_err(|error| StoreError::InvalidCheckpoint(error.to_string()))?;
        store.apply_checkpoint(&checkpoint)?;
    }
    for (version, sql) in DOWNGRADES {
        if version > schema {
            store.connection.execute_batch(sql)?;
        }
    }
    store
        .connection
        .execute("UPDATE schema_info SET version=?1 WHERE id=1", [schema])?;
    Ok(())
}

fn database(directory: &Path) -> std::path::PathBuf {
    directory.join(crate::paths::DATABASE_FILE)
}

/// The schema version a profile's file records, read without the writer.
///
/// # Errors
/// An unreadable file.
pub fn schema_version(directory: &Path) -> Result<i64, StoreError> {
    let connection = rusqlite::Connection::open_with_flags(
        database(directory),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    Ok(
        connection.query_row("SELECT version FROM schema_info WHERE id=1", [], |row| {
            row.get(0)
        })?,
    )
}

/// Records `version` as the profile's schema (a newer build's file).
///
/// # Errors
/// An unwritable file.
pub fn set_schema_version(directory: &Path, version: i64) -> Result<(), StoreError> {
    let connection = rusqlite::Connection::open(database(directory))?;
    connection.execute("UPDATE schema_info SET version=?1 WHERE id=1", [version])?;
    Ok(())
}
