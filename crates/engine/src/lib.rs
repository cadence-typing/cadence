//! `cadence-engine` — the single state owner (A-01, A-02).
//!
//! The public mutation API is [`Engine::reduce`]: bounded owned effects,
//! dirty/presentation information and domain outcomes. View preparation
//! is explicit ([`Engine::prepare`]); Ratatui paint receives
//! `&PreparedView`. Paint cannot correct scroll state, initiate I/O,
//! alter counters or advance time.
//!
//! P1 scope: the vertical slice — Today/start, one practice exercise
//! with error/cursor feedback, pause/resume with clock boundaries,
//! result screen, normal quit. Phase budgets and the planner land with
//! P2; workers and persistence with P3.

pub mod events;
pub mod view;

pub use events::{EventEnvelope, EventKind, Key};
pub use view::{PreparedView, ScreenView, StatusFigures, UnitSpan};

use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar, validate_target};
use cadence_core::ledger::{Applied, AttemptLedger, ExtraInputAllowance, LedgerEvent};
use cadence_core::metrics::{
    ExerciseMetrics, Ratio, correct_wpm, exercise_metrics, first_pass_accuracy,
};
use cadence_core::session::{SessionState, SessionTransition, TimingSegments, next_state};
use cadence_core::tick::Micros;
use cadence_core::{CoreError, EventSeq, ExerciseId, FrameRevision, MonoTick};

/// Which interactive surface owns input right now (U-03, H-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputOwner {
    /// Menus: arrows/Tab/Enter navigate, `q` quits, `?` help.
    Navigation,
    /// Active practice: all printable characters are training input.
    Practice,
}

/// The outcome facts of one reduction, for tracing and tests (VR-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainOutcome {
    /// Nothing notable.
    None,
    /// A drill started.
    DrillStarted {
        /// The new exercise identity.
        exercise: ExerciseId,
    },
    /// The exercise completed (buffer equals target).
    ExerciseCompleted {
        /// The finished exercise.
        exercise: ExerciseId,
        /// First-pass accuracy numerator/denominator, exact.
        first_pass: Option<(u128, u128)>,
    },
    /// The exercise was abandoned/partial at quit or phase end.
    ExercisePartial {
        /// The abandoned exercise.
        exercise: ExerciseId,
    },
    /// Measurement became invalid (backward time, over-bound input);
    /// the session pauses scoring rather than emitting a normal result
    /// (L-03). The reason travels for the diagnostic surface.
    MeasurementInvalid {
        /// Why measurement was invalidated.
        reason: CoreError,
    },
}

/// The result of one reduction: bounded owned effects, dirty flag and
/// domain outcome (A-01). P1 has no async effects yet; the shape is the
/// contract the P3 transport plugs into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    /// Whether presentation should be reprepared and repainted.
    pub dirty: bool,
    /// Whether the run should terminate through the normal quit path.
    pub quit: bool,
    /// Domain facts for tracing/tests.
    pub outcome: DomainOutcome,
}

/// The engine's private application state. One owner thread; no sibling
/// mutation, no `Arc<Mutex<..>>` anywhere (A-02).
#[derive(Debug, Clone, PartialEq, Eq)]
struct EngineState {
    screen: Screen,
    session: Option<PracticeSession>,
    next_exercise: u64,
    revision: FrameRevision,
    /// Pre-validated targets offered by the P1 experimental slice; P2
    /// replaces this with the planner's committed observations.
    slice_targets: Vec<Vec<TrainingChar>>,
    /// The validated extra-input allowance (policy
    /// `maximum_extra_input_units`).
    allowance: ExtraInputAllowance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Screen {
    Today,
    Practice,
    Result { lines: Vec<String>, completed: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PracticeSession {
    exercise: ExerciseId,
    ledger: AttemptLedger,
    timing: TimingSegments,
    state: SessionState,
}

/// The reducer. Construct, then feed [`EventEnvelope`]s in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    state: EngineState,
    consumed: EventSeq,
}

impl Engine {
    /// Creates the engine for the P1 experimental slice with the given
    /// target texts (validated once against the V1 alphabet; targets
    /// are rejected before a scored session, L-02).
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidIdentity`] when a slice target is
    /// outside the training alphabet or the list is empty.
    pub fn new_slice(slice_targets: Vec<String>) -> Result<Self, CoreError> {
        let mut validated = Vec::with_capacity(slice_targets.len());
        for target in &slice_targets {
            let units = validate_target(target, LayoutSupportsPound::US).map_err(|_| {
                CoreError::InvalidIdentity {
                    kind: "Engine::new_slice",
                    reason: "slice target outside the training alphabet",
                }
            })?;
            validated.push(units);
        }
        if validated.is_empty() {
            return Err(CoreError::InvalidIdentity {
                kind: "Engine::new_slice",
                reason: "the experimental slice needs at least one target",
            });
        }
        // Named policy limit mirrored from policies/trainer-v1.toml
        // [runtime] maximum_extra_input_units = 128.
        const MAXIMUM_EXTRA_INPUT_UNITS: u32 = 128;
        let allowance = ExtraInputAllowance::new(MAXIMUM_EXTRA_INPUT_UNITS)?;
        Ok(Self {
            state: EngineState {
                screen: Screen::Today,
                session: None,
                next_exercise: 1,
                revision: FrameRevision::INITIAL,
                slice_targets: validated,
                allowance,
            },
            consumed: EventSeq::INITIAL,
        })
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
        // Ordered admission: the event sequence must advance by one
        // from the last consumed event; gaps and replays are refused
        // without state mutation (A-07 trace rule).
        if envelope.seq.get() != self.consumed.get().saturating_add(1) {
            return no_change();
        }
        self.consumed = envelope.seq;
        let transition = self.apply_kind(envelope.at, envelope.kind);
        if transition.dirty {
            self.state.revision = self.state.revision.next().unwrap_or(self.state.revision);
        }
        transition
    }

    /// Applies an event kind at a timestamp (single dispatch table).
    fn apply_kind(&mut self, at: MonoTick, kind: EventKind) -> Transition {
        match (&self.state.screen, kind) {
            (Screen::Today, EventKind::Key(Key::Enter)) => self.start_drill(),
            (Screen::Today, EventKind::Key(Key::Interrupt)) => quit(),
            (Screen::Today, EventKind::Key(Key::Content(character))) if character.get() == 'q' => {
                quit()
            }
            (Screen::Result { .. }, EventKind::Key(Key::Escape | Key::Enter)) => {
                self.state.screen = Screen::Today;
                dirty()
            }
            (Screen::Result { .. }, EventKind::Key(Key::Interrupt)) => quit(),
            (Screen::Practice, EventKind::Key(key)) => self.practice_key(at, key),
            (Screen::Practice, EventKind::Resized { .. } | EventKind::Tick) => dirty(),
            (_, _) => no_change(),
        }
    }

    /// Starts the next slice drill.
    fn start_drill(&mut self) -> Transition {
        let zero_based = self.state.next_exercise.saturating_sub(1);
        let len = u64::try_from(self.state.slice_targets.len()).unwrap_or(u64::MAX);
        let index = usize::try_from(zero_based.checked_rem(len.max(1)).unwrap_or(0)).unwrap_or(0);
        let Some(units) = self.state.slice_targets.get(index).cloned() else {
            return measurement_invalid(CoreError::InvalidIdentity {
                kind: "Engine::start_drill",
                reason: "slice targets list became empty",
            });
        };
        let ledger = match AttemptLedger::new(units, self.state.allowance) {
            Ok(ledger) => ledger,
            Err(error) => return measurement_invalid(error),
        };
        let exercise = match ExerciseId::new(self.state.next_exercise) {
            Ok(id) => id,
            Err(error) => return measurement_invalid(error),
        };
        self.state.next_exercise = self.state.next_exercise.saturating_add(1);
        self.state.session = Some(PracticeSession {
            exercise,
            ledger,
            timing: TimingSegments::new(),
            state: SessionState::Running,
        });
        self.state.screen = Screen::Practice;
        Transition {
            dirty: true,
            quit: false,
            outcome: DomainOutcome::DrillStarted { exercise },
        }
    }

    /// Practice-screen key handling: training input, pause/resume.
    fn practice_key(&mut self, at: MonoTick, key: Key) -> Transition {
        let Some(session) = self.state.session.as_mut() else {
            self.state.screen = Screen::Today;
            return dirty();
        };
        match (session.state, key) {
            (SessionState::Running, Key::Content(character)) => {
                // First valid content input starts the active clock.
                if let Err(error) = session.timing.first_content(at) {
                    return measurement_invalid(error);
                }
                match session.ledger.apply(LedgerEvent::Content(character)) {
                    Ok(Applied::Content { .. }) => {
                        if session.ledger.is_complete() {
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
            (SessionState::Running, Key::Backspace) => {
                match session.ledger.apply(LedgerEvent::Backspace) {
                    Ok(_) => dirty(),
                    Err(error) => measurement_invalid(error),
                }
            }
            (SessionState::Running, Key::Escape) => {
                // Pause records a clock boundary before changing state.
                if let Err(error) = session.timing.pause(at) {
                    return measurement_invalid(error);
                }
                session.state = step_session(session.state, SessionTransition::Pause);
                dirty()
            }
            (SessionState::Paused, Key::Escape | Key::Enter) => {
                if let Err(error) = session.timing.resume(at) {
                    return measurement_invalid(error);
                }
                session.state = step_session(session.state, SessionTransition::Resume);
                dirty()
            }
            (SessionState::Paused | SessionState::Running, Key::Interrupt) => {
                // Controlled quit: retain the partial exercise facts.
                let exercise = session.exercise;
                session.ledger.mark_interrupted();
                self.state.screen = Screen::Today;
                Transition {
                    dirty: true,
                    quit: true,
                    outcome: DomainOutcome::ExercisePartial { exercise },
                }
            }
            _ => no_change(),
        }
    }

    /// Computes the exercise result and shows it (P3 adds durable
    /// commitment).
    fn finish_exercise(&mut self, at: MonoTick) -> Transition {
        let Some(session) = self.state.session.as_mut() else {
            self.state.screen = Screen::Today;
            return dirty();
        };
        if let Err(error) = session.timing.pause(at) {
            return measurement_invalid(error);
        }
        let active = session.timing.active_through(at).unwrap_or(Micros::ZERO);
        let metrics = exercise_metrics(&session.ledger, active);
        let first_pass = metrics
            .first_pass_accuracy
            .map(|ratio| (ratio.numerator(), ratio.denominator()));
        let exercise = session.exercise;
        let lines = result_lines(&metrics);
        self.state.screen = Screen::Result {
            lines,
            completed: metrics.completed,
        };
        Transition {
            dirty: true,
            quit: false,
            outcome: DomainOutcome::ExerciseCompleted {
                exercise,
                first_pass,
            },
        }
    }

    /// Prepares the immutable view snapshot (A-01: explicit operation;
    /// copies cheap IDs and visible spans, never whole-profile work).
    #[must_use]
    pub fn prepare(&self, now: MonoTick) -> PreparedView {
        let screen = match &self.state.screen {
            Screen::Today => ScreenView::Today {
                plan_hint: "experimental slice — adaptive plan arrives with P2".to_string(),
            },
            Screen::Practice => {
                let Some(session) = self.state.session.as_ref() else {
                    return PreparedView {
                        revision: self.state.revision,
                        screen: ScreenView::Today {
                            plan_hint: "experimental slice".to_string(),
                        },
                        active: Micros::ZERO,
                    };
                };
                let active = session.timing.active_through(now).unwrap_or(Micros::ZERO);
                ScreenView::Practice {
                    spans: spans_for(&session.ledger),
                    status: StatusFigures {
                        wpm: live_wpm(&session.ledger, active),
                        first_pass_percent: percent_of(first_pass_accuracy(&session.ledger)),
                        active_seconds: active.as_us().checked_div(1_000_000).unwrap_or(0),
                        paused: session.state == SessionState::Paused,
                    },
                    paused: session.state == SessionState::Paused,
                }
            }
            Screen::Result { lines, completed } => ScreenView::Result {
                lines: lines.clone(),
                completed: *completed,
            },
        };
        let active = self
            .state
            .session
            .as_ref()
            .and_then(|session| session.timing.active_through(now).ok())
            .unwrap_or(Micros::ZERO);
        PreparedView {
            revision: self.state.revision,
            screen,
            active,
        }
    }
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
    if buffer.len() > target.len() {
        for typed in buffer.get(target.len()..).unwrap_or_default() {
            spans.push(UnitSpan::Wrong {
                typed: typed.get(),
                expected: ' ',
            });
        }
    }
    spans
}

fn result_lines(metrics: &ExerciseMetrics) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "first-pass {}/{}  attempt {}/{}  deletions {}",
        metrics.first_correct,
        metrics.first_attempted,
        metrics.correct_attempts,
        metrics.content_attempts,
        metrics.deletions
    ));
    if let Some(ratio) = metrics.correct_wpm {
        lines.push(format!("correct wpm {:.1}", ratio.as_f64()));
    } else {
        lines.push("correct wpm — (too little active time)".to_string());
    }
    if metrics.clean_completion {
        lines.push("clean completion".to_string());
    } else {
        lines.push("not a clean first attempt — corrections recorded honestly".to_string());
    }
    lines
}

fn percent_of(ratio: Option<Ratio>) -> Option<u16> {
    ratio.map(|ratio| {
        // Integer truncating percent from the exact ratio; no float
        // rounding at a display boundary.
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
    // Backward time or counter exhaustion invalidates measurement
    // rather than producing a normal score (L-03); the reason is
    // carried in the outcome for the P3 diagnostic surface.
    Transition {
        dirty: true,
        quit: false,
        outcome: DomainOutcome::MeasurementInvalid { reason: error },
    }
}
