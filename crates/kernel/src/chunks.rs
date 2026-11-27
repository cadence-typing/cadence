//! Observation-chunk admission (L-01, D-02, VP-03, #11). A session's chunks
//! are numbered densely from 1, one per closed exercise; a checkpoint
//! carries those after the last acknowledged one. Each incoming chunk is
//! decided against what the store holds: the next one is appended, a
//! stored one with the same payload is a replay that changes nothing, a
//! stored one with a changed payload is a conflict, and a later one is a
//! gap. A conflict or a gap refuses the whole transaction.

/// The decision for one incoming chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    /// The next chunk: append it and fold it into the derived totals.
    Append,
    /// Already stored with the same payload: change nothing.
    Replay,
    /// Already stored with a different payload: refuse.
    Conflict,
    /// Not the next chunk: refuse.
    Gap,
}

/// Decides an incoming chunk `incoming_seq` against the stored last
/// sequence (`has_stored` false when the session has none);
/// `same_digest` says whether the incoming payload equals the stored one at
/// that sequence (meaningful only for a stored sequence).
#[must_use]
pub fn admit(stored_last: u64, has_stored: bool, incoming_seq: u64, same_digest: bool) -> Admit {
    if !has_stored {
        return if incoming_seq == 1 {
            Admit::Append
        } else {
            Admit::Gap
        };
    }
    if incoming_seq <= stored_last {
        return if same_digest {
            Admit::Replay
        } else {
            Admit::Conflict
        };
    }
    match stored_last.checked_add(1) {
        Some(next) if incoming_seq == next => Admit::Append,
        Some(_) | None => Admit::Gap,
    }
}

/// Whether the derived totals' source watermark is consistent with the
/// stored chunks: never ahead of them.
#[must_use]
pub fn watermark_ok(derived: u64, stored: u64) -> bool {
    derived <= stored
}
