//! Bounded candidate selection (L-05, VK-04, #35).
//!
//! Each candidate's utility is the weighted mean of four basis-point terms
//! (error, hesitation, relevance, uncertainty); the scan returns the
//! eligible candidate with the greatest `(utility, tie_break, id)`, the
//! first one among equal keys, from a vector no longer than the policy
//! bound. Optimality is within the materialized vector only.
//! `cadence_trainer::candidate::scan_select` is the adapter and
//! `trainer::plan_words::WordSelector::select` its caller.

/// One basis-point scale: 10,000 is 100%.
pub const BASIS: u64 = 10_000;

/// The four utility terms, or their weights, in basis points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Terms {
    /// Posterior error estimate (0 = no errors).
    pub error: u64,
    /// Hesitation relative to clean timing.
    pub hesitation: u64,
    /// Corpus relevance.
    pub relevance: u64,
    /// Exploration/uncertainty.
    pub uncertainty: u64,
}

/// One materialized candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// Stable corpus identity (the last tie-break).
    pub id: u64,
    /// Whether the candidate passes lane and content eligibility.
    pub eligible: bool,
    /// Utility inputs.
    pub inputs: Terms,
    /// Seeded tie-break from the plan's deterministic stream.
    pub tie_break: u64,
}

/// The scan's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scan {
    /// The vector is longer than the bound: nothing was scanned.
    TooLarge,
    /// No candidate is eligible.
    Empty,
    /// The index of the best eligible candidate.
    Best(usize),
}

/// A term clamped to the basis-point scale (inputs are `BasisPoints`, so
/// this never changes a value in production; it makes the arithmetic total).
#[must_use]
pub fn cap(v: u64) -> u64 {
    if v > BASIS { BASIS } else { v }
}

/// `a * b`, or 0 on overflow (unreachable for capped terms).
#[must_use]
pub fn mul(a: u64, b: u64) -> u64 {
    a.checked_mul(b).unwrap_or(0)
}

/// `a + b`, or 0 on overflow (unreachable for capped terms).
#[must_use]
pub fn add(a: u64, b: u64) -> u64 {
    a.checked_add(b).unwrap_or(0)
}

/// The weighted mean of the terms in basis points: `sum(w * t) / sum(w)`
/// (0 when every weight is 0), always within `0..=10_000`.
#[must_use]
pub fn utility(inputs: Terms, weights: Terms) -> u64 {
    let we = cap(weights.error);
    let wh = cap(weights.hesitation);
    let wr = cap(weights.relevance);
    let wu = cap(weights.uncertainty);
    let e = mul(we, cap(inputs.error));
    let h = mul(wh, cap(inputs.hesitation));
    let r = mul(wr, cap(inputs.relevance));
    let u = mul(wu, cap(inputs.uncertainty));
    let numerator = add(add(add(e, h), r), u);
    let total = add(add(add(we, wh), wr), wu);
    numerator.checked_div(total).unwrap_or(0)
}

/// Whether key `(u, t, id)` is strictly greater than `(bu, bt, bid)` in
/// lexicographic order.
#[must_use]
pub fn greater(u: u64, t: u64, id: u64, bu: u64, bt: u64, bid: u64) -> bool {
    u > bu || (u == bu && (t > bt || (t == bt && id > bid)))
}

/// Whether a candidate with key `(u, t, id)` replaces the best so far: the
/// first eligible one always does, a later one only with a strictly greater
/// key, so the first wins among equal keys.
#[must_use]
pub fn beats(best: Option<usize>, u: u64, t: u64, id: u64, bu: u64, bt: u64, bid: u64) -> bool {
    match best {
        None => true,
        Some(_) => greater(u, t, id, bu, bt, bid),
    }
}

/// The best eligible candidate of a vector of at most `bound` candidates.
#[must_use]
pub fn scan(candidates: &[Candidate], weights: Terms, bound: usize) -> Scan {
    if candidates.len() > bound {
        return Scan::TooLarge;
    }
    let mut i = 0;
    let mut best: Option<usize> = None;
    let mut bu: u64 = 0;
    let mut bt: u64 = 0;
    let mut bid: u64 = 0;
    while i < candidates.len() {
        if let Some(c) = candidates.get(i) {
            let u = utility(c.inputs, weights);
            let take = c.eligible && beats(best, u, c.tie_break, c.id, bu, bt, bid);
            if take {
                best = Some(i);
                bu = u;
                bt = c.tie_break;
                bid = c.id;
            }
        }
        i = i.saturating_add(1);
    }
    match best {
        None => Scan::Empty,
        Some(b) => Scan::Best(b),
    }
}
