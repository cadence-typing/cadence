//! Comparable sessions (L-06, #77): two sessions are compared only on the
//! same basis: the same layout revision, the same content class, both live
//! (never a synthetic session) and the same set of headline tracks.

/// What a comparison rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Basis {
    /// The layout revision (`unspecified` history reads as its adopter's).
    pub layout: u64,
    /// The content class (standard, imported, code).
    pub content_class: u64,
    /// Recorded by the headless driver, not typed live.
    pub synthetic: bool,
    /// The headline tracks typed, as a bit set.
    pub tracks: u64,
}

/// Whether two sessions may be compared.
#[must_use]
pub fn same_basis(a: Basis, b: Basis) -> bool {
    a.layout == b.layout
        && a.content_class == b.content_class
        && !a.synthetic
        && !b.synthetic
        && a.tracks == b.tracks
}
