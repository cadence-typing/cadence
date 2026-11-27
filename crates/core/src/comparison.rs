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
///
/// The `operators` class describes only the numpad entries (#101), so it
/// enters the basis when the compared tracks include the numpad or when no
/// tracks are named (a whole-session comparison); words, number-row and
/// prose comparisons carry `standard` and stay unbroken by the setting.
#[must_use]
pub fn basis(
    layout: Option<Layout>,
    content_class: ContentClass,
    synthetic: bool,
    tracks: impl IntoIterator<Item = Track>,
) -> Basis {
    let tracks = tracks
        .into_iter()
        .filter(|track| track.in_headline())
        .fold(0, |bits, track| bits | track_bit(track));
    let numpad_compared = tracks == 0 || tracks & track_bit(Track::Numpad) != 0;
    let content_class = match content_class {
        ContentClass::Operators if !numpad_compared => ContentClass::Standard,
        ContentClass::Operators => ContentClass::Operators,
        ContentClass::Standard => ContentClass::Standard,
        ContentClass::Imported => ContentClass::Imported,
        ContentClass::Code => ContentClass::Code,
    };
    Basis {
        layout: layout_code(layout),
        content_class: u64::from(content_class.code()),
        synthetic,
        tracks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #101: the operators class splits numpad and whole-session
    /// comparisons only; other tracks compare across it.
    #[test]
    fn operators_split_only_comparisons_that_include_the_numpad() {
        let at = |class, tracks: &[Track]| basis(None, class, false, tracks.iter().copied());
        let (std, ops) = (ContentClass::Standard, ContentClass::Operators);
        for track in [Track::Words, Track::NumberRow, Track::Prose] {
            assert!(
                same_basis(at(std, &[track]), at(ops, &[track])),
                "{track:?}"
            );
        }
        assert!(!same_basis(
            at(std, &[Track::Numpad]),
            at(ops, &[Track::Numpad])
        ));
        assert!(!same_basis(at(std, &[]), at(ops, &[])));
        let all = [Track::Words, Track::Numpad];
        assert!(!same_basis(at(std, &all), at(ops, &all)));
        assert!(same_basis(at(ops, &all), at(ops, &all)));
        let imported = ContentClass::Imported;
        assert!(!same_basis(
            at(std, &[Track::Words]),
            at(imported, &[Track::Words])
        ));
    }
}
