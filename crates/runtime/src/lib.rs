//! `cadence-runtime` — composition and transport (A-01, A-04).
//!
//! The writer retains terminal outcomes until consumed. Terminal ingress
//! and database work never mutate application state directly.

pub mod live;
mod presenter;
mod text_protocol;
pub mod writer;

pub use live::{TerminalError, TerminalGuard, elapsed_since};
pub use writer::{SaveWorker, WriterError};
