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
/// `minimums[i]` is each phase's validated minimum; `weights[i]` is its
/// proportional weight (session policies use default minus minimum).
///
/// # Errors
/// - [`CoreError::InvalidDenominator`] when there are no phases.
/// - [`CoreError::CapacityExhausted`] when minimums sum above the
///   budget (infeasible; the caller rejects the configuration instead
///   of quietly dropping a track, P-02).
/// - [`CoreError::ArithmeticOverflow`] on intermediate overflow.
pub fn allocate(budget: u64, minimums: &[u64], weights: &[u64]) -> Result<Vec<u64>, CoreError> {
    let overflow = || CoreError::ArithmeticOverflow {
        operation: "phase allocation",
    };
    if minimums.is_empty() || minimums.len() != weights.len() {
        return Err(CoreError::InvalidDenominator);
    }
    if minimums.contains(&0) {
        return Err(CoreError::InvalidIdentity {
            kind: "PhaseAllocation",
            reason: "every enabled phase needs a positive minimum",
        });
    }
    let minimum_total = minimums.iter().try_fold(0u64, |sum, value| {
        sum.checked_add(*value).ok_or_else(overflow)
    })?;
    let remaining = budget
        .checked_sub(minimum_total)
        .ok_or(CoreError::CapacityExhausted {
            limit: "phase minimums exceed the session budget",
        })?;
    if remaining == 0 {
        return Ok(minimums.to_vec());
    }
    let weight_total = weights.iter().try_fold(0u128, |sum, value| {
        sum.checked_add(u128::from(*value)).ok_or_else(overflow)
    })?;
    if weight_total == 0 {
        return Err(CoreError::InvalidDenominator);
    }
    let mut result = Vec::with_capacity(minimums.len());
    let mut remainders = Vec::with_capacity(minimums.len());
    let mut allocated = 0u64;
    for (index, (minimum, weight)) in minimums.iter().zip(weights).enumerate() {
        let scaled = u128::from(remaining)
            .checked_mul(u128::from(*weight))
            .ok_or_else(overflow)?;
        let floor = u64::try_from(scaled.checked_div(weight_total).ok_or_else(overflow)?)
            .map_err(|_| overflow())?;
        result.push(minimum.checked_add(floor).ok_or_else(overflow)?);
        remainders.push((
            scaled.checked_rem(weight_total).ok_or_else(overflow)?,
            index,
        ));
        allocated = allocated.checked_add(floor).ok_or_else(overflow)?;
    }
    let leftover = usize::try_from(remaining.checked_sub(allocated).ok_or_else(overflow)?)
        .map_err(|_| overflow())?;
    if leftover > result.len() {
        return Err(overflow());
    }
    remainders.sort_by(|(left, left_index), (right, right_index)| {
        right.cmp(left).then(left_index.cmp(right_index))
    });
    for (_, index) in remainders.into_iter().take(leftover) {
        let value = result.get_mut(index).ok_or_else(overflow)?;
        *value = value.checked_add(1).ok_or_else(overflow)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// Default and custom budgets share minimum-plus-residual apportionment.
    #[test]
    fn default_policy_allocates_documented_split() {
        let policy = crate::policy::parse_policy(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../policies/trainer-v2.toml"
            ))
            .unwrap()
            .as_slice(),
        )
        .unwrap();
        assert_eq!(
            policy.session.default_allocation().unwrap(),
            policy.session.phase_seconds
        );
        // The pinned default table is exact for the default budget.
        assert_eq!(
            policy.session.phase_seconds,
            vec![60, 300, 120, 180, 180, 60]
        );
        assert_eq!(
            policy.session.phase_seconds.iter().copied().sum::<u64>(),
            policy.session.default_budget_seconds
        );
        for (seconds, minimum) in policy
            .session
            .phase_seconds
            .iter()
            .zip(&policy.session.phase_minimum_seconds)
        {
            assert!(seconds >= minimum);
        }
        // Custom budgets apportion exactly (P-02 presets 5..30 minutes).
        for budget in (300..1800).step_by(11) {
            let allocation = allocate(
                budget,
                &policy.session.phase_minimum_seconds,
                &policy.session.allocation_weights().unwrap(),
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
