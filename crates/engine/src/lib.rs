//! Pure single-owner reducer for the static development binary.
//! Plans are frozen upfront and cycle deterministically until phase deadlines;
//! this is not an adaptive-planner or formal-proof qualification claim.
pub mod events;
mod practice;
mod presentation;
mod storage;
pub mod view;
use cadence_core::ledger::AttemptLedger;
use cadence_core::progress::{Checkpoint, HistorySummary, ProgressStatus, TrackProgress};
use cadence_core::session::{SessionState, TimingSegments};
use cadence_core::{CoreError, EventSeq, ExerciseId, FrameRevision, Micros, MonoTick};
use cadence_trainer::{Phase, SessionPlan};
pub use events::{EventEnvelope, EventKind, Key};
use storage::Storage;
pub use view::{PreparedView, ScreenView, StatusFigures, UnitSpan};

/// Current input-routing surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOwner {
    /// Navigation or paused controls, not scored typing.
    Navigation,
    /// Printable keys are exercise input.
    Practice,
}
/// Domain facts produced by one event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainOutcome {
    /// No boundary crossed.
    None,
    /// Session began.
    SessionStarted,
    /// A new phase was offered for practice.
    PhaseStarted {
        /// Phase identity.
        phase: Phase,
    },
    /// An exact exercise completed.
    ExerciseCompleted {
        /// Exercise identity.
        exercise: ExerciseId,
        /// Exact first-correct/first-attempted counts.
        first_pass: Option<(u128, u128)>,
    },
    /// Half-open deadline closed a phase.
    PhaseDeadline {
        /// Phase identity.
        phase: Phase,
        /// Whether an unfinished buffer was retained.
        partial: bool,
    },
    /// Session reached review.
    SessionSummary,
    /// Session stopped early.
    SessionAbandoned,
    /// Measurement was visibly invalidated and scoring stopped.
    MeasurementInvalid {
        /// Typed reason.
        reason: CoreError,
    },
}
/// Bounded effects returned to the runtime; reducer performs no I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    /// Reprepare and paint.
    pub dirty: bool,
    /// Exit permitted (final durability acknowledged, or explicit failure exit).
    pub quit: bool,
    /// Domain boundary facts.
    pub outcome: DomainOutcome,
    /// At most one outstanding full cumulative checkpoint.
    pub checkpoint: Option<Checkpoint>,
    /// Explicit non-success exit with an undurable tail.
    pub exit_failed: bool,
}
impl Transition {
    fn changed(outcome: DomainOutcome) -> Self {
        Self {
            dirty: true,
            quit: false,
            outcome,
            checkpoint: None,
            exit_failed: false,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
    Today,
    Practice,
    PhaseComplete { partial: bool },
    Summary,
}

/// Pure plan-driven engine. Storage readiness is supplied as an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    plan: SessionPlan,
    screen: Screen,
    phase_index: usize,
    exercise_index: usize,
    next_exercise: u64,
    ledger: Option<AttemptLedger>,
    timing: TimingSegments,
    session_state: SessionState,
    phase_start: Micros,
    exercise_start: Micros,
    active: Micros,
    tracks: Vec<TrackProgress>,
    consumed: EventSeq,
    last_at: Option<MonoTick>,
    revision: FrameRevision,
    invalid: bool,
    notice: Option<String>,
    history: HistorySummary,
    storage: Option<Storage>,
    status: ProgressStatus,
    checkpoint_due: bool,
    finishing: bool,
}
impl Engine {
    /// Validates the frozen plan before accepting any input.
    /// # Errors
    /// Refuses empty practice phases, mismatched kinds and oversized targets.
    pub fn new_plan(plan: SessionPlan) -> Result<Self, CoreError> {
        if !plan.phases.iter().any(|p| p.phase != Phase::Review) {
            return Err(invalid("session needs a typing phase"));
        }
        for (index, p) in plan.phases.iter().enumerate() {
            if p.phase == Phase::Review
                && (!p.exercises.is_empty() || index.checked_add(1) != Some(plan.phases.len()))
            {
                return Err(invalid("review must be empty and last"));
            }
            Micros::from_seconds(p.seconds)?;
            if p.phase != Phase::Review && (p.exercises.is_empty() || p.seconds == 0) {
                return Err(invalid("empty practice phase"));
            }
            for e in &p.exercises {
                if e.units.is_empty()
                    || e.units.len() > cadence_core::TextUnitIndex::MAXIMUM.get() as usize
                    || practice::track_kind(e.kind) != practice::track_phase(p.phase)?
                {
                    return Err(invalid("invalid exercise target or phase kind"));
                }
            }
        }
        Ok(Self {
            plan,
            screen: Screen::Today,
            phase_index: 0,
            exercise_index: 0,
            next_exercise: 1,
            ledger: None,
            timing: TimingSegments::new(),
            session_state: SessionState::Ready,
            phase_start: Micros::ZERO,
            exercise_start: Micros::ZERO,
            active: Micros::ZERO,
            tracks: Vec::with_capacity(5),
            consumed: EventSeq::INITIAL,
            last_at: None,
            revision: FrameRevision::INITIAL,
            invalid: false,
            notice: None,
            history: HistorySummary::default(),
            storage: None,
            status: ProgressStatus::Running,
            checkpoint_due: false,
            finishing: false,
        })
    }
    /// Active practice alone owns printable characters.
    #[must_use]
    pub fn input_owner(&self) -> InputOwner {
        if self.screen == Screen::Practice
            && self.session_state == SessionState::Running
            && !self.finishing
        {
            InputOwner::Practice
        } else {
            InputOwner::Navigation
        }
    }
    /// Last admitted sequence; rejected sequence gaps do not advance it.
    #[must_use]
    pub const fn consumed_events(&self) -> EventSeq {
        self.consumed
    }
    /// Applies one sequenced event, with no I/O or background mutation.
    pub fn reduce(&mut self, envelope: EventEnvelope) -> Transition {
        let result = self.reduce_checked(envelope);
        let mut transition = match result {
            Ok(t) => t,
            Err(e) => self.invalidate(e),
        };
        if transition.dirty {
            match self.revision.next() {
                Ok(r) => self.revision = r,
                Err(e) => transition = self.invalidate(e),
            }
        }
        transition
    }
    fn reduce_checked(&mut self, envelope: EventEnvelope) -> Result<Transition, CoreError> {
        if envelope.seq != self.consumed.next()? {
            return Err(invalid("event sequence gap or replay"));
        }
        self.consumed = envelope.seq;
        if let Some(last) = self.last_at {
            envelope.at.elapsed_since(last)?;
        }
        self.last_at = Some(envelope.at);
        let at = envelope.at;
        let mut t = Transition::changed(DomainOutcome::None);
        // Invalidate before scoring a delayed event, but still consume a
        // terminal storage reply so the single-flight slot cannot get stuck.
        if let Err(error) = self.watchdog(at, &mut t) {
            t = self.invalidate(error);
        }
        // Control and storage responses are never swallowed by a deadline.
        let boundary = self.advance_clock(at)?;
        match envelope.kind {
            EventKind::StorageReady {
                next_session,
                day,
                history,
            } => self.storage_ready(next_session, day, history)?,
            EventKind::SaveResult(result) => self.save_result(result, &mut t)?,
            EventKind::Key(Key::Interrupt) => self.request_exit(at, &mut t)?,
            EventKind::Key(Key::Content(c)) if self.screen == Screen::Today && c.get() == 'q' => {
                self.request_exit(at, &mut t)?
            }
            EventKind::Key(Key::Enter | Key::Escape) if self.screen == Screen::Summary => {
                self.request_exit(at, &mut t)?
            }
            EventKind::PasteRejected | EventKind::FocusLost if self.screen == Screen::Practice => {
                return Err(invalid("paste or focus loss interrupted measurement"));
            }
            EventKind::Key(Key::Content(c))
                if self.session_state == SessionState::Paused && c.get() == 'r' =>
            {
                self.retry_save()
            }
            kind if !self.finishing && !self.invalid => {
                if let Some(outcome) = boundary {
                    t.outcome = outcome;
                } else {
                    t.outcome = self.apply_kind(at, kind)?;
                }
            }
            _ => {}
        }
        self.watchdog(at, &mut t)?;
        self.dispatch_checkpoint(at, &mut t)?;
        Ok(t)
    }
    fn invalidate(&mut self, reason: CoreError) -> Transition {
        if let Some(at) = self.last_at
            && let Err(error) = self.timing.pause(at)
        {
            self.notice = Some(format!("Timing invalid: {error}"));
        }
        if let Some(l) = self.ledger.as_mut() {
            l.mark_interrupted();
        }
        self.invalid = true;
        self.session_state = SessionState::Paused;
        self.notice = Some(format!(
            "Measurement invalid: {reason}. Quit to retain partial evidence; no qualified score."
        ));
        self.checkpoint_due = true;
        Transition::changed(DomainOutcome::MeasurementInvalid { reason })
    }
    fn apply_kind(&mut self, at: MonoTick, kind: EventKind) -> Result<DomainOutcome, CoreError> {
        match (&self.screen, kind) {
            (Screen::Today, EventKind::Key(Key::Enter)) => {
                self.screen = Screen::Practice;
                self.session_state = SessionState::Running;
                self.open_exercise()?;
                Ok(DomainOutcome::SessionStarted)
            }
            (Screen::PhaseComplete { .. }, EventKind::Key(Key::Enter | Key::Escape)) => {
                self.advance_phase()
            }
            (Screen::Practice, EventKind::Key(key)) => self.practice_key(at, key),
            _ => Ok(DomainOutcome::None),
        }
    }
}
fn invalid(reason: &'static str) -> CoreError {
    CoreError::InvalidIdentity {
        kind: "Engine",
        reason,
    }
}
