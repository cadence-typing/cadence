//! Read-only presentation; only admitted reducer events advance measurements.
use super::{Engine, PreparedView, Screen, ScreenView, StatusFigures, UnitSpan};
use crate::summary::{track_accuracy, track_rate};
use crate::view::{LearningView, PhaseOverview, PhaseState, RateUnit, ViewMode};
use cadence_core::learning::{PlannerMode, Skill, SkillKey};
use cadence_core::ledger::AttemptLedger;
use cadence_core::progress::Track;
use cadence_core::session::SessionState;
use cadence_core::{Micros, MonoTick, label};
use cadence_trainer::{Lane, Phase};
impl Engine {
    /// Builds a view without I/O, state mutation or advancing the clock.
    /// An invalid view timestamp is visibly rejected, never converted to zero time.
    #[must_use]
    pub fn prepare(&self, now: MonoTick) -> PreparedView {
        let notice = if self
            .last_at
            .is_some_and(|last| now.elapsed_since(last).is_err())
        {
            Some("Invalid presentation timestamp: backward time; displayed measurements remain at the last admitted event.".to_string())
        } else {
            self.notice.clone()
        };
        let screen = match self.screen {
            Screen::Today => ScreenView::Today {
                total_seconds: self.plan.budget_seconds,
            },
            Screen::Practice => self.practice_view(),
            Screen::PhaseComplete { partial } => ScreenView::PhaseComplete {
                label: self
                    .plan
                    .phases
                    .get(self.phase_index)
                    .map_or("phase", |p| phase_label(p.phase))
                    .to_string(),
                partial,
                next: self
                    .phase_index
                    .checked_add(1)
                    .and_then(|i| self.plan.phases.get(i))
                    .map(|p| phase_label(p.phase).to_string()),
            },
            Screen::Summary => ScreenView::Summary,
        };
        PreparedView {
            revision: self.revision,
            screen,
            active: self.active,
            notice,
            history: self.history.clone(),
            text_size: self.text_size,
            text_support: self.text_support,
            mode: self.view_mode(),
            durability: self.durability(),
            phases: self.phase_overview(),
            phase_report: if matches!(self.screen, Screen::PhaseComplete { .. }) {
                self.current_track()
                    .ok()
                    .and_then(|track| self.tracks.iter().find(|p| p.track == track).cloned())
            } else {
                None
            },
            summary_reports: if self.screen == Screen::Summary {
                self.tracks.clone()
            } else {
                Vec::new()
            },
            summary_rate_valid: !self.invalid,
            learning: self.learning_view(),
            pause_reason: if self.session_state == SessionState::Paused {
                self.pause_reason
            } else {
                None
            },
            last_session: if self.screen == Screen::Today {
                self.last_session()
            } else {
                None
            },
            review: self.review_context(),
        }
    }
    fn learning_view(&self) -> LearningView {
        let evidence = &self.learning.evidence;
        LearningView {
            adaptive: evidence.plan.mode == PlannerMode::Adaptive,
            focus: evidence
                .plan
                .focus
                .iter()
                .map(|key| skill_label(*key))
                .collect(),
            due_reviews: self
                .plan
                .phases
                .iter()
                .flat_map(|phase| phase.exercises.iter())
                .flat_map(|exercise| exercise.words.iter())
                .filter(|word| word.due_review)
                .count(),
            reinforced: evidence.reinforced,
            missed_words: evidence.words.values().filter(|t| t.errors > 0).count(),
            observed_keys: evidence
                .skills
                .iter()
                .filter(|(key, _)| matches!(key.skill, Skill::Unit(_)))
                .map(|(_, tally)| tally.attempts)
                .fold(0_u64, u64::saturating_add),
        }
    }
    /// Why the current target was chosen, without efficacy claims.
    fn exercise_context(&self) -> Option<String> {
        let exercise = self
            .plan
            .phases
            .get(self.phase_index)?
            .exercises
            .get(self.exercise_index)?;
        if let Some(caption) = &exercise.caption {
            return Some(caption.clone());
        }
        // A returned numeric miss (#59) is labelled by the proved label::of.
        if self.learning.numeric_again
            && label::of(label::Facts {
                served_before: 1,
                reinforcement: true,
                due_review: false,
                wrapped: false,
                recent_passage: false,
            }) == label::Label::Again
        {
            return Some("again".to_string());
        }
        let words = &self.learning.words;
        let mut parts = Vec::new();
        let mut focus: Vec<String> = Vec::new();
        for word in words {
            if let Some(key) = word.focus {
                let label = skill_label(key);
                if !focus.contains(&label) {
                    focus.push(label);
                }
            }
        }
        if !focus.is_empty() {
            parts.push(format!("Targeting {}", focus.join(" \u{b7} ")));
        }
        // Each slot's label is the proved `cadence_kernel::label::of`: a
        // review-lane slot whose word differs from the planned one is a
        // reinforcement (`Again`), a planned due review is `DueReview`.
        let labels: Vec<label::Label> = words
            .iter()
            .zip(exercise.words.iter().map(|w| w.id))
            .map(|(word, planned)| {
                label::of(label::Facts {
                    served_before: 0,
                    reinforcement: word.lane == Lane::Review && word.id != planned,
                    due_review: word.due_review,
                    wrapped: false,
                    recent_passage: false,
                })
            })
            .collect();
        if labels.contains(&label::Label::DueReview) {
            parts.push("due review".to_string());
        }
        // Name the re-served words exactly as they appear in the target.
        let again: Vec<String> = self
            .ledger
            .as_ref()
            .map(|ledger| {
                ledger
                    .target()
                    .split(|unit| unit.get() == ' ')
                    .zip(labels.iter())
                    .filter(|(_, label)| **label == label::Label::Again)
                    .map(|(units, _)| units.iter().map(|unit| unit.get()).collect())
                    .collect()
            })
            .unwrap_or_default();
        if !again.is_empty() {
            parts.push(format!("again: {}", again.join(", ")));
        }
        (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
    }
    fn practice_view(&self) -> ScreenView {
        let phase = self.plan.phases.get(self.phase_index);
        let paused = self.session_state != SessionState::Running || self.finishing;
        let elapsed = self.active.checked_sub(self.phase_start);
        let remaining = phase
            .and_then(|p| Micros::from_seconds(p.seconds).ok())
            .and_then(|b| elapsed.ok().and_then(|e| b.checked_sub(e).ok()));
        let rate_unit = if self.current_track().is_ok_and(|t| t == Track::Numpad) {
            RateUnit::DigitsPerMinute
        } else {
            RateUnit::WordsPerMinute
        };
        // Completed exercises of this track only: the figures move at exercise
        // boundaries, never with every keystroke (P-05).
        let completed = self
            .current_track()
            .ok()
            .and_then(|track| self.tracks.iter().find(|p| p.track == track));
        let rate = if self.invalid {
            None
        } else {
            completed
                .and_then(track_rate)
                .map(|r| format!("{:.0}", r.as_f64()))
        };
        let first_pass_percent = completed
            .and_then(|p| track_accuracy(p).ok().flatten())
            .map(|a| a.get().checked_div(100).unwrap_or(0));
        let typing: Vec<usize> = self
            .plan
            .phases
            .iter()
            .enumerate()
            .filter(|(_, p)| p.phase != Phase::Review)
            .map(|(index, _)| index)
            .collect();
        let phase_position = typing
            .iter()
            .position(|index| *index == self.phase_index)
            .and_then(|p| p.checked_add(1))
            .and_then(|p| u32::try_from(p).ok())
            .unwrap_or(0);
        let phase_count = u32::try_from(typing.len()).unwrap_or(0);
        ScreenView::Practice {
            spans: self.ledger.as_ref().map_or_else(Vec::new, spans_for),
            paused,
            status: StatusFigures {
                rate,
                rate_unit,
                first_pass_percent,
                active_seconds: self.active.as_us().checked_div(1_000_000).unwrap_or(0),
                paused,
                phase_label: phase
                    .map_or("practice", |p| phase_label(p.phase))
                    .to_string(),
                phase_position,
                phase_count,
                phase_seconds_remaining: remaining
                    .map_or(0, |r| r.as_us().checked_div(1_000_000).unwrap_or(0)),
                enter_submits: self.current_track().is_ok_and(|t| t == Track::Numpad),
                corrections: self
                    .ledger
                    .as_ref()
                    .map_or(0, |ledger| ledger.counters().deletions()),
                context: self.exercise_context(),
            },
        }
    }
    fn view_mode(&self) -> ViewMode {
        if self.durability().is_failed() {
            ViewMode::SaveError
        } else if self.invalid {
            ViewMode::Invalid
        } else if self.finishing {
            ViewMode::Saving
        } else {
            match self.screen {
                Screen::Today => ViewMode::Ready,
                Screen::Practice if self.session_state == SessionState::Running => ViewMode::Typing,
                Screen::Practice => ViewMode::Paused,
                Screen::PhaseComplete { .. } => ViewMode::Next,
                Screen::Summary => ViewMode::Review,
            }
        }
    }

    fn phase_overview(&self) -> Vec<PhaseOverview> {
        self.plan
            .phases
            .iter()
            .enumerate()
            .map(|(index, phase)| {
                let state = match self.screen {
                    Screen::Today => PhaseState::Upcoming,
                    Screen::Summary => PhaseState::Complete,
                    _ if index < self.phase_index => PhaseState::Complete,
                    Screen::PhaseComplete { .. } if index == self.phase_index => {
                        PhaseState::Complete
                    }
                    Screen::Practice if index == self.phase_index => PhaseState::Active,
                    Screen::Practice | Screen::PhaseComplete { .. } => PhaseState::Upcoming,
                };
                PhaseOverview {
                    label: phase_label(phase.phase),
                    purpose: phase_purpose(phase.phase, self.learning.evidence.plan.mode),
                    seconds: phase.seconds,
                    state,
                }
            })
            .collect()
    }
}
fn spans_for(ledger: &AttemptLedger) -> Vec<UnitSpan> {
    let mut spans = Vec::with_capacity(ledger.target().len().max(ledger.buffer().len()));
    for (position, expected) in ledger.target().iter().enumerate() {
        spans.push(match ledger.buffer().get(position) {
            Some(typed) if typed == expected => UnitSpan::Correct {
                unit: expected.get(),
            },
            Some(typed) => UnitSpan::Wrong {
                typed: typed.get(),
                expected: expected.get(),
            },
            None if position == ledger.buffer().len() => UnitSpan::Cursor {
                expected: expected.get(),
            },
            None => UnitSpan::Upcoming {
                expected: expected.get(),
            },
        });
    }
    if let Some(extra) = ledger.buffer().get(ledger.target().len()..) {
        for c in extra {
            spans.push(UnitSpan::Extra { typed: c.get() });
        }
    }
    spans
}
/// A skill as the learner sees it; space and newline are made visible.
pub(super) fn skill_label(key: SkillKey) -> String {
    key.skill
        .text()
        .chars()
        .map(|c| match c {
            ' ' => '\u{2423}',
            '\n' => '\u{21b5}',
            other => other,
        })
        .collect()
}

fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::WarmUp => "warm-up",
        Phase::AdaptiveWords => "words",
        Phase::NumberRow => "number row",
        Phase::Numpad => "numpad",
        Phase::Prose => "prose",
        Phase::Review => "review",
    }
}

fn phase_purpose(phase: Phase, mode: PlannerMode) -> &'static str {
    match phase {
        Phase::AdaptiveWords if mode == PlannerMode::ColdStart => {
            "Broad vocabulary while Cadence records your slips."
        }
        Phase::WarmUp => "Settle into an easy rhythm.",
        Phase::AdaptiveWords => "Words chosen from your recorded slips, plus variety.",
        Phase::NumberRow => "Digits in short text contexts.",
        Phase::Numpad => "Exact entries. Leading zeros count.",
        Phase::Prose => {
            "Not targeted: this measures how accuracy carries into sentences. Slips here feed your closing review and tomorrow's words."
        }
        Phase::Review => "Look back at the session's real measurements.",
    }
}
