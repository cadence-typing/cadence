//! Session flow transitions that are not scoring (plan 0004): pausing for an
//! external reason, ending early into the review, and the interlude grace.
//! None of these forgive a mistake or change a measured figure.
use super::{DomainOutcome, Engine, PauseReason, Screen, Transition};
use cadence_core::progress::ProgressStatus;
use cadence_core::session::SessionState;
use cadence_core::{CoreError, MonoTick};

/// Keys arriving this soon after a phase-complete screen opens were already
/// in flight for the previous exercise (typically the Enter submitting the
/// last numpad entry). Comfortably above a fast typist's inter-key gap,
/// well below a deliberate read-then-continue.
const INTERLUDE_INPUT_GRACE_US: u64 = 700_000;

impl Engine {
    /// Pauses running practice for `reason`; an existing pause keeps its reason.
    pub(super) fn pause_for(&mut self, at: MonoTick, reason: PauseReason) -> Result<(), CoreError> {
        if self.screen != Screen::Practice || self.session_state != SessionState::Running {
            return Ok(());
        }
        self.timing.pause(at)?;
        self.session_state = SessionState::Paused;
        self.pause_reason = Some(reason);
        self.checkpoint_due = true;
        Ok(())
    }

    /// Resumes paused practice when it is safe to do so.
    pub(super) fn resume(&mut self, at: MonoTick) -> Result<(), CoreError> {
        if !self.invalid
            && self.viewport == super::ViewportState::Usable
            && !self.storage.as_ref().is_some_and(|s| s.failed.is_some())
        {
            self.timing.resume(at)?;
            self.session_state = SessionState::Running;
            self.pause_reason = None;
        }
        Ok(())
    }

    /// Ctrl-C: during practice or an interlude it ends the session into the
    /// review; elsewhere (start, review, failure, invalid) it exits.
    pub(super) fn interrupt(&mut self, at: MonoTick, t: &mut Transition) -> Result<(), CoreError> {
        let failed = self.storage.as_ref().is_some_and(|s| s.failed.is_some());
        if !self.finishing
            && !self.invalid
            && !failed
            && matches!(self.screen, Screen::Practice | Screen::PhaseComplete { .. })
        {
            t.outcome = self.end_early(at)?;
            return Ok(());
        }
        self.request_exit(at, t)
    }

    /// Stops practice, keeps every recorded attempt (the open exercise is
    /// marked interrupted, as on quit) and opens the review. An unfinished
    /// plan is persisted as abandoned, never as completed.
    pub(super) fn end_early(&mut self, at: MonoTick) -> Result<DomainOutcome, CoreError> {
        self.timing.pause(at)?;
        if let Some(ledger) = self.ledger.as_mut() {
            ledger.mark_interrupted();
        }
        self.record_exercise()?;
        let outcome = if self.status == ProgressStatus::Completed {
            DomainOutcome::SessionSummary
        } else {
            self.status = ProgressStatus::Abandoned;
            self.ended_early = true;
            DomainOutcome::SessionAbandoned
        };
        self.screen = Screen::Summary;
        self.session_state = SessionState::Review;
        self.pause_reason = None;
        self.interlude_opened = None;
        self.checkpoint_due = true;
        Ok(outcome)
    }

    /// Whether the phase-complete screen has been visible long enough to
    /// accept a deliberate Enter/Esc.
    pub(super) fn interlude_settled(&self, at: MonoTick) -> Result<bool, CoreError> {
        Ok(match self.interlude_opened {
            Some(opened) => at.elapsed_since(opened)?.as_us() >= INTERLUDE_INPUT_GRACE_US,
            None => true,
        })
    }
}

/// A reason a learner can read; internal kinds stay in the typed error.
pub(super) fn describe(error: &CoreError) -> String {
    match error {
        CoreError::InvalidIdentity { reason, .. } => (*reason).to_string(),
        other => other.to_string(),
    }
}
