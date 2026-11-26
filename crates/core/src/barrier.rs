//! Headless barriers (VP-04, #71). The decisions are the proved
//! `cadence_kernel::barrier::{causal, durable}` (Lean: done exactly when the
//! admitted scope has no outstanding job, pending dispatch or unconsumed
//! result; a barrier through a future event is out of scope; a durable
//! barrier is done only with an acknowledgement at or above its watermark).
pub use cadence_kernel::barrier::{Barrier, causal, durable};
