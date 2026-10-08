//! Typed error taxonomy for pure domain failures (AGENTS.md: production
//! libraries return typed errors; `anyhow` is confined to CLI/xtask
//! aggregation).
//!
//! This enum covers only failures owned by `cadence-core`. Every variant
//! names the invariant that was violated so callers can distinguish
//! corruption, capacity limits and arithmetic impossibility.

use core::fmt;

/// Failures raised by pure domain foundations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// A checked arithmetic operation would overflow its type. The domain
    /// never wraps or saturates to conceal lost data.
    ArithmeticOverflow {
        /// The operation that overflowed, for diagnostics.
        operation: &'static str,
    },
    /// A subtraction would produce a negative value where the type is
    /// non-negative (for example a watermark moving backwards).
    InvalidSubtraction {
        /// Left operand.
        left: u64,
        /// Right operand.
        right: u64,
    },
    /// A ratio was constructed with a zero denominator. Metrics with zero
    /// denominators are `None`, never infinity or zero (L-02).
    InvalidDenominator,
    /// A basis-point value exceeded the 0..=10_000 range.
    InvalidBasisPoints {
        /// The offending value.
        value: u16,
    },
    /// An identity was constructed from an unusable raw value (for example
    /// a zero name identifier).
    InvalidIdentity {
        /// Which identity kind rejected the value.
        kind: &'static str,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// A generational counter or name space is exhausted; the owning slot
    /// or subsystem must be retired, not wrapped.
    IdExhaustion {
        /// Which counter is exhausted.
        kind: &'static str,
        /// Retirement guidance for diagnostics.
        reason: &'static str,
    },
    /// A bounded capacity (buffer, queue, ledger, viewport) is full and the
    /// specified behaviour is to refuse rather than grow or drop.
    CapacityExhausted {
        /// Which named limit was hit.
        limit: &'static str,
    },
    /// Time moved backwards where ordering is required (A-05: reject
    /// backward time as measurement-invalid).
    BackwardTime {
        /// The earlier-stamped tick that the new tick would precede.
        admitted: u64,
        /// The newly arriving tick.
        arriving: u64,
    },
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArithmeticOverflow { operation } => {
                write!(f, "checked arithmetic overflow in {operation}")
            }
            Self::InvalidSubtraction { left, right } => {
                write!(f, "invalid subtraction: {left} - {right} would be negative")
            }
            Self::InvalidDenominator => write!(f, "zero denominator where a ratio was required"),
            Self::InvalidBasisPoints { value } => {
                write!(f, "basis points out of range 0..=10000: {value}")
            }
            Self::InvalidIdentity { kind, reason } => write!(f, "invalid {kind}: {reason}"),
            Self::IdExhaustion { kind, reason } => write!(f, "{kind} exhausted: {reason}"),
            Self::CapacityExhausted { limit } => write!(f, "capacity exhausted: {limit}"),
            Self::BackwardTime { admitted, arriving } => write!(
                f,
                "backward time rejected: tick {arriving} precedes admitted tick {admitted}"
            ),
        }
    }
}

impl std::error::Error for CoreError {}
