//! Session state machine and timing segments (A-05, L-03).
//!
//! States: `Ready`, `Running`, `Paused`, `PhaseComplete`, `Review`,
//! `Finishing`, `Saved`, decided by `cadence_kernel::session::step`. Start freezes the plan
//! and policy identity. The first content input starts the phase's
//! active clock. Pause records a clock boundary before changing state;
//! resume uses a fresh contiguous timing segment. Phase timeouts admit
//! only events whose timestamp is within the half-open phase boundary
//! (`Micros::within_half_open` over active time, which is the proved
//! `cadence_kernel::time::in_window`; keys at the exact deadline are
//! outside the scored slice). Active time itself is the proved
//! `cadence_kernel::time::segment_active`.

use crate::CoreError;
use crate::tick::{Micros, MonoTick};
use cadence_kernel::time::{Through, segment_active};

/// Session lifecycle states (A-05): the kernel's [`cadence_kernel::session::State`].
/// Every change of state goes through the proved
/// [`cadence_kernel::session::step`] table.
pub use cadence_kernel::session::State as SessionState;
/// The proved session table and its vocabulary, for the engine's adapter.
pub use cadence_kernel::session::{
    Clock as SessionClock, Event as SessionEvent, Outcome as SessionOutcome, Step as SessionStep,
    step as session_step,
};

/// Active-time accounting for one exercise: contiguous timing segments
/// separated by explicit pause boundaries (L-03). On crash/restart a
/// resumed exercise is an interrupted new segment; wall-clock downtime is
/// never subtracted from a claimed uninterrupted trial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimingSegments {
    /// Tick of the first valid content input in the current segment;
    /// `None` before the first input.
    segment_start: Option<MonoTick>,
    /// Constant-space accumulated active duration.
    completed: Micros,
    /// Most recently observed timing boundary.
    last: Option<MonoTick>,
    /// Whether any content has started this phase.
    started: bool,
}

impl Default for TimingSegments {
    fn default() -> Self {
        Self::new()
    }
}

impl TimingSegments {
    /// Fresh timing, no active segment.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            segment_start: None,
            completed: Micros::ZERO,
            last: None,
            started: false,
        }
    }

    /// The first valid content input starts (or continues) the segment.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] when `at` precedes the
    /// previously recorded boundary.
    pub fn first_content(&mut self, at: MonoTick) -> Result<(), CoreError> {
        self.observe(at)?;
        if self.segment_start.is_none() {
            self.segment_start = Some(at);
        }
        self.started = true;
        Ok(())
    }

    /// Pausing before the first content input is valid and accumulates no time.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] for backward clocks.
    pub fn pause(&mut self, at: MonoTick) -> Result<(), CoreError> {
        self.observe(at)?;
        let total = self.active_through(at)?;
        self.completed = total;
        self.segment_start = None;
        Ok(())
    }

    /// Resume opens a fresh contiguous segment.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] for backward clocks.
    pub fn resume(&mut self, at: MonoTick) -> Result<(), CoreError> {
        self.observe(at)?;
        if self.segment_start.is_some() {
            return Err(CoreError::InvalidIdentity {
                kind: "TimingSegments",
                reason: "resume while a segment is open",
            });
        }
        if self.started {
            self.segment_start = Some(at);
        }
        Ok(())
    }

    /// Closes a phase; the next phase waits for its first content input.
    ///
    /// # Errors
    /// Refuses backward time.
    pub fn end_phase(&mut self, at: MonoTick) -> Result<(), CoreError> {
        self.pause(at)?;
        self.started = false;
        Ok(())
    }

    fn observe(&mut self, at: MonoTick) -> Result<(), CoreError> {
        if let Some(last) = self.last {
            at.elapsed_since(last)?;
        }
        self.last = Some(at);
        Ok(())
    }

    /// Accumulated active duration of closed segments.
    #[must_use]
    pub const fn accumulated_active(&self) -> Micros {
        self.completed
    }

    /// Active time through `now`: completed segments plus the open
    /// segment's elapsed time.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] when `now` precedes the open
    /// segment's start.
    pub fn active_through(&self, now: MonoTick) -> Result<Micros, CoreError> {
        if let Some(last) = self.last {
            now.elapsed_since(last)?;
        }
        match segment_active(
            self.completed.as_us(),
            self.segment_start.map(MonoTick::as_us),
            now.as_us(),
        ) {
            Through::Time(total) => Ok(Micros::from_us(total)),
            Through::Backward => Err(CoreError::BackwardTime {
                admitted: self.segment_start.map_or(0, MonoTick::as_us),
                arriving: now.as_us(),
            }),
            Through::Overflow => Err(CoreError::ArithmeticOverflow {
                operation: "TimingSegments::active_through",
            }),
        }
    }

    /// Whether a segment is currently open.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.segment_start.is_some()
    }
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

    use super::*;

    #[test]
    fn pause_resume_segment_arithmetic() {
        let mut timing = TimingSegments::new();
        timing.first_content(MonoTick::from_us(1_000_000)).unwrap();
        // Active 3s, pause 4s, resume, active through 10s mark.
        timing.pause(MonoTick::from_us(4_000_000)).unwrap();
        timing.resume(MonoTick::from_us(8_000_000)).unwrap();
        assert_eq!(
            timing
                .active_through(MonoTick::from_us(11_000_000))
                .unwrap()
                .as_us(),
            6_000_000
        );
        assert_eq!(timing.accumulated_active().as_us(), 3_000_000);
    }

    #[test]
    fn backward_time_is_measurement_invalid() {
        let mut timing = TimingSegments::new();
        timing.first_content(MonoTick::from_us(5_000_000)).unwrap();
        assert!(timing.pause(MonoTick::from_us(4_000_000)).is_err());
        assert!(timing.active_through(MonoTick::from_us(1_000_000)).is_err());
    }

    #[test]
    fn pause_before_input_preserves_first_key_start() -> Result<(), CoreError> {
        let mut timing = TimingSegments::new();
        timing.pause(MonoTick::from_us(10))?;
        timing.resume(MonoTick::from_us(100))?;
        assert_eq!(timing.active_through(MonoTick::from_us(200))?, Micros::ZERO);
        timing.first_content(MonoTick::from_us(300))?;
        assert_eq!(timing.active_through(MonoTick::from_us(400))?.as_us(), 100);
        Ok(())
    }

    #[test]
    fn closed_phase_excludes_transition_and_waits_for_content() -> Result<(), CoreError> {
        let mut timing = TimingSegments::new();
        timing.first_content(MonoTick::from_us(10))?;
        timing.end_phase(MonoTick::from_us(20))?;
        assert_eq!(timing.active_through(MonoTick::from_us(1000))?.as_us(), 10);
        assert!(timing.first_content(MonoTick::from_us(19)).is_err());
        timing.first_content(MonoTick::from_us(2000))?;
        assert_eq!(timing.active_through(MonoTick::from_us(2010))?.as_us(), 20);
        Ok(())
    }
}
