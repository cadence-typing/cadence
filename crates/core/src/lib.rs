//! `cadence-core` — pure domain foundations for Cadence.
//!
//! Ownership (docs/03-architecture.md A-01): units, IDs, session state
//! machine foundations, first-attempt ledger types, exact scoring types and
//! verified kernels. This crate MUST NOT depend on Ratatui, the terminal,
//! SQL, OS clocks or any async runtime, and has no workspace dependencies.
//!
//! All arithmetic on external inputs and counters is checked; generations
//! never wrap. Errors are typed ([`CoreError`]); there is no `unwrap`,
//! `expect`, `panic!` or unchecked indexing in this crate.
//!
//! Verus-verified kernels (VK-01..VK-06) will live in submodules here once
//! the P1 kernels land; both Verus and ordinary builds consume the same
//! source files.

pub mod error;
pub mod id;
pub mod ports;
pub mod tick;
pub mod units;

pub use error::CoreError;
pub use id::{
    BasisPoints, CommitSeq, ContentDigest, CorpusVersion, EventSeq, ExerciseId, FrameRevision,
    JobId, LayoutRevision, LearnerRevision, PolicyVersion, ProfileId, RequestRevision,
    SessionEpoch, SessionId, ViewId, ViewSlot, WordId,
};
pub use ports::Clock;
pub use tick::{Micros, MonoTick, UtcDay};
pub use units::{CellColumn, TerminalColumns, TerminalRows, TextUnitIndex, UnitCount};
