use cadence_core::progress::ProgressStatus;
use rusqlite::Connection;

use super::{TestResult, checkpoint};
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
            assert!(store.bootstrap().is_err(), "{mutation}");
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
