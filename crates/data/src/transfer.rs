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
        let sessions = self
            .session_history()?
            .into_iter()
            .map(|session| {
                let checkpoint = session.checkpoint;
                PortableSession {
                    session_id: checkpoint.key.session.get(),
                    sequence: checkpoint.key.sequence.get(),
                    digest: checkpoint.key.digest.to_hex(),
                    learning: checkpoint.learning.as_ref().map(PortableLearning::new),
                    progress: checkpoint.progress,
                }
            })
            .collect();
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
