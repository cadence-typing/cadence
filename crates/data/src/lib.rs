//! `cadence-data` — persistence and content adapters (A-01, D-01..D-04).
//!
//! Owns: XDG path resolution, the single-writer SQLite progress store
//! (WAL, explicit transactions, FULL durability, cumulative
//! idempotent checkpoints with receipts), backups via SQLite's online
//! backup, and portable export. Must not own UI mutation or training
//! policy decisions.

mod error;
mod files;
mod records;
mod schema;
mod transfer;

pub mod paths;
pub mod store;

pub use cadence_core::progress::{Checkpoint, SaveKey, SessionProgress, Track, TrackProgress};
pub use paths::{PathsError, ProfilePaths, data_dir, resolve_data_dir};
pub use store::{
    CommitReceipt, ProgressStore, SUPPORTED_SCHEMA, StoreBootstrap, StoreError, StoredSession,
};

#[cfg(test)]
mod tests;
