//! Time and measurement types (L-03).
//!
//! * [`MonoTick`] — checked microseconds since the run's timing origin, for
//!   intra-run timing. Constructed by the runtime's clock port, never
//!   sampled inside pure functions.
//! * [`Micros`] — a checked duration in microseconds used in tick
//!   arithmetic and phase budgets.
//! * [`UtcDay`] — injected calendar day (days since 1970-01-01) for review
//!   scheduling. Calendar values come from the runtime port; wall-clock
//!   rollback handling is owned by the review policy, not by this type.
//!
//! Negative or backward values are rejected ([`CoreError::BackwardTime`]);
//! equal timestamps are representable (insufficient resolution is handled
//! by timing-exclusion policy, not by inventing intervals).
use crate::CoreError;
use core::fmt;

/// A duration in microseconds. Bounded by `u64` (~585k years); all
/// conversions into policy durations are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Micros(u64);

impl Micros {
    /// Zero duration.
    pub const ZERO: Self = Self(0);

    /// Constructs a duration from microseconds.
    #[must_use]
    pub const fn from_us(microseconds: u64) -> Self {
        Self(microseconds)
    }

    /// Constructs from whole seconds.
    ///
    /// # Errors
    /// Returns [`CoreError::ArithmeticOverflow`] when the seconds count
    /// cannot be represented in microseconds.
    pub const fn from_seconds(seconds: u64) -> Result<Self, CoreError> {
        match seconds.checked_mul(1_000_000) {
            Some(microseconds) => Ok(Self(microseconds)),
            None => Err(CoreError::ArithmeticOverflow {
                operation: "Micros::from_seconds",
            }),
        }
    }

    /// Constructs from milliseconds.
    ///
    /// # Errors
    /// Returns [`CoreError::ArithmeticOverflow`] on multiplication
    /// overflow.
    pub const fn from_millis(milliseconds: u64) -> Result<Self, CoreError> {
        match milliseconds.checked_mul(1_000) {
            Some(microseconds) => Ok(Self(microseconds)),
            None => Err(CoreError::ArithmeticOverflow {
                operation: "Micros::from_millis",
            }),
        }
    }

    /// The duration in microseconds.
    #[must_use]
    pub const fn as_us(self) -> u64 {
        self.0
    }

    /// Checked addition.
    ///
    /// # Errors
    /// Returns [`CoreError::ArithmeticOverflow`] on overflow.
    pub const fn checked_add(self, other: Self) -> Result<Self, CoreError> {
        match self.0.checked_add(other.0) {
            Some(total) => Ok(Self(total)),
            None => Err(CoreError::ArithmeticOverflow {
                operation: "Micros::checked_add",
            }),
        }
    }

    /// Checked subtraction; durations never go negative.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidSubtraction`] when `other > self`.
    pub const fn checked_sub(self, other: Self) -> Result<Self, CoreError> {
        match self.0.checked_sub(other.0) {
            Some(difference) => Ok(Self(difference)),
            None => Err(CoreError::InvalidSubtraction {
                left: self.0,
                right: other.0,
            }),
        }
    }
}

impl fmt::Display for Micros {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}µs", self.0)
    }
}

/// Checked microseconds since the run's timing origin. The origin is the
/// run start recorded in the trace header (A-07); ticks are ordered within
/// a run. Backward arrival is measurement-invalid and rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonoTick(u64);

impl MonoTick {
    /// The timing origin.
    pub const ORIGIN: Self = Self(0);

    /// Constructs a tick from raw microseconds since origin.
    #[must_use]
    pub const fn from_us(microseconds: u64) -> Self {
        Self(microseconds)
    }

    /// The tick's microseconds since origin.
    #[must_use]
    pub const fn as_us(self) -> u64 {
        self.0
    }

    /// Elapsed duration from `earlier` to `self`.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] when `self` precedes `earlier`.
    /// Equal ticks return zero (callers treat that as insufficient timing
    /// resolution per L-03, never as infinite speed).
    pub const fn elapsed_since(self, earlier: Self) -> Result<Micros, CoreError> {
        match self.0.checked_sub(earlier.0) {
            Some(elapsed) => Ok(Micros(elapsed)),
            None => Err(CoreError::BackwardTime {
                admitted: earlier.0,
                arriving: self.0,
            }),
        }
    }

    /// The tick advanced by a duration.
    ///
    /// # Errors
    /// Returns [`CoreError::ArithmeticOverflow`] on overflow.
    pub const fn checked_add(self, duration: Micros) -> Result<Self, CoreError> {
        match self.0.checked_add(duration.0) {
            Some(total) => Ok(Self(total)),
            None => Err(CoreError::ArithmeticOverflow {
                operation: "MonoTick::checked_add",
            }),
        }
    }

    /// Whether this tick is strictly within the half-open interval
    /// `[start, deadline)` — the phase-boundary admission rule (A-05: keys
    /// at the exact deadline are outside the scored slice).
    #[must_use]
    pub const fn within_half_open(self, start: Self, deadline: Self) -> bool {
        self.0 >= start.0 && self.0 < deadline.0
    }
}

impl fmt::Display for MonoTick {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tick({}µs)", self.0)
    }
}

/// An injected calendar day: whole days since 1970-01-01 under the
/// profile's recorded calendar/timezone identity (L-06). Used for review
/// interval arithmetic; not a monotonic clock and never used for intra-run
/// timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcDay(i64);

impl UtcDay {
    /// Constructs a calendar day from days since the Unix epoch.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidIdentity`] outside the representable
    /// window year 1..=9999 so interval additions cannot silently overflow.
    pub const fn from_days_since_epoch(days: i64) -> Result<Self, CoreError> {
        // 719_162 = days from 0001-01-01 to 1970-01-01; 2_932_896 spans to
        // 9999-12-31. Bounded so a bounded number of interval additions
        // fails loudly instead of wrapping.
        if days < -719_162 || days > 2_932_896 {
            return Err(CoreError::InvalidIdentity {
                kind: "UtcDay",
                reason: "day outside the year 1..=9999 window",
            });
        }
        Ok(Self(days))
    }

    /// Days since 1970-01-01 (possibly negative for earlier dates).
    #[must_use]
    pub const fn days_since_epoch(self) -> i64 {
        self.0
    }

    /// The day advanced by a whole number of days (review intervals).
    ///
    /// # Errors
    /// Returns [`CoreError::ArithmeticOverflow`] when the result leaves the
    /// representable window; review scheduling then surfaces a typed error
    /// instead of wrapping.
    pub const fn checked_add_days(self, days: i64) -> Result<Self, CoreError> {
        match self.0.checked_add(days) {
            // Reuse the same window validation without duplication.
            Some(total) => match Self::from_days_since_epoch(total) {
                Ok(day) => Ok(day),
                Err(_) => Err(CoreError::ArithmeticOverflow {
                    operation: "UtcDay::checked_add_days",
                }),
            },
            None => Err(CoreError::ArithmeticOverflow {
                operation: "UtcDay::checked_add_days",
            }),
        }
    }

    /// Whole days from `earlier` to `self`; positive when `self` is later.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] when `self` precedes `earlier`.
    /// Wall-clock rollback is handled by review policy guards, not by
    /// silently returning negative intervals.
    pub const fn days_since(self, earlier: Self) -> Result<i64, CoreError> {
        match self.0.checked_sub(earlier.0) {
            Some(days) => Ok(days),
            None => Err(CoreError::BackwardTime {
                // Negative day indices (pre-1970) clamp to 0 in the
                // diagnostic; the value is informational, never arithmetic.
                admitted: if earlier.0 < 0 { 0 } else { earlier.0 as u64 },
                arriving: if self.0 < 0 { 0 } else { self.0 as u64 },
            }),
        }
    }
}

impl fmt::Display for UtcDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "utcday({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    #[test]
    fn elapsed_rejects_backward_time() {
        let early = MonoTick::from_us(1_000);
        let late = MonoTick::from_us(2_000);
        assert_eq!(late.elapsed_since(early).map(Micros::as_us), Ok(1_000));
        assert_eq!(
            early.elapsed_since(late),
            Err(CoreError::BackwardTime {
                admitted: 2_000,
                arriving: 1_000
            })
        );
        // Equal ticks: zero interval, resolution handled by policy.
        assert_eq!(early.elapsed_since(early).map(Micros::as_us), Ok(0));
    }

    #[test]
    fn phase_boundary_is_half_open() {
        let start = MonoTick::ORIGIN;
        let deadline = start
            .checked_add(Micros::from_seconds(60).expect("60s fits"))
            .expect("fits");
        assert!(MonoTick::from_us(59_999_999).within_half_open(start, deadline));
        assert!(!deadline.within_half_open(start, deadline));
        assert!(!MonoTick::ORIGIN.within_half_open(MonoTick::from_us(1), deadline));
    }

    #[test]
    fn utc_day_window_and_intervals() {
        let day = UtcDay::from_days_since_epoch(20_000).expect("in window");
        assert_eq!(
            day.checked_add_days(7).map(UtcDay::days_since_epoch),
            Ok(20_007)
        );
        assert!(UtcDay::from_days_since_epoch(4_000_000).is_err());
        assert_eq!(
            UtcDay::from_days_since_epoch(20_007)
                .expect("in window")
                .days_since(day),
            Ok(7)
        );
    }

    #[test]
    fn micros_checked_arithmetic() {
        let a = Micros::from_us(5);
        let b = Micros::from_us(3);
        assert_eq!(a.checked_add(b).map(Micros::as_us), Ok(8));
        assert_eq!(
            b.checked_sub(a),
            Err(CoreError::InvalidSubtraction { left: 3, right: 5 })
        );
        assert!(Micros::from_millis(u64::MAX).is_err());
    }
}
