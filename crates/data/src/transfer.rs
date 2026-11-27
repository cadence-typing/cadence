//! Actual SQLite online backups and schema-versioned portable progress exports.

use std::path::Path;

use cadence_core::learning::SessionLearning;
use cadence_core::progress::SessionProgress;
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::Serialize;

use crate::paths::DATABASE_FILE;
use crate::{ProgressStore, SUPPORTED_SCHEMA, StoreError, files, records, schema};

/// The replaced database a restore keeps, beside the restored one (D-02).
pub const PREVIOUS_DATABASE_FILE: &str = "progress.sqlite3.previous";

#[derive(Serialize)]
struct PortableSession {
    session_id: u64,
    sequence: u64,
    digest: String,
    progress: SessionProgress,
    /// Schema-3 evidence; absent for sessions recorded before it existed.
    learning: Option<PortableLearning>,
    /// Schema 4: the local UTC offset in minutes the session's day was
    /// counted in (#52); `null` when it was not recorded.
    utc_offset_minutes: Option<i64>,
    /// Schema 5: `live`, or `synthetic` for a headless-driver session (#64).
    provenance: String,
    /// Schema 7: the session's observation chunks in order, each its
    /// sequence, track, digest and canonical payload (#11); empty for
    /// legacy sessions.
    chunks: Vec<PortableChunk>,
}

#[derive(Serialize)]
struct PortableChunk {
    seq: u64,
    track: &'static str,
    digest: String,
    payload: serde_json::Value,
}

impl PortableChunk {
    fn new(chunk: &cadence_core::chunks::ObservationChunk) -> Result<Self, StoreError> {
        Ok(Self {
            seq: chunk.seq,
            track: chunk.track.key(),
            digest: chunk.digest().to_hex(),
            payload: serde_json::from_slice(&chunk.encode())
                .map_err(|error| StoreError::Export(error.to_string()))?,
        })
    }
}

#[derive(Serialize)]
struct PortableSkill {
    lane: &'static str,
    kind: &'static str,
    skill: String,
    attempts: u64,
    errors: u64,
    timed: u64,
    timed_micros: u64,
}

#[derive(Serialize)]
struct PortableWord {
    word_id: u64,
    attempts: u64,
    errors: u64,
}

#[derive(Serialize)]
struct PortableLearning {
    seed: String,
    mode: &'static str,
    focus: Vec<String>,
    reinforced: u64,
    skills: Vec<PortableSkill>,
    words: Vec<PortableWord>,
    passages: Vec<String>,
}

impl PortableLearning {
    fn new(learning: &SessionLearning) -> Self {
        Self {
            seed: format!("{:016x}", learning.plan.seed),
            mode: learning.plan.mode.key(),
            focus: learning
                .plan
                .focus
                .iter()
                .map(|key| format!("{}:{}", key.lane.key(), key.skill.text()))
                .collect(),
            reinforced: learning.reinforced,
            skills: learning
                .skills
                .iter()
                .map(|(key, tally)| PortableSkill {
                    lane: key.lane.key(),
                    kind: key.skill.kind_key(),
                    skill: key.skill.text(),
                    attempts: tally.attempts,
                    errors: tally.errors,
                    timed: tally.timed,
                    timed_micros: tally.timed_micros,
                })
                .collect(),
            words: learning
                .words
                .iter()
                .map(|(word, tally)| PortableWord {
                    word_id: word.get(),
                    attempts: tally.attempts,
                    errors: tally.errors,
                })
                .collect(),
            passages: learning
                .passages
                .iter()
                .map(|passage| format!("{:016x}", passage.get()))
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct PortableExport {
    schema_version: i64,
    sessions: Vec<PortableSession>,
}

impl ProgressStore {
    /// Creates an online SQLite backup, validating its actual sessions before success.
    ///
    /// The destination is exclusively reserved, never overwritten, and synced before return.
    /// No raw copy of a live WAL database is used.
    ///
    /// # Errors
    /// Returns typed corruption, existing-output, filesystem, or SQLite failures.
    pub fn backup_to(&mut self, destination: &Path) -> Result<(), StoreError> {
        let expected = self.session_history()?;
        let file = files::reserve(destination)?;
        let result = (|| {
            let mut target =
                Connection::open_with_flags(destination, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
            {
                let backup = Backup::new(&self.connection, &mut target)?;
                // The source is exclusively owned by this worker; contention is a visible failure,
                // not a retry loop with sleeps. -1 copies all remaining pages through SQLite.
                match backup.step(-1)? {
                    StepResult::Done => {}
                    other => {
                        return Err(StoreError::Sql(format!(
                            "online backup did not finish: {other:?}"
                        )));
                    }
                }
            }
            if schema::inspect(&target)? != SUPPORTED_SCHEMA {
                return Err(StoreError::Corrupt(
                    "backup schema differs from source".into(),
                ));
            }
            if records::validate_database(&target)? != expected {
                return Err(StoreError::Corrupt(
                    "backup progress differs from source".into(),
                ));
            }
            target
                .close()
                .map_err(|(_, error)| StoreError::Sql(error.to_string()))?;
            file.sync_all()
                .map_err(|error| StoreError::Open(error.to_string()))?;
            files::sync_parent(destination)
        })();
        drop(file);
        result.map_err(|error| files::remove_failed(destination, error))
    }

    /// Exports all latest sessions and exact track counters as versioned portable JSON.
    ///
    /// # Errors
    /// Rejects corrupt data and existing outputs; reports encoding/write/sync failures.
    pub fn export_to(&self, destination: &Path) -> Result<(), StoreError> {
        let mut offsets = std::collections::HashMap::new();
        {
            let mut statement = self
                .connection
                .prepare("SELECT session_id, utc_offset_minutes, provenance FROM sessions")?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (session, offset, provenance) = row?;
                offsets.insert(session, (offset, provenance));
            }
        }
        let sessions = self
            .session_history()?
            .into_iter()
            .map(|session| {
                let chunks = self
                    .session_chunks(session.checkpoint.key.session)?
                    .iter()
                    .map(PortableChunk::new)
                    .collect::<Result<Vec<_>, _>>()?;
                let checkpoint = session.checkpoint;
                let stored = i64::try_from(checkpoint.key.session.get())
                    .ok()
                    .and_then(|session| offsets.get(&session));
                Ok(PortableSession {
                    session_id: checkpoint.key.session.get(),
                    sequence: checkpoint.key.sequence.get(),
                    digest: checkpoint.key.digest.to_hex(),
                    learning: session.learning.as_ref().map(PortableLearning::new),
                    utc_offset_minutes: stored.and_then(|(offset, _)| *offset),
                    provenance: stored
                        .map_or_else(|| "live".to_string(), |(_, provenance)| provenance.clone()),
                    progress: checkpoint.progress,
                    chunks,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let mut file = files::reserve(destination)?;
        let result = (|| {
            serde_json::to_writer_pretty(
                &mut file,
                &PortableExport {
                    schema_version: SUPPORTED_SCHEMA,
                    sessions,
                },
            )
            .map_err(|error| StoreError::Export(error.to_string()))?;
            file.sync_all()
                .map_err(|error| StoreError::Open(error.to_string()))?;
            files::sync_parent(destination)
        })();
        drop(file);
        result.map_err(|error| files::remove_failed(destination, error))
    }
}

/// What a restore did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restored {
    /// Sessions in the restored database.
    pub sessions: usize,
    /// The schema the backup was written with (an older one migrates on the
    /// next open).
    pub schema: i64,
    /// Where the replaced database is kept, when there was one.
    pub previous: Option<std::path::PathBuf>,
}

fn validated(connection: &Connection) -> Result<(i64, usize), StoreError> {
    let version = schema::inspect(connection)?;
    if version > SUPPORTED_SCHEMA {
        return Err(StoreError::SchemaTooNew {
            found: version,
            supported: SUPPORTED_SCHEMA,
        });
    }
    if version == 0 {
        return Err(StoreError::Corrupt(
            "the backup holds no progress database".into(),
        ));
    }
    let sessions = if version == SUPPORTED_SCHEMA {
        records::validate_database(connection)?.len()
    } else {
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        usize::try_from(count)
            .map_err(|_negative| StoreError::Corrupt("negative session count".into()))?
    };
    Ok((version, sessions))
}

impl ProgressStore {
    /// Restores a backup into `directory` with the writer closed (D-02):
    /// the backup is validated before anything is replaced, copied and
    /// validated again beside the database, the current database is kept as
    /// [`PREVIOUS_DATABASE_FILE`], and only then does the copy replace it
    /// in one rename. An interruption at any point leaves a working
    /// database: the current one, or the restored one.
    ///
    /// # Errors
    /// A held writer lock, an invalid or newer backup, an existing previous
    /// file, or a filesystem or SQLite failure; nothing is replaced then.
    pub fn restore(directory: &Path, backup: &Path) -> Result<Restored, StoreError> {
        let source = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let (version, sessions) = validated(&source)?;
        files::private_directory(directory)?;
        let _lock = files::WriterLock::acquire(directory)?;
        let database = directory.join(DATABASE_FILE);
        let previous = directory.join(PREVIOUS_DATABASE_FILE);
        if previous
            .try_exists()
            .map_err(|error| StoreError::Open(error.to_string()))?
        {
            return Err(StoreError::DestinationExists(previous));
        }
        let staged = directory.join(format!("{DATABASE_FILE}.restoring"));
        let file = files::reserve(&staged)?;
        let copied = (|| {
            let mut target =
                Connection::open_with_flags(&staged, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
            {
                let backup = Backup::new(&source, &mut target)?;
                let step = backup.step(-1)?;
                if step != StepResult::Done {
                    return Err(StoreError::Sql(format!(
                        "the restore copy did not finish: {step:?}"
                    )));
                }
            }
            if validated(&target)? != (version, sessions) {
                return Err(StoreError::Corrupt(
                    "the restore copy differs from the backup".into(),
                ));
            }
            target
                .close()
                .map_err(|(_, error)| StoreError::Sql(error.to_string()))?;
            file.sync_all()
                .map_err(|error| StoreError::Open(error.to_string()))
        })();
        if let Err(error) = copied {
            return Err(files::remove_failed(&staged, error));
        }
        crate::faults::point("restore-staged");
        let kept = if database
            .try_exists()
            .map_err(|error| StoreError::Open(error.to_string()))?
        {
            // Fold the current WAL into the file before keeping it.
            let current =
                Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
            current.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
            current
                .close()
                .map_err(|(_, error)| StoreError::Sql(error.to_string()))?;
            std::fs::hard_link(&database, &previous)
                .map_err(|error| StoreError::Open(error.to_string()))?;
            files::sync_parent(&previous)?;
            Some(previous)
        } else {
            None
        };
        crate::faults::point("restore-swap");
        for stale in ["-wal", "-shm"] {
            let path = directory.join(format!("{DATABASE_FILE}{stale}"));
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(StoreError::Open(error.to_string())),
            }
        }
        std::fs::rename(&staged, &database).map_err(|error| StoreError::Open(error.to_string()))?;
        files::sync_parent(&database)?;
        Ok(Restored {
            sessions,
            schema: version,
            previous: kept,
        })
    }
}
