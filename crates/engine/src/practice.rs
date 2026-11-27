//! Practice timing, bounded ledger updates and fixed-track accumulation.
use super::{DomainOutcome, Engine, Key, Screen, invalid};
use cadence_core::label::advance as cycle_advance;
use cadence_core::learning::{KeyLane, ProbeReason, ProbeRecord};
use cadence_core::ledger::{Applied, AttemptLedger, ExtraInputAllowance, LedgerEvent, Slot};
use cadence_core::metrics::{correct_digits, first_pass_accuracy};
use cadence_core::progress::{Track, TrackProgress, credit_once};
use cadence_core::session::{SessionEvent, SessionState};
use cadence_core::{CoreError, ExerciseId, Micros, MonoTick};
use cadence_trainer::{ExerciseKind, Phase};

/// Mirrors runtime policy `maximum_extra_input_units` (trainer-v2.toml).
const EXTRA_INPUT_UNITS: u32 = 128;
/// One accumulator per distinct `Track` variant.
pub(super) const TRACK_CAPACITY: usize = 7;

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
            let next = self
                .phase_index
                .checked_add(1)
                .ok_or_else(|| invalid("phase index exhausted"))?;
            let last = self
                .plan
                .phases
                .get(next..)
                .is_some_and(|remaining| remaining.iter().all(|p| p.phase == Phase::Review));
            let partial = self
                .ledger
                .as_ref()
                .is_some_and(|l| !l.buffer().is_empty() && !l.is_complete());
            self.timing.end_phase(boundary)?;
            self.active = deadline;
            self.record_exercise()?;
            if phase == Phase::Probe {
                self.record_probe(true, false)?;
            }
            if phase == Phase::Review {
                // The closing review's deadline opens the results (#49).
                self.screen = Screen::Summary;
                self.transition(Some(at), SessionEvent::PlanDone)?;
                self.mark_due();
                return Ok(Some(DomainOutcome::SessionSummary));
            }
            self.screen = Screen::PhaseComplete { partial };
            self.transition(
                Some(at),
                if last {
                    SessionEvent::LastDeadline
                } else {
                    SessionEvent::Deadline
                },
            )?;
            self.interlude_opened = Some(at);
            self.mark_due();
            return Ok(Some(DomainOutcome::PhaseDeadline { phase, partial }));
        }
        self.active = active;
        Ok(None)
    }
    pub(super) fn open_exercise(&mut self, at: MonoTick) -> Result<(), CoreError> {
        let phase = self
            .plan
            .phases
            .get(self.phase_index)
            .ok_or_else(|| invalid("missing phase"))?;
        if phase.phase == Phase::Review && phase.exercises.is_empty() {
            self.screen = Screen::Summary;
            self.transition(Some(at), SessionEvent::PlanDone)?;
            self.mark_due();
            return Ok(());
        }
        let exercise = phase
            .exercises
            .get(self.exercise_index)
            .ok_or_else(|| invalid("missing exercise"))?;
        let units = self.learning.open(exercise)?;
        self.opened = self
            .opened
            .checked_add(1)
            .ok_or_else(|| invalid("exercise ledger sequence exhausted"))?;
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
            // The kernel decides whether this ledger may be credited (Lean:
            // only one newer than every credited ledger is, so no exercise
            // is merged twice).
            if !credit_once(self.last_credited, self.opened) {
                return Err(invalid("exercise credited twice"));
            }
            merge_track(&mut self.tracks, progress)?;
            self.last_credited = Some(self.opened);
        }
        if let Some(ledger) = self.ledger.take() {
            let exercise = self
                .plan
                .phases
                .get(self.phase_index)
                .and_then(|phase| phase.exercises.get(self.exercise_index))
                .ok_or_else(|| invalid("missing exercise"))?;
            self.learning.close(exercise, &ledger)?;
            // The closed exercise's evidence becomes the next chunk (#11).
            let track = self
                .plan
                .phases
                .get(self.phase_index)
                .map(|phase| track_phase(phase.phase))
                .ok_or_else(|| invalid("missing phase"))??;
            let learning = &mut self.learning;
            learning.chunks.cut(track, &learning.evidence)?;
        }
        Ok(())
    }
    pub(super) fn advance_phase(&mut self, at: MonoTick) -> Result<DomainOutcome, CoreError> {
        self.phase_index = self
            .phase_index
            .checked_add(1)
            .ok_or_else(|| invalid("phase index exhausted"))?;
        self.exercise_index = 0;
        self.list_wrapped = false;
        self.learning.end_phase();
        self.phase_start = self.active;
        self.interlude_opened = None;
        let Some(phase) = self.plan.phases.get(self.phase_index) else {
            self.screen = Screen::Summary;
            self.transition(Some(at), SessionEvent::PlanDone)?;
            self.mark_due();
            return Ok(DomainOutcome::SessionSummary);
        };
        let phase = phase.phase;
        self.screen = Screen::Practice;
        self.transition(Some(at), SessionEvent::NextPhase)?;
        self.open_exercise(at)?;
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
                Key::Content(c) if c.get() == 'e' && !self.invalid && !self.save.failed => {
                    return self.end_early(at);
                }
                Key::Content(_)
                | Key::Backspace
                | Key::Interrupt
                | Key::ToggleTextSize
                | Key::Arrow(_)
                | Key::Tab
                | Key::BackTab
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
        // In an exercise with newlines (code packs, #85) Enter types the
        // newline unit; elsewhere Enter is never content.
        let key = if key == Key::Enter
            && !numpad
            && ledger.target().iter().any(|unit| unit.get() == '\n')
        {
            Key::Content(
                cadence_core::TrainingChar::new('\n', cadence_core::Layout::Us)
                    .map_err(|_alphabet| invalid("newline unit"))?,
            )
        } else {
            key
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
                        let previous = position
                            .checked_sub(1)
                            .and_then(|before| {
                                self.ledger
                                    .as_ref()
                                    .and_then(|l| l.first_outcomes().get(before).copied())
                            })
                            .unwrap_or(Slot::Open);
                        let segment = self.timing.segment_ordinal();
                        self.learning.first_attempt(
                            target, position, matched, previous, active, segment, lane,
                        )?;
                    }
                    Applied::Content { .. }
                    | Applied::Deleted
                    | Applied::DeletedNothing
                    | Applied::RejectedOverBound => self.learning.break_timing(),
                }
                if complete && !numpad {
                    return self.finish_exercise(at);
                }
            }
            Key::Backspace => {
                ledger.apply(LedgerEvent::Backspace)?;
                self.learning.break_timing();
            }
            Key::Enter if numpad && ledger.is_complete() => return self.finish_exercise(at),
            Key::Enter
            | Key::Escape
            | Key::Interrupt
            | Key::ToggleTextSize
            | Key::Arrow(_)
            | Key::Tab
            | Key::BackTab
            | Key::Other => {}
        }
        Ok(DomainOutcome::None)
    }
    fn finish_exercise(&mut self, at: MonoTick) -> Result<DomainOutcome, CoreError> {
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
        // The proved `cadence_kernel::cycle::advance`: the next prepared
        // exercise, or the first again when the list is exhausted.
        let advance = cycle_advance(self.exercise_index, count);
        self.exercise_index = advance.next;
        self.list_wrapped = self.list_wrapped || advance.wrapped;
        self.open_exercise(at)?;
        self.mark_due();
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
impl Engine {
    /// Whether practice is in the transfer probe's phase (#64).
    pub(super) fn in_probe(&self) -> bool {
        self.plan
            .phases
            .get(self.phase_index)
            .is_some_and(|p| p.phase == Phase::Probe)
            && matches!(self.screen, Screen::Practice)
    }

    /// Records the shown probe once, with whether it may be compared
    /// (the proved `benchmark::comparable`): the probe passage is retired
    /// for the profile from then on.
    pub(super) fn record_probe(
        &mut self,
        completed_window: bool,
        ended_early: bool,
    ) -> Result<(), CoreError> {
        let Some(planned) = self.plan.probe.planned() else {
            return Ok(());
        };
        if self.learning.evidence.probe.is_some() {
            return Ok(());
        }
        let facts = cadence_core::benchmark::Facts {
            completed_window,
            paused: self.probe_paused,
            ended_early,
            pasted: self.probe_pasted,
            synthetic: false,
            same_layout: true,
            same_strata: planned.same_strata,
        };
        let comparable = cadence_core::benchmark::comparable(facts);
        let reason = if comparable {
            ProbeReason::Comparable
        } else if ended_early {
            ProbeReason::EndedEarly
        } else if facts.paused {
            ProbeReason::Paused
        } else if facts.pasted {
            ProbeReason::Pasted
        } else if !completed_window {
            ProbeReason::CutShort
        } else {
            ProbeReason::OtherStrata
        };
        self.learning.evidence.probe = Some(ProbeRecord {
            passage: planned.passage,
            strata: planned.strata,
            comparable,
            reason,
        });
        Ok(())
    }
}

pub(super) const fn track_kind(kind: ExerciseKind) -> Track {
    match kind {
        ExerciseKind::WarmUp => Track::WarmUp,
        ExerciseKind::Words => Track::Words,
        ExerciseKind::NumberRow => Track::NumberRow,
        ExerciseKind::Numpad => Track::Numpad,
        ExerciseKind::Prose => Track::Prose,
        ExerciseKind::Probe => Track::Benchmark,
    }
}
pub(super) fn track_phase(phase: Phase) -> Result<Track, CoreError> {
    match phase {
        Phase::WarmUp => Ok(Track::WarmUp),
        Phase::AdaptiveWords => Ok(Track::Words),
        Phase::NumberRow => Ok(Track::NumberRow),
        Phase::Numpad => Ok(Track::Numpad),
        Phase::Prose => Ok(Track::Prose),
        Phase::Review => Ok(Track::Review),
        Phase::Probe => Ok(Track::Benchmark),
    }
}

/// Whether an exercise kind belongs in a phase: the closing review (#49)
/// is typed as words; every other phase's kind matches its track.
pub(super) fn kind_fits(kind: ExerciseKind, phase: Phase) -> Result<bool, CoreError> {
    if phase == Phase::Review {
        return Ok(kind == ExerciseKind::Words);
    }
    Ok(track_kind(kind) == track_phase(phase)?)
}
