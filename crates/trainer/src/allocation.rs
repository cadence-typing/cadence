//! Exact integer phase allocation (P-02, VK-03 / V-04 budget shape).
//!
//! Take validated nonnegative minimum allocations whose sum is ≤ budget
//! (else infeasible). Allocate the remaining integer seconds by
//! quotient/remainder against positive integer weights: the sum of
//! floors is ≤ the remainder budget; the unallocated part is less than
//! the count of positive-weight phases. Add one second to that many
//! phases in descending fractional remainder, stable phase-index order
//! on ties. Deterministic; exact sum; minimums preserved; no overflow.
//!
//! VK-03 proof target: exact sum, minimum preservation, determinism,
//! overflow-safe. Property tests below pin the executable contract.

use cadence_core::CoreError;

/// Allocates `budget` seconds across phases.
///
/// `minimums[i]` is each phase's validated minimum; `weights[i]` the
/// proportional weight (the default phase seconds serve as weights).
///
/// # Errors
/// - [`CoreError::InvalidDenominator`] when there are no phases.
/// - [`CoreError::CapacityExhausted`] when minimums sum above the
///   budget (infeasible; the caller rejects the configuration instead
///   of quietly dropping a track, P-02).
/// - [`CoreError::ArithmeticOverflow`] on intermediate overflow.
pub fn allocate(budget: u64, minimums: &[u64], weights: &[u64]) -> Result<Vec<u64>, CoreError> {
    if minimums.is_empty() || weights.is_empty() || minimums.len() != weights.len() {
        return Err(CoreError::InvalidDenominator);
    }
    let phases = minimums.len();
    let mut minimum_total: u128 = 0;
    for minimum in minimums {
        if *minimum == 0 {
            return Err(CoreError::InvalidIdentity {
                kind: "PhaseAllocation",
                reason: "every enabled phase needs a positive minimum",
            });
        }
        minimum_total += u128::from(*minimum);
    }
    if minimum_total > u128::from(budget) {
        return Err(CoreError::CapacityExhausted {
            limit: "phase minimums exceed the session budget",
        });
    }
    let weight_total: u128 = weights.iter().map(|weight| u128::from(*weight)).sum();
    if weight_total == 0 {
        return Err(CoreError::InvalidDenominator);
    }
    let remaining = u128::from(budget) - minimum_total;

    // Largest-remainder apportionment over the remaining budget with the
    // weights; scale in u128 so products cannot overflow.
    let mut floors = vec![0u128; phases];
    let mut remainders = vec![0u128; phases];
    let mut allocated: u128 = 0;
    for index in 0..phases {
        let scaled = remaining * u128::from(weights[index]);
        let floor = scaled / weight_total;
        floors[index] = floor;
        remainders[index] = scaled % weight_total;
        allocated += floor;
    }
    let leftover = remaining - allocated;
    // leftover < phases (standard largest-remainder property); enforce
    // the bound defensively anyway.
    let increments = leftover.min(u128::try_from(phases).unwrap_or(u128::MAX));

    // Descending fractional remainder, stable index order on ties.
    let mut order: Vec<usize> = (0..phases).collect();
    order.sort_by(|left, right| {
        remainders[*right]
            .cmp(&remainders[*left])
            .then(left.cmp(right))
    });
    let mut extras = vec![0u128; phases];
    for index in order
        .iter()
        .take(usize::try_from(increments).unwrap_or(usize::MAX))
    {
        extras[*index] = 1;
    }

    let mut result = Vec::with_capacity(phases);
    for index in 0..phases {
        let total = u128::from(minimums[index]) + floors[index] + extras[index];
        result.push(
            u64::try_from(total).map_err(|_| CoreError::ArithmeticOverflow {
                operation: "PhaseAllocation::allocate",
            })?,
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// The documented default plan for the default budget is the pinned
    /// `phase_seconds` table itself (exact for 900s); custom budgets
    /// apportion with minimums + largest remainder (P-02).
    #[test]
    fn default_policy_allocates_documented_split() {
        let policy = crate::policy::parse_policy(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../policies/trainer-v1.toml"
            ))
            .unwrap()
            .as_slice(),
        )
        .unwrap();
        // The pinned default table is exact for the default budget.
        assert_eq!(
            policy.session.phase_seconds,
            vec![60, 300, 120, 180, 180, 60]
        );
        assert_eq!(
            policy.session.phase_seconds.iter().copied().sum::<u64>(),
            policy.session.default_budget_seconds
        );
        for (index, seconds) in policy.session.phase_seconds.iter().enumerate() {
            assert!(
                *seconds >= policy.session.phase_minimum_seconds[index],
                "default table respects phase minimum {index}"
            );
        }
        // Custom budgets apportion exactly (P-02 presets 5..30 minutes).
        for budget in (300..1800).step_by(11) {
            let allocation = allocate(
                budget,
                &policy.session.phase_minimum_seconds,
                &policy.session.phase_seconds,
            )
            .unwrap();
            assert_eq!(allocation.iter().copied().sum::<u64>(), budget);
        }
    }

    #[test]
    fn five_minutes_still_includes_all_tracks() {
        // 300s with all minimums (230 total): every track keeps ≥ minimum.
        let minimums = [20u64, 60, 30, 45, 60, 15];
        let weights = [60u64, 300, 120, 180, 180, 60];
        let allocation = allocate(300, &minimums, &weights).unwrap();
        assert_eq!(allocation.iter().copied().sum::<u64>(), 300);
        assert!(allocation.iter().all(|value| *value > 0));
    }

    #[test]
    fn infeasible_budget_is_explicit() {
        let minimums = [100u64, 100, 100];
        let weights = [1u64, 1, 1];
        match allocate(299, &minimums, &weights) {
            Err(CoreError::CapacityExhausted { limit }) => {
                assert!(limit.contains("minimums"));
            }
            other => panic!("expected infeasible, got {other:?}"),
        }
    }

    #[test]
    fn deterministic_tie_break_by_index() {
        // Equal weights and remainders: extra seconds go to the lowest
        // indices first, stably.
        let first = allocate(10, &[1, 1, 1], &[1, 1, 1]).unwrap();
        let second = allocate(10, &[1, 1, 1], &[1, 1, 1]).unwrap();
        assert_eq!(first, second);
        // 7 remaining across 3 equal weights: floors 2,2,2 leftover 1 →
        // index 0 gets the extra second.
        assert_eq!(first, vec![4, 3, 3]);
    }
}
