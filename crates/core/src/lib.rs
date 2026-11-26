//! `cadence-core` — pure domain foundations for Cadence.
//!
//! Ownership: typed identities, checked time, first-attempt accounting,
//! exact scoring, and portable progress. No terminal, SQL, OS clocks or runtime.
//!
//! All arithmetic on external inputs and counters is checked; generations
//! never wrap. Errors are typed ([`CoreError`]); there is no `unwrap`,
//! `expect`, `panic!` or unchecked indexing in production code here.
//!
//! The scoring and checkpoint decisions are made by `cadence-kernel`, a
//! `no_std` crate whose functions carry Lean theorems (`docs/TAUTEN.md`);
//! the rest of this crate is ordinary Rust with behavioural tests.

pub mod alphabet;
pub mod barrier;
pub mod benchmark;
pub mod calendar;
pub mod comparison;
pub mod error;
pub mod evidence;
pub mod focus;
pub mod id;
pub mod label;
pub mod lanes;
pub mod learning;
pub mod ledger;
pub mod metrics;
pub mod pace;
pub mod plan;
pub mod ports;
pub mod progress;
pub mod queue;
pub mod request;
pub mod review;
pub mod select;
pub mod session;
pub mod settings;
pub mod slots;
pub mod tick;
pub mod ticket;
pub mod units;

pub use alphabet::{AlphabetError, Layout, TrainingChar, validate_target};
pub use error::CoreError;
pub use id::{
    BasisPoints, CommitSeq, ContentDigest, CorpusVersion, EventSeq, ExerciseId, FrameRevision,
    JobId, LayoutRevision, LearnerRevision, PassageId, PolicyVersion, ProfileId, RequestRevision,
    SessionEpoch, SessionId, ViewId, ViewSlot, WordId,
};
pub use ledger::{
    Applied, AttemptLedger, ExtraInputAllowance, LedgerCounters, LedgerEvent, Slot, WordFlags,
};
pub use metrics::{
    ExerciseMetrics, MICROSECONDS_PER_MINUTE, NumericMetrics, Ratio, WPM_UNITS_PER_WORD,
    attempt_accuracy, correct_wpm, digits_per_minute, exercise_metrics, first_pass_accuracy,
    numeric_metrics,
};
pub use ports::{Calendar, Clock};
pub use session::{SessionState, TimingSegments};
pub use tick::{Micros, MonoTick, UtcDay};
pub use units::{CellColumn, TerminalColumns, TerminalRows, TextUnitIndex, UnitCount};
