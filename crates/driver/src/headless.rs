//! The headless host (H-01, H-02, #69): requests drive the same
//! `Engine::reduce` and the same `cadence-tui` paint as live mode (rule 5).
//! Time is a virtual clock: `key` arrives at the current tick, `text`
//! advances `inter_key_microseconds` before each later unit (delivering due
//! clock events first), `advance` moves the clock. Saves go through a
//! service the composition root provides (a fixture or the profile store)
//! and are acknowledged in the reducer before the request answers.

use cadence_core::barrier::{Barrier, causal, durable};
use cadence_core::progress::{Checkpoint, HistorySummary, SaveResult};
use cadence_core::{Layout, MonoTick, SessionId, TrainingChar};
use cadence_engine::{Direction, Engine, EventEnvelope, EventKind, Key, ScreenView};
use serde_json::{Map, Value, json};

use crate::envelope::{Code, DeliverVariant, Operation, Request};
use crate::protocol::Host;

/// What the composition root provides: scenarios and durable saves.
pub trait Services {
    /// The engine for a named fixture scenario, or the real daily plan.
    ///
    /// # Errors
    /// An unknown scenario or content failure.
    fn engine(&mut self, scenario: Option<&str>) -> Result<Engine, String>;
    /// Startup storage facts: the next session id, the local day, history.
    ///
    /// # Errors
    /// Storage failures.
    fn storage(&mut self) -> Result<(SessionId, i64, HistorySummary), String>;
    /// Saves a checkpoint durably; synthetic provenance is the service's.
    ///
    /// # Errors
    /// A failed save, reported to the engine as a save failure.
    fn save(&mut self, checkpoint: &Checkpoint) -> Result<(), String>;
    /// Writes or reloads the settings, as the live config worker does
    /// (#79); fixture scenarios may simulate an external edit.
    fn settings(
        &mut self,
        request: &cadence_engine::settings::SettingsRequest,
    ) -> cadence_engine::jobs::JobOutcome;
    /// The newest `limit` sessions for Progress (#81).
    ///
    /// # Errors
    /// Storage failures.
    fn history(
        &mut self,
        limit: usize,
    ) -> Result<Vec<cadence_core::progress::HistoryPoint>, String>;
    /// The engine again after setup is written (`None`), or for a Train or
    /// Numpad plan (#78, #80).
    ///
    /// # Errors
    /// As [`Services::engine`].
    fn rebuild(&mut self, track: Option<(cadence_engine::Phase, u64)>) -> Result<Engine, String>;
    /// The settings file's text, for snapshots (#86).
    fn config_text(&self) -> Option<String>;
}

/// A settings write held for `deliver` (#86).
struct SettingsJob {
    id: u64,
    ticket: cadence_engine::jobs::JobTicket,
    request: cadence_engine::settings::SettingsRequest,
}

/// A checkpoint save held for `deliver` (manual fixture delivery, #71).
struct Job {
    id: u64,
    group: u64,
    checkpoint: Checkpoint,
}

/// The headless host.
pub struct Headless<S: Services> {
    services: S,
    engine: Option<Engine>,
    now_us: u64,
    columns: u16,
    rows: u16,
    durable_seq: Option<u64>,
    finished: bool,
    /// Saves wait for `deliver` instead of completing at once.
    manual: bool,
    jobs: Vec<Job>,
    settings_jobs: Vec<SettingsJob>,
    next_job: u64,
}

/// Every headless action name, as `contracts/headless-request.schema.json`
/// lists them (decision 0008): the command table's actions and the aliases
/// recorded journeys use.
pub const ACTION_NAMES: &[&str] = &[
    "Begin",
    "StartDailyPlan",
    "StartDrill",
    "Continue",
    "NextPhase",
    "Quit",
    "Pause",
    "Resume",
    "EndAndReview",
    "SaveAndExit",
    "RetrySave",
    "ForceExit",
    "FinishInvalid",
    "SubmitEntry",
    "TextSize",
    "NextDestination",
    "PreviousDestination",
    "Help",
    "Settings",
    "OpenSettings",
    "NextTrack",
    "PreviousTrack",
    "Longer",
    "Shorter",
];

/// A headless action's typed command. `StartDailyPlan` and `StartDrill`
/// (journeys recorded before #80) are `Begin`; `NextPhase` is `Continue`.
fn action_named(name: &str) -> Option<cadence_engine::commands::Action> {
    use cadence_engine::commands::Action;
    Some(match name {
        "Begin" | "StartDailyPlan" | "StartDrill" => Action::Begin,
        "Continue" | "NextPhase" => Action::Continue,
        "Quit" => Action::Quit,
        "Pause" => Action::Pause,
        "Resume" => Action::Resume,
        "EndAndReview" => Action::EndAndReview,
        "SaveAndExit" => Action::SaveAndExit,
        "RetrySave" => Action::RetrySave,
        "ForceExit" => Action::ForceExit,
        "FinishInvalid" => Action::FinishInvalid,
        "SubmitEntry" => Action::SubmitEntry,
        "TextSize" => Action::TextSize,
        "NextDestination" => Action::NextDestination,
        "PreviousDestination" => Action::PreviousDestination,
        "Help" => Action::Help,
        "Settings" | "OpenSettings" => Action::Settings,
        "NextTrack" => Action::NextTrack,
        "PreviousTrack" => Action::PreviousTrack,
        "Longer" => Action::Longer,
        "Shorter" => Action::Shorter,
        _ => return None,
    })
}

fn view_digest(text: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(text.as_bytes())
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn invalid(message: &str) -> (Code, String) {
    (Code::InvalidArgument, message.to_string())
}

impl<S: Services> Headless<S> {
    /// A host before `hello`.
    pub const fn new(services: S) -> Self {
        Self {
            services,
            engine: None,
            now_us: 1,
            columns: 100,
            rows: 32,
            durable_seq: None,
            finished: false,
            manual: false,
            jobs: Vec::new(),
            settings_jobs: Vec::new(),
            next_job: 1,
        }
    }

    /// Whether `quit` finished the session.
    #[must_use]
    pub const fn finished(&self) -> bool {
        self.finished
    }

    /// The engine, after `hello`.
    #[must_use]
    pub const fn engine(&self) -> Option<&Engine> {
        self.engine.as_ref()
    }

    /// Sends one event through the shared reducer and absorbs what it
    /// dispatched. Returns whether the engine allows exit.
    fn send(&mut self, kind: EventKind) -> Result<bool, (Code, String)> {
        let transition = self.reduce(kind)?;
        self.absorb(transition)
    }

    fn reduce(&mut self, kind: EventKind) -> Result<cadence_engine::Transition, (Code, String)> {
        let at = MonoTick::from_us(self.now_us);
        let engine = self.engine.as_mut().ok_or_else(|| invalid("hello first"))?;
        let seq = engine
            .consumed_events()
            .next()
            .map_err(|error| invalid(&error.to_string()))?;
        Ok(engine.reduce(EventEnvelope { seq, at, kind }))
    }

    /// A dispatched checkpoint becomes a job in its causal group. With
    /// automatic delivery it is saved and acknowledged at once, and a
    /// follow-up dispatched while consuming the acknowledgement (the child)
    /// is absorbed in the same step, before the parent counts as drained.
    fn absorb(
        &mut self,
        mut transition: cadence_engine::Transition,
    ) -> Result<bool, (Code, String)> {
        let mut quit = transition.quit;
        let job = transition.job;
        let outcome = transition.outcome.clone();
        while let Some(checkpoint) = transition.checkpoint.take() {
            let group = transition.checkpoint_group.unwrap_or_else(|| {
                self.engine
                    .as_ref()
                    .map_or(0, |e| e.consumed_events().get())
            });
            let id = self.next_job;
            self.next_job = self.next_job.saturating_add(1);
            if self.manual {
                self.jobs.push(Job {
                    id,
                    group,
                    checkpoint,
                });
                break;
            }
            transition = self.complete(&checkpoint, true)?;
            quit |= transition.quit;
        }
        if let Some(ticket) = job {
            quit |= self.service_job(ticket)?;
        }
        match outcome {
            cadence_engine::DomainOutcome::SetupComplete => quit |= self.restart(None)?,
            cadence_engine::DomainOutcome::PlanRequested { phase, minutes } => {
                quit |= self.restart(Some((phase, minutes)))?;
            }
            cadence_engine::DomainOutcome::None
            | cadence_engine::DomainOutcome::SessionStarted
            | cadence_engine::DomainOutcome::PhaseStarted { .. }
            | cadence_engine::DomainOutcome::ExerciseCompleted { .. }
            | cadence_engine::DomainOutcome::PhaseDeadline { .. }
            | cadence_engine::DomainOutcome::SessionSummary
            | cadence_engine::DomainOutcome::SessionAbandoned
            | cadence_engine::DomainOutcome::MeasurementInvalid { .. } => {}
        }
        Ok(quit)
    }

    /// A started worker job (#79, #81): settings writes wait for `deliver`
    /// in manual delivery; everything else is answered at once, through the
    /// same `JobResult` admission as live.
    fn service_job(
        &mut self,
        ticket: cadence_engine::jobs::JobTicket,
    ) -> Result<bool, (Code, String)> {
        let engine = self.engine.as_ref().ok_or_else(|| invalid("hello first"))?;
        let outcome = if let Some(request) = engine.settings_request(ticket) {
            if self.manual {
                let id = self.next_job;
                self.next_job = self.next_job.saturating_add(1);
                self.settings_jobs.push(SettingsJob {
                    id,
                    ticket,
                    request,
                });
                return Ok(false);
            }
            self.services.settings(&request)
        } else if let Some(limit) = engine.history_request(ticket) {
            match self.services.history(limit) {
                Ok(points) => cadence_engine::jobs::JobOutcome::History(points),
                Err(reason) => cadence_engine::jobs::JobOutcome::Failed(reason),
            }
        } else {
            return Ok(false);
        };
        self.send(EventKind::JobResult { ticket, outcome })
    }

    /// The engine again after setup or for a Train or Numpad plan, given the
    /// same start-up facts as at `hello`; a requested plan begins at once.
    fn restart(
        &mut self,
        track: Option<(cadence_engine::Phase, u64)>,
    ) -> Result<bool, (Code, String)> {
        let engine = self
            .services
            .rebuild(track)
            .map_err(|error| invalid(&error))?;
        self.engine = Some(engine);
        let (next_session, day, history) =
            self.services.storage().map_err(|error| invalid(&error))?;
        let mut quit = self.send(EventKind::StorageReady {
            next_session,
            day,
            history,
        })?;
        quit |= self.send(EventKind::Resized {
            columns: self.columns,
            rows: self.rows,
        })?;
        if track.is_some() {
            quit |= self.send(EventKind::Command(cadence_engine::commands::Action::Begin))?;
        }
        Ok(quit)
    }

    /// Completes a save: through the service when `success`, else as a
    /// failure; the result is admitted to the reducer.
    fn complete(
        &mut self,
        checkpoint: &Checkpoint,
        success: bool,
    ) -> Result<cadence_engine::Transition, (Code, String)> {
        let result = if success {
            self.services.save(checkpoint)
        } else {
            Err("fixture failure".to_string())
        };
        if result.is_ok() {
            self.durable_seq = Some(checkpoint.key.sequence.get());
        }
        self.reduce(EventKind::SaveResult(SaveResult {
            key: checkpoint.key,
            result,
        }))
    }

    fn deliver(&mut self, job_id: u64, variant: DeliverVariant) -> Result<(), (Code, String)> {
        if !self.manual {
            return Err(invalid("deliver is for manual fixture delivery only"));
        }
        if let Some(index) = self.settings_jobs.iter().position(|job| job.id == job_id) {
            let job = self.settings_jobs.remove(index);
            let outcome = match variant {
                DeliverVariant::Success | DeliverVariant::Duplicate => {
                    self.services.settings(&job.request)
                }
                DeliverVariant::Failure => cadence_engine::jobs::JobOutcome::Failed(
                    "fixture failure: the settings write was refused".to_string(),
                ),
                DeliverVariant::StaleSuccess | DeliverVariant::StaleFailure => {
                    return Err(invalid("a settings job is delivered as success or failure"));
                }
            };
            self.finished |= self.send(EventKind::JobResult {
                ticket: job.ticket,
                outcome: outcome.clone(),
            })?;
            if variant == DeliverVariant::Duplicate {
                // The same answer again: the proved ticket admission refuses it.
                self.finished |= self.send(EventKind::JobResult {
                    ticket: job.ticket,
                    outcome,
                })?;
            }
            return Ok(());
        }
        let index = self
            .jobs
            .iter()
            .position(|job| job.id == job_id)
            .ok_or_else(|| invalid("no such pending job"))?;
        match variant {
            DeliverVariant::Success | DeliverVariant::Failure | DeliverVariant::Duplicate => {
                let job = self.jobs.remove(index);
                let success = variant != DeliverVariant::Failure;
                let transition = self.complete(&job.checkpoint, success)?;
                self.finished |= self.absorb(transition)?;
                if variant == DeliverVariant::Duplicate {
                    // The same completion again: the engine ignores it.
                    let again = self.reduce(EventKind::SaveResult(SaveResult {
                        key: job.checkpoint.key,
                        result: Ok(()),
                    }))?;
                    self.finished |= self.absorb(again)?;
                }
            }
            DeliverVariant::StaleSuccess | DeliverVariant::StaleFailure => {
                // A completion for an outdated request: an older sequence.
                let job = self
                    .jobs
                    .get(index)
                    .ok_or_else(|| invalid("no such pending job"))?;
                let mut key = job.checkpoint.key;
                key.sequence =
                    cadence_core::CommitSeq::from_raw(key.sequence.get().saturating_sub(1));
                let result = if variant == DeliverVariant::StaleSuccess {
                    Ok(())
                } else {
                    Err("fixture stale failure".to_string())
                };
                let transition = self.reduce(EventKind::SaveResult(SaveResult { key, result }))?;
                self.finished |= self.absorb(transition)?;
            }
        }
        Ok(())
    }

    /// One barrier (H-03): the proved `barrier::causal` over the jobs whose
    /// group is in scope, and for the durable scope a checkpoint request
    /// admitted into its own scope and `barrier::durable` over the
    /// watermark. Virtual mode does not wait: undelivered jobs fail at once
    /// with their ids, as at the watchdog bound. Practice time does not move.
    fn barrier(
        &mut self,
        durable_scope: bool,
        through: u64,
    ) -> Result<Map<String, Value>, (Code, String)> {
        let engine = self.engine.as_ref().ok_or_else(|| invalid("hello first"))?;
        let latest = engine.consumed_events().get();
        if causal(through, latest, 0, 0, 0) == Barrier::OutOfScope {
            return Err((
                Code::BarrierOutOfScope,
                "through_event_seq is beyond the latest admitted event".into(),
            ));
        }
        let mut scope = through;
        if durable_scope {
            self.send(EventKind::CheckpointRequested)?;
            scope = self
                .engine
                .as_ref()
                .map_or(scope, |e| e.consumed_events().get());
        }
        let latest = self
            .engine
            .as_ref()
            .map_or(latest, |e| e.consumed_events().get());
        let pending: Vec<u64> = self
            .jobs
            .iter()
            .filter(|job| job.group <= scope)
            .map(|job| job.id)
            .collect();
        let outstanding = u64::try_from(pending.len()).unwrap_or(u64::MAX);
        let mut decision = causal(scope, latest, outstanding, 0, 0);
        let (dispatched, acked) = self
            .engine
            .as_ref()
            .map_or((None, None), Engine::save_watermarks);
        let watermark = dispatched.unwrap_or(0);
        if durable_scope {
            decision = durable(decision, acked.unwrap_or(0), acked.is_some(), watermark);
        }
        match decision {
            Barrier::Done => {
                let mut result = self.revisions();
                result.insert(
                    "scope".into(),
                    json!(if durable_scope { "durable" } else { "causal" }),
                );
                result.insert("through_event_seq".into(), json!(scope.to_string()));
                if durable_scope {
                    result.insert("watermark".into(), json!(watermark.to_string()));
                }
                Ok(result)
            }
            Barrier::Waiting | Barrier::OutOfScope => Err((
                Code::BarrierTimeout,
                format!("pending jobs at the watchdog: {pending:?}"),
            )),
        }
    }

    fn key(&mut self, name: &str, modifiers: &[String]) -> Result<Option<Key>, (Code, String)> {
        let control = modifiers.iter().any(|m| m == "control");
        let key = match name {
            "Enter" => Key::Enter,
            "Escape" => Key::Escape,
            "Backspace" => Key::Backspace,
            "F2" => Key::ToggleTextSize,
            "Left" => Key::Arrow(Direction::Left),
            "Right" => Key::Arrow(Direction::Right),
            "Up" => Key::Arrow(Direction::Up),
            "Down" => Key::Arrow(Direction::Down),
            "Tab" => Key::Tab,
            "BackTab" => Key::BackTab,
            "Home" | "End" => Key::Other,
            "c" if control => Key::Interrupt,
            single => {
                let mut chars = single.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if !control => Key::Content(
                        TrainingChar::new(c, Layout::Us)
                            .map_err(|error| invalid(&error.to_string()))?,
                    ),
                    _ => return Err(invalid("unsupported key")),
                }
            }
        };
        Ok(Some(key))
    }

    fn advance(&mut self, microseconds: u64) -> Result<(), (Code, String)> {
        self.now_us = self
            .now_us
            .checked_add(microseconds)
            .ok_or_else(|| invalid("virtual clock overflow"))?;
        Ok(())
    }

    fn revisions(&self) -> Map<String, Value> {
        let mut result = Map::new();
        if let Some(engine) = &self.engine {
            result.insert(
                "state_revision".into(),
                json!(engine.state_revision().to_string()),
            );
            result.insert(
                "event_seq".into(),
                json!(engine.consumed_events().get().to_string()),
            );
            // Pins the whole prepared view (#72): a replay that typed a
            // different key diverges at that request, not later.
            let view = engine.prepare(MonoTick::from_us(self.now_us));
            result.insert(
                "view_digest".into(),
                json!(view_digest(&format!("{view:?}"))),
            );
        } else {
            result.insert("state_revision".into(), json!("0"));
            result.insert("event_seq".into(), json!("0"));
        }
        if let Some(durable) = self.durable_seq {
            result.insert("durable_commit_seq".into(), json!(durable.to_string()));
        }
        result
    }

    fn snapshot(&self, include_target: bool) -> Result<Map<String, Value>, (Code, String)> {
        let engine = self.engine.as_ref().ok_or_else(|| invalid("hello first"))?;
        let view = engine.prepare(MonoTick::from_us(self.now_us));
        let screen = match &view.screen {
            ScreenView::Today { .. } => "today",
            ScreenView::Practice { .. } => "practice",
            ScreenView::PhaseComplete { .. } => "phase_complete",
            ScreenView::Summary => "summary",
        };
        let target = match &view.screen {
            ScreenView::Practice { spans, .. } if include_target => Some(
                spans
                    .iter()
                    .filter_map(|span| match *span {
                        cadence_engine::UnitSpan::Correct { unit } => Some(unit),
                        cadence_engine::UnitSpan::Wrong { expected, .. }
                        | cadence_engine::UnitSpan::Cursor { expected }
                        | cadence_engine::UnitSpan::Upcoming { expected } => Some(expected),
                        cadence_engine::UnitSpan::Extra { .. } => None,
                    })
                    .collect::<String>(),
            ),
            ScreenView::Today { .. }
            | ScreenView::Practice { .. }
            | ScreenView::PhaseComplete { .. }
            | ScreenView::Summary => None,
        };
        let tracks: Vec<Value> = engine
            .track_totals()
            .map_err(|error| invalid(&error.to_string()))?
            .iter()
            .map(|t| {
                json!({
                    "track": t.track.label(),
                    "first_pass": {"correct": t.first_correct.to_string(), "attempted": t.first_attempted.to_string()},
                    "attempt": {"correct": t.correct_attempts.to_string(), "attempted": t.content_attempts.to_string()},
                    "deletions": t.deletions.to_string(),
                    "active_micros": t.active_micros.to_string(),
                })
            })
            .collect();
        let actions: Vec<&str> = match &view.screen {
            ScreenView::Today { .. } => vec!["StartDailyPlan"],
            ScreenView::Practice { paused: true, .. } => vec!["Resume"],
            ScreenView::Practice { .. } => vec!["Pause"],
            ScreenView::PhaseComplete { .. } => vec!["NextPhase"],
            ScreenView::Summary => vec![],
        };
        let mut result = self.revisions();
        result.insert("screen".into(), json!(screen));
        result.insert(
            "destination".into(),
            json!(view.destination.selected.label()),
        );
        result.insert("setup".into(), json!(view.setup.is_some()));
        result.insert("settings_open".into(), json!(view.settings.is_some()));
        result.insert("notice".into(), json!(view.notice));
        result.insert("theme".into(), json!(view.display.theme));
        result.insert(
            "pending_settings_jobs".into(),
            json!(
                self.settings_jobs
                    .iter()
                    .map(|job| job.id.to_string())
                    .collect::<Vec<_>>()
            ),
        );
        if let Some(text) = self.services.config_text() {
            result.insert("config_text".into(), json!(text));
        }
        result.insert(
            "input_owner".into(),
            json!(format!("{:?}", engine.input_owner())),
        );
        result.insert(
            "active_micros".into(),
            json!(view.active.as_us().to_string()),
        );
        result.insert("elapsed_micros".into(), json!(self.now_us.to_string()));
        result.insert("tracks".into(), Value::Array(tracks));
        result.insert(
            "plan_basis".into(),
            json!(if view.learning.adaptive {
                "adaptive"
            } else {
                "cold_start"
            }),
        );
        result.insert(
            "pending_jobs".into(),
            json!(
                self.jobs
                    .iter()
                    .map(|job| job.id.to_string())
                    .collect::<Vec<_>>()
            ),
        );
        result.insert("available_actions".into(), json!(actions));
        result.insert("ended_early".into(), json!(view.review.ended_early));
        if let Some(target) = target {
            result.insert("target_text".into(), json!(target));
        }
        Ok(result)
    }

    fn capture(&self, format: &str) -> Result<Map<String, Value>, (Code, String)> {
        let engine = self.engine.as_ref().ok_or_else(|| invalid("hello first"))?;
        let frame = crate::capture::capture(
            engine,
            MonoTick::from_us(self.now_us),
            self.columns,
            self.rows,
        );
        // No frame leaves the driver without its cross-field invariants.
        frame
            .validate()
            .map_err(|error| invalid(&format!("invalid frame: {error}")))?;
        let mut result = self.revisions();
        if format != "cells" {
            result.insert("text".into(), json!(frame.text()));
        }
        if format != "text" {
            result.insert("frame".into(), frame.to_json());
        }
        Ok(result)
    }
}

impl<S: Services> Host for Headless<S> {
    fn state_revision(&self) -> u64 {
        self.engine.as_ref().map_or(0, Engine::state_revision)
    }

    fn apply(&mut self, request: &Request) -> Result<Map<String, Value>, (Code, String)> {
        match &request.op {
            Operation::Hello {
                scenario,
                auto_delivery,
            } => {
                self.manual = *auto_delivery == Some(false);
                let engine = self
                    .services
                    .engine(scenario.as_deref())
                    .map_err(|error| invalid(&error))?;
                self.engine = Some(engine);
                let (next_session, day, history) =
                    self.services.storage().map_err(|error| invalid(&error))?;
                self.send(EventKind::StorageReady {
                    next_session,
                    day,
                    history,
                })?;
                self.send(EventKind::Resized {
                    columns: self.columns,
                    rows: self.rows,
                })?;
                let mut result = self.revisions();
                result.insert("version".into(), json!(env!("CARGO_PKG_VERSION")));
                result.insert("clock".into(), json!("virtual"));
                result.insert("scenario".into(), json!(scenario));
                result.insert("limits".into(), json!({"line_bytes": crate::envelope::MAXIMUM_LINE_BYTES, "text_units": crate::envelope::MAXIMUM_TEXT_UNITS, "response_cache": crate::protocol::CACHE_CAPACITY}));
                Ok(result)
            }
            Operation::Key {
                key,
                modifiers,
                kind,
                ..
            } => {
                if kind != "release"
                    && let Some(key) = self.key(key, modifiers)?
                {
                    self.finished |= self.send(EventKind::Key(key))?;
                }
                Ok(self.revisions())
            }
            Operation::Text {
                text,
                inter_key_microseconds,
            } => {
                for (index, unit) in text.chars().enumerate() {
                    if index > 0 {
                        self.advance(*inter_key_microseconds)?;
                        self.send(EventKind::Tick)?;
                    }
                    let unit = TrainingChar::new(unit, Layout::Us)
                        .map_err(|error| invalid(&error.to_string()))?;
                    self.send(EventKind::Key(Key::Content(unit)))?;
                }
                Ok(self.revisions())
            }
            Operation::Advance { microseconds } => {
                self.advance(*microseconds)?;
                self.send(EventKind::Tick)?;
                Ok(self.revisions())
            }
            Operation::Resize { columns, rows } => {
                self.columns = *columns;
                self.rows = *rows;
                self.send(EventKind::Resized {
                    columns: *columns,
                    rows: *rows,
                })?;
                Ok(self.revisions())
            }
            Operation::Action { name, .. } => {
                // The same typed command table the keys resolve through
                // (#80): an action this state does not offer is refused.
                let action =
                    action_named(name).ok_or_else(|| invalid(&format!("unknown action {name}")))?;
                let engine = self.engine.as_ref().ok_or_else(|| invalid("hello first"))?;
                if !engine.offers(action) {
                    return Err(invalid(&format!("{name} is not offered in this state")));
                }
                self.finished |= self.send(EventKind::Command(action))?;
                Ok(self.revisions())
            }
            Operation::Snapshot {
                include_target_text,
            } => self.snapshot(*include_target_text),
            Operation::Capture { format } => self.capture(format),
            Operation::Barrier {
                durable,
                through_event_seq,
                ..
            } => self.barrier(*durable, *through_event_seq),
            Operation::Deliver { job_id, variant } => {
                self.deliver(*job_id, *variant)?;
                Ok(self.revisions())
            }
            Operation::Quit => {
                self.finished |= self.send(EventKind::Terminate)?;
                self.finished = true;
                let mut result = self.revisions();
                result.insert("saved".into(), json!(self.durable_seq.is_some()));
                Ok(result)
            }
        }
    }
}
