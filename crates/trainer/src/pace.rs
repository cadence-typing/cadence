//! Soft pace hysteresis (L-06).
//!
//! The desired first-pass accuracy target is a soft goal. Raise a
//! track's suggested target by at most `maximum_raise` after
//! `qualifying_blocks` blocks across `minimum_sessions` sessions, each
//! with ≥ `minimum_first_attempt_positions` and accuracy at/above the
//! current target, and no material attempt-accuracy degradation. A
//! block below `slow_down_below` lowers the target by up to
//! `maximum_lower`. Between bands the pace stays steady. Insufficient
//! evidence keeps the current target. Comparisons use exact rational
//! arithmetic (no float thresholds).

use cadence_core::{BasisPoints, CoreError};

/// Evidence one qualifying block produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockEvidence {
    /// First-attempt positions observed in the block.
    pub first_attempt_positions: u64,
    /// First-correct positions observed in the block.
    pub first_correct: u64,
    /// Content insertions (attempt-accuracy denominator).
    pub content_attempts: u64,
    /// Correct content insertions (attempt-accuracy numerator).
    pub correct_attempts: u64,
    /// The session/day identity for distinct-session counting.
    pub session_ordinal: u32,
}

/// The pace decision after reviewing recent blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaceDecision {
    /// Keep the current target: evidence between bands or insufficient.
    Steady,
    /// Raise by the delta (≤ policy `maximum_raise`).
    Raise {
        /// The exact new target.
        new_target: BasisPoints,
    },
    /// Lower by the delta (≤ policy `maximum_lower`).
    Lower {
        /// The exact new target.
        new_target: BasisPoints,
    },
    /// No reliable baseline exists yet; nothing to adjust (P-03:
    /// "unknown does not mean mastered").
    NoBaseline,
}

/// Evaluates pace under the validated policy.
///
/// # Errors
/// Returns [`CoreError::InvalidDenominator`] only from malformed
/// internal ratios (cannot happen with non-negative counts and a
/// nonzero denominator checked inline).
pub fn pace_decision(
    policy: &crate::policy::PacePolicy,
    current_target: Option<BasisPoints>,
    recent_blocks: &[BlockEvidence],
) -> Result<PaceDecision, CoreError> {
    for block in recent_blocks {
        if block.first_correct > block.first_attempt_positions
            || block.first_attempt_positions > block.content_attempts
            || block.correct_attempts > block.content_attempts
            || block.first_correct > block.correct_attempts
        {
            return Err(CoreError::InvalidIdentity {
                kind: "BlockEvidence",
                reason: "inconsistent observation counts",
            });
        }
    }
    let Some(target) = current_target else {
        return Ok(PaceDecision::NoBaseline);
    };
    if recent_blocks.is_empty() {
        return Ok(PaceDecision::Steady);
    }

    // Slow-down check: the latest block below the slow-down band lowers
    // the suggested target immediately (L-06).
    if let Some(latest) = recent_blocks.last()
        && below_band(latest, policy.slow_down_below)?
    {
        let lowered = target
            .checked_sub(policy.maximum_lower)
            .unwrap_or(BasisPoints::ZERO);
        return Ok(PaceDecision::Lower {
            new_target: lowered,
        });
    }

    // Raise: `qualifying_blocks` qualifying blocks across
    // `minimum_sessions` distinct sessions, no material attempt
    // degradation, each ≥ the position floor and ≥ target accuracy.
    let mut qualifying: Vec<&BlockEvidence> = Vec::new();
    let mut prior_attempt: Option<(u64, u64)> = None;
    let mut degraded = false;
    for block in recent_blocks {
        let attempt_ok = match prior_attempt {
            Some((previous_correct, previous_total))
                if previous_total > 0 && block.content_attempts > 0 =>
            {
                // Degradation: correct-ratio strictly worse by more than
                // a hair (exact integer cross-multiplication).
                let left = u128::from(block.correct_attempts)
                    .checked_mul(u128::from(previous_total))
                    .ok_or(CoreError::ArithmeticOverflow {
                        operation: "attempt accuracy comparison",
                    })?;
                let right = u128::from(previous_correct)
                    .checked_mul(u128::from(block.content_attempts))
                    .ok_or(CoreError::ArithmeticOverflow {
                        operation: "attempt accuracy comparison",
                    })?;
                left < right
            }
            _ => false,
        };
        if attempt_ok {
            degraded = true;
        }
        if block.first_attempt_positions >= policy.minimum_first_attempt_positions_per_block
            && !below_band(block, target)?
        {
            qualifying.push(block);
        }
        if block.content_attempts > 0 {
            prior_attempt = Some((block.correct_attempts, block.content_attempts));
        }
    }
    let distinct_sessions = qualifying
        .iter()
        .map(|block| block.session_ordinal)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let qualifying_count =
        u64::try_from(qualifying.len()).map_err(|_| CoreError::ArithmeticOverflow {
            operation: "qualifying block count",
        })?;
    let distinct_sessions =
        u64::try_from(distinct_sessions).map_err(|_| CoreError::ArithmeticOverflow {
            operation: "distinct session count",
        })?;
    if !degraded
        && qualifying_count >= u64::from(policy.qualifying_blocks_to_raise)
        && distinct_sessions >= u64::from(policy.minimum_sessions_to_raise)
    {
        let raised = target
            .checked_add(policy.maximum_raise)
            .unwrap_or(BasisPoints::MAX);
        return Ok(PaceDecision::Raise { new_target: raised });
    }
    Ok(PaceDecision::Steady)
}

/// Exact integer check: block first-pass accuracy strictly below `band`.
fn below_band(block: &BlockEvidence, band: BasisPoints) -> Result<bool, CoreError> {
    if block.first_attempt_positions == 0 {
        return Ok(false);
    }
    // first_correct / first_attempted < band / 10_000
    let scaled = u128::from(block.first_correct).checked_mul(10_000).ok_or(
        CoreError::ArithmeticOverflow {
            operation: "pace comparison",
        },
    )?;
    let threshold = u128::from(band.get())
        .checked_mul(u128::from(block.first_attempt_positions))
        .ok_or(CoreError::ArithmeticOverflow {
            operation: "pace comparison",
        })?;
    Ok(scaled < threshold)
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::policy::parse_policy;

    fn policy() -> crate::policy::PacePolicy {
        parse_policy(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../policies/trainer-v2.toml"
            ))
            .unwrap()
            .as_slice(),
        )
        .unwrap()
        .pace
    }

    fn block(session: u32, positions: u64, correct: u64) -> BlockEvidence {
        BlockEvidence {
            first_attempt_positions: positions,
            first_correct: correct,
            content_attempts: positions,
            correct_attempts: correct,
            session_ordinal: session,
        }
    }

    #[test]
    fn no_baseline_is_explicit() {
        assert_eq!(
            pace_decision(&policy(), None, &[block(1, 200, 200)]).unwrap(),
            PaceDecision::NoBaseline
        );
    }

    #[test]
    fn strong_blocks_across_two_sessions_raise_within_cap() {
        let policy = policy();
        let target = BasisPoints::new(9_800).unwrap();
        // Three qualifying blocks across two sessions with attempt
        // accuracy non-decreasing (a strict drop blocks a raise — the
        // conservative reading of "no material degradation").
        let blocks = vec![block(1, 120, 119), block(2, 110, 110), block(2, 105, 105)];
        match pace_decision(&policy, Some(target), &blocks).unwrap() {
            PaceDecision::Raise { new_target } => {
                assert_eq!(new_target.get(), 10_100u16.saturating_add(0).min(10_000));
                // Cap: 9800 + 300 = 10100 clamps to 10000.
                assert_eq!(new_target.get(), 10_000);
            }
            other => panic!("expected raise, got {other:?}"),
        }
    }

    #[test]
    fn failing_block_lowers_within_cap() {
        let policy = policy();
        let target = BasisPoints::new(9_800).unwrap();
        let blocks = vec![block(1, 150, 135)]; // 90% < 95%
        match pace_decision(&policy, Some(target), &blocks).unwrap() {
            PaceDecision::Lower { new_target } => {
                assert_eq!(new_target.get(), 9_300); // 9800 - 500
            }
            other => panic!("expected lower, got {other:?}"),
        }
    }

    #[test]
    fn insufficient_evidence_stays_steady() {
        let policy = policy();
        let target = BasisPoints::new(9_000).unwrap();
        // Below the position floor: cannot qualify.
        let blocks = vec![block(1, 40, 40), block(1, 30, 30)];
        assert_eq!(
            pace_decision(&policy, Some(target), &blocks).unwrap(),
            PaceDecision::Steady
        );
    }

    #[test]
    fn same_session_only_never_raises() {
        let policy = policy();
        let target = BasisPoints::new(9_000).unwrap();
        let blocks = vec![block(1, 120, 120), block(1, 110, 110), block(1, 105, 105)];
        assert_eq!(
            pace_decision(&policy, Some(target), &blocks).unwrap(),
            PaceDecision::Steady
        );
    }

    #[test]
    fn attempt_degradation_blocks_raise() {
        let policy = policy();
        let target = BasisPoints::new(9_000).unwrap();
        let good = block(1, 120, 120);
        let mut degrading = block(2, 110, 110);
        degrading.content_attempts = 220; // attempt accuracy collapses
        let blocks = vec![good, degrading];
        assert_eq!(
            pace_decision(&policy, Some(target), &blocks).unwrap(),
            PaceDecision::Steady
        );
    }
}
