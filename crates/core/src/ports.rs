//! Ports owned by pure domain layers (A-01: prefer port types in
//! core/engine over a shared "utils" crate).
//!
//! A *port* is a trait the domain defines and the runtime implements.
//! Pure code depends on the trait, never on OS clocks or RNG globals, so
//! headless/replay inject virtual implementations and the same reducer
//! runs unmodified (VR-01).

/// Source of monotonic intra-run timing. Implementations: the runtime's
/// OS-backed clock for live mode, the driver's virtual clock for headless
/// and replay. Time is an explicit input, never sampled inside pure
/// functions (L-03, A-07).
pub trait Clock {
    /// The current [`crate::MonoTick`] for this run's timing origin.
    fn now(&self) -> crate::MonoTick;
}

/// Source of injected calendar values for review scheduling (L-03).
/// Distinct from [`Clock`]: calendar days are not monotonic and rollback
/// is a policy-guard condition, not an impossibility here.
pub trait Calendar {
    /// The current whole [`crate::UtcDay`] under the profile's recorded
    /// calendar/timezone identity.
    fn today(&self) -> crate::UtcDay;
}
