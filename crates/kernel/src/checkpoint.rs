//! Durable checkpoint admission: which checkpoint writes are credited.
//! Models `formal/DurableCheckpoint.tla` (`NoDoubleCredit`): a sequence is
//! applied at most once, strictly after the latest applied one.

/// What the store knows about the incoming (session, sequence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receipt {
    /// No receipt for this sequence.
    Absent,
    /// A receipt with the same digest exists.
    Same,
    /// A receipt with a different digest exists.
    Different,
}

/// What to do with an incoming checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Apply it and record its receipt.
    Apply,
    /// Return the existing receipt; apply nothing.
    Replay,
    /// Refuse: the sequence was committed with different content.
    Conflict,
    /// Refuse: not newer than the latest applied sequence.
    Stale,
}

/// Admits an incoming checkpoint with sequence `incoming` given the latest
/// applied sequence of its session (`None` for a new session).
#[must_use]
pub fn admit(latest: Option<u64>, receipt: Receipt, incoming: u64) -> Admission {
    match receipt {
        Receipt::Same => Admission::Replay,
        Receipt::Different => Admission::Conflict,
        Receipt::Absent => match latest {
            Some(latest) => {
                if incoming <= latest {
                    Admission::Stale
                } else {
                    Admission::Apply
                }
            }
            None => Admission::Apply,
        },
    }
}

/// The latest applied sequence after an admission: the incoming one only
/// when it was applied.
#[must_use]
pub fn latest_after(latest: Option<u64>, admission: Admission, incoming: u64) -> Option<u64> {
    match admission {
        Admission::Apply => Some(incoming),
        Admission::Replay | Admission::Conflict | Admission::Stale => latest,
    }
}
