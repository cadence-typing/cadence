//! Numbered transactional migrations and read-only compatibility admission.
//!
//! Draft v1 omitted deletions, overflow, digit units and per-track time. A populated
//! v1 is deliberately refused rather than backfilling fabricated measurements.
//! An empty v1 can safely acquire those columns. Schema 3 (plan 0003) adds
//! per-session learning evidence in new tables only; a populated schema 2
//! migrates with those tables empty, because per-key evidence cannot be
//! reconstructed from track totals. Every 0/1/2 -> 3 path is one
//! transaction; no destructive reset path exists.

use rusqlite::{Connection, OptionalExtension};

use crate::StoreError;

/// Highest supported persistence schema version.
pub const SUPPORTED_SCHEMA: i64 = 3;

pub(crate) fn inspect(connection: &Connection) -> Result<i64, StoreError> {
    let has_schema: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='schema_info')",
        [],
        |row| row.get(0),
    )?;
    if !has_schema {
        let tables: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        return if tables == 0 {
            Ok(0)
        } else {
            Err(StoreError::Corrupt("unversioned nonempty database".into()))
        };
    }
    let version: Option<i64> = connection
        .query_row("SELECT version FROM schema_info WHERE id=1", [], |row| {
            row.get(0)
        })
        .optional()?;
    let version = version.ok_or_else(|| StoreError::Corrupt("missing schema version".into()))?;
    if version > SUPPORTED_SCHEMA {
        return Err(StoreError::SchemaTooNew {
            found: version,
            supported: SUPPORTED_SCHEMA,
        });
    }
    if !(1..=SUPPORTED_SCHEMA).contains(&version) {
        return Err(StoreError::Corrupt(format!(
            "invalid schema version {version}"
        )));
    }
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM schema_info", [], |row| row.get(0))?;
    if count != 1 {
        return Err(StoreError::Corrupt(
            "schema_info must contain exactly one row".into(),
        ));
    }
    if version == 1 {
        let populated: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM profiles) OR EXISTS(SELECT 1 FROM sessions)
                OR EXISTS(SELECT 1 FROM session_tracks) OR EXISTS(SELECT 1 FROM commit_receipts)",
            [],
            |row| row.get(0),
        )?;
        if populated {
            return Err(StoreError::LegacySchema);
        }
    }
    let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(StoreError::Corrupt(integrity));
    }
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    if statement.query([])?.next()?.is_some() {
        return Err(StoreError::Corrupt("foreign key violation".into()));
    }
    Ok(version)
}

pub(crate) fn migrate(connection: &mut Connection, current: i64) -> Result<(), StoreError> {
    if current == SUPPORTED_SCHEMA {
        return Ok(());
    }
    let tx = connection.transaction()?;
    for (target, sql) in [(1, MIGRATION_V1), (2, MIGRATION_V2), (3, MIGRATION_V3)] {
        if current < target {
            tx.execute_batch(sql)
                .map_err(|error| StoreError::Migration {
                    target,
                    message: error.to_string(),
                })?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(crate) const MIGRATION_V1: &str = "
CREATE TABLE schema_info (id INTEGER PRIMARY KEY CHECK(id=1), version INTEGER NOT NULL);
INSERT INTO schema_info VALUES (1,1);
CREATE TABLE profiles (name TEXT PRIMARY KEY);
CREATE TABLE sessions (
    session_id INTEGER PRIMARY KEY,
    profile TEXT NOT NULL REFERENCES profiles(name),
    day INTEGER NOT NULL,
    policy_digest TEXT NOT NULL,
    corpus_digest TEXT NOT NULL,
    status TEXT NOT NULL,
    active_micros INTEGER NOT NULL,
    commit_seq INTEGER NOT NULL
);
CREATE TABLE session_tracks (
    session_id INTEGER NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    track TEXT NOT NULL,
    first_correct INTEGER NOT NULL,
    first_attempted INTEGER NOT NULL,
    correct_attempts INTEGER NOT NULL,
    content_attempts INTEGER NOT NULL,
    correct_units INTEGER NOT NULL,
    PRIMARY KEY(session_id,track)
);
CREATE TABLE commit_receipts (
    session_id INTEGER NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    commit_seq INTEGER NOT NULL,
    digest TEXT NOT NULL,
    created_at_epoch INTEGER NOT NULL,
    PRIMARY KEY(session_id,commit_seq)
);
CREATE INDEX idx_sessions_day ON sessions(day);
";

pub(crate) const MIGRATION_V2: &str = "
ALTER TABLE session_tracks ADD COLUMN deletions INTEGER NOT NULL CHECK(deletions>=0) DEFAULT 0;
ALTER TABLE session_tracks ADD COLUMN overflow_attempts INTEGER NOT NULL CHECK(overflow_attempts>=0) DEFAULT 0;
ALTER TABLE session_tracks ADD COLUMN correct_digits INTEGER NOT NULL CHECK(correct_digits>=0) DEFAULT 0;
ALTER TABLE session_tracks ADD COLUMN active_micros INTEGER NOT NULL CHECK(active_micros>=0) DEFAULT 0;
UPDATE schema_info SET version=2 WHERE id=1;
";

/// Learning evidence (plan 0003). Seeds and passage ids are full `u64`
/// values, stored as 16 lowercase hex digits because SQLite integers are i64.
const MIGRATION_V3: &str = "
CREATE TABLE session_plans (
    session_id INTEGER PRIMARY KEY REFERENCES sessions(session_id) ON DELETE CASCADE,
    seed TEXT NOT NULL CHECK(length(seed)=16),
    mode TEXT NOT NULL CHECK(mode IN ('cold_start','adaptive')),
    reinforced INTEGER NOT NULL CHECK(reinforced>=0)
);
CREATE TABLE plan_focus (
    session_id INTEGER NOT NULL REFERENCES session_plans(session_id) ON DELETE CASCADE,
    rank INTEGER NOT NULL CHECK(rank>=0),
    lane TEXT NOT NULL,
    kind TEXT NOT NULL,
    skill TEXT NOT NULL,
    PRIMARY KEY(session_id,rank)
);
CREATE TABLE skill_observations (
    session_id INTEGER NOT NULL REFERENCES session_plans(session_id) ON DELETE CASCADE,
    lane TEXT NOT NULL,
    kind TEXT NOT NULL,
    skill TEXT NOT NULL,
    attempts INTEGER NOT NULL CHECK(attempts>=0),
    errors INTEGER NOT NULL CHECK(errors>=0 AND errors<=attempts),
    timed INTEGER NOT NULL CHECK(timed>=0 AND timed<=attempts),
    timed_micros INTEGER NOT NULL CHECK(timed_micros>=0),
    PRIMARY KEY(session_id,lane,kind,skill)
);
CREATE TABLE word_outcomes (
    session_id INTEGER NOT NULL REFERENCES session_plans(session_id) ON DELETE CASCADE,
    word_id INTEGER NOT NULL CHECK(word_id>0),
    attempts INTEGER NOT NULL CHECK(attempts>=0),
    errors INTEGER NOT NULL CHECK(errors>=0 AND errors<=attempts),
    PRIMARY KEY(session_id,word_id)
);
CREATE TABLE prose_seen (
    session_id INTEGER NOT NULL REFERENCES session_plans(session_id) ON DELETE CASCADE,
    passage_id TEXT NOT NULL CHECK(length(passage_id)=16),
    PRIMARY KEY(session_id,passage_id)
);
UPDATE schema_info SET version=3 WHERE id=1;
";
