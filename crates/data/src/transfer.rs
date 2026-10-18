//! Actual SQLite online backups and schema-versioned portable progress exports.

use std::path::Path;

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
