//! Single-writer SQLite store with FULL-durable, exact-key checkpoint receipts.
//!
//! Schema 2 preserves all shared progress DTO counters. Populated draft schema 1
//! is explicitly refused, unchanged, because its missing measurements cannot be
//! reconstructed honestly. Empty draft databases migrate transactionally.

use std::path::Path;
use std::time::Duration;

use cadence_core::SessionId;
use cadence_core::learning::{HistoricSession, LearnerHistory};
use cadence_core::progress::{
    Checkpoint, HistorySummary, ProgressStatus, RECENT_SESSION_CAPACITY, SaveKey, SessionRecap,
};
use cadence_kernel::checkpoint::{Admission, Receipt, admit};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

pub use crate::error::StoreError;
use crate::error::sql_integer;
use crate::files::{self, WriterLock};
use crate::paths::{DATABASE_FILE, data_dir};
pub use crate::records::StoredSession;
pub use crate::schema::SUPPORTED_SCHEMA;
use crate::{records, schema};

/// Startup facts; history counts never imply restoration of an interrupted word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreBootstrap {
    /// Fresh session identity, never reusing a committed identity.
    pub next_session: SessionId,
    /// Counts of completed versus interrupted saved sessions.
    pub history: HistorySummary,
    /// The most recent sessions' learning evidence, oldest first.
    pub learner: LearnerHistory,
}

/// Receipt returned only after the transaction has committed durably.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitReceipt {
    /// Exact accepted key, including on acknowledgement-loss retry.
    pub key: SaveKey,
    /// False when this exact key was already accepted.
    pub created: bool,
}

/// Owns one writer connection and its safe OS lock for the entire lifetime.
pub struct ProgressStore {
    pub(crate) connection: Connection,
    _lock: WriterLock,
}

/// Bounded SQLite contention budget; never an unbounded UI-thread wait.
const BUSY_TIMEOUT: Duration = Duration::from_secs(2);

impl ProgressStore {
    /// Opens the explicit profile, or resolves its XDG data directory.
    ///
    /// # Errors
    /// Returns path, lock, schema, corruption, migration, or SQLite failures.
    pub fn open(override_dir: Option<&Path>) -> Result<Self, StoreError> {
        let paths = data_dir(override_dir).map_err(|error| StoreError::Open(error.to_string()))?;
        Self::open_in(&paths.data)
    }

    /// Opens a private profile directory, refusing incompatible data before writes/configuration.
    ///
    /// # Errors
    /// Returns typed lock, schema, filesystem, corruption, or SQLite failures.
    pub fn open_in(directory: &Path) -> Result<Self, StoreError> {
        let database = directory.join(DATABASE_FILE);
        // Read-only preflight even precedes creating a lock file in an existing profile.
        if database
            .try_exists()
            .map_err(|error| StoreError::Open(error.to_string()))?
        {
            let connection =
                Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            preflight(&connection)?;
        }
        files::private_directory(directory)?;
        let lock = WriterLock::acquire(directory)?;
        files::create_database(&database)?;
        let mut connection =
            Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        // Recheck under the lifetime lock; the preflight cannot authorize a raced replacement.
        let current = preflight(&connection)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let journal: String =
            connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        let synchronous: i64 =
            connection.pragma_query_value(None, "synchronous", |row| row.get(0))?;
        let foreign_keys: bool =
            connection.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
        // SQLite's documented encoding for synchronous=FULL is 2.
        if journal != "wal" || synchronous != 2 || !foreign_keys {
            return Err(StoreError::Open(
                "SQLite could not enable WAL, FULL durability and foreign keys".into(),
            ));
        }
        schema::migrate(&mut connection, current)?;
        files::sync_parent(&database)?;
        Ok(Self {
            connection,
            _lock: lock,
        })
    }

    /// Returns the next legal SQLite-backed session identity, honest durable
    /// history and up to `learner_window` most recent sessions' evidence.
    ///
    /// # Errors
    /// Rejects corrupt rows and exhausted identity space rather than clamping either.
    pub fn bootstrap(&self, learner_window: usize) -> Result<StoreBootstrap, StoreError> {
        let sessions = self.session_history()?;
        let mut history = HistorySummary::default();
        let mut maximum = 0_u64;
        let mut learner: Vec<HistoricSession> = sessions
            .iter()
            .rev()
            .filter_map(|session| {
                let checkpoint = &session.checkpoint;
                checkpoint.learning.clone().map(|learning| HistoricSession {
                    session: checkpoint.key.session,
                    day: checkpoint.progress.day,
                    learning,
                })
            })
            .take(learner_window)
            .collect();
        learner.reverse();
        // Sessions arrive in day/session order; keep the newest ones that
        // recorded any typing, finished or not (the status is carried).
        history.recent = sessions
            .iter()
            .rev()
            .filter(|session| {
                session
                    .checkpoint
                    .progress
                    .tracks
                    .iter()
                    .any(|track| track.first_attempted > 0)
            })
            .take(RECENT_SESSION_CAPACITY)
            .map(|session| SessionRecap {
                day: session.checkpoint.progress.day,
                completed: session.checkpoint.progress.status == ProgressStatus::Completed,
                tracks: session.checkpoint.progress.tracks.clone(),
            })
            .collect();
        for session in sessions {
            let checkpoint = session.checkpoint;
            maximum = maximum.max(checkpoint.key.session.get());
            let counter = if checkpoint.progress.status == ProgressStatus::Completed {
                &mut history.completed_sessions
            } else {
                &mut history.interrupted_sessions
            };
            *counter = counter.checked_add(1).ok_or(StoreError::IntegerRange {
                field: "history count",
            })?;
            history.latest_day = Some(history.latest_day.map_or(checkpoint.progress.day, |day| {
                day.max(checkpoint.progress.day)
            }));
        }
        let next = maximum.checked_add(1).ok_or(StoreError::IntegerRange {
            field: "next session",
        })?;
        sql_integer(next, "next session")?;
        let next_session =
            SessionId::new(next).map_err(|error| StoreError::Corrupt(error.to_string()))?;
        Ok(StoreBootstrap {
            next_session,
            history,
            learner: LearnerHistory { sessions: learner },
        })
    }

    /// Applies a validated cumulative checkpoint atomically, or returns its exact prior receipt.
    ///
    /// An unrecorded stale sequence is an error, never an acknowledgement of a different key.
    /// Existing historical receipts remain replayable after newer checkpoints were committed.
    ///
    /// # Errors
    /// Rejects digest tampering, identity/counter regressions, conflicts, stale sequences,
    /// SQLite integer overflow, and SQL/commit failures. Failed transactions roll back.
    pub fn apply_checkpoint(
        &mut self,
        checkpoint: &Checkpoint,
    ) -> Result<CommitReceipt, StoreError> {
        records::validate_checkpoint(checkpoint)?;
        let key = checkpoint.key;
        let session = sql_integer(key.session.get(), "session_id")?;
        let sequence = sql_integer(key.sequence.get(), "commit_seq")?;
        let digest = key.digest.to_hex();
        let tx = self.connection.transaction()?;
        let previous = records::load_session(&tx, key.session)?;
        let stored_digest: Option<String> = tx
            .query_row(
                "SELECT digest FROM commit_receipts WHERE session_id=?1 AND commit_seq=?2",
                params![session, sequence],
                |row| row.get(0),
            )
            .optional()?;
        let receipt = match &stored_digest {
            None => Receipt::Absent,
            Some(stored) if *stored == digest => Receipt::Same,
            Some(_) => Receipt::Different,
        };
        let latest = previous
            .as_ref()
            .map(|previous| previous.checkpoint.key.sequence.get());
        // The kernel decides which checkpoint is credited (Lean: no sequence
        // is applied twice, and a replay or stale request applies nothing).
        match admit(latest, receipt, key.sequence.get()) {
            Admission::Replay => {
                // Read-only replay: nothing is reapplied, even when a newer sequence exists.
                tx.commit()?;
                return Ok(CommitReceipt {
                    key,
                    created: false,
                });
            }
            Admission::Conflict => {
                return Err(StoreError::CommitConflict {
                    session: key.session,
                    sequence: key.sequence,
                });
            }
            Admission::Stale => return Err(StoreError::StaleSequence),
            Admission::Apply => {}
        }
        if let Some(previous) = previous {
            records::validate_advance(&previous.checkpoint, checkpoint)?;
        }
        let progress = &checkpoint.progress;
        tx.execute(
            "INSERT INTO profiles(name) VALUES (?1) ON CONFLICT(name) DO NOTHING",
            [&progress.profile],
        )?;
        tx.execute(
            "INSERT INTO sessions(session_id,profile,day,policy_digest,corpus_digest,status,active_micros,commit_seq)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(session_id) DO UPDATE SET status=excluded.status,
                active_micros=excluded.active_micros,commit_seq=excluded.commit_seq",
            params![session, progress.profile, progress.day, progress.policy_digest, progress.corpus_digest,
                records::status_name(progress.status), sql_integer(progress.active_micros, "active_micros")?, sequence],
        )?;
        tx.execute("DELETE FROM session_tracks WHERE session_id=?1", [session])?;
        for track in &progress.tracks {
            tx.execute(
                "INSERT INTO session_tracks(session_id,track,first_correct,first_attempted,correct_attempts,
                    content_attempts,deletions,overflow_attempts,correct_units,correct_digits,active_micros)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![session, records::track_name(track.track), sql_integer(track.first_correct, "first_correct")?,
                    sql_integer(track.first_attempted, "first_attempted")?, sql_integer(track.correct_attempts, "correct_attempts")?,
                    sql_integer(track.content_attempts, "content_attempts")?, sql_integer(track.deletions, "deletions")?,
                    sql_integer(track.overflow_attempts, "overflow_attempts")?, sql_integer(track.correct_units, "correct_units")?,
                    sql_integer(track.correct_digits, "correct_digits")?, sql_integer(track.active_micros, "track active_micros")?],
            )?;
        }
        if let Some(learning) = &checkpoint.learning {
            crate::learning_rows::save(&tx, session, learning)?;
        }
        tx.execute(
            "INSERT INTO commit_receipts(session_id,commit_seq,digest,created_at_epoch) VALUES (?1,?2,?3,?4)",
            params![session, sequence, digest, progress.day],
        )?;
        tx.commit()?;
        Ok(CommitReceipt { key, created: true })
    }

    /// Reads complete latest checkpoints in day/session order with checked decoding.
    ///
    /// # Errors
    /// Rejects malformed integers, invalid progress, or mismatched durable digests.
    pub fn session_history(&self) -> Result<Vec<StoredSession>, StoreError> {
        records::validate_database(&self.connection)
    }
}

fn preflight(connection: &Connection) -> Result<i64, StoreError> {
    let version = schema::inspect(connection)?;
    if version == SUPPORTED_SCHEMA {
        records::validate_database(connection)?;
    }
    Ok(version)
}
