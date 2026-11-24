//! Exact ratios and the pace formulas (L-02). Ratios are `(numerator,
//! denominator)` pairs, never reduced; an empty measurement is `None`,
//! never infinity or 0%.

/// The conventional five-unit word length in the WPM definition.
pub const WPM_UNITS_PER_WORD: u128 = 5;

/// Microseconds per minute, so pace stays in integer arithmetic.
pub const MICROSECONDS_PER_MINUTE: u128 = 60_000_000;

/// Basis points in 100%.
pub const BASIS_POINTS_FULL: u64 = 10_000;

/// An exact non-negative ratio with a nonzero denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fraction {
    /// Numerator.
    pub num: u128,
    /// Denominator, never zero.
    pub den: u128,
}

/// `num / den` as an exact pair, or `None` when `den` is zero.
#[must_use]
pub fn fraction(num: u64, den: u64) -> Option<Fraction> {
    if den == 0 {
        return None;
    }
    Some(Fraction {
        num: u128::from(num),
        den: u128::from(den),
    })
}

/// What a pace computation decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    /// No units or no active time: there is no pace to report.
    Empty,
    /// The exact pace.
    Rate(Fraction),
    /// A product overflowed `u128` (proved unreachable for `u64` inputs).
    Overflow,
}

/// Correct pace in words per minute: `units * 60e6 / (5 * active_us)`.
/// [`Pace::Empty`] when either input is zero.
#[must_use]
pub fn wpm(units: u64, active_us: u64) -> Pace {
    if units == 0 || active_us == 0 {
        return Pace::Empty;
    }
    match u128::from(units).checked_mul(MICROSECONDS_PER_MINUTE) {
        None => Pace::Overflow,
        Some(num) => match u128::from(active_us).checked_mul(WPM_UNITS_PER_WORD) {
            None => Pace::Overflow,
            Some(den) => Pace::Rate(Fraction { num, den }),
        },
    }
}

/// Numpad pace in digits per minute: `digits * 60e6 / active_us`.
/// [`Pace::Empty`] when either input is zero.
#[must_use]
pub fn digits_per_minute(digits: u64, active_us: u64) -> Pace {
    if digits == 0 || active_us == 0 {
        return Pace::Empty;
    }
    match u128::from(digits).checked_mul(MICROSECONDS_PER_MINUTE) {
        None => Pace::Overflow,
        Some(num) => Pace::Rate(Fraction {
            num,
            den: u128::from(active_us),
        }),
    }
}

/// What a truncating basis-point ratio decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasisPoints {
    /// The denominator was zero.
    ZeroDenominator,
    /// The numerator exceeded the denominator (over 100%).
    OverFull,
    /// The scaled numerator overflowed.
    Overflow,
    /// `floor(num * 10_000 / den)`, at most [`BASIS_POINTS_FULL`].
    Value(u64),
}

/// `floor(num * 10_000 / den)` as basis points of a ratio at most 100%.
#[must_use]
pub fn basis_points(num: u64, den: u64) -> BasisPoints {
    if den == 0 {
        return BasisPoints::ZeroDenominator;
    }
    if num > den {
        return BasisPoints::OverFull;
    }
    match num.checked_mul(BASIS_POINTS_FULL) {
        Some(scaled) => match scaled.checked_div(den) {
            Some(value) => BasisPoints::Value(value),
            None => BasisPoints::ZeroDenominator,
        },
        None => BasisPoints::Overflow,
    }
}
