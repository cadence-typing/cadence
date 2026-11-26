//! Focus selection in integers (L-03/L-05, #38, #56).
//!
//! Evidence is fixed point: one undecayed attempt weighs [`WEIGHT_ONE`].
//! An older session's weight halves exactly every half-life and falls
//! linearly in between. The error estimate is the Beta-prior posterior mean
//! in basis points; hesitation is the skill's mean clean interval over its
//! group's, minus one. A candidate qualifies only with enough weighted
//! attempts and a signal above a threshold; [`pick`] takes the qualifying,
//! not yet chosen candidate with the greatest signal, the lower index first.
//! `cadence_trainer::learner::LearnerModel` builds the integer evidence and
//! calls these functions; there is no float on the focus path.

/// The weight of one undecayed attempt.
pub const WEIGHT_ONE: u64 = 65_536;

/// One basis-point scale.
pub const BASIS: u64 = 10_000;

/// `WEIGHT_ONE` halved `halvings` times (0 once it is used up).
#[must_use]
pub fn halved(halvings: u64) -> u64 {
    let mut base = WEIGHT_ONE;
    let mut i: u64 = 0;
    while i < halvings && base > 0 {
        base = base.checked_div(2).unwrap_or(0);
        i = i.saturating_add(1);
    }
    base
}

/// The weight of a session `age` sessions old: `WEIGHT_ONE` halved once
/// per whole half-life, then falling linearly toward the next halving.
/// 0 for a zero half-life or an unrepresentable product.
#[must_use]
pub fn decay_weight(age: u64, half_life: u64) -> u64 {
    let halvings = match age.checked_div(half_life) {
        None => return 0,
        Some(k) => k,
    };
    let rem = match age.checked_rem(half_life) {
        None => return 0,
        Some(r) => r,
    };
    let span = match half_life.checked_mul(2) {
        None => return 0,
        Some(s) => s,
    };
    let left = match span.checked_sub(rem) {
        None => return 0,
        Some(l) => l,
    };
    match halved(halvings).checked_mul(left) {
        None => 0,
        Some(scaled) => scaled.checked_div(span).unwrap_or(0),
    }
}

/// The posterior mean error rate in basis points under a
/// Beta(`alpha`, `beta`) prior: `(errors + alpha*ONE)*10000 /
/// (attempts + (alpha+beta)*ONE)` over weighted counts; 0 when it is
/// undefined or does not fit.
#[must_use]
pub fn posterior_bp(errors: u64, attempts: u64, alpha: u64, beta: u64) -> u64 {
    let prior_errors = match alpha.checked_mul(WEIGHT_ONE) {
        None => return 0,
        Some(p) => p,
    };
    let prior_total = match alpha.checked_add(beta) {
        None => return 0,
        Some(t) => match t.checked_mul(WEIGHT_ONE) {
            None => return 0,
            Some(p) => p,
        },
    };
    let numerator = match errors.checked_add(prior_errors) {
        None => return 0,
        Some(n) => match n.checked_mul(BASIS) {
            None => return 0,
            Some(m) => m,
        },
    };
    let denominator = match attempts.checked_add(prior_total) {
        None => return 0,
        Some(d) => d,
    };
    numerator.checked_div(denominator).unwrap_or(0)
}

/// The prior's share of the posterior in basis points:
/// `(alpha+beta)*ONE*10000 / (attempts + (alpha+beta)*ONE)`; sparse
/// evidence is uncertain, never mastered.
#[must_use]
pub fn uncertainty_bp(attempts: u64, alpha: u64, beta: u64) -> u64 {
    let prior_total = match alpha.checked_add(beta) {
        None => return BASIS,
        Some(t) => match t.checked_mul(WEIGHT_ONE) {
            None => return BASIS,
            Some(p) => p,
        },
    };
    let numerator = match prior_total.checked_mul(BASIS) {
        None => return BASIS,
        Some(n) => n,
    };
    match attempts.checked_add(prior_total) {
        None => 0,
        Some(d) => numerator.checked_div(d).unwrap_or(BASIS),
    }
}

/// Relative hesitation in basis points: 0 below `minimum` weighted timed
/// samples; otherwise the skill's mean interval over the group's mean,
/// minus one, clamped to `0..=10_000`.
#[must_use]
pub fn hesitation_bp(
    skill_timed: u64,
    skill_micros: u64,
    group_timed: u64,
    group_micros: u64,
    minimum: u64,
) -> u64 {
    if skill_timed < minimum {
        return 0;
    }
    // Means in microseconds (weights cancel), then the ratio in basis points.
    let skill_mean = match skill_micros.checked_div(skill_timed) {
        None => return 0,
        Some(m) => m,
    };
    let group_mean = match group_micros.checked_div(group_timed) {
        None => return 0,
        Some(m) => m,
    };
    let ratio = match skill_mean.checked_mul(BASIS) {
        None => return BASIS,
        Some(n) => match n.checked_div(group_mean) {
            None => return 0,
            Some(r) => r,
        },
    };
    let excess = ratio.saturating_sub(BASIS);
    if excess > BASIS { BASIS } else { excess }
}

/// One candidate skill, in integers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// Weighted attempts.
    pub attempts: u64,
    /// Weighted errors.
    pub errors: u64,
    /// Posterior error rate (basis points).
    pub posterior: u64,
    /// The group's observed error rate (basis points).
    pub group_rate: u64,
    /// Relative hesitation (basis points).
    pub hesitation: u64,
}

/// What makes a candidate a focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    /// Weighted attempts a candidate needs.
    pub attempts: u64,
    /// Error excess over the group rate that counts.
    pub excess: u64,
    /// Hesitation that counts.
    pub hesitation: u64,
    /// Weight of the error excess.
    pub weight_error: u64,
    /// Weight of the hesitation.
    pub weight_hesitation: u64,
}

/// A weighted term: both factors are basis points, capped at 10,000, so
/// the product never exceeds 10^8 (the cap changes no `BasisPoints` input).
#[must_use]
pub fn weighted(weight: u64, value: u64) -> u64 {
    let w = if weight > BASIS { BASIS } else { weight };
    let v = if value > BASIS { BASIS } else { value };
    w.checked_mul(v).unwrap_or(0)
}

/// `value` when it reaches `threshold`, else 0.
#[must_use]
pub fn above(value: u64, threshold: u64) -> u64 {
    if value >= threshold { value } else { 0 }
}

/// The candidate's focus signal: the weighted error excess over the group
/// rate (only with errors) and the weighted hesitation, each counted only
/// at or above its threshold.
#[must_use]
pub fn signal(c: Candidate, t: Thresholds) -> u64 {
    let excess = if c.errors > 0 {
        c.posterior.saturating_sub(c.group_rate)
    } else {
        0
    };
    let e = weighted(t.weight_error, above(excess, t.excess));
    let h = weighted(t.weight_hesitation, above(c.hesitation, t.hesitation));
    e.saturating_add(h)
}

/// Whether a candidate may become a focus: enough weighted attempts and a
/// positive signal.
#[must_use]
pub fn qualifies(c: Candidate, t: Thresholds) -> bool {
    c.attempts >= t.attempts && signal(c, t) > 0
}

/// Whether candidate `i` is already chosen (a missing flag counts as
/// chosen, so a short flag list never admits a candidate twice).
#[must_use]
pub fn taken(chosen: &[bool], i: usize) -> bool {
    match chosen.get(i) {
        None => true,
        Some(b) => *b,
    }
}

/// Whether signal `s` replaces the best so far: the first qualifying
/// candidate always does, a later one only when strictly greater.
#[must_use]
pub fn beats(best: Option<usize>, best_signal: u64, s: u64) -> bool {
    match best {
        None => true,
        Some(_) => s > best_signal,
    }
}

/// The qualifying candidate not yet `chosen` with the greatest signal, the
/// lower index first among equals; `None` when there is none.
#[must_use]
pub fn pick(candidates: &[Candidate], chosen: &[bool], t: Thresholds) -> Option<usize> {
    let mut i = 0;
    let mut best: Option<usize> = None;
    let mut best_signal: u64 = 0;
    while i < candidates.len() {
        if let Some(c) = candidates.get(i) {
            let s = signal(*c, t);
            if !taken(chosen, i) && qualifies(*c, t) && beats(best, best_signal, s) {
                best = Some(i);
                best_signal = s;
            }
        }
        i = i.saturating_add(1);
    }
    best
}
