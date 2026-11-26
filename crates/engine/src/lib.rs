//! Pure single-owner reducer for the static development binary.
//! Plans are built at startup from the learner model and cycle
//! deterministically until phase deadlines. The reducer records first-attempt
//! evidence and re-serves missed words within bounds (plan 0003); this is not
//! a formal-proof qualification claim.
pub mod commands;
pub mod events;
mod flow;
pub mod jobs;
mod learning;
mod practice;
mod presentation;
mod review;
mod storage;
pub mod summary;
pub mod view;
use cadence_core::ledger::AttemptLedger;
use cadence_core::progress::{Checkpoint, HistorySummary, ProgressStatus, TrackProgress};
use cadence_core::session::{
    SessionEvent, SessionState, SessionStep, TimingSegments, session_step,
};
use cadence_core::{CoreError, EventSeq, ExerciseId, FrameRevision, Micros, MonoTick};
use cadence_trainer::{Phase, SessionPlan};
pub use events::{EventEnvelope, EventKind, Key};
use storage::Storage;
pub use view::{
    Baseline, LearningView, PauseReason, PreparedView, ProbeView, ReviewContext, ScreenView, Slip,
    StatusFigures, TargetGroup, UnitSpan,
};
use view::{MINIMUM_COLUMNS, MINIMUM_ROWS, TextSize, TextSupport};

/// Geometry below the full-practice minimum can never accept scored input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewportState {
    Usable,
    TooSmall,
}

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
    /// A ticketed job started by this event (#75).
    pub job: Option<jobs::JobTicket>,
    /// The causal group of the dispatched checkpoint (#71): the event that
    /// caused it, or, for a follow-up dispatched when an acknowledgement is
    /// consumed, its parent's group.
    pub checkpoint_group: Option<u64>,
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
            checkpoint_group: None,
            job: None,
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
    /// Sequence of the open exercise's ledger: every opened ledger gets a
    /// new one, so a closing exercise is identified even when the exercise
    /// counter does not advance (a deadline cuts it).
    opened: u64,
    /// The last ledger sequence whose counters were merged into a track.
    last_credited: Option<u64>,
    /// The current phase's prepared list has wrapped to its start (#39;
    /// shown from #7).
    list_wrapped: bool,
    /// Admitted events so far (#69): rises on every admitted event.
    state_revision: u64,
    /// Ticketed worker jobs (#75).
    jobs: jobs::Jobs,
    /// The group of the checkpoint whose acknowledgement is being consumed
    /// (#71): a follow-up dispatched now inherits it.
    ack_group: Option<u64>,
    /// Practice paused during the transfer probe (#64).
    probe_paused: bool,
    /// A paste was refused during the transfer probe (#64).
    probe_pasted: bool,
    /// The UTC offset in minutes the session's day is counted in (#52).
    utc_offset_minutes: Option<i32>,
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
    learning: learning::Learning,
    status: ProgressStatus,
    checkpoint_due: bool,
    finishing: bool,
    text_size: TextSize,
    text_support: TextSupport,
    viewport: ViewportState,
    pause_reason: Option<PauseReason>,
    interlude_opened: Option<MonoTick>,
    ended_early: bool,
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
            // The closing review (#49) may be typed; it is always last.
            if p.phase == Phase::Review && index.checked_add(1) != Some(plan.phases.len()) {
                return Err(invalid("review must be last"));
            }
            Micros::from_seconds(p.seconds)?;
            if p.phase != Phase::Review && (p.exercises.is_empty() || p.seconds == 0) {
                return Err(invalid("empty practice phase"));
            }
            if p.phase == Phase::Review && !p.exercises.is_empty() && p.seconds == 0 {
                return Err(invalid("a typed review needs time"));
            }
            for e in &p.exercises {
                if e.units.is_empty()
                    || e.units.len()
                        > usize::try_from(cadence_core::TextUnitIndex::MAXIMUM.get())
                            .unwrap_or(usize::MAX)
                    || !practice::kind_fits(e.kind, p.phase)?
                {
                    return Err(invalid("invalid exercise target or phase kind"));
                }
            }
        }
        let learning = learning::Learning::new(
            {
                // The plan's pace goals are part of the session's record (#62).
                let mut evidence =
                    cadence_core::learning::SessionLearning::new(plan.record.clone());
                evidence.pace.clone_from(&plan.pace);
                evidence
            },
            plan.adaptation,
        );
        let notice = (plan.probe == cadence_trainer::ProbePlan::NoPassage).then(|| {
            "A transfer probe is due, but every probe passage has been typed.".to_string()
        });
        Ok(Self {
            plan,
            learning,
            screen: Screen::Today,
            phase_index: 0,
            exercise_index: 0,
            next_exercise: 1,
            ledger: None,
            timing: TimingSegments::new(),
            session_state: SessionState::Ready,
            opened: 0,
            last_credited: None,
            list_wrapped: false,
            state_revision: 0,
            jobs: jobs::Jobs::default(),
            ack_group: None,
            probe_paused: false,
            probe_pasted: false,
            utc_offset_minutes: None,
            phase_start: Micros::ZERO,
            exercise_start: Micros::ZERO,
            active: Micros::ZERO,
            tracks: Vec::with_capacity(practice::TRACK_CAPACITY),
            consumed: EventSeq::INITIAL,
            last_at: None,
            revision: FrameRevision::INITIAL,
            invalid: false,
            notice,
            history: HistorySummary::default(),
            storage: None,
            status: ProgressStatus::Running,
            checkpoint_due: false,
            finishing: false,
            text_size: TextSize::Auto,
            text_support: TextSupport::NormalOnly,
            viewport: ViewportState::Usable,
            pause_reason: None,
            interlude_opened: None,
            ended_early: false,
        })
    }
    /// Records the UTC offset (minutes) the session's day was counted in,
    /// so it is saved with the session (#52). Set once, before the session;
    /// without a detected local time zone the start screen says days are
    /// counted in UTC.
    #[must_use]
    pub fn with_utc_offset(mut self, minutes: Option<i32>, detected: bool) -> Self {
        self.utc_offset_minutes = minutes;
        if !detected {
            self.notice = Some("Days are counted in UTC (no local time zone found).".to_string());
        }
        self
    }
    /// Whether the current phase's prepared list has wrapped to its start:
    /// the last prepared exercise was finished and the first is served again.
    #[must_use]
    pub fn list_wrapped(&self) -> bool {
        self.list_wrapped
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

    /// The state revision (#69): the number of admitted events, which a
    /// headless request's `expected_state_revision` is checked against.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// The ticketed job table (#75).
    #[must_use]
    pub const fn jobs(&self) -> &jobs::Jobs {
        &self.jobs
    }

    /// The newest dispatched checkpoint sequence and the newest
    /// acknowledged one (#71 durable barriers).
    #[must_use]
    pub fn save_watermarks(&self) -> (Option<u64>, Option<u64>) {
        self.storage
            .as_ref()
            .map_or((None, None), |storage| storage.watermarks())
    }

    /// Exact track totals now, including the open exercise (#69 snapshot).
    ///
    /// # Errors
    /// Refuses counter overflow.
    pub fn track_totals(&self) -> Result<Vec<cadence_core::progress::TrackProgress>, CoreError> {
        let mut tracks = self.tracks.clone();
        if let Some(open) = self.current_progress()? {
            practice::merge_track(&mut tracks, open)?;
        }
        Ok(tracks)
    }
    /// Applies one sequenced event, with no I/O or background mutation.
    pub fn reduce(&mut self, envelope: EventEnvelope) -> Transition {
        let before = self.consumed;
        let result = self.reduce_checked(envelope);
        if self.consumed != before {
            self.state_revision = self.state_revision.saturating_add(1);
        }
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
            EventKind::SetTextSize(size) => self.text_size = size,
            EventKind::SetTextSupport(support) => self.text_support = support,
            EventKind::Key(Key::ToggleTextSize) => self.text_size = self.text_size.next(),
            EventKind::Resized { columns, rows } => self.resize(at, columns, rows)?,
            EventKind::StorageReady {
                next_session,
                day,
                history,
            } => self.storage_ready(next_session, day, history)?,
            EventKind::SaveResult(result) => self.save_result(result, &mut t)?,
            EventKind::StartJob(kind) => t.job = self.jobs.start(kind),
            EventKind::JobResult { ticket, outcome } => {
                self.jobs.result(ticket, outcome);
            }
            EventKind::RestartJobs => self.jobs.restart(),
            EventKind::CheckpointRequested => {
                if self.storage.is_some() && !self.finishing {
                    self.checkpoint_due = true;
                }
            }
            EventKind::Key(Key::Interrupt) => self.interrupt(at, &mut t)?,
            EventKind::Terminate => self.request_exit(at, &mut t)?,
            EventKind::Key(Key::Content(c)) if self.screen == Screen::Today && c.get() == 'q' => {
                self.request_exit(at, &mut t)?
            }
            EventKind::Key(Key::Enter | Key::Escape) if self.screen == Screen::Summary => {
                self.request_exit(at, &mut t)?
            }
            EventKind::PasteRejected if !self.finishing && !self.invalid => {
                self.pause_for(at, PauseReason::PasteRejected)?;
            }
            EventKind::FocusLost if !self.finishing && !self.invalid => {
                self.pause_for(at, PauseReason::FocusLost)?;
            }
            EventKind::Key(Key::Content(c))
                if c.get() == 'r'
                    && (self.session_state == SessionState::Paused
                        || self.storage.as_ref().is_some_and(|s| s.failed.is_some())) =>
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
            // Guarded arms above did not apply (finishing, invalid, or a
            // key owned by no screen): the event changes nothing.
            EventKind::PasteRejected
            | EventKind::FocusLost
            | EventKind::Key(_)
            | EventKind::Tick => {}
        }
        self.watchdog(at, &mut t)?;
        self.dispatch_checkpoint(at, &mut t)?;
        // Only a follow-up dispatched in this reduction inherits (#71).
        self.ack_group = None;
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
        // The table admits Invalidate in every state (Lean:
        // `Session.invalidate_admitted`); the clock was stopped above.
        if let SessionStep::Move { next, .. } =
            session_step(self.session_state, SessionEvent::Invalidate)
        {
            self.session_state = next;
        }
        self.notice = Some(format!(
            "Measurement stopped: {}. Ctrl-C keeps the partial evidence; no qualified score.",
            flow::describe(&reason)
        ));
        self.checkpoint_due = true;
        Transition::changed(DomainOutcome::MeasurementInvalid { reason })
    }
    fn apply_kind(&mut self, at: MonoTick, kind: EventKind) -> Result<DomainOutcome, CoreError> {
        match (&self.screen, kind) {
            (Screen::Today, EventKind::Key(Key::Enter))
                if self.viewport == ViewportState::Usable =>
            {
                self.screen = Screen::Practice;
                self.transition(Some(at), SessionEvent::Start)?;
                self.open_exercise(at)?;
                Ok(DomainOutcome::SessionStarted)
            }
            (Screen::PhaseComplete { .. }, EventKind::Key(Key::Enter | Key::Escape))
                if self.viewport == ViewportState::Usable && self.interlude_settled(at)? =>
            {
                self.advance_phase(at)
            }
            (Screen::Practice, EventKind::Key(key)) => self.practice_key(at, key),
            _ => Ok(DomainOutcome::None),
        }
    }

    fn resize(&mut self, at: MonoTick, columns: u16, rows: u16) -> Result<(), CoreError> {
        self.viewport = if columns < MINIMUM_COLUMNS || rows < MINIMUM_ROWS {
            ViewportState::TooSmall
        } else {
            ViewportState::Usable
        };
        if self.viewport == ViewportState::TooSmall
            && self.screen == Screen::Practice
            && self.session_state == SessionState::Running
        {
            self.transition(Some(at), SessionEvent::Pause)?;
            self.pause_reason = Some(PauseReason::TooSmall);
            self.checkpoint_due = true;
        }
        Ok(())
    }
}
fn invalid(reason: &'static str) -> CoreError {
    CoreError::InvalidIdentity {
        kind: "Engine",
        reason,
    }
}
