//! Cycling through a phase's prepared list (#39, provides #7).
//! `engine::practice::finish_exercise` advances with [`advance`] and keeps
//! the `wrapped` flag for the phase.

/// The next index and whether the list wrapped to its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Advance {
    /// The next index.
    pub next: usize,
    /// The list started over.
    pub wrapped: bool,
}

/// The index after `index` in a list of `count`: the next one, or the
/// first one when `index` was the last.
#[must_use]
pub fn advance(index: usize, count: usize) -> Advance {
    let n = index.saturating_add(1);
    if n >= count {
        Advance {
            next: 0,
            wrapped: true,
        }
    } else {
        Advance {
            next: n,
            wrapped: false,
        }
    }
}
