//! `cadence-data` — persistence and content adapters (A-01, D-01..D-04).
//!
//! Owns: the single-writer SQLite adapter (WAL, explicit transactions,
//! cumulative checkpoints with monotonic `CommitSeq` and digests),
//! configuration loading/validation, corpus pack loading/indexing, import
//! normalization, backup and portable export.
//!
//! Must not own: UI mutation or training policy decisions. Heavy work is
//! executed behind the runtime's bounded workers, never on the owner
//! thread.
//!
//! Status: the storage schema and writer land with P3; XDG path handling
//! follows D-01 exactly (relative XDG variables are rejected as invalid).
