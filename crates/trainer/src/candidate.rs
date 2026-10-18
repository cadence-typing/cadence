//! Bounded eligible candidate selection (L-05, VK-04 / V-04).
//!
//! The scan visits a bounded materialized vector (≤ policy
//! `maximum_candidates_per_slot`) and returns the highest-utility
//! eligible candidate under the total order
//! `(utility, seeded_tiebreak, stable_id)`. The scan invariant (V-04):
//! the current best, if any, is eligible and at least as good as every
//! previously visited eligible candidate; the loop index strictly
//! advances and is bounded. Proves optimality only within the
//! materialized vector, not across all vocabulary.

use cadence_core::{BasisPoints, WordId};

/// Per-candidate utility inputs, all in basis points 0..=10_000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtilityInputs {
    /// Posterior error estimate weight input (0 = no errors).
    pub error: BasisPoints,
    /// Hesitation score relative to clean timing (0 = no hesitation).
    pub hesitation: BasisPoints,
    /// Corpus frequency / pack relevance.
    pub relevance: BasisPoints,
    /// Exploration/uncertainty term.
    pub uncertainty: BasisPoints,
}

/// One materialized candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// Stable corpus identity (tie-break by this on equal utility).
    pub id: WordId,
    /// Whether the candidate passes lane/content eligibility.
    pub eligible: bool,
    /// Utility inputs.
    pub inputs: UtilityInputs,
    /// Seeded tie-break value from the deterministic PRNG stream.
    pub tie_break: u64,
}

/// Weighted-sum failure modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateError {
    /// The materialized vector exceeds the policy bound.
    VectorTooLarge {
        /// Actual length.
        len: usize,
        /// Policy bound.
        bound: usize,
    },
}

impl std::fmt::Display for CandidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VectorTooLarge { len, bound } => {
                write!(f, "candidate vector {len} exceeds bound {bound}")
            }
        }
    }
}

impl std::error::Error for CandidateError {}

/// Computes the weighted utility in basis points. Bounded fixed-point:
/// each term ≤ 10_000 and the weight sum is the denominator, so the
/// result is in 0..=10_000 with no float and no overflow (terms are
/// widened to u64 before multiplication).
#[must_use]
pub fn utility(candidate: &Candidate, weights: &crate::policy::UtilityWeights) -> u64 {
    let denominator = weights.total().max(1);
    let term = |weight: BasisPoints, value: BasisPoints| -> u64 {
        u64::from(weight.get()).saturating_mul(u64::from(value.get()))
    };
    let numerator = term(weights.error, candidate.inputs.error)
        .saturating_add(term(weights.hesitation, candidate.inputs.hesitation))
        .saturating_add(term(weights.relevance, candidate.inputs.relevance))
        .saturating_add(term(weights.uncertainty, candidate.inputs.uncertainty));
    // denominator ≤ 40_000 and numerator ≤ 40_000 × 10_000 fits u64.
    numerator.checked_div(denominator).unwrap_or_default()
}

/// Selects the best eligible candidate from the bounded vector, or
/// `None` when no candidate is eligible (caller follows the documented
/// fallback order).
///
/// # Errors
/// Returns [`CandidateError::VectorTooLarge`] when the caller violated
/// the materialization bound.
pub fn scan_select(
    candidates: &[Candidate],
    weights: &crate::policy::UtilityWeights,
    bound: usize,
) -> Result<Option<WordId>, CandidateError> {
    if candidates.len() > bound {
        return Err(CandidateError::VectorTooLarge {
            len: candidates.len(),
            bound,
        });
    }
    let mut best: Option<(u64, u64, u64)> = None;
    // Loop invariant: `best`, if some, holds an eligible candidate at
    // least as good as every previously visited eligible candidate
    // under (utility, tie_break, id).
    for candidate in candidates {
        if !candidate.eligible {
            continue;
        }
        let utility_value = utility(candidate, weights);
        let key = (utility_value, candidate.tie_break, candidate.id.get());
        match best {
            None => best = Some(key),
            Some(current) if key > current => best = Some(key),
            Some(_) => {}
        }
    }
    // WordId::new only fails for 0, and candidate ids are validated at
    // construction; reconstruct via checked new without panics.
    Ok(match best {
        Some((_, _, id)) => WordId::new(id).ok(),
        None => None,
    })
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use crate::policy::UtilityWeights;
    use cadence_core::BasisPoints;

    fn weights() -> UtilityWeights {
        UtilityWeights {
            error: BasisPoints::new(5_000).unwrap(),
            hesitation: BasisPoints::new(2_500).unwrap(),
            relevance: BasisPoints::new(1_500).unwrap(),
            uncertainty: BasisPoints::new(1_000).unwrap(),
        }
    }

    fn candidate(id: u64, eligible: bool, error: u16) -> Candidate {
        Candidate {
            id: WordId::new(id).unwrap(),
            eligible,
            inputs: UtilityInputs {
                error: BasisPoints::new(error).unwrap(),
                hesitation: BasisPoints::ZERO,
                relevance: BasisPoints::new(2_000).unwrap(),
                uncertainty: BasisPoints::ZERO,
            },
            tie_break: 0,
        }
    }

    #[test]
    fn utility_bounded_and_error_dominant() {
        let weights = weights();
        let clean = candidate(1, true, 0);
        let weak = candidate(2, true, 9_000);
        assert!(utility(&clean, &weights) < utility(&weak, &weights));
        assert!(utility(&weak, &weights) <= 10_000);
    }

    #[test]
    fn selects_highest_utility_eligible() {
        let candidates = vec![
            candidate(1, true, 1_000),
            candidate(2, true, 9_000),
            candidate(3, true, 4_000),
            candidate(4, false, 10_000), // ineligible despite utility
        ];
        let selected = scan_select(&candidates, &weights(), 256).unwrap().unwrap();
        assert_eq!(selected.get(), 2);
    }

    #[test]
    fn none_when_no_eligible() {
        let candidates = vec![candidate(1, false, 9_000)];
        assert!(scan_select(&candidates, &weights(), 256).unwrap().is_none());
    }

    #[test]
    fn tie_break_then_stable_id_order() {
        let mut a = candidate(7, true, 5_000);
        let mut b = candidate(9, true, 5_000);
        a.tie_break = 10;
        b.tie_break = 99;
        let selected = scan_select(&[a, b], &weights(), 256).unwrap().unwrap();
        assert_eq!(selected.get(), 9, "higher tie_break wins on equal utility");
        // Equal tie_break: larger stable id wins (max by tuple).
        b.tie_break = 10;
        let selected = scan_select(&[a, b], &weights(), 256).unwrap().unwrap();
        assert_eq!(selected.get(), 9);
    }

    #[test]
    fn vector_bound_is_enforced() {
        let candidates: Vec<Candidate> = (1..=10).map(|id| candidate(id, true, 1_000)).collect();
        match scan_select(&candidates, &weights(), 4) {
            Err(CandidateError::VectorTooLarge { len, bound }) => {
                assert_eq!((len, bound), (10, 4));
            }
            other => panic!("expected bound error, got {other:?}"),
        }
    }

    #[test]
    fn empty_vector_is_no_candidate() {
        assert!(scan_select(&[], &weights(), 256).unwrap().is_none());
    }
}
