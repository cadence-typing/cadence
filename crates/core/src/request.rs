//! Headless request admission (VP-05, #67). The decisions are the proved
//! `cadence_kernel::request::{admit, revision_ok, evict}` (Lean: new exactly
//! when the id is greater than every applied id; an old id replays,
//! conflicts or has expired and never mutates; a stale expected revision is
//! refused; evicting when full keeps the cache bounded).
pub use cadence_kernel::request::{Admit, Lookup, admit, evict, revision_ok};
