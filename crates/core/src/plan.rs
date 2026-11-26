//! Exact integer phase allocation (P-02). The decision is the proved
//! `cadence_kernel::plan::allocate` (Lean: the seconds sum exactly to the
//! budget, every phase keeps its minimum, an infeasible budget is reported
//! exactly, and among equal remainders the lower index gets the extra
//! second); this adapter owns the vectors and the typed errors.

use crate::CoreError;
use cadence_kernel::plan::{self, Allocation};

/// Allocates `budget` seconds across phases.
///
/// `minimums[i]` is each phase's validated minimum; `weights[i]` is its
/// proportional weight (session policies use default minus minimum).
///
/// # Errors
/// - [`CoreError::InvalidDenominator`] when there are no phases, the
///   slices differ in length, or seconds remain but every weight is zero.
/// - [`CoreError::InvalidIdentity`] when a minimum is zero.
/// - [`CoreError::CapacityExhausted`] when minimums sum above the
///   budget (infeasible; the caller rejects the configuration instead
///   of quietly dropping a track).
/// - [`CoreError::ArithmeticOverflow`] on overflow.
pub fn allocate(budget: u64, minimums: &[u64], weights: &[u64]) -> Result<Vec<u64>, CoreError> {
    if minimums.contains(&0) {
        return Err(CoreError::InvalidIdentity {
            kind: "PhaseAllocation",
            reason: "every enabled phase needs a positive minimum",
        });
    }
    let mut seconds = vec![0; minimums.len()];
    let mut remainders = vec![0; minimums.len()];
    match plan::allocate(budget, minimums, weights, &mut seconds, &mut remainders) {
        Allocation::Done => Ok(seconds),
        Allocation::Shape | Allocation::NoWeight => Err(CoreError::InvalidDenominator),
        Allocation::Infeasible => Err(CoreError::CapacityExhausted {
            limit: "phase minimums exceed the session budget",
        }),
        Allocation::Overflow => Err(CoreError::ArithmeticOverflow {
            operation: "phase allocation",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_minutes_still_includes_all_tracks() {
        // 300s with all minimums (230 total): every track keeps ≥ minimum.
        let minimums = [20u64, 60, 30, 45, 60, 15];
        let weights = [60u64, 300, 120, 180, 180, 60];
        let allocation = allocate(300, &minimums, &weights).unwrap();
        assert_eq!(allocation.iter().copied().sum::<u64>(), 300);
        assert!(allocation.iter().zip(minimums).all(|(v, m)| *v >= m));
    }

    #[test]
    fn infeasible_budget_is_explicit() {
        match allocate(299, &[100, 100, 100], &[1, 1, 1]) {
            Err(CoreError::CapacityExhausted { limit }) => assert!(limit.contains("minimums")),
            other => panic!("expected infeasible, got {other:?}"),
        }
    }

    #[test]
    fn deterministic_tie_break_by_index() {
        // 7 remaining across 3 equal weights: floors 2,2,2 leftover 1 →
        // index 0 gets the extra second.
        assert_eq!(allocate(10, &[1, 1, 1], &[1, 1, 1]).unwrap(), vec![4, 3, 3]);
        // 8 remaining: leftover 2 → indices 0 and 1.
        assert_eq!(allocate(11, &[1, 1, 1], &[1, 1, 1]).unwrap(), vec![4, 4, 3]);
    }

    #[test]
    fn largest_remainder_wins_before_index() {
        // remaining 10, weights 1,1,3 (total 5): exact 2,2,6, no leftover.
        assert_eq!(allocate(13, &[1, 1, 1], &[1, 1, 3]).unwrap(), vec![3, 3, 7]);
        // remaining 4, weights 1,2,4 (total 7): 4/7, 8/7, 16/7 → floors
        // 0,1,2 (leftover 1), remainders 4,1,2 → index 0 gets it.
        assert_eq!(allocate(7, &[1, 1, 1], &[1, 2, 4]).unwrap(), vec![2, 2, 3]);
    }

    #[test]
    fn errors_are_typed() {
        assert_eq!(allocate(10, &[], &[]), Err(CoreError::InvalidDenominator));
        assert_eq!(
            allocate(10, &[1, 1], &[1]),
            Err(CoreError::InvalidDenominator)
        );
        assert_eq!(
            allocate(10, &[1, 1], &[0, 0]),
            Err(CoreError::InvalidDenominator)
        );
        assert_eq!(allocate(2, &[1, 1], &[0, 0]), Ok(vec![1, 1]));
        assert!(matches!(
            allocate(10, &[0, 1], &[1, 1]),
            Err(CoreError::InvalidIdentity { .. })
        ));
        assert!(matches!(
            allocate(u64::MAX, &[1, 1], &[u64::MAX, 1]),
            Err(CoreError::ArithmeticOverflow { .. })
        ));
    }

    /// Exhaustive small-space comparison with the reference sort-based
    /// largest-remainder rule the trainer used before #34.
    #[test]
    fn matches_the_reference_largest_remainder_rule() {
        fn reference(budget: u64, minimums: &[u64], weights: &[u64]) -> Option<Vec<u64>> {
            let remaining = budget.checked_sub(minimums.iter().sum())?;
            let w: u64 = weights.iter().sum();
            if w == 0 {
                return (remaining == 0).then(|| minimums.to_vec());
            }
            let mut out: Vec<u64> = Vec::new();
            let mut rems = Vec::new();
            let mut allocated = 0;
            for (i, (m, wi)) in minimums.iter().zip(weights).enumerate() {
                let (q, r) = (
                    (remaining * wi).checked_div(w)?,
                    (remaining * wi).checked_rem(w)?,
                );
                out.push(m + q);
                rems.push((r, i));
                allocated += q;
            }
            rems.sort_by(|(l, li), (r, ri)| r.cmp(l).then(li.cmp(ri)));
            for (_, i) in rems
                .into_iter()
                .take(usize::try_from(remaining - allocated).ok()?)
            {
                out[i] += 1;
            }
            Some(out)
        }
        for budget in 0..40 {
            for a in 1..4 {
                for b in 1..4 {
                    for c in 1..4 {
                        for wa in 0..5 {
                            for wb in 0..5 {
                                for wc in 0..5 {
                                    let m = [a, b, c];
                                    let w = [wa, wb, wc];
                                    assert_eq!(
                                        allocate(budget, &m, &w).ok(),
                                        reference(budget, &m, &w),
                                        "{budget} {m:?} {w:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
