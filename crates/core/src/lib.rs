//! `cadence-core` — pure domain foundations for Cadence.
//!
//! Ownership (docs/03-architecture.md A-01): units, IDs, session state
//! machine foundations, first-attempt ledger, exact scoring and verified
//! kernels. This crate MUST NOT depend on Ratatui, the terminal,
//! SQL, OS clocks or any async runtime, and has no workspace dependencies.
//!
//! All arithmetic on external inputs and counters is checked; generations
//! never wrap. Errors are typed ([`CoreError`]); there is no `unwrap`,
//! `expect`, `panic!` or unchecked indexing in production code here.
//!
//! Verus-verified kernels (VK-01..VK-06) live in these modules as the
//! same source consumed by ordinary builds; the Verus lane proof wiring
//! arrives with P1 kernels and the P7 inventory.

pub mod alphabet;
pub mod error;
pub mod id;
pub mod ledger;
pub mod metrics;
pub mod ports;
pub mod session;
pub mod tick;
pub mod units;

pub use alphabet::{AlphabetError, LayoutSupportsPound, TrainingChar, validate_target};
pub use error::CoreError;
pub use id::{
    BasisPoints, CommitSeq, ContentDigest, CorpusVersion, EventSeq, ExerciseId, FrameRevision,
    JobId, LayoutRevision, LearnerRevision, PolicyVersion, ProfileId, RequestRevision,
    SessionEpoch, SessionId, ViewId, ViewSlot, WordId,
};
pub use ledger::{
    Applied, AttemptLedger, ExtraInputAllowance, LedgerCounters, LedgerEvent, Outcome, WordFlags,
};
pub use metrics::{
    ExerciseMetrics, MICROSECONDS_PER_MINUTE, NumericMetrics, Ratio, WPM_UNITS_PER_WORD,
    active_time, attempt_accuracy, correct_wpm, digits_per_minute, exercise_metrics,
    first_pass_accuracy, numeric_metrics,
};
pub use ports::{Calendar, Clock};
pub use session::{SessionState, SessionTransition, TimingSegments, next_state};
pub use tick::{Micros, MonoTick, UtcDay};
pub use units::{CellColumn, TerminalColumns, TerminalRows, TextUnitIndex, UnitCount};
