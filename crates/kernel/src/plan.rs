//! Exact integer phase allocation (P-02, VK-03, #34).
//!
//! Each enabled phase gets its minimum, then the seconds left over are
//! shared by weight: phase `i` gets `remaining * weight_i / weight_total`
//! (floor), and the seconds the floors leave unallocated go one each to
//! the phases with the largest remainders, the lower index first among
//! equal remainders. `cadence_core::plan::allocate` is the adapter: it owns
//! the vectors and the typed error; `trainer::plan::build_session_plan`
//! calls it.

/// Why an allocation was not made. On [`Allocation::Done`] the `seconds`
/// slice holds the result; on any other outcome its contents are
/// unspecified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allocation {
    /// Every phase has its seconds; they sum exactly to the budget.
    Done,
    /// There are no phases, or the slices differ in length.
    Shape,
    /// The minimums sum above the budget; the caller rejects the
    /// configuration instead of quietly dropping a phase.
    Infeasible,
    /// Seconds remain after the minimums but every weight is zero.
    NoWeight,
    /// A sum or product does not fit in `u64`.
    Overflow,
}

/// A checked sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sum {
    /// The sum fits.
    Fits(u64),
    /// The sum does not fit in `u64`.
    Overflow,
}

/// The checked sum of `values`.
#[must_use]
pub fn total(values: &[u64]) -> Sum {
    let mut i = 0;
    let mut sum: u64 = 0;
    while i < values.len() {
        if let Some(value) = values.get(i) {
            match sum.checked_add(*value) {
                None => return Sum::Overflow,
                Some(next) => sum = next,
            }
        }
        i = i.saturating_add(1);
    }
    Sum::Fits(sum)
}

/// Whether position `j` with remainder `r` comes after `after` in the
/// order of largest remainder first, lower index first among equals.
#[must_use]
pub fn after_pick(remainders: &[u64], after: Option<usize>, j: usize, r: u64) -> bool {
    match after {
        None => true,
        Some(a) => match remainders.get(a) {
            None => false,
            Some(ra) => r < *ra || (r == *ra && j > a),
        },
    }
}

/// Whether remainder `r` replaces the best so far: the first candidate
/// always does; a later one only with a strictly larger remainder, so the
/// lower index wins among equals.
#[must_use]
pub fn beats(best: Option<usize>, best_r: u64, r: u64) -> bool {
    match best {
        None => true,
        Some(_) => r > best_r,
    }
}

/// The next phase to receive a leftover second: the first in the order of
/// largest remainder first, lower index first among equals, among the
/// phases after `after` in that order. `None` when there is none.
#[must_use]
pub fn pick(remainders: &[u64], after: Option<usize>) -> Option<usize> {
    let mut j = 0;
    let mut best: Option<usize> = None;
    let mut best_r: u64 = 0;
    while j < remainders.len() {
        if let Some(r) = remainders.get(j) {
            let r = *r;
            let take = after_pick(remainders, after, j, r) && beats(best, best_r, r);
            if take {
                best = Some(j);
                best_r = r;
            }
        }
        j = j.saturating_add(1);
    }
    best
}

/// The minimum plus the weighted floor for every phase, with the floor's
/// remainder into `seconds` and `remainders`; the sum of the floors.
#[must_use]
pub fn shares(
    remaining: u64,
    divisor: u64,
    minimums: &[u64],
    weights: &[u64],
    seconds: &mut [u64],
    remainders: &mut [u64],
) -> Sum {
    let mut i = 0;
    let mut allocated: u64 = 0;
    while i < minimums.len() {
        let minimum = match minimums.get(i) {
            None => return Sum::Overflow,
            Some(m) => *m,
        };
        let weight = match weights.get(i) {
            None => return Sum::Overflow,
            Some(w) => *w,
        };
        let scaled = match remaining.checked_mul(weight) {
            None => return Sum::Overflow,
            Some(s) => s,
        };
        let floor = match scaled.checked_div(divisor) {
            None => return Sum::Overflow,
            Some(f) => f,
        };
        let rem = match scaled.checked_rem(divisor) {
            None => return Sum::Overflow,
            Some(r) => r,
        };
        let share = match minimum.checked_add(floor) {
            None => return Sum::Overflow,
            Some(s) => s,
        };
        allocated = match allocated.checked_add(floor) {
            None => return Sum::Overflow,
            Some(a) => a,
        };
        match seconds.get_mut(i) {
            None => return Sum::Overflow,
            Some(slot) => *slot = share,
        }
        match remainders.get_mut(i) {
            None => return Sum::Overflow,
            Some(slot) => *slot = rem,
        }
        i = i.saturating_add(1);
    }
    Sum::Fits(allocated)
}

/// Hands out `leftover` single seconds in remainder order; `false` when a
/// second could not be placed.
#[must_use]
pub fn hand_out(remainders: &[u64], seconds: &mut [u64], leftover: u64) -> bool {
    let mut k: u64 = 0;
    let mut prev: Option<usize> = None;
    while k < leftover {
        let next = match pick(remainders, prev) {
            None => return false,
            Some(n) => n,
        };
        match seconds.get_mut(next) {
            None => return false,
            Some(slot) => match slot.checked_add(1) {
                None => return false,
                Some(v) => *slot = v,
            },
        }
        prev = Some(next);
        k = match k.checked_add(1) {
            None => return false,
            Some(v) => v,
        };
    }
    true
}

/// Allocates `budget` seconds across the phases with `minimums` and
/// `weights` into `seconds`, using `remainders` as scratch. All four slices
/// must have the same, positive length.
#[must_use]
pub fn allocate(
    budget: u64,
    minimums: &[u64],
    weights: &[u64],
    seconds: &mut [u64],
    remainders: &mut [u64],
) -> Allocation {
    let n = minimums.len();
    if n == 0 || weights.len() != n || seconds.len() != n || remainders.len() != n {
        return Allocation::Shape;
    }
    let minimum_total = match total(minimums) {
        Sum::Overflow => return Allocation::Overflow,
        Sum::Fits(t) => t,
    };
    let remaining = match budget.checked_sub(minimum_total) {
        None => return Allocation::Infeasible,
        Some(r) => r,
    };
    let weight_total = match total(weights) {
        Sum::Overflow => return Allocation::Overflow,
        Sum::Fits(t) => t,
    };
    if weight_total == 0 && remaining != 0 {
        return Allocation::NoWeight;
    }
    let divisor = if weight_total == 0 { 1 } else { weight_total };
    let allocated = match shares(remaining, divisor, minimums, weights, seconds, remainders) {
        Sum::Overflow => return Allocation::Overflow,
        Sum::Fits(a) => a,
    };
    let leftover = match remaining.checked_sub(allocated) {
        None => return Allocation::Overflow,
        Some(l) => l,
    };
    if hand_out(remainders, seconds, leftover) {
        Allocation::Done
    } else {
        Allocation::Overflow
    }
}
