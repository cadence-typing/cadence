//! Headless request admission (VP-05, #67): at-most-once application of
//! request ids over an ordered stream with a bounded response cache.
//!
//! A request id greater than every id applied so far is new and applies. A
//! smaller or equal id never applies again: with the same digest it replays
//! the cached response, with another digest it is a conflict, and once
//! evicted from the cache it has expired. None of these mutate.

/// What the response cache holds for a request id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// Not in the cache (never seen, or evicted).
    Miss,
    /// Cached with the same request digest.
    SameDigest,
    /// Cached with a different request digest.
    OtherDigest,
}

/// The admission decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admit {
    /// Apply the request: it mutates state.
    New,
    /// Return the cached response; no mutation.
    Replay,
    /// The id was used for another request; no mutation.
    Conflict,
    /// The id is old and its response was evicted; no mutation.
    Expired,
}

/// Admission of `id` after the newest applied id `last_new` (none yet:
/// every id is new).
#[must_use]
pub fn admit(last_new: Option<u64>, id: u64, lookup: Lookup) -> Admit {
    let fresh = match last_new {
        None => true,
        Some(last) => id > last,
    };
    if fresh {
        return Admit::New;
    }
    match lookup {
        Lookup::SameDigest => Admit::Replay,
        Lookup::OtherDigest => Admit::Conflict,
        Lookup::Miss => Admit::Expired,
    }
}

/// Whether a request's expected state revision allows it: always when it
/// names none, otherwise only when it equals the current revision.
#[must_use]
pub fn revision_ok(expected: u64, present: bool, current: u64) -> bool {
    !present || expected == current
}

/// Whether to evict the oldest cached response before inserting one more,
/// so that the cache never holds more than `capacity`.
#[must_use]
pub fn evict(len: usize, capacity: usize) -> bool {
    len >= capacity
}
