//! Headless barriers (VP-04, #71): whether a causal or durable barrier
//! may return.
//!
//! A causal barrier covers the work caused by events through
//! `through_seq`, including children registered when a parent's result is
//! consumed. It returns only when that scope has no outstanding job, no
//! pending dispatch and no unconsumed result. A barrier through an event
//! that was not admitted yet is out of scope. A durable barrier also needs
//! an acknowledged checkpoint at or above the captured watermark.

/// The barrier decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Barrier {
    /// The scope is drained (and, for a durable barrier, persisted).
    Done,
    /// Work in scope remains.
    Waiting,
    /// `through_seq` is beyond the latest admitted event.
    OutOfScope,
}

/// The causal barrier through `through_seq`, the latest admitted event
/// being `latest_seq`, with the counts of the scope's outstanding jobs,
/// pending dispatches and unconsumed results.
#[must_use]
pub fn causal(
    through_seq: u64,
    latest_seq: u64,
    outstanding: u64,
    pending_dispatch: u64,
    unconsumed: u64,
) -> Barrier {
    if through_seq > latest_seq {
        return Barrier::OutOfScope;
    }
    if outstanding == 0 && pending_dispatch == 0 && unconsumed == 0 {
        Barrier::Done
    } else {
        Barrier::Waiting
    }
}

/// The durable barrier over a causal decision: done only when the causal
/// scope is done and an acknowledged checkpoint covers the watermark.
#[must_use]
pub fn durable(causal: Barrier, acked_seq: u64, has_ack: bool, watermark_seq: u64) -> Barrier {
    match causal {
        Barrier::OutOfScope => Barrier::OutOfScope,
        Barrier::Waiting => Barrier::Waiting,
        Barrier::Done => {
            if has_ack && acked_seq >= watermark_seq {
                Barrier::Done
            } else {
                Barrier::Waiting
            }
        }
    }
}
