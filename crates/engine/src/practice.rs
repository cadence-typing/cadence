//! Practice timing, bounded ledger updates and fixed-track accumulation.
use super::{DomainOutcome, Engine, Key, Screen, invalid};
use cadence_core::learning::KeyLane;
use cadence_core::ledger::{Applied, AttemptLedger, ExtraInputAllowance, LedgerEvent, Slot};
use cadence_core::metrics::{correct_digits, first_pass_accuracy};
use cadence_core::progress::{ProgressStatus, Track, TrackProgress};
use cadence_core::session::SessionState;
use cadence_core::{CoreError, ExerciseId, Micros, MonoTick};
use cadence_trainer::{ExerciseKind, Phase};

/// Mirrors runtime policy `maximum_extra_input_units` (trainer-v2.toml).
const EXTRA_INPUT_UNITS: u32 = 128;
/// One accumulator per distinct `Track` variant.
pub(super) const TRACK_CAPACITY: usize = 5;

impl Engine {
    pub(super) fn advance_clock(
        &mut self,
        at: MonoTick,
    ) -> Result<Option<DomainOutcome>, CoreError> {
        if self.session_state != SessionState::Running || self.screen != Screen::Practice {
            return Ok(None);
        }
        let active = self.timing.active_through(at)?;
        let phase = self
            .plan
            .phases
            .get(self.phase_index)
            .ok_or_else(|| invalid("missing phase"))?;
        let deadline = self
            .phase_start
            .checked_add(Micros::from_seconds(phase.seconds)?)?;
        if active < self.phase_start {
            return Err(invalid("active time precedes the phase start"));
        }
        if !active.within_half_open(self.phase_start, deadline) {
            let overshoot = active.checked_sub(deadline)?;
            let boundary = MonoTick::from_us(
                at.as_us()
                    .checked_sub(overshoot.as_us())
                    .ok_or_else(|| invalid("invalid phase boundary"))?,
            );
            let phase = phase.phase;
            let partial = self
                .ledger
                .as_ref()
                .is_some_and(|l| !l.buffer().is_empty() && !l.is_complete());
            self.timing.end_phase(boundary)?;
            self.active = deadline;
            self.record_exercise()?;
            self.screen = Screen::PhaseComplete { partial };
            self.session_state = SessionState::PhaseComplete;
            self.interlude_opened = Some(at);
            let next = self
                .phase_index
                .checked_add(1)
                .ok_or_else(|| invalid("phase index exhausted"))?;
            if self
                .plan
                .phases
                .get(next..)
                .is_some_and(|remaining| remaining.iter().all(|p| p.phase == Phase::Review))
            {
                self.status = ProgressStatus::Completed;
            }
            self.checkpoint_due = true;
            return Ok(Some(DomainOutcome::PhaseDeadline { phase, partial }));
        }
        self.active = active;
        Ok(None)
    }
    pub(super) fn open_exercise(&mut self) -> Result<(), CoreError> {
        let phase = self
            .plan
            .phases
            .get(self.phase_index)
            .ok_or_else(|| invalid("missing phase"))?;
        if phase.phase == Phase::Review {
            self.screen = Screen::Summary;
            self.session_state = SessionState::Review;
            self.status = ProgressStatus::Completed;
            self.checkpoint_due = true;
            return Ok(());
        }
        let exercise = phase
            .exercises
            .get(self.exercise_index)
            .ok_or_else(|| invalid("missing exercise"))?;
        let units = self.learning.open(exercise)?;
        self.ledger = Some(AttemptLedger::new(
            units,
            ExtraInputAllowance::new(EXTRA_INPUT_UNITS)?,
        )?);
        self.exercise_start = self.active;
        Ok(())
    }
    pub(super) fn current_track(&self) -> Result<Track, CoreError> {
        let phase = self
            .plan
            .phases
            .get(self.phase_index)
            .ok_or_else(|| invalid("missing phase"))?;
        track_phase(phase.phase)
    }
    pub(super) fn current_progress(&self) -> Result<Option<TrackProgress>, CoreError> {
        let Some(ledger) = self.ledger.as_ref() else {
            return Ok(None);
        };
        let c = ledger.counters();
        Ok(Some(TrackProgress {
            track: self.current_track()?,
            first_correct: ledger.first_correct().get(),
            first_attempted: ledger.first_attempted().get(),
            correct_attempts: c.correct_attempts(),
            content_attempts: c.content_attempts(),
            deletions: c.deletions(),
            overflow_attempts: c.overflow_attempts(),
            correct_units: ledger.correct_visible_units().get(),
            correct_digits: correct_digits(ledger).get(),
            active_micros: self.active.checked_sub(self.exercise_start)?.as_us(),
        }))
    }
    pub(super) fn record_exercise(&mut self) -> Result<(), CoreError> {
        if let Some(progress) = self.current_progress()? {
            merge_track(&mut self.tracks, progress)?;
        }
        if let Some(ledger) = self.ledger.take() {
            let exercise = self
                .plan
                .phases
                .get(self.phase_index)
                .and_then(|phase| phase.exercises.get(self.exercise_index))
                .ok_or_else(|| invalid("missing exercise"))?;
            self.learning.close(exercise, &ledger)?;
        }
        Ok(())
    }
    pub(super) fn advance_phase(&mut self) -> Result<DomainOutcome, CoreError> {
        self.phase_index = self
            .phase_index
            .checked_add(1)
            .ok_or_else(|| invalid("phase index exhausted"))?;
        self.exercise_index = 0;
        self.phase_start = self.active;
        self.interlude_opened = None;
        let Some(phase) = self.plan.phases.get(self.phase_index) else {
            self.screen = Screen::Summary;
            self.session_state = SessionState::Review;
            self.status = ProgressStatus::Completed;
            self.checkpoint_due = true;
            return Ok(DomainOutcome::SessionSummary);
        };
        let phase = phase.phase;
        self.screen = Screen::Practice;
        self.session_state = SessionState::Running;
        self.open_exercise()?;
        if self.screen == Screen::Summary {
            Ok(DomainOutcome::SessionSummary)
        } else {
            Ok(DomainOutcome::PhaseStarted { phase })
        }
    }
    pub(super) fn practice_key(
        &mut self,
        at: MonoTick,
        key: Key,
    ) -> Result<DomainOutcome, CoreError> {
        if self.session_state == SessionState::Paused {
            match key {
                Key::Escape | Key::Enter => self.resume(at)?,
                Key::Content(c)
                    if c.get() == 'e'
                        && !self.invalid
                        && !self.storage.as_ref().is_some_and(|s| s.failed.is_some()) =>
                {
                    return self.end_early(at);
                }
                Key::Content(_)
                | Key::Backspace
                | Key::Interrupt
                | Key::ToggleTextSize
                | Key::Other => {}
            }
            return Ok(DomainOutcome::None);
        }
        if self.invalid {
            return Ok(DomainOutcome::None);
        }
        if key == Key::Escape {
            self.pause_for(at, super::PauseReason::Requested)?;
            return Ok(DomainOutcome::None);
        }
        let numpad = self.current_track()? == Track::Numpad;
        let Some(ledger) = self.ledger.as_mut() else {
            return Err(invalid("missing practice ledger"));
        };
        match key {
            Key::Content(c) => {
                self.timing.first_content(at)?;
                let position = ledger.buffer().len();
                let first = ledger
                    .first_outcomes()
                    .get(position)
                    .is_some_and(|slot| *slot == Slot::Open);
                let applied = ledger.apply(LedgerEvent::Content(c))?;
                if applied == Applied::RejectedOverBound {
                    // Refused, visibly: the footer asks for Backspace. The
                    // ledger marks the tail word interrupted, and units past
                    // the target never count as correct, so no score improves.
                    self.learning.break_timing();
                    return Ok(DomainOutcome::None);
                }
                let complete = ledger.is_complete();
                match applied {
                    Applied::Content {
                        position: Some(p),
                        matched,
                    } if first => {
                        let active = self.timing.active_through(at)?;
                        let lane = if numpad {
                            KeyLane::Numpad
                        } else {
                            KeyLane::Main
                        };
                        let target = self
                            .ledger
                            .as_ref()
                            .map(|l| l.target())
                            .ok_or_else(|| invalid("missing practice ledger"))?;
                        let position = usize::try_from(p.get()).map_err(|_unrepresentable| {
                            invalid("unit index outside address space")
                        })?;
                        self.learning
                            .first_attempt(target, position, matched, active, lane)?;
                    }
                    Applied::Content { .. }
                    | Applied::Deleted
                    | Applied::DeletedNothing
                    | Applied::RejectedOverBound => self.learning.break_timing(),
                }
                if complete && !numpad {
                    return self.finish_exercise();
                }
            }
            Key::Backspace => {
                ledger.apply(LedgerEvent::Backspace)?;
                self.learning.break_timing();
            }
            Key::Enter if numpad && ledger.is_complete() => return self.finish_exercise(),
            Key::Enter | Key::Escape | Key::Interrupt | Key::ToggleTextSize | Key::Other => {}
        }
        Ok(DomainOutcome::None)
    }
    fn finish_exercise(&mut self) -> Result<DomainOutcome, CoreError> {
        let exercise = ExerciseId::new(self.next_exercise)?;
        let next = self
            .next_exercise
            .checked_add(1)
            .ok_or_else(|| invalid("exercise identity exhausted"))?;
        let first_pass = self
            .ledger
            .as_ref()
            .and_then(first_pass_accuracy)
            .map(|r| (r.numerator(), r.denominator()));
        self.record_exercise()?;
        self.next_exercise = next;
        let count = self
            .plan
            .phases
            .get(self.phase_index)
            .ok_or_else(|| invalid("missing phase"))?
            .exercises
            .len();
        let next_index = self
            .exercise_index
            .checked_add(1)
            .ok_or_else(|| invalid("exercise index exhausted"))?;
        self.exercise_index = if next_index == count { 0 } else { next_index };
        self.open_exercise()?;
        self.checkpoint_due = true;
        Ok(DomainOutcome::ExerciseCompleted {
            exercise,
            first_pass,
        })
    }
}
pub(super) fn merge_track(
    tracks: &mut Vec<TrackProgress>,
    progress: TrackProgress,
) -> Result<(), CoreError> {
    if let Some(existing) = tracks.iter_mut().find(|t| t.track == progress.track) {
        existing.add(&progress)?;
    } else {
        if tracks.len() >= TRACK_CAPACITY {
            return Err(invalid("track capacity exhausted"));
        }
        tracks.push(progress);
    }
    Ok(())
}
pub(super) const fn track_kind(kind: ExerciseKind) -> Track {
    match kind {
        ExerciseKind::WarmUp => Track::WarmUp,
        ExerciseKind::Words => Track::Words,
        ExerciseKind::NumberRow => Track::NumberRow,
        ExerciseKind::Numpad => Track::Numpad,
        ExerciseKind::Prose => Track::Prose,
    }
}
pub(super) fn track_phase(phase: Phase) -> Result<Track, CoreError> {
    match phase {
        Phase::WarmUp => Ok(Track::WarmUp),
        Phase::AdaptiveWords => Ok(Track::Words),
        Phase::NumberRow => Ok(Track::NumberRow),
        Phase::Numpad => Ok(Track::Numpad),
        Phase::Prose => Ok(Track::Prose),
        Phase::Review => Err(invalid("review is not a typing track")),
    }
}
