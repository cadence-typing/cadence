//! Typed persistence failures; corrupt values never become valid defaults.

use std::path::PathBuf;

use cadence_core::{CommitSeq, SessionId};

/// A storage failure that must be shown to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// Filesystem/open/locking failure other than contention.
    Open(String),
    /// Another writer owns the profile lock.
    WriterLockHeld {
        /// Contended lock path.
        path: PathBuf,
    },
    /// Unsupported future schema; no database configuration or writes performed.
    SchemaTooNew {
        /// Version found on disk.
        found: i64,
        /// Highest understood version.
        supported: i64,
    },
    /// Populated draft v1 lacks exact measurements needed by v2; retained unchanged.
    LegacySchema,
    /// A transactional numbered migration failed.
    Migration {
        /// Target version.
        target: i64,
        /// SQLite diagnostic.
        message: String,
    },
    /// Malformed persisted data or schema.
    Corrupt(String),
    /// Invalid checkpoint, including a tampered digest.
    InvalidCheckpoint(String),
    /// A value cannot be represented by SQLite's signed integer domain.
    IntegerRange {
        /// Field at the boundary.
        field: &'static str,
    },
    /// The same session/sequence was previously accepted with another digest.
    CommitConflict {
        /// Session identity.
        session: SessionId,
        /// Attempted sequence.
        sequence: CommitSeq,
    },
    /// An unrecorded older sequence cannot be accepted as a durable checkpoint.
    StaleSequence,
    /// Frozen identity, counters, or terminal status regressed.
    Regression(String),
    /// Refusing to overwrite any existing export/backup destination.
    DestinationExists(PathBuf),
    /// SQLite operation failed.
    Sql(String),
    /// Portable JSON encoding failed.
    Export(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open(message) => write!(f, "cannot open progress store: {message}"),
            Self::WriterLockHeld { path } => write!(f, "another cadence writer holds {}", path.display()),
            Self::SchemaTooNew { found, supported } => write!(f, "progress schema {found} is newer than supported schema {supported}"),
            Self::LegacySchema => f.write_str("populated draft schema 1 cannot be upgraded without inventing missing measurements; database retained unchanged"),
            Self::Migration { target, message } => write!(f, "migration to schema {target} failed: {message}"),
            Self::Corrupt(message) => write!(f, "corrupt progress database: {message}"),
            Self::InvalidCheckpoint(message) => write!(f, "invalid checkpoint: {message}"),
            Self::IntegerRange { field } => write!(f, "{field} is outside SQLite's supported integer range"),
            Self::CommitConflict { session, sequence } => write!(f, "conflicting checkpoint for {session} at {sequence}"),
            Self::StaleSequence => f.write_str("unrecorded checkpoint sequence is older than durable progress"),
            Self::Regression(message) => write!(f, "checkpoint regression: {message}"),
            Self::DestinationExists(path) => write!(f, "refusing to overwrite {}", path.display()),
            Self::Sql(message) => write!(f, "database error: {message}"),
            Self::Export(message) => write!(f, "export error: {message}"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value.to_string())
    }
}

pub(crate) fn sql_integer(value: u64, field: &'static str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_unrepresentable| StoreError::IntegerRange { field })
}

pub(crate) fn read_integer(value: i64, field: &'static str) -> Result<u64, StoreError> {
    u64::try_from(value)
        .map_err(|_unrepresentable| StoreError::Corrupt(format!("negative {field}")))
}
