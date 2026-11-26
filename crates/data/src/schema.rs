//! Numbered transactional migrations and read-only compatibility admission.
//!
//! Draft v1 omitted deletions, overflow, digit units and per-track time. A populated
//! v1 is deliberately refused rather than backfilling fabricated measurements.
//! An empty v1 can safely acquire those columns. Schema 3 (plan 0003) adds
//! per-session learning evidence in new tables only; a populated schema 2
//! migrates with those tables empty, because per-key evidence cannot be
//! reconstructed from track totals. Schema 4 (#45) admits the `review`
//! track (track names are not constrained in SQL; the reader maps them) and
//! adds the nullable `sessions.utc_offset_minutes` (written by #52); no row
//! changes. Schema 5 (#64) adds `pace_targets`, `benchmark_probes` and
//! `sessions.provenance` (`live` by default) and admits the `benchmark`
//! track. Schema 6 (#77) records each session's layout and content class
//! (existing rows read `unspecified` and `standard`), a nullable pack digest
//! and `settings_audit`. Every 0/1/2/3/4/5 -> 6 path is one transaction; no
//! destructive reset path exists. `word_outcomes` also holds warm-up and prose words from
//! alpha.7 (columns unchanged).

use rusqlite::{Connection, OptionalExtension};

use crate::StoreError;

/// Highest supported persistence schema version.
pub const SUPPORTED_SCHEMA: i64 = 6;

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
    for (target, sql) in [
        (1, MIGRATION_V1),
        (2, MIGRATION_V2),
        (3, MIGRATION_V3),
        (4, MIGRATION_V4),
        (5, MIGRATION_V5),
        (6, MIGRATION_V6),
    ] {
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

/// The closing review track and the session's UTC offset (#45, #52).
const MIGRATION_V4: &str = "
ALTER TABLE sessions ADD COLUMN utc_offset_minutes INTEGER NULL
    CHECK(utc_offset_minutes IS NULL OR utc_offset_minutes BETWEEN -1440 AND 1440);
UPDATE schema_info SET version=4 WHERE id=1;
";

/// Pace targets, transfer probes and session provenance (#64). The
/// `benchmark` track needs no SQL change (track names are mapped by the
/// reader). No row changes; nothing is backfilled.
const MIGRATION_V5: &str = "
CREATE TABLE pace_targets (
    session_id INTEGER NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    track TEXT NOT NULL,
    target_cpm INTEGER NULL CHECK(target_cpm IS NULL OR target_cpm>0),
    decision TEXT NOT NULL CHECK(decision IN ('no_baseline','steady','raise','lower')),
    PRIMARY KEY(session_id,track)
);
CREATE TABLE benchmark_probes (
    session_id INTEGER PRIMARY KEY REFERENCES sessions(session_id) ON DELETE CASCADE,
    passage_id TEXT NOT NULL CHECK(length(passage_id)=16),
    strata TEXT NOT NULL,
    comparable INTEGER NOT NULL CHECK(comparable IN (0,1)),
    reason TEXT NOT NULL
);
ALTER TABLE sessions ADD COLUMN provenance TEXT NOT NULL DEFAULT 'live'
    CHECK(provenance IN ('live','synthetic'));
UPDATE schema_info SET version=5 WHERE id=1;
";

/// Layout and content class per session, the pack digest, and the audit of
/// score-affecting settings (#77). Existing sessions read `unspecified` and
/// `standard`; nothing is rewritten.
const MIGRATION_V6: &str = "
ALTER TABLE sessions ADD COLUMN layout TEXT NOT NULL DEFAULT 'unspecified'
    CHECK(layout IN ('unspecified','us','uk','custom'));
ALTER TABLE sessions ADD COLUMN content_class TEXT NOT NULL DEFAULT 'standard'
    CHECK(content_class IN ('standard','imported','code'));
ALTER TABLE sessions ADD COLUMN pack_digest TEXT NULL CHECK(pack_digest IS NULL OR length(pack_digest)=64);
CREATE TABLE settings_audit (
    at_session INTEGER NOT NULL,
    key TEXT NOT NULL,
    old TEXT NOT NULL,
    new TEXT NOT NULL,
    PRIMARY KEY(at_session,key)
);
UPDATE schema_info SET version=6 WHERE id=1;
";
