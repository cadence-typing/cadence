//! Single-writer SQLite store with FULL-durable, exact-key checkpoint receipts.
//!
//! Schema 2 preserves all shared progress DTO counters. Populated draft schema 1
//! is explicitly refused, unchanged, because its missing measurements cannot be
//! reconstructed honestly. Empty draft databases migrate transactionally.

use std::path::Path;
use std::time::Duration;

use cadence_core::SessionId;
use cadence_core::learning::{HistoricSession, LearnerHistory, PaceTargets, ProbeHistory};
use cadence_core::progress::{
    Checkpoint, HistorySummary, ProgressStatus, RECENT_SESSION_CAPACITY, SaveKey, SessionRecap,
    Track,
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

/// A letter or letter-pair skill on the main lane: the same keys on every
/// layout (#77).
fn letters_only(key: cadence_core::learning::SkillKey) -> bool {
    use cadence_core::learning::{KeyLane, Skill};
    let letter =
        |unit: cadence_core::TrainingChar| unit.get().is_ascii_lowercase() || unit.get() == '\'';
    key.lane == KeyLane::Main
        && match key.skill {
            Skill::Unit(unit) => letter(unit),
            Skill::Bigram(first, second) => letter(first) && letter(second),
        }
}

/// Startup facts; history counts never imply restoration of an interrupted word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreBootstrap {
    /// Fresh session identity, never reusing a committed identity.
    pub next_session: SessionId,
    /// Counts of completed versus interrupted saved sessions.
    pub history: HistorySummary,
    /// The most recent sessions' learning evidence, oldest first.
    pub learner: LearnerHistory,
    /// Shown probes, the last comparable strata and sessions since a probe.
    pub probes: ProbeHistory,
    /// The latest suggested pace per typing track (#62).
    pub pace_targets: PaceTargets,
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
        self.bootstrap_for(learner_window, None)
    }

    /// The layout that adopted the sessions recorded before layouts were
    /// (#77), if any did.
    ///
    /// # Errors
    /// SQL failures or a corrupt audit value.
    pub fn adopter(&self) -> Result<Option<cadence_core::Layout>, StoreError> {
        let value: Option<String> = self
            .connection
            .query_row(
                "SELECT new FROM settings_audit WHERE key='layout.adopted' ORDER BY at_session LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|key| {
                cadence_core::Layout::parse(&key)
                    .ok_or_else(|| StoreError::Corrupt("unknown adopted layout".into()))
            })
            .transpose()
    }

    /// The first chosen layout adopts every `unspecified` session, once,
    /// and the adoption is written to `settings_audit` (#77).
    ///
    /// # Errors
    /// SQL failures.
    pub fn adopt_unspecified(
        &mut self,
        layout: cadence_core::Layout,
        at_session: SessionId,
    ) -> Result<(), StoreError> {
        if self.adopter()?.is_some() {
            return Ok(());
        }
        let unspecified: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM sessions WHERE layout='unspecified'",
            [],
            |row| row.get(0),
        )?;
        if unspecified == 0 {
            return Ok(());
        }
        self.connection.execute(
            "INSERT INTO settings_audit(at_session,key,old,new) VALUES (?1,'layout.adopted','unspecified',?2)",
            params![sql_integer(at_session.get(), "session")?, layout.key()],
        )?;
        Ok(())
    }

    /// The newest `limit` sessions with their basis and exact track totals,
    /// oldest first (#81): synthetic and probe sessions included, so the
    /// caller can keep them out of a series visibly. Bounded by `limit`.
    ///
    /// # Errors
    /// Corrupt rows or SQL failures.
    pub fn history_points(
        &self,
        limit: usize,
    ) -> Result<Vec<cadence_core::progress::HistoryPoint>, StoreError> {
        let adopter = self.adopter()?;
        let limit = i64::try_from(limit)
            .map_err(|error| StoreError::Corrupt(format!("history limit: {error}")))?;
        let mut statement = self.connection.prepare(
            "SELECT s.session_id, s.provenance, p.comparable
             FROM sessions s LEFT JOIN benchmark_probes p ON p.session_id = s.session_id
             ORDER BY s.session_id DESC LIMIT ?1",
        )?;
        let rows: Vec<(i64, String, Option<i64>)> = statement
            .query_map([limit], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<_, _>>()?;
        let mut points = Vec::with_capacity(rows.len());
        for (id, provenance, comparable) in rows.into_iter().rev() {
            let session = SessionId::new(
                u64::try_from(id)
                    .map_err(|error| StoreError::Corrupt(format!("session id: {error}")))?,
            )
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
            let Some(stored) = records::load_session(&self.connection, session)? else {
                continue;
            };
            let progress = stored.checkpoint.progress;
            points.push(cadence_core::progress::HistoryPoint {
                session,
                day: progress.day,
                layout: progress.layout.or(adopter),
                content_class: progress.content_class,
                synthetic: provenance == "synthetic",
                probe_comparable: comparable.map(|c| c == 1),
                tracks: progress.tracks,
            });
        }
        Ok(points)
    }

    /// Records the score-affecting settings a session starts with (#79):
    /// for each key whose value differs from its last audited value (its
    /// default when never audited), one row at `at_session`. Launching
    /// again before that session is saved updates the same row, and a
    /// value changed back removes it. Returns the rows now recorded at
    /// `at_session`.
    ///
    /// # Errors
    /// SQL failures.
    pub fn audit_settings(
        &mut self,
        at_session: SessionId,
        values: &[(&str, String)],
    ) -> Result<u64, StoreError> {
        let at = sql_integer(at_session.get(), "session")?;
        let transaction = self.connection.transaction()?;
        for (key, new) in values {
            let last: Option<String> = transaction
                .query_row(
                    "SELECT new FROM settings_audit WHERE key=?1 AND at_session<?2
                     ORDER BY at_session DESC LIMIT 1",
                    params![key, at],
                    |row| row.get(0),
                )
                .optional()?;
            let old = match last {
                Some(value) => value,
                None => cadence_core::settings::KEYS
                    .iter()
                    .find(|spec| spec.name == *key)
                    .map(|spec| spec.default.setting().to_string())
                    .ok_or_else(|| StoreError::Corrupt("unknown setting audited".into()))?,
            };
            if &old == new {
                transaction.execute(
                    "DELETE FROM settings_audit WHERE at_session=?1 AND key=?2",
                    params![at, key],
                )?;
            } else {
                transaction.execute(
                    "INSERT INTO settings_audit(at_session,key,old,new) VALUES (?1,?2,?3,?4)
                     ON CONFLICT(at_session,key) DO UPDATE SET new=excluded.new",
                    params![at, key, old, new],
                )?;
            }
        }
        let recorded: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM settings_audit WHERE at_session=?1 AND key<>'layout.adopted'",
            [at],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        u64::try_from(recorded)
            .map_err(|error| StoreError::Corrupt(format!("audit count: {error}")))
    }

    /// Startup facts for a layout (#77): sessions read their own layout,
    /// or the adopter's when they predate layouts. History from another
    /// layout keeps its letter skills and word outcomes (the letters are the
    /// same keys) but not its digit and symbol skills.
    ///
    /// # Errors
    /// Corrupt rows or SQL failures.
    pub fn bootstrap_for(
        &self,
        learner_window: usize,
        layout: Option<cadence_core::Layout>,
    ) -> Result<StoreBootstrap, StoreError> {
        let adopter = self.adopter()?;
        let all = self.session_history()?;
        // Synthetic (headless) sessions never enter history, learning,
        // comparisons or probes (#69); they only reserve their ids.
        let synthetic = self.synthetic_sessions()?;
        let newest_id = all
            .iter()
            .map(|s| s.checkpoint.key.session.get())
            .max()
            .unwrap_or(0);
        let sessions: Vec<StoredSession> = all
            .into_iter()
            .filter(|s| !synthetic.contains(&s.checkpoint.key.session.get()))
            .collect();
        let mut history = HistorySummary::default();
        let mut maximum = 0_u64;
        let mut probes = ProbeHistory::default();
        let mut pace_targets: PaceTargets = Vec::new();
        for session in &sessions {
            let checkpoint = &session.checkpoint;
            for record in checkpoint.learning.iter().flat_map(|l| l.pace.iter()) {
                if let Some(cpm) = record.target_cpm {
                    pace_targets.retain(|(track, _)| *track != record.track);
                    pace_targets.push((record.track, cpm));
                }
            }
            if let Some(probe) = checkpoint.learning.as_ref().and_then(|l| l.probe) {
                probes.shown.insert(probe.passage);
                if probe.comparable {
                    probes.last_comparable_strata = Some(probe.strata);
                    history.comparable_probes = history.comparable_probes.saturating_add(1);
                    history.last_comparable_probe = checkpoint
                        .progress
                        .tracks
                        .iter()
                        .find(|t| t.track == Track::Benchmark)
                        .cloned();
                }
                probes.completed_with_prose_since = 0;
            } else if checkpoint.progress.status == ProgressStatus::Completed
                && checkpoint
                    .progress
                    .tracks
                    .iter()
                    .any(|t| t.track == Track::Prose && t.first_attempted > 0)
            {
                probes.completed_with_prose_since =
                    probes.completed_with_prose_since.saturating_add(1);
            }
        }
        let mut learner: Vec<HistoricSession> = sessions
            .iter()
            .rev()
            .filter_map(|session| {
                let checkpoint = &session.checkpoint;
                let effective = checkpoint.progress.layout.or(adopter);
                checkpoint.learning.clone().map(|mut learning| {
                    if layout.is_some() && effective != layout {
                        learning.skills.retain(|key, _| letters_only(*key));
                    }
                    HistoricSession {
                        session: checkpoint.key.session,
                        day: checkpoint.progress.day,
                        learning,
                    }
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
                layout: session.checkpoint.progress.layout.or(adopter),
                content_class: session.checkpoint.progress.content_class,
            })
            .collect();
        history.latest_corpus_digest = sessions
            .last()
            .map(|session| session.checkpoint.progress.corpus_digest.clone());
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
        let next = maximum
            .max(newest_id)
            .checked_add(1)
            .ok_or(StoreError::IntegerRange {
                field: "next session",
            })?;
        sql_integer(next, "next session")?;
        let next_session =
            SessionId::new(next).map_err(|error| StoreError::Corrupt(error.to_string()))?;
        Ok(StoreBootstrap {
            next_session,
            history,
            learner: LearnerHistory { sessions: learner },
            probes,
            pace_targets,
        })
    }

    /// Marks a session as synthetic (a headless-driver session, #69): it is
    /// kept, exported with its provenance, and never read back as history.
    ///
    /// # Errors
    /// SQL failures; an unknown session is refused.
    pub fn mark_synthetic(&mut self, session: SessionId) -> Result<(), StoreError> {
        let changed = self.connection.execute(
            "UPDATE sessions SET provenance='synthetic' WHERE session_id=?1",
            [sql_integer(session.get(), "session")?],
        )?;
        if changed == 0 {
            return Err(StoreError::Corrupt(
                "no such session to mark synthetic".into(),
            ));
        }
        Ok(())
    }

    fn synthetic_sessions(&self) -> Result<std::collections::HashSet<u64>, StoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT session_id FROM sessions WHERE provenance='synthetic'")?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
        let mut ids = std::collections::HashSet::new();
        for row in rows {
            ids.insert(crate::error::read_integer(row?, "session")?);
        }
        Ok(ids)
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
            "INSERT INTO sessions(session_id,profile,day,policy_digest,corpus_digest,status,active_micros,commit_seq,utc_offset_minutes,layout,content_class)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
             ON CONFLICT(session_id) DO UPDATE SET status=excluded.status,
                active_micros=excluded.active_micros,commit_seq=excluded.commit_seq",
            params![session, progress.profile, progress.day, progress.policy_digest, progress.corpus_digest,
                records::status_name(progress.status), sql_integer(progress.active_micros, "active_micros")?, sequence,
                progress.utc_offset_minutes,
                progress.layout.map_or("unspecified", cadence_core::Layout::key),
                progress.content_class.key()],
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
