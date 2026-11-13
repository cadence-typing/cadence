//! `cadence-core` — pure domain foundations for Cadence.
//!
//! Ownership: typed identities, checked time, first-attempt accounting,
//! exact scoring, and portable progress. No terminal, SQL, OS clocks or runtime.
//!
//! All arithmetic on external inputs and counters is checked; generations
//! never wrap. Errors are typed ([`CoreError`]); there is no `unwrap`,
//! `expect`, `panic!` or unchecked indexing in production code here.
//!
//! These are ordinary Rust implementations with behavioral tests, not
//! machine-checked formal proofs.

pub mod alphabet;
pub mod error;
pub mod id;
pub mod learning;
pub mod ledger;
pub mod metrics;
pub mod ports;
pub mod progress;
pub mod session;
pub mod tick;
pub mod units;

pub use alphabet::{AlphabetError, LayoutSupportsPound, TrainingChar, validate_target};
pub use error::CoreError;
pub use id::{
    BasisPoints, CommitSeq, ContentDigest, CorpusVersion, EventSeq, ExerciseId, FrameRevision,
    JobId, LayoutRevision, LearnerRevision, PassageId, PolicyVersion, ProfileId, RequestRevision,
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
