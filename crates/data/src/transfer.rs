//! Actual SQLite online backups and schema-versioned portable progress exports.

use std::path::Path;

use cadence_core::learning::SessionLearning;
use cadence_core::progress::SessionProgress;
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::Serialize;

use crate::{ProgressStore, SUPPORTED_SCHEMA, StoreError, files, records, schema};

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
