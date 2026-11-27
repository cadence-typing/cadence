use cadence_core::progress::ProgressStatus;
use rusqlite::Connection;

use super::{LEARNER_WINDOW, TestResult, checkpoint};
use crate::{ProgressStore, SUPPORTED_SCHEMA, StoreError, paths::DATABASE_FILE};

#[test]
fn newer_schema_refused_without_configuration_or_database_writes() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join(DATABASE_FILE);
    {
        let connection = Connection::open(&database)?;
        connection.execute_batch("CREATE TABLE schema_info(id INTEGER PRIMARY KEY,version INTEGER NOT NULL); INSERT INTO schema_info VALUES(1,999);")?;
    }
    let before = std::fs::read(&database)?;
    assert!(matches!(
        ProgressStore::open_in(directory.path()),
        Err(StoreError::SchemaTooNew { found: 999, .. })
    ));
    assert_eq!(std::fs::read(&database)?, before);
    assert!(!directory.path().join("progress.sqlite3-wal").try_exists()?);
    assert!(
        !directory
            .path()
            .join(crate::paths::LOCK_FILE)
            .try_exists()?
    );
    let connection = Connection::open(&database)?;
    let mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    assert_eq!(mode, "delete");
    Ok(())
}

#[test]
fn empty_draft_v1_migrates_transactionally_to_exact_schema() -> TestResult {
    let directory = tempfile::tempdir()?;
    {
        let connection = Connection::open(directory.path().join(DATABASE_FILE))?;
        connection.execute_batch(crate::schema::MIGRATION_V1)?;
    }
    let mut store = ProgressStore::open_in(directory.path())?;
    let version: i64 =
        store
            .connection
            .query_row("SELECT version FROM schema_info", [], |row| row.get(0))?;
    assert_eq!(version, SUPPORTED_SCHEMA);
    let saved = checkpoint(1, 1, ProgressStatus::Running)?;
    store.apply_checkpoint(&saved)?;
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&saved)
    );
    Ok(())
}

#[test]
fn populated_draft_v1_is_explicitly_refused_and_retained_byte_for_byte() -> TestResult {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join(DATABASE_FILE);
    {
        let connection = Connection::open(&database)?;
        connection.execute_batch(crate::schema::MIGRATION_V1)?;
        connection.execute_batch("INSERT INTO profiles(name) VALUES('default'); INSERT INTO sessions VALUES(1,'default',20000,'policy','corpus','running',900000,1);")?;
    }
    let before = std::fs::read(&database)?;
    assert!(matches!(
        ProgressStore::open_in(directory.path()),
        Err(StoreError::LegacySchema)
    ));
    assert_eq!(std::fs::read(&database)?, before);
    Ok(())
}

#[test]
fn migration_failure_rolls_back_all_numbered_steps() -> TestResult {
    let mut connection = Connection::open_in_memory()?;
    connection.execute_batch(crate::schema::MIGRATION_V1)?;
    // Force failure after v2 has added its first column.
    connection.execute_batch(
        "ALTER TABLE session_tracks ADD COLUMN overflow_attempts INTEGER NOT NULL DEFAULT 0;",
    )?;
    assert!(matches!(
        crate::schema::migrate(&mut connection, 1),
        Err(StoreError::Migration { target: 2, .. })
    ));
    let version: i64 =
        connection.query_row("SELECT version FROM schema_info", [], |row| row.get(0))?;
    assert_eq!(version, 1);
    let deletions: i64 = connection.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('session_tracks') WHERE name='deletions'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(deletions, 0);
    Ok(())
}

#[test]
fn malformed_saved_integers_and_digest_tampering_fail_before_open_writes() -> TestResult {
    for mutation in [
        "UPDATE sessions SET active_micros=-1",
        "UPDATE sessions SET active_micros=1.5",
        "UPDATE sessions SET commit_seq=-1",
        "UPDATE sessions SET status='invented'",
        "UPDATE session_tracks SET first_correct=-1",
        "UPDATE session_tracks SET correct_units=2",
        "UPDATE commit_receipts SET digest='bad'",
        "DELETE FROM commit_receipts",
    ] {
        let directory = tempfile::tempdir()?;
        let database = directory.path().join(DATABASE_FILE);
        {
            let mut store = ProgressStore::open_in(directory.path())?;
            store.apply_checkpoint(&checkpoint(1, 1, ProgressStatus::Running)?)?;
            store.connection.execute_batch(mutation)?;
            assert!(store.bootstrap(LEARNER_WINDOW).is_err(), "{mutation}");
        }
        let before = std::fs::read(&database)?;
        assert!(
            ProgressStore::open_in(directory.path()).is_err(),
            "{mutation}"
        );
        assert_eq!(std::fs::read(&database)?, before, "{mutation}");
    }
    Ok(())
}

#[test]
fn malformed_schema_version_and_unversioned_data_are_not_reset() -> TestResult {
    for setup in [
        "CREATE TABLE owner_data(value TEXT); INSERT INTO owner_data VALUES('preserve me');",
        "CREATE TABLE schema_info(id INTEGER PRIMARY KEY,version); INSERT INTO schema_info VALUES(1,'broken');",
        "CREATE TABLE schema_info(id INTEGER PRIMARY KEY,version); INSERT INTO schema_info VALUES(1,-1);",
    ] {
        let directory = tempfile::tempdir()?;
        let database = directory.path().join(DATABASE_FILE);
        {
            Connection::open(&database)?.execute_batch(setup)?;
        }
        let before = std::fs::read(&database)?;
        assert!(ProgressStore::open_in(directory.path()).is_err());
        assert_eq!(std::fs::read(&database)?, before);
    }
    Ok(())
}

/// #45: a populated schema-3 profile migrates to schema 4 with every
/// session, receipt and digest intact, in one transaction.
#[test]
fn populated_schema_three_migrates_intact() -> TestResult {
    let directory = tempfile::tempdir()?;
    let saved = [
        checkpoint(1, 1, ProgressStatus::Completed)?,
        checkpoint(2, 3, ProgressStatus::Abandoned)?,
    ];
    let before = {
        let mut store = ProgressStore::open_in(directory.path())?;
        for checkpoint in &saved {
            store.apply_checkpoint(checkpoint)?;
        }
        let history = store.session_history()?;
        // Turn the file back into its schema-3 layout.
        store.connection.execute_batch(
            "DROP TABLE settings_audit;
             ALTER TABLE sessions DROP COLUMN pack_digest;
             ALTER TABLE sessions DROP COLUMN content_class;
             ALTER TABLE sessions DROP COLUMN layout;
             DROP TABLE pace_targets;
             DROP TABLE benchmark_probes;
             ALTER TABLE sessions DROP COLUMN provenance;
             ALTER TABLE sessions DROP COLUMN utc_offset_minutes;
             UPDATE schema_info SET version=3 WHERE id=1;",
        )?;
        history
    };
    let store = ProgressStore::open_in(directory.path())?;
    let version: i64 =
        store
            .connection
            .query_row("SELECT version FROM schema_info", [], |row| row.get(0))?;
    assert_eq!(version, 6);
    assert_eq!(store.session_history()?, before);
    let offsets: i64 = store.connection.query_row(
        "SELECT COUNT(*) FROM sessions WHERE utc_offset_minutes IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(offsets, 0, "no backfill");
    Ok(())
}

/// #45, #64, #77: a schema newer than this build (as schema 6 is for
/// beta.2) is refused, not migrated or reset.
#[test]
fn the_next_schema_is_refused() -> TestResult {
    let directory = tempfile::tempdir()?;
    {
        let store = ProgressStore::open_in(directory.path())?;
        store
            .connection
            .execute_batch("UPDATE schema_info SET version=7 WHERE id=1;")?;
    }
    assert!(matches!(
        ProgressStore::open_in(directory.path()),
        Err(StoreError::SchemaTooNew {
            found: 7,
            supported: 6
        })
    ));
    Ok(())
}

/// #64: a populated schema-4 profile migrates to schema 5 with every
/// session, receipt and digest intact; every session is `live`; the new
/// tables are empty.
#[test]
fn populated_schema_four_migrates_intact() -> TestResult {
    let directory = tempfile::tempdir()?;
    let saved = [
        checkpoint(1, 1, ProgressStatus::Completed)?,
        checkpoint(2, 3, ProgressStatus::Abandoned)?,
    ];
    let (before, receipts) = {
        let mut store = ProgressStore::open_in(directory.path())?;
        for checkpoint in &saved {
            store.apply_checkpoint(checkpoint)?;
        }
        let history = store.session_history()?;
        let receipts: Vec<(i64, i64, String)> = {
            let mut statement = store.connection.prepare(
                "SELECT session_id, commit_seq, digest FROM commit_receipts ORDER BY 1, 2",
            )?;
            let rows =
                statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        store.connection.execute_batch(
            "DROP TABLE settings_audit;
             ALTER TABLE sessions DROP COLUMN pack_digest;
             ALTER TABLE sessions DROP COLUMN content_class;
             ALTER TABLE sessions DROP COLUMN layout;
             DROP TABLE pace_targets;
             DROP TABLE benchmark_probes;
             ALTER TABLE sessions DROP COLUMN provenance;
             UPDATE schema_info SET version=4 WHERE id=1;",
        )?;
        (history, receipts)
    };
    let store = ProgressStore::open_in(directory.path())?;
    let version: i64 =
        store
            .connection
            .query_row("SELECT version FROM schema_info", [], |row| row.get(0))?;
    assert_eq!(version, 6);
    assert_eq!(store.session_history()?, before);
    let after: Vec<(i64, i64, String)> = {
        let mut statement = store
            .connection
            .prepare("SELECT session_id, commit_seq, digest FROM commit_receipts ORDER BY 1, 2")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    assert_eq!(after, receipts, "receipts intact");
    let count = |sql: &str| -> rusqlite::Result<i64> {
        store.connection.query_row(sql, [], |row| row.get(0))
    };
    assert_eq!(
        count("SELECT COUNT(*) FROM sessions WHERE provenance<>'live'")?,
        0
    );
    assert_eq!(count("SELECT COUNT(*) FROM pace_targets")?, 0);
    assert_eq!(count("SELECT COUNT(*) FROM benchmark_probes")?, 0);
    Ok(())
}

/// #64: a checkpoint with a `benchmark` track round-trips.
#[test]
fn a_benchmark_track_round_trips() -> TestResult {
    use cadence_core::progress::{Track, TrackProgress};
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let base = checkpoint(1, 1, ProgressStatus::Running)?;
    let mut progress = base.progress.clone();
    progress.active_micros = 1_500_000;
    progress.tracks.push(TrackProgress {
        track: Track::Benchmark,
        first_correct: 90,
        first_attempted: 100,
        correct_attempts: 100,
        content_attempts: 104,
        deletions: 4,
        overflow_attempts: 0,
        correct_units: 96,
        correct_digits: 0,
        active_micros: 600_000,
    });
    let saved = super::rekey(&base, 1, progress)?;
    store.apply_checkpoint(&saved)?;
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&saved)
    );
    Ok(())
}

/// #45: a checkpoint with a `review` track round-trips.
#[test]
fn a_review_track_round_trips() -> TestResult {
    use cadence_core::progress::{Track, TrackProgress};
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let base = checkpoint(1, 1, ProgressStatus::Running)?;
    let mut progress = base.progress.clone();
    progress.active_micros = 1_500_000;
    progress.tracks.push(TrackProgress {
        track: Track::Review,
        first_correct: 9,
        first_attempted: 10,
        correct_attempts: 11,
        content_attempts: 12,
        deletions: 1,
        overflow_attempts: 0,
        correct_units: 9,
        correct_digits: 0,
        active_micros: 600_000,
    });
    let saved = super::rekey(&base, 1, progress)?;
    store.apply_checkpoint(&saved)?;
    assert_eq!(
        store.session_history()?.first().map(|row| &row.checkpoint),
        Some(&saved)
    );
    Ok(())
}

/// #49: after the last scored deadline a completed session still types its
/// closing review: a later checkpoint that only advances the review track
/// (and active time) is accepted; changing a scored track or the status is
/// refused.
#[test]
fn a_completed_session_advances_only_its_review() -> TestResult {
    use cadence_core::progress::{Track, TrackProgress};
    let directory = tempfile::tempdir()?;
    let mut store = ProgressStore::open_in(directory.path())?;
    let base = checkpoint(1, 1, ProgressStatus::Completed)?;
    store.apply_checkpoint(&base)?;
    let review = |first: u64| TrackProgress {
        track: Track::Review,
        first_correct: first,
        first_attempted: first,
        correct_attempts: first,
        content_attempts: first,
        correct_units: first,
        active_micros: 500_000,
        ..TrackProgress::empty(Track::Review)
    };
    let mut progress = base.progress.clone();
    progress.active_micros = progress.active_micros.saturating_add(500_000);
    progress.tracks.push(review(5));
    let next = super::rekey(&base, 2, progress.clone())?;
    store.apply_checkpoint(&next)?;
    progress.tracks.pop();
    progress.tracks.push(review(9));
    store.apply_checkpoint(&super::rekey(&base, 3, progress.clone())?)?;
    // A scored track changing after completion is refused.
    let mut scored = progress.clone();
    if let Some(track) = scored.tracks.first_mut() {
        track.first_attempted = track.first_attempted.saturating_add(1);
        track.content_attempts = track.content_attempts.saturating_add(1);
    }
    assert!(matches!(
        store.apply_checkpoint(&super::rekey(&base, 4, scored)?),
        Err(StoreError::Regression(_))
    ));
    // So is leaving the completed status.
    let mut reopened = progress;
    reopened.status = ProgressStatus::Abandoned;
    assert!(matches!(
        store.apply_checkpoint(&super::rekey(&base, 4, reopened)?),
        Err(StoreError::Regression(_))
    ));
    Ok(())
}

/// #77: a populated schema-5 profile migrates to schema 6 with every
/// session, receipt and digest intact; existing sessions read layout
/// `unspecified` and class `standard`; the audit table is empty.
#[test]
fn populated_schema_five_migrates_intact() -> TestResult {
    let directory = tempfile::tempdir()?;
    let saved = [
        checkpoint(1, 1, ProgressStatus::Completed)?,
        checkpoint(2, 3, ProgressStatus::Abandoned)?,
    ];
    let before = {
        let mut store = ProgressStore::open_in(directory.path())?;
        for checkpoint in &saved {
            store.apply_checkpoint(checkpoint)?;
        }
        let history = store.session_history()?;
        store.connection.execute_batch(
            "DROP TABLE settings_audit;
             ALTER TABLE sessions DROP COLUMN pack_digest;
             ALTER TABLE sessions DROP COLUMN content_class;
             ALTER TABLE sessions DROP COLUMN layout;
             UPDATE schema_info SET version=5 WHERE id=1;",
        )?;
        history
    };
    let store = ProgressStore::open_in(directory.path())?;
    let version: i64 =
        store
            .connection
            .query_row("SELECT version FROM schema_info", [], |row| row.get(0))?;
    assert_eq!(version, 6);
    assert_eq!(store.session_history()?, before, "digests intact");
    let count = |sql: &str| -> rusqlite::Result<i64> {
        store.connection.query_row(sql, [], |row| row.get(0))
    };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM sessions WHERE layout<>'unspecified' OR content_class<>'standard'"
        )?,
        0
    );
    assert_eq!(count("SELECT COUNT(*) FROM settings_audit")?, 0);
    Ok(())
}

/// RUSTSEC-2026-0459 (CVE-2026-119375): SQLite before 3.54.0 can return
/// uninitialised page-cache memory for a database opened with a crafted
/// sibling `-wal`. The bundled SQLite must stay at or above the patched
/// release.
#[test]
fn the_bundled_sqlite_is_patched() {
    assert!(
        rusqlite::version_number() >= 3_054_000,
        "bundled SQLite {} is older than 3.54.0",
        rusqlite::version()
    );
}
