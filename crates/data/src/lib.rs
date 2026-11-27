//! `cadence-data` — persistence and content adapters (A-01, D-01..D-04).
//!
//! Owns: XDG path resolution, the single-writer SQLite progress store
//! (WAL, explicit transactions, FULL durability, cumulative
//! idempotent checkpoints with receipts), backups via SQLite's online
//! backup, and portable export. Must not own UI mutation or training
//! policy decisions.

mod chunk_rows;
pub use chunk_rows::Watermark;
mod error;
mod faults;
mod files;
mod learning_rows;
mod records;
mod schema;
mod transfer;

pub mod config;
pub mod config_write;
#[cfg(feature = "test-faults")]
pub mod fixtures;
pub mod packs;
pub mod paths;
pub mod store;
pub mod theme_file;

pub use cadence_core::progress::{Checkpoint, SaveKey, SessionProgress, Track, TrackProgress};
pub use paths::{PathsError, ProfilePaths, data_dir, resolve_data_dir};
pub use store::{
    CommitReceipt, ProgressStore, SUPPORTED_SCHEMA, StoreBootstrap, StoreError, StoredSession,
};
pub use transfer::{PREVIOUS_DATABASE_FILE, Restored};

#[cfg(test)]
mod tests;
