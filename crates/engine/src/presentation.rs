//! Read-only presentation; only admitted reducer events advance measurements.
use super::{Engine, PreparedView, Screen, ScreenView, StatusFigures, UnitSpan};
use crate::view::{LearningView, PhaseOverview, PhaseState, RateUnit, ViewMode};
use cadence_core::learning::{PlannerMode, Skill, SkillKey};
use cadence_core::ledger::AttemptLedger;
use cadence_core::metrics::{correct_digits, correct_wpm, digits_per_minute, first_pass_accuracy};
use cadence_core::progress::Track;
use cadence_core::session::SessionState;
use cadence_core::{Micros, MonoTick};
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
        if words.iter().any(|word| word.due_review) {
            parts.push("due review".to_string());
        }
        let planned = exercise.words.iter().map(|w| w.id);
        if words
            .iter()
            .zip(planned)
            .any(|(word, planned)| word.lane == Lane::Review && word.id != planned)
        {
            parts.push("missed earlier, again".to_string());
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
        let duration = self.active.checked_sub(self.exercise_start).ok();
        let rate_unit = if self.current_track().is_ok_and(|t| t == Track::Numpad) {
            RateUnit::DigitsPerMinute
        } else {
            RateUnit::WordsPerMinute
        };
        let rate = if self.invalid {
            None
        } else {
            self.ledger
                .as_ref()
                .and_then(|l| {
                    duration.and_then(|d| match rate_unit {
                        RateUnit::WordsPerMinute => correct_wpm(l.correct_visible_units().get(), d),
                        RateUnit::DigitsPerMinute => digits_per_minute(correct_digits(l).get(), d),
                    })
                })
                .map(|r| format!("{:.1}", r.as_f64()))
        };
        let first_pass_percent = self
            .ledger
            .as_ref()
            .and_then(first_pass_accuracy)
            .and_then(|r| {
                r.numerator()
                    .checked_mul(100)
                    .and_then(|n| n.checked_div(r.denominator()))
            })
            .and_then(|p| u16::try_from(p).ok());
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
                exercise_index: self
                    .exercise_index
                    .checked_add(1)
                    .and_then(|i| u32::try_from(i).ok())
                    .unwrap_or(0),
                exercise_total: phase
                    .and_then(|p| u32::try_from(p.exercises.len()).ok())
                    .unwrap_or(0),
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
                    _ => PhaseState::Upcoming,
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
            spans.push(UnitSpan::Wrong {
                typed: c.get(),
                expected: ' ',
            });
        }
    }
    spans
}
/// A skill as the learner sees it; space and newline are made visible.
fn skill_label(key: SkillKey) -> String {
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
        Phase::Prose => "Carry accuracy into sentences.",
        Phase::Review => "Look back at the session's real measurements.",
    }
}
