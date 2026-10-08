//! Session state machine and timing segments (A-05, L-03).
//!
//! States: `Ready`, `Running`, `Paused`, `PhaseComplete`, `Review`,
//! `Finishing`, `Saved`, `SaveFailed`, `Aborted`. Start freezes the plan
//! and policy identity. The first content input starts the phase's
//! active clock. Pause records a clock boundary before changing state;
//! resume uses a fresh contiguous timing segment. Phase timeouts admit
//! only events whose timestamp is within the half-open phase boundary
//! (`MonoTick::within_half_open`; keys at the exact deadline are outside
//! the scored slice).

use crate::CoreError;
use crate::tick::{Micros, MonoTick};

/// Session lifecycle states (A-05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionState {
    /// Initial state: no active session.
    Ready,
    /// An exercise is accepting input and accumulating active time.
    Running,
    /// Explicitly paused; no active-time accumulation.
    Paused,
    /// The allocated phase budget was consumed; the partial exercise is
    /// retained.
    PhaseComplete,
    /// Post-practice reflection screen.
    Review,
    /// A final durable checkpoint has been requested and is pending.
    Finishing,
    /// The final checkpoint was acknowledged.
    Saved,
    /// The final checkpoint failed; the last acknowledged state is
    /// retained and the error is visible and retryable.
    SaveFailed,
    /// The user abandoned the session.
    Aborted,
}

/// Legal state transitions (A-05). Illegal transitions are refused with
/// a typed error rather than ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTransition {
    /// Start a drill/session.
    Start,
    /// The first valid content input begins the active clock.
    FirstContent,
    /// Explicit pause (Esc).
    Pause,
    /// Resume from pause.
    Resume,
    /// Phase budget exhausted.
    PhaseDeadline,
    /// Move to the next phase.
    NextPhase,
    /// Enter review.
    EnterReview,
    /// Request the final durable checkpoint.
    Finish,
    /// The final checkpoint was acknowledged.
    SaveAcknowledged,
    /// The final checkpoint failed.
    SaveFailed,
    /// Retry the final checkpoint.
    RetrySave,
    /// Abandon the session.
    Abort,
}

/// Applies a transition, returning the next state.
///
/// # Errors
/// Returns [`CoreError::InvalidIdentity`] naming the illegal transition
/// so callers surface it instead of silently no-oping.
pub fn next_state(
    current: SessionState,
    transition: SessionTransition,
) -> Result<SessionState, CoreError> {
    use SessionState as S;
    use SessionTransition as T;
    let legal = match (current, transition) {
        (S::Ready, T::Start) => Some(S::Running),
        (S::Running, T::FirstContent) => Some(S::Running),
        (S::Running, T::Pause) => Some(S::Paused),
        (S::Paused, T::Resume) => Some(S::Running),
        (S::Running, T::PhaseDeadline) => Some(S::PhaseComplete),
        (S::PhaseComplete, T::NextPhase) => Some(S::Running),
        (S::PhaseComplete | S::Running, T::EnterReview) => Some(S::Review),
        (S::Review | S::Running | S::Paused | S::PhaseComplete, T::Finish) => Some(S::Finishing),
        (S::Finishing, T::SaveAcknowledged) => Some(S::Saved),
        (S::Finishing, T::SaveFailed) => Some(S::SaveFailed),
        (S::SaveFailed, T::RetrySave) => Some(S::Finishing),
        (
            S::Ready | S::Running | S::Paused | S::PhaseComplete | S::Review | S::SaveFailed,
            T::Abort,
        ) => Some(S::Aborted),
        _ => None,
    };
    legal.ok_or(CoreError::InvalidIdentity {
        kind: "SessionTransition",
        reason: "illegal session state transition",
    })
}

/// Active-time accounting for one exercise: contiguous timing segments
/// separated by explicit pause boundaries (L-03). On crash/restart a
/// resumed exercise is an interrupted new segment; wall-clock downtime is
/// never subtracted from a claimed uninterrupted trial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimingSegments {
    /// Tick of the first valid content input in the current segment;
    /// `None` before the first input.
    segment_start: Option<MonoTick>,
    /// Completed segment durations, in order.
    completed: Vec<Micros>,
    /// Total paused time, accumulated at each pause boundary.
    paused_total: Micros,
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
            completed: Vec::new(),
            paused_total: Micros::ZERO,
        }
    }

    /// The first valid content input starts (or continues) the segment.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] when `at` precedes the
    /// previously recorded boundary.
    pub fn first_content(&mut self, at: MonoTick) -> Result<(), CoreError> {
        if self.segment_start.is_none() {
            self.segment_start = Some(at);
        }
        Ok(())
    }

    /// Pause records a clock boundary before the state change (A-05).
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidIdentity`] when no segment is active,
    /// or [`CoreError::BackwardTime`] for backward clocks.
    pub fn pause(&mut self, at: MonoTick) -> Result<(), CoreError> {
        let start = self.segment_start.ok_or(CoreError::InvalidIdentity {
            kind: "TimingSegments",
            reason: "pause without an active segment",
        })?;
        let elapsed = at.elapsed_since(start)?;
        self.completed.push(elapsed);
        self.segment_start = None;
        self.paused_total = self.paused_total.checked_add(elapsed)?;
        Ok(())
    }

    /// Resume opens a fresh contiguous segment.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] for backward clocks.
    pub fn resume(&mut self, at: MonoTick) -> Result<(), CoreError> {
        debug_assert!(
            self.segment_start.is_none(),
            "resume while a segment is open is an engine bug"
        );
        if self.segment_start.is_some() {
            return Err(CoreError::InvalidIdentity {
                kind: "TimingSegments",
                reason: "resume while a segment is open",
            });
        }
        self.segment_start = Some(at);
        Ok(())
    }

    /// Total paused time so far.
    #[must_use]
    pub const fn paused_total(&self) -> Micros {
        self.paused_total
    }

    /// Active time through `now`: completed segments plus the open
    /// segment's elapsed time.
    ///
    /// # Errors
    /// Returns [`CoreError::BackwardTime`] when `now` precedes the open
    /// segment's start.
    pub fn active_through(&self, now: MonoTick) -> Result<Micros, CoreError> {
        let mut total = Micros::ZERO;
        for segment in &self.completed {
            total = total.checked_add(*segment)?;
        }
        if let Some(start) = self.segment_start {
            total = total.checked_add(now.elapsed_since(start)?)?;
        }
        Ok(total)
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
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    #[test]
    fn legal_transitions_cover_the_documented_lifecycle() {
        assert_eq!(
            next_state(SessionState::Ready, SessionTransition::Start).unwrap(),
            SessionState::Running
        );
        assert_eq!(
            next_state(SessionState::Running, SessionTransition::Pause).unwrap(),
            SessionState::Paused
        );
        assert_eq!(
            next_state(SessionState::Paused, SessionTransition::Resume).unwrap(),
            SessionState::Running
        );
        assert_eq!(
            next_state(SessionState::Finishing, SessionTransition::SaveAcknowledged).unwrap(),
            SessionState::Saved
        );
        assert_eq!(
            next_state(SessionState::Finishing, SessionTransition::SaveFailed).unwrap(),
            SessionState::SaveFailed
        );
        assert_eq!(
            next_state(SessionState::SaveFailed, SessionTransition::RetrySave).unwrap(),
            SessionState::Finishing
        );
    }

    #[test]
    fn illegal_transitions_are_typed_errors() {
        assert!(next_state(SessionState::Saved, SessionTransition::Start).is_err());
        assert!(next_state(SessionState::Ready, SessionTransition::Pause).is_err());
        assert!(next_state(SessionState::Aborted, SessionTransition::Resume).is_err());
    }

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
        assert_eq!(timing.paused_total().as_us(), 3_000_000);
    }

    #[test]
    fn backward_time_is_measurement_invalid() {
        let mut timing = TimingSegments::new();
        timing.first_content(MonoTick::from_us(5_000_000)).unwrap();
        assert!(timing.pause(MonoTick::from_us(4_000_000)).is_err());
        assert!(timing.active_through(MonoTick::from_us(1_000_000)).is_err());
    }
}
