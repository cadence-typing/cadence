//! Text, terminal and count units (L-02, U-02).
//!
//! [`TextUnitIndex`] identifies an expected logical training unit — one
//! ASCII character, `£`, or explicit newline — never a UTF-8 byte or a
//! terminal cell. Renderer-introduced wraps are not expected input and so
//! never produce unit indices. [`CellColumn`]/[`TerminalRows`]/
//! [`TerminalColumns`] bound viewport geometry; the renderer must be total
//! for every nonzero bounded viewport including 1×1.

use core::fmt;

use crate::CoreError;

/// Generates a zero-based bounded unit whose minimum is the type's zero.
macro_rules! bounded_unit {
    ($(#[$doc:meta])* $name:ident: $ty:ty, maximum = $max:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($ty);

        impl $name {
            /// Largest legal value for this unit (frame schema maximum).
            pub const MAXIMUM: Self = Self($max);

            /// Constructs the unit, validating its bounds.
            ///
            /// # Errors
            /// Returns [`CoreError::InvalidIdentity`] when the value
            /// exceeds `MAXIMUM`.
            pub const fn new(value: $ty) -> Result<Self, CoreError> {
                if value > $max {
                    return Err(CoreError::InvalidIdentity {
                        kind: stringify!($name),
                        reason: "value outside the validated range",
                    });
                }
                Ok(Self(value))
            }

            /// The underlying integer value.
            pub const fn get(self) -> $ty {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

/// Generates a bounded unit with an explicit nonzero minimum (for example
/// viewport dimensions, where zero is not a renderable geometry).
macro_rules! bounded_unit_ranged {
    ($(#[$doc:meta])* $name:ident: $ty:ty, minimum = $min:expr, maximum = $max:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name($ty);

        impl $name {
            /// Smallest legal value for this unit.
            pub const MINIMUM: Self = Self($min);
            /// Largest legal value for this unit (frame schema maximum).
            pub const MAXIMUM: Self = Self($max);

            /// Constructs the unit, validating its bounds.
            ///
            /// # Errors
            /// Returns [`CoreError::InvalidIdentity`] when the value is
            /// outside `MINIMUM..=MAXIMUM`.
            pub const fn new(value: $ty) -> Result<Self, CoreError> {
                if value < $min || value > $max {
                    return Err(CoreError::InvalidIdentity {
                        kind: stringify!($name),
                        reason: "value outside the validated viewport range",
                    });
                }
                Ok(Self(value))
            }

            /// The underlying integer value.
            pub const fn get(self) -> $ty {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "({})"), self.0)
            }
        }
    };
}

bounded_unit!(
    /// Zero-based index of an expected logical training unit within an
    /// immutable exercise target (bounded by policy's maximum target
    /// length, not by `u32`).
    TextUnitIndex: u32,
    maximum = 4_095
);
bounded_unit!(
    /// Zero-based terminal cell column (frame schema maximum 240).
    CellColumn: u16,
    maximum = 239
);
bounded_unit_ranged!(
    /// Number of terminal columns (frame schema maximum 240).
    TerminalColumns: u16,
    minimum = 1,
    maximum = 240
);
bounded_unit_ranged!(
    /// Number of terminal rows (frame schema maximum 80).
    TerminalRows: u16,
    minimum = 1,
    maximum = 80
);

/// A count of logical training units (positions, attempts, deletions,
/// correct units). Counters at external-input boundaries use checked
/// arithmetic ([`UnitCount::checked_add`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct UnitCount(u64);

impl UnitCount {
    /// Zero units.
    pub const ZERO: Self = Self(0);

    /// Constructs a count from a raw value.
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// The underlying count.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Checked addition for event counters.
    ///
    /// # Errors
    /// Returns [`CoreError::IdExhaustion`] when the counter space is
    /// exhausted (counters are bounded and never wrap).
    pub const fn checked_add(self, other: Self) -> Result<Self, CoreError> {
        match self.0.checked_add(other.0) {
            Some(total) => Ok(Self(total)),
            None => Err(CoreError::IdExhaustion {
                kind: "UnitCount",
                reason: "unit counter exhausted; refuse instead of wrapping",
            }),
        }
    }

    /// Checked subtraction; counts never go negative.
    ///
    /// # Errors
    pub const fn checked_sub(self, other: Self) -> Result<Self, CoreError> {
        match self.0.checked_sub(other.0) {
            Some(difference) => Ok(Self(difference)),
            None => Err(CoreError::InvalidSubtraction {
                left: self.0,
                right: other.0,
            }),
        }
    }
    /// Whether the count is zero (used for `None` metric denominators).
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for UnitCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} units", self.0)
    }
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    #[test]
    fn viewport_units_validate_bounds() {
        assert!(TerminalColumns::new(1).is_ok());
        assert!(TerminalColumns::new(240).is_ok());
        assert!(TerminalColumns::new(241).is_err());
        assert!(TerminalRows::new(0).is_err());
        assert!(CellColumn::new(239).is_ok());
        assert!(CellColumn::new(240).is_err());
    }

    #[test]
    fn unit_counts_never_wrap_or_go_negative() {
        let two = UnitCount::from_raw(2);
        assert_eq!(
            UnitCount::from_raw(u64::MAX).checked_add(two),
            Err(CoreError::IdExhaustion {
                kind: "UnitCount",
                reason: "unit counter exhausted; refuse instead of wrapping"
            })
        );
        assert_eq!(
            two.checked_sub(UnitCount::from_raw(3)),
            Err(CoreError::InvalidSubtraction { left: 2, right: 3 })
        );
        assert!(UnitCount::ZERO.is_zero());
    }

    #[test]
    fn text_unit_indices_bounded_by_policy_maximum() {
        assert!(TextUnitIndex::new(0).is_ok());
        assert!(TextUnitIndex::new(4_095).is_ok());
        assert!(TextUnitIndex::new(4_096).is_err());
    }
}
