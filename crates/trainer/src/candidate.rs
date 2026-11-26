//! Bounded eligible candidate selection (L-05, VK-04 / V-04).
//!
//! The scan visits a bounded materialized vector (≤ policy
//! `maximum_candidates_per_slot`) and returns the highest-utility
//! eligible candidate under the total order
//! `(utility, seeded_tiebreak, stable_id)`. The scan invariant (V-04):
//! the current best, if any, is eligible and at least as good as every
//! previously visited eligible candidate; the loop index strictly
//! advances and is bounded. Proves optimality only within the
//! materialized vector, not across all vocabulary. The decisions are the
//! proved `cadence_kernel::select` (VK-04, #35); this module adapts the
//! typed candidates.

use cadence_core::{BasisPoints, WordId, select};

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

fn terms(t: UtilityInputs) -> select::Terms {
    select::Terms {
        error: u64::from(t.error.get()),
        hesitation: u64::from(t.hesitation.get()),
        relevance: u64::from(t.relevance.get()),
        uncertainty: u64::from(t.uncertainty.get()),
    }
}

fn weight_terms(w: &crate::policy::UtilityWeights) -> select::Terms {
    terms(UtilityInputs {
        error: w.error,
        hesitation: w.hesitation,
        relevance: w.relevance,
        uncertainty: w.uncertainty,
    })
}

/// The weighted utility in basis points: the proved
/// `cadence_kernel::select::utility`, within `0..=10_000`.
#[must_use]
pub fn utility(candidate: &Candidate, weights: &crate::policy::UtilityWeights) -> u64 {
    select::utility(terms(candidate.inputs), weight_terms(weights))
}

/// Selects the best eligible candidate from the bounded vector with the
/// proved `cadence_kernel::select::scan`, or `None` when no candidate is
/// eligible (caller follows the documented fallback order).
///
/// # Errors
/// Returns [`CandidateError::VectorTooLarge`] when the caller violated
/// the materialization bound.
pub fn scan_select(
    candidates: &[Candidate],
    weights: &crate::policy::UtilityWeights,
    bound: usize,
) -> Result<Option<WordId>, CandidateError> {
    let kernel: Vec<select::Candidate> = candidates
        .iter()
        .map(|c| select::Candidate {
            id: c.id.get(),
            eligible: c.eligible,
            inputs: terms(c.inputs),
            tie_break: c.tie_break,
        })
        .collect();
    match select::scan(&kernel, weight_terms(weights), bound) {
        select::Scan::TooLarge => Err(CandidateError::VectorTooLarge {
            len: candidates.len(),
            bound,
        }),
        select::Scan::Empty => Ok(None),
        select::Scan::Best(index) => Ok(candidates.get(index).map(|c| c.id)),
    }
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

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
    /// The kernel scan agrees with the pre-#35 trainer implementation
    /// (saturating weighted sum, max over `(utility, tie_break, id)`, first
    /// among equals) on a pseudo-random sweep.
    #[test]
    fn kernel_scan_matches_the_previous_rule() {
        fn reference(candidates: &[Candidate], w: &UtilityWeights) -> Option<u64> {
            let term = |a: BasisPoints, b: BasisPoints| u64::from(a.get()) * u64::from(b.get());
            let mut best: Option<(u64, u64, u64)> = None;
            for c in candidates.iter().filter(|c| c.eligible) {
                let num = term(w.error, c.inputs.error)
                    + term(w.hesitation, c.inputs.hesitation)
                    + term(w.relevance, c.inputs.relevance)
                    + term(w.uncertainty, c.inputs.uncertainty);
                let key = (
                    num.checked_div(w.total().max(1)).unwrap(),
                    c.tie_break,
                    c.id.get(),
                );
                if best.is_none_or(|b| key > b) {
                    best = Some(key);
                }
            }
            best.map(|(_, _, id)| id)
        }
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = |m: u64| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (state >> 33) % m
        };
        for round in 0..2_000 {
            let bp = |v: u64| BasisPoints::new(u16::try_from(v).unwrap()).unwrap();
            let w = UtilityWeights {
                error: bp(next(10_001)),
                hesitation: bp(next(10_001)),
                relevance: bp(next(3) * 5_000),
                uncertainty: bp(if round % 7 == 0 { 0 } else { next(10_001) }),
            };
            let n = next(12);
            let candidates: Vec<Candidate> = (0..n)
                .map(|i| Candidate {
                    id: WordId::new(1 + next(5)).unwrap(),
                    eligible: next(4) != 0,
                    inputs: UtilityInputs {
                        error: bp(next(3) * 5_000),
                        hesitation: bp(next(10_001)),
                        relevance: bp(next(2) * 10_000),
                        uncertainty: bp(i % 2 * 5_000),
                    },
                    tie_break: next(3),
                })
                .collect();
            assert_eq!(
                scan_select(&candidates, &w, 16).unwrap().map(WordId::get),
                reference(&candidates, &w),
                "round {round}"
            );
        }
    }
}
