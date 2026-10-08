//! `cadence-engine` — the single state owner (A-01, A-02, A-05).
//!
//! The public mutation API is [`Engine::reduce`]: bounded owned effects,
//! dirty/presentation information and domain outcomes. View preparation
//! is explicit ([`Engine::prepare`]); Ratatui paint receives
//! `&PreparedView`. Paint cannot correct scroll state, initiate I/O,
//! alter counters or advance time.
//!
//! P2 scope: plan-driven sessions — the deterministic
//! [`SessionPlan`](cadence_trainer::SessionPlan) sequences phases
//! (warm-up → adaptive words → number row → numpad → prose → review)
//! with exact second budgets measured on the active clock. At a phase
//! deadline the scored slice ends at that event boundary, the partial
//! exercise is retained, and the next phase is offered (P-02). Workers
//! and durable checkpoints land with P3; today the plan is built
//! deterministically at session start (bounded in-memory work).

pub mod events;
pub mod view;

pub use events::{EventEnvelope, EventKind, Key};
pub use view::{PreparedView, ScreenView, StatusFigures, UnitSpan};

use cadence_core::alphabet::{LayoutSupportsPound, validate_target};
use cadence_core::ledger::{Applied, AttemptLedger, ExtraInputAllowance, LedgerEvent};
use cadence_core::metrics::{Ratio, correct_wpm, exercise_metrics, first_pass_accuracy};
use cadence_core::session::{SessionState, SessionTransition, TimingSegments, next_state};
use cadence_core::tick::Micros;
use cadence_core::{CoreError, EventSeq, ExerciseId, FrameRevision, MonoTick};
use cadence_trainer::{ExerciseKind, Phase, PlannedExercise, SessionPlan};

/// Which interactive surface owns input right now (U-03, H-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOwner {
    /// Menus: Enter continues, `q` quits, `?` help.
    Navigation,
    /// Active practice: all printable characters are training input.
    Practice,
}

/// The outcome facts of one reduction, for tracing and tests (VR-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainOutcome {
    /// Nothing notable.
    None,
    /// The session plan started.
    SessionStarted,
    /// A phase started.
    PhaseStarted {
        /// Which phase.
        phase: Phase,
    },
    /// The exercise completed (buffer equals target).
    ExerciseCompleted {
        /// The finished exercise.
        exercise: ExerciseId,
        /// First-pass accuracy numerator/denominator, exact.
        first_pass: Option<(u128, u128)>,
    },
    /// The phase reached its deadline with a partial exercise.
    PhaseDeadline {
        /// Which phase ended.
        phase: Phase,
        /// Whether an exercise was left partial.
        partial: bool,
    },
    /// The session reached its summary.
    SessionSummary,
    /// The user quit mid-session; partial facts are retained.
    SessionAbandoned,
    /// Measurement became invalid (backward time, over-bound input);
    /// the session pauses scoring rather than emitting a normal result
    /// (L-03).
    MeasurementInvalid {
        /// Why measurement was invalidated.
        reason: CoreError,
    },
}

/// The result of one reduction: bounded owned effects, dirty flag and
/// domain outcome (A-01). P3 plugs the async transport into this shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    /// Whether presentation should be reprepared and repainted.
    pub dirty: bool,
    /// Whether the run should terminate through the normal quit path.
    pub quit: bool,
    /// Domain facts for tracing/tests.
    pub outcome: DomainOutcome,
}

/// Aggregate facts for one finished exercise, retained for the summary
/// (numerators and denominators, never averaged percentages — L-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExerciseRecord {
    kind: ExerciseKind,
    first_correct: u64,
    first_attempted: u64,
    correct_attempts: u64,
    content_attempts: u64,
    correct_visible_units: u64,
    /// Session-active time when this exercise finished (for WPM at the
    /// aggregate level the exact per-exercise split arrives with P3's
    /// observation ledger; the summary uses numerators/denominators).
    active_at_finish: Micros,
}

/// The engine's private application state. One owner thread; no sibling
/// mutation, no `Arc<Mutex<..>>` anywhere (A-02).
#[derive(Debug, Clone, PartialEq, Eq)]
struct EngineState {
    screen: Screen,
    plan: SessionPlan,
    phase_index: usize,
    exercise_index: usize,
    ledger: Option<AttemptLedger>,
    timing: TimingSegments,
    session_state: SessionState,
    /// Active time accumulated when the current phase started.
    phase_start_active: Micros,
    records: Vec<ExerciseRecord>,
    next_exercise: u64,
    revision: FrameRevision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
    Today,
    Practice,
    PhaseComplete { partial: bool },
    Summary,
}

/// The reducer. Construct with a deterministic plan, then feed
/// [`EventEnvelope`]s in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    state: EngineState,
    consumed: EventSeq,
}

impl Engine {
    /// Creates the engine for a planned session.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidIdentity`] when the plan carries no
    /// phases, or [`CoreError::CapacityExhausted`] when an exercise
    /// violates the length bound (targets were validated at plan build;
    /// this is the engine-side guard).
    pub fn new_plan(plan: SessionPlan) -> Result<Self, CoreError> {
        if plan.phases.is_empty() {
            return Err(CoreError::InvalidIdentity {
                kind: "Engine::new_plan",
                reason: "the session plan needs at least one phase",
            });
        }
        for phase in &plan.phases {
            for exercise in &phase.exercises {
                if exercise.units.len() > cadence_core::TextUnitIndex::MAXIMUM.get() as usize {
                    return Err(CoreError::CapacityExhausted {
                        limit: "maximum target units",
                    });
                }
            }
        }
        Ok(Self {
            state: EngineState {
                screen: Screen::Today,
                plan,
                phase_index: 0,
                exercise_index: 0,
                ledger: None,
                timing: TimingSegments::new(),
                session_state: SessionState::Ready,
                phase_start_active: Micros::ZERO,
                records: Vec::new(),
                next_exercise: 1,
                revision: FrameRevision::INITIAL,
            },
            consumed: EventSeq::INITIAL,
        })
    }

    /// Convenience for tests and the legacy slice: a single-phase plan
    /// with one exercise per target (labelled adaptive words).
    ///
    /// # Errors
    /// Propagates alphabet/bound violations from the targets.
    pub fn new_slice(slice_targets: Vec<String>) -> Result<Self, CoreError> {
        let mut exercises = Vec::with_capacity(slice_targets.len());
        for target in &slice_targets {
            let units = validate_target(target, LayoutSupportsPound::US).map_err(|_| {
                CoreError::InvalidIdentity {
                    kind: "Engine::new_slice",
                    reason: "slice target outside the training alphabet",
                }
            })?;
            exercises.push(PlannedExercise {
                units,
                kind: ExerciseKind::Words,
            });
        }
        if exercises.is_empty() {
            return Err(CoreError::InvalidIdentity {
                kind: "Engine::new_slice",
                reason: "the experimental slice needs at least one target",
            });
        }
        let plan = SessionPlan {
            budget_seconds: 60,
            policy_digest: cadence_core::ContentDigest::from_bytes([0; 32]),
            corpus_digest: cadence_core::ContentDigest::from_bytes([0; 32]),
            seed: 0,
            phases: vec![cadence_trainer::PlannedPhase {
                phase: Phase::AdaptiveWords,
                seconds: 60,
                exercises,
            }],
        };
        Self::new_plan(plan)
    }

    /// The input owner for the current screen (U-03).
    #[must_use]
    pub fn input_owner(&self) -> InputOwner {
        match self.state.screen {
            Screen::Practice => InputOwner::Practice,
            _ => InputOwner::Navigation,
        }
    }

    /// Admitted event count so far (trace consumption order, A-07).
    #[must_use]
    pub fn consumed_events(&self) -> EventSeq {
        self.consumed
    }

    /// Applies one admitted event. Synchronous, bounded, no I/O.
    pub fn reduce(&mut self, envelope: EventEnvelope) -> Transition {
        // Ordered admission: the event sequence must advance by one from
        // the last consumed event; gaps and replays are refused without
        // state mutation (A-07 trace rule).
        if envelope.seq.get() != self.consumed.get().saturating_add(1) {
            return no_change();
        }
        self.consumed = envelope.seq;
        // Phase deadlines are evaluated against the event's own
        // timestamp (A-05): keys at/after the boundary are outside the
        // scored slice; the boundary event itself closes the phase.
        if self.deadline_exceeded(envelope.at) {
            return self.close_phase(true, envelope.at);
        }
        let transition = self.apply_kind(envelope.at, envelope.kind);
        if transition.dirty {
            self.state.revision = self.state.revision.next().unwrap_or(self.state.revision);
        }
        transition
    }

    /// Whether the current phase's budget is exhausted at `at`
    /// (half-open: keys at/after the deadline are outside, A-05).
    fn deadline_exceeded(&self, at: MonoTick) -> bool {
        if self.state.screen != Screen::Practice
            || self.state.session_state != SessionState::Running
        {
            return false;
        }
        let Some(current) = self.state.plan.phases.get(self.state.phase_index) else {
            return false;
        };
        if current.exercises.is_empty() {
            return false;
        }
        let Ok(active) = self.state.timing.active_through(at) else {
            return false;
        };
        let Ok(elapsed_in_phase) = active.checked_sub(self.state.phase_start_active) else {
            return false;
        };
        match Micros::from_seconds(current.seconds) {
            Ok(budget) => elapsed_in_phase.as_us() >= budget.as_us(),
            Err(_) => false,
        }
    }

    /// Closes the current phase, retaining partial facts when the
    /// deadline hit mid-exercise, and offers the next phase (P-02: no
    /// forced overtime).
    fn close_phase(&mut self, partial: bool, at: MonoTick) -> Transition {
        let phase = self
            .state
            .plan
            .phases
            .get(self.state.phase_index)
            .map(|phase| phase.phase)
            .unwrap_or(Phase::Review);
        let active = self.state.timing.active_through(at).unwrap_or(Micros::ZERO);
        if let Some(ledger) = self.state.ledger.take() {
            self.record_exercise_at(ledger, partial, active);
        }
        self.state.screen = Screen::PhaseComplete { partial };
        self.state.session_state = SessionState::PhaseComplete;
        dirty_with(DomainOutcome::PhaseDeadline { phase, partial })
    }

    /// Applies an event kind at a timestamp (single dispatch table).
    fn apply_kind(&mut self, at: MonoTick, kind: EventKind) -> Transition {
        match (&self.state.screen, kind) {
            (Screen::Today, EventKind::Key(Key::Enter)) => self.start_session(),
            (Screen::Today, EventKind::Key(Key::Interrupt)) => quit(),
            (Screen::Today, EventKind::Key(Key::Content(character))) if character.get() == 'q' => {
                quit()
            }
            (Screen::PhaseComplete { .. }, EventKind::Key(Key::Enter | Key::Escape)) => {
                self.advance_phase(at)
            }
            (Screen::PhaseComplete { .. }, EventKind::Key(Key::Interrupt)) => quit(),
            (Screen::Summary, EventKind::Key(Key::Escape | Key::Enter)) => {
                self.state.screen = Screen::Today;
                self.reset_session_progress();
                dirty()
            }
            (Screen::Summary, EventKind::Key(Key::Interrupt)) => quit(),
            (Screen::Practice, EventKind::Key(key)) => self.practice_key(at, key),
            (Screen::Practice, EventKind::Resized { .. } | EventKind::Tick) => dirty(),
            (_, _) => no_change(),
        }
    }

    /// Starts the session: the plan is already frozen at build; the
    /// first phase's clock baseline is zero active time (A-05).
    fn start_session(&mut self) -> Transition {
        self.reset_session_progress();
        self.state.screen = Screen::Practice;
        self.state.session_state = SessionState::Running;
        self.state.phase_start_active = Micros::ZERO;
        self.open_exercise();
        dirty_with(DomainOutcome::SessionStarted)
    }

    fn reset_session_progress(&mut self) {
        self.state.phase_index = 0;
        self.state.exercise_index = 0;
        self.state.ledger = None;
        self.state.timing = TimingSegments::new();
        self.state.session_state = SessionState::Ready;
        self.state.phase_start_active = Micros::ZERO;
        self.state.records.clear();
        self.state.next_exercise = 1;
    }

    /// Loads the current exercise's ledger (bounded work; the corpus is
    /// already in memory — P3 moves index reads to workers).
    fn open_exercise(&mut self) {
        let units = self.current_exercise_units();
        self.state.ledger = units.and_then(|units| {
            let allowance = match ExtraInputAllowance::new(MAXIMUM_EXTRA_INPUT_UNITS) {
                Ok(allowance) => allowance,
                Err(_) => return None,
            };
            AttemptLedger::new(units, allowance).ok()
        });
    }

    fn current_exercise_units(&self) -> Option<Vec<cadence_core::TrainingChar>> {
        self.state
            .plan
            .phases
            .get(self.state.phase_index)
            .and_then(|phase| phase.exercises.get(self.state.exercise_index))
            .map(|exercise| exercise.units.clone())
    }

    fn current_kind(&self) -> ExerciseKind {
        self.state
            .plan
            .phases
            .get(self.state.phase_index)
            .and_then(|phase| phase.exercises.get(self.state.exercise_index))
            .map(|exercise| exercise.kind)
            .unwrap_or(ExerciseKind::Words)
    }

    /// Advances from PhaseComplete to the next phase or the summary.
    fn advance_phase(&mut self, at: MonoTick) -> Transition {
        let next_index = self.state.phase_index.saturating_add(1);
        let Some(next_phase) = self.state.plan.phases.get(next_index) else {
            self.state.screen = Screen::Summary;
            self.state.session_state = SessionState::Review;
            return dirty_with(DomainOutcome::SessionSummary);
        };
        self.state.phase_index = next_index;
        self.state.exercise_index = 0;
        let phase = next_phase.phase;
        if next_phase.exercises.is_empty() {
            // Non-typing phases (review) complete immediately.
            self.state.screen = Screen::PhaseComplete { partial: false };
            self.state.session_state = SessionState::PhaseComplete;
            return dirty_with(DomainOutcome::PhaseStarted { phase });
        }
        // New phase: baseline the deadline at the current active time.
        if let Ok(active) = self.state.timing.active_through(at) {
            self.state.phase_start_active = active;
        }
        self.state.screen = Screen::Practice;
        self.state.session_state = SessionState::Running;
        self.open_exercise();
        dirty_with(DomainOutcome::PhaseStarted { phase })
    }

    /// Practice-screen key handling: training input, submit, pause.
    fn practice_key(&mut self, at: MonoTick, key: Key) -> Transition {
        // The kind decision is needed inside the match while `ledger`
        // holds a mutable borrow; clone it beforehand.
        let numpad_phase = self.current_kind() == ExerciseKind::Numpad;
        let Some(ledger) = self.state.ledger.as_mut() else {
            return dirty();
        };
        match (self.state.session_state, key) {
            (SessionState::Running, Key::Content(character)) => {
                if let Err(error) = self.state.timing.first_content(at) {
                    return measurement_invalid(error);
                }
                match ledger.apply(LedgerEvent::Content(character)) {
                    Ok(Applied::Content { .. }) => {
                        // Numpad entries complete only via Enter
                        // (submit); word/prose targets complete on
                        // their last unit (L-02).
                        if ledger.is_complete() && !numpad_phase {
                            self.finish_exercise(at)
                        } else {
                            dirty()
                        }
                    }
                    Ok(Applied::RejectedOverBound) => dirty(),
                    Ok(Applied::Deleted | Applied::DeletedNothing) => dirty(),
                    Err(error) => measurement_invalid(error),
                }
            }
            (SessionState::Running, Key::Backspace) => match ledger.apply(LedgerEvent::Backspace) {
                Ok(_) => dirty(),
                Err(error) => measurement_invalid(error),
            },
            (SessionState::Running, Key::Enter) if numpad_phase => {
                // Enter submits only; it must not auto-advance a wrong
                // value (L-02).
                if ledger.is_complete() {
                    self.finish_exercise(at)
                } else {
                    dirty()
                }
            }
            (SessionState::Running, Key::Escape) => {
                if let Err(error) = self.state.timing.pause(at) {
                    return measurement_invalid(error);
                }
                self.state.session_state =
                    step_session(self.state.session_state, SessionTransition::Pause);
                dirty()
            }
            (SessionState::Paused, Key::Escape | Key::Enter) => {
                if let Err(error) = self.state.timing.resume(at) {
                    return measurement_invalid(error);
                }
                self.state.session_state =
                    step_session(self.state.session_state, SessionTransition::Resume);
                dirty()
            }
            (SessionState::Paused | SessionState::Running, Key::Interrupt) => {
                // Controlled quit: retain the partial exercise facts.
                if let Some(ledger) = self.state.ledger.take() {
                    let active = self.state.timing.active_through(at).unwrap_or(Micros::ZERO);
                    self.record_exercise_at(ledger, true, active);
                }
                self.state.screen = Screen::Today;
                self.state.session_state = SessionState::Aborted;
                Transition {
                    dirty: true,
                    quit: true,
                    outcome: DomainOutcome::SessionAbandoned,
                }
            }
            _ => no_change(),
        }
    }

    /// Records the finished exercise and moves to the next one, the
    /// phase boundary, or the summary.
    fn finish_exercise(&mut self, at: MonoTick) -> Transition {
        let exercise = match ExerciseId::new(self.state.next_exercise) {
            Ok(id) => id,
            Err(error) => return measurement_invalid(error),
        };
        self.state.next_exercise = self.state.next_exercise.saturating_add(1);
        let Some(ledger) = self.state.ledger.take() else {
            return dirty();
        };
        if let Err(error) = self.state.timing.pause(at) {
            return measurement_invalid(error);
        }
        let active = self.state.timing.active_through(at).unwrap_or(Micros::ZERO);
        let metrics = exercise_metrics(&ledger, active);
        let first_pass = metrics
            .first_pass_accuracy
            .map(|ratio| (ratio.numerator(), ratio.denominator()));
        self.record_exercise_at(ledger, false, active);
        // Exercise boundaries are not pauses (L-03): only explicit
        // pauses are excluded, so reopen the clock immediately.
        if let Err(error) = self.state.timing.resume(at) {
            return measurement_invalid(error);
        }
        let phase_len = self
            .state
            .plan
            .phases
            .get(self.state.phase_index)
            .map(|phase| phase.exercises.len())
            .unwrap_or(0);
        let next_index = self.state.exercise_index.saturating_add(1);
        if next_index >= phase_len {
            let mut transition = self.close_phase(false, at);
            transition.outcome = DomainOutcome::ExerciseCompleted {
                exercise,
                first_pass,
            };
            return transition;
        }
        self.state.exercise_index = next_index;
        self.open_exercise();
        Transition {
            dirty: true,
            quit: false,
            outcome: DomainOutcome::ExerciseCompleted {
                exercise,
                first_pass,
            },
        }
    }

    fn record_exercise_at(&mut self, ledger: AttemptLedger, interrupted: bool, active: Micros) {
        let metrics = exercise_metrics(&ledger, active);
        self.state.records.push(ExerciseRecord {
            kind: self.current_kind(),
            first_correct: metrics.first_correct,
            first_attempted: metrics.first_attempted,
            correct_attempts: metrics.correct_attempts,
            content_attempts: metrics.content_attempts,
            correct_visible_units: metrics.correct_visible_units,
            active_at_finish: active,
        });
        let _ = interrupted;
    }

    /// Prepares the immutable view snapshot (A-01: explicit operation).
    #[must_use]
    pub fn prepare(&self, now: MonoTick) -> PreparedView {
        let active = self
            .state
            .timing
            .active_through(now)
            .unwrap_or(Micros::ZERO);
        let screen = match &self.state.screen {
            Screen::Today => ScreenView::Today {
                plan_hint: plan_hint(&self.state.plan),
                total_seconds: self.state.plan.budget_seconds,
            },
            Screen::Practice => self.prepare_practice(active),
            Screen::PhaseComplete { partial } => {
                let label = self
                    .state
                    .plan
                    .phases
                    .get(self.state.phase_index)
                    .map(|phase| phase_label(phase.phase))
                    .unwrap_or("phase");
                let next = self
                    .state
                    .plan
                    .phases
                    .get(self.state.phase_index.saturating_add(1))
                    .map(|phase| phase_label(phase.phase));
                ScreenView::PhaseComplete {
                    label: label.to_string(),
                    partial: *partial,
                    next: next.map(String::from),
                }
            }
            Screen::Summary => ScreenView::Summary {
                lines: summary_lines(&self.state.records),
            },
        };
        PreparedView {
            revision: self.state.revision,
            screen,
            active,
        }
    }

    fn prepare_practice(&self, active: Micros) -> ScreenView {
        let Some(ledger) = self.state.ledger.as_ref() else {
            return ScreenView::Today {
                plan_hint: plan_hint(&self.state.plan),
                total_seconds: self.state.plan.budget_seconds,
            };
        };
        let phase = self.state.plan.phases.get(self.state.phase_index);
        let (label, seconds, total) = match phase {
            Some(phase) => (
                phase_label(phase.phase).to_string(),
                phase.seconds,
                phase.exercises.len(),
            ),
            None => ("practice".to_string(), 0, 0),
        };
        let elapsed = active
            .checked_sub(self.state.phase_start_active)
            .unwrap_or(Micros::ZERO);
        let remaining = Micros::from_seconds(seconds)
            .ok()
            .and_then(|budget| budget.checked_sub(elapsed).ok())
            .map(|value| value.as_us().checked_div(1_000_000).unwrap_or(0))
            .unwrap_or(0);
        ScreenView::Practice {
            spans: spans_for(ledger),
            status: StatusFigures {
                wpm: live_wpm(ledger, active),
                first_pass_percent: percent_of(first_pass_accuracy(ledger)),
                active_seconds: active.as_us().checked_div(1_000_000).unwrap_or(0),
                paused: self.state.session_state == SessionState::Paused,
                phase_label: label,
                exercise_index: u32::try_from(self.state.exercise_index.saturating_add(1))
                    .unwrap_or(1),
                exercise_total: u32::try_from(total).unwrap_or(0),
                phase_seconds_remaining: remaining,
                enter_submits: self.current_kind() == ExerciseKind::Numpad,
            },
            paused: self.state.session_state == SessionState::Paused,
        }
    }
}

/// Named policy limit mirrored from policies/trainer-v1.toml
/// [runtime] maximum_extra_input_units = 128 (positive; `new` cannot
/// fail for it — the fallback keeps the helper total without unwrap).
const MAXIMUM_EXTRA_INPUT_UNITS: u32 = 128;

fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::WarmUp => "warm-up",
        Phase::AdaptiveWords => "adaptive words",
        Phase::NumberRow => "number row",
        Phase::Numpad => "numpad",
        Phase::Prose => "prose",
        Phase::Review => "review",
    }
}

fn plan_hint(plan: &SessionPlan) -> String {
    let parts: Vec<String> = plan
        .phases
        .iter()
        .map(|phase| format!("{} {}s", phase_label(phase.phase), phase.seconds))
        .collect();
    parts.join(" · ")
}

fn spans_for(ledger: &AttemptLedger) -> Vec<UnitSpan> {
    let target = ledger.target();
    let buffer = ledger.buffer();
    let mut spans = Vec::with_capacity(target.len());
    for (position, expected) in target.iter().enumerate() {
        let span = match buffer.get(position) {
            Some(typed) if typed == expected => UnitSpan::Correct {
                unit: expected.get(),
            },
            Some(typed) => UnitSpan::Wrong {
                typed: typed.get(),
                expected: expected.get(),
            },
            None if position == buffer.len() => UnitSpan::Cursor {
                expected: expected.get(),
            },
            None => UnitSpan::Upcoming {
                expected: expected.get(),
            },
        };
        spans.push(span);
    }
    if let Some(overflow) = buffer.get(target.len()..) {
        for typed in overflow {
            spans.push(UnitSpan::Wrong {
                typed: typed.get(),
                expected: ' ',
            });
        }
    }
    spans
}

/// Honest aggregate summary: numerators and denominators per track,
/// aggregated across exercises (never averaged percentages, L-02).
fn summary_lines(records: &[ExerciseRecord]) -> Vec<String> {
    let mut lines = Vec::new();
    if records.is_empty() {
        lines.push("no scored exercises this session".to_string());
        return lines;
    }
    for kind in [
        ExerciseKind::WarmUp,
        ExerciseKind::Words,
        ExerciseKind::NumberRow,
        ExerciseKind::Numpad,
        ExerciseKind::Prose,
    ] {
        let track: Vec<&ExerciseRecord> = records
            .iter()
            .filter(|record| record.kind == kind)
            .collect();
        if track.is_empty() {
            continue;
        }
        let first_correct: u64 = track.iter().map(|record| record.first_correct).sum();
        let first_attempted: u64 = track.iter().map(|record| record.first_attempted).sum();
        let correct_attempts: u64 = track.iter().map(|record| record.correct_attempts).sum();
        let content_attempts: u64 = track.iter().map(|record| record.content_attempts).sum();
        let units: u64 = track
            .iter()
            .map(|record| record.correct_visible_units)
            .sum();
        let active_span: u128 = track
            .last()
            .map(|record| u128::from(record.active_at_finish.as_us()))
            .unwrap_or(0);
        lines.push(format!(
            "{}: first-pass {}/{}  attempt {}/{}  correct units {}",
            kind.label(),
            first_correct,
            first_attempted,
            correct_attempts,
            content_attempts,
            units,
        ));
        if let Some(ratio) = correct_wpm(units, micros_of(active_span)) {
            lines.push(format!("   correct wpm {:.1}", ratio.as_f64()));
        } else {
            lines.push("   correct wpm — (too little active time)".to_string());
        }
    }
    lines.push("sparse evidence so far — cadence is still building your baseline".to_string());
    lines
}

fn micros_of(value: u128) -> Micros {
    Micros::from_us(u64::try_from(value).unwrap_or(u64::MAX))
}

fn percent_of(ratio: Option<Ratio>) -> Option<u16> {
    ratio.map(|ratio| {
        ratio
            .numerator()
            .checked_mul(100)
            .and_then(|scaled| scaled.checked_div(ratio.denominator()))
            .and_then(|percent| u16::try_from(percent).ok())
            .unwrap_or(0)
    })
}

fn live_wpm(ledger: &AttemptLedger, active: Micros) -> Option<String> {
    correct_wpm(ledger.correct_visible_units().get(), active)
        .map(|ratio| format!("{:.1}", ratio.as_f64()))
}

fn step_session(current: SessionState, transition: SessionTransition) -> SessionState {
    next_state(current, transition).unwrap_or(current)
}

fn dirty() -> Transition {
    Transition {
        dirty: true,
        quit: false,
        outcome: DomainOutcome::None,
    }
}

fn dirty_with(outcome: DomainOutcome) -> Transition {
    Transition {
        dirty: true,
        quit: false,
        outcome,
    }
}

fn no_change() -> Transition {
    Transition {
        dirty: false,
        quit: false,
        outcome: DomainOutcome::None,
    }
}

fn quit() -> Transition {
    Transition {
        dirty: false,
        quit: true,
        outcome: DomainOutcome::None,
    }
}

fn measurement_invalid(error: CoreError) -> Transition {
    Transition {
        dirty: true,
        quit: false,
        outcome: DomainOutcome::MeasurementInvalid { reason: error },
    }
}
