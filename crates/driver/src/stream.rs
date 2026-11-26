//! Ordered request admission (H-02, VP-05, #67). Every request id and its
//! digest go through the proved `cadence_kernel::request::admit`; only a
//! `New` request may mutate. The response cache that answers the lookup is
//! #68.

use cadence_core::ContentDigest;
use cadence_core::request::{Admit, Lookup, admit};

/// A client's request id: strictly increasing over a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(u64);

impl RequestId {
    /// Wraps a raw id.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw id.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The stream's admission state: the newest applied id.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stream {
    last_new: Option<RequestId>,
}

impl Stream {
    /// A stream that has applied nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self { last_new: None }
    }

    /// The newest applied id.
    #[must_use]
    pub const fn last_new(&self) -> Option<RequestId> {
        self.last_new
    }

    /// Admits `id` with request `digest`; `cached` answers what the
    /// response cache holds for the id (its request digest). On `New` the
    /// id becomes the newest applied id; nothing else changes the stream.
    pub fn admit(
        &mut self,
        id: RequestId,
        digest: ContentDigest,
        cached: impl FnOnce(RequestId) -> Option<ContentDigest>,
    ) -> Admit {
        let lookup = match cached(id) {
            None => Lookup::Miss,
            Some(stored) if stored == digest => Lookup::SameDigest,
            Some(_) => Lookup::OtherDigest,
        };
        let decision = admit(self.last_new.map(RequestId::get), id.get(), lookup);
        if decision == Admit::New {
            self.last_new = Some(id);
        }
        decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: u8) -> ContentDigest {
        ContentDigest::from_bytes([byte; 32])
    }

    #[test]
    fn increasing_ids_apply_and_old_ids_never_do() {
        let mut stream = Stream::new();
        let none = |_| None;
        assert_eq!(stream.admit(RequestId::new(1), digest(1), none), Admit::New);
        assert_eq!(stream.admit(RequestId::new(5), digest(2), none), Admit::New);
        assert_eq!(stream.last_new(), Some(RequestId::new(5)));
        // Equal and smaller ids are never new, whatever the cache says.
        assert_eq!(
            stream.admit(RequestId::new(5), digest(2), |_| Some(digest(2))),
            Admit::Replay
        );
        assert_eq!(
            stream.admit(RequestId::new(5), digest(3), |_| Some(digest(2))),
            Admit::Conflict
        );
        assert_eq!(
            stream.admit(RequestId::new(1), digest(1), none),
            Admit::Expired
        );
        assert_eq!(
            stream.last_new(),
            Some(RequestId::new(5)),
            "nothing but New moves the stream"
        );
        assert_eq!(stream.admit(RequestId::new(6), digest(4), none), Admit::New);
    }
}
