//! Worker result admission (VP-01, VP-02, #75): epoch and request tickets.
//!
//! Every asynchronous job carries the epoch of the screen or session that
//! asked for it and a request id. A result is accepted only when its epoch
//! is current and its request is the one pending for its kind; a late
//! result from an older epoch, a duplicate, or a result nobody waits for
//! changes nothing. Failures obey the same rule as successes.

/// The admission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    /// Apply the result; the pending request is cleared.
    Accept,
    /// The result belongs to an earlier epoch.
    StaleEpoch,
    /// No request of this id is pending (a duplicate or an unknown one).
    NotPending,
}

/// Admission of a result `(result_epoch, result_request)` against the
/// current epoch and the pending request (if any).
#[must_use]
pub fn admit(
    result_epoch: u64,
    result_request: u64,
    current_epoch: u64,
    pending_request: u64,
    has_pending: bool,
) -> Admit {
    if result_epoch != current_epoch {
        return Admit::StaleEpoch;
    }
    if !has_pending || pending_request != result_request {
        return Admit::NotPending;
    }
    Admit::Accept
}
