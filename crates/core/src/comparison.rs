//! Comparable sessions (#77): the proved
//! `cadence_kernel::comparison::same_basis` (Lean: never across layouts,
//! content classes or with a synthetic session; comparable exactly when all
//! hold) and the mapping of a session's facts onto its basis.

pub use cadence_kernel::comparison::{Basis, same_basis};

use crate::alphabet::Layout;
use crate::progress::{ContentClass, Track};

const fn layout_code(layout: Option<Layout>) -> u64 {
    match layout {
        None => 0,
        Some(Layout::Us) => 1,
        Some(Layout::Uk) => 2,
        Some(Layout::Custom) => 3,
    }
}

const fn track_bit(track: Track) -> u64 {
    match track {
        Track::WarmUp => 1,
        Track::Words => 1 << 1,
        Track::NumberRow => 1 << 2,
        Track::Numpad => 1 << 3,
        Track::Prose => 1 << 4,
        Track::Review => 1 << 5,
        Track::Benchmark => 1 << 6,
    }
}

/// A session's basis: its layout (`None` when unrecorded and unadopted),
/// content class, provenance and the headline tracks it typed.
#[must_use]
pub fn basis(
    layout: Option<Layout>,
    content_class: ContentClass,
    synthetic: bool,
    tracks: impl IntoIterator<Item = Track>,
) -> Basis {
    Basis {
        layout: layout_code(layout),
        content_class: u64::from(content_class.code()),
        synthetic,
        tracks: tracks
            .into_iter()
            .filter(|track| track.in_headline())
            .fold(0, |bits, track| bits | track_bit(track)),
    }
}
