//! Read-only presentation; only admitted reducer events advance measurements.
use super::{Engine, PreparedView, Screen, ScreenView, StatusFigures, UnitSpan};
use crate::view::RateUnit;
use cadence_core::ledger::AttemptLedger;
use cadence_core::metrics::{correct_digits, correct_wpm, digits_per_minute, first_pass_accuracy};
use cadence_core::progress::Track;
use cadence_core::session::SessionState;
use cadence_core::{Micros, MonoTick};
use cadence_trainer::Phase;
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
                plan_hint: self
                    .plan
                    .phases
                    .iter()
                    .map(|p| format!("{} {}s", phase_label(p.phase), p.seconds))
                    .collect::<Vec<_>>()
                    .join(" · "),
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
            Screen::Summary => ScreenView::Summary {
                lines: self.summary_lines(),
            },
        };
        PreparedView {
            revision: self.revision,
            screen,
            active: self.active,
            notice,
            history: self.history.clone(),
            saving: self.saving(),
            save_failed: self.save_failed(),
        }
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
            },
        }
    }
    fn summary_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for track in &self.tracks {
            lines.push(format!(
                "{}: first-pass {}/{}  attempt {}/{}  correct units {}",
                track.track.label(),
                track.first_correct,
                track.first_attempted,
                track.correct_attempts,
                track.content_attempts,
                track.correct_units
            ));
            let duration = Micros::from_us(track.active_micros);
            let (label, rate) = if track.track == Track::Numpad {
                (
                    "correct digits/min",
                    digits_per_minute(track.correct_digits, duration),
                )
            } else {
                ("correct wpm", correct_wpm(track.correct_units, duration))
            };
            if self.invalid {
                lines.push("   measurement interrupted — no qualified rate".to_string());
            } else if let Some(rate) = rate {
                lines.push(format!("   {label} {:.1}", rate.as_f64()));
            } else {
                lines.push(format!("   {label} — (too little active time)"));
            }
        }
        if lines.is_empty() {
            lines.push("no scored exercises this session".to_string());
        }
        lines.push(
            "Frozen-plan cycling; sparse evidence, not an adaptive baseline qualification."
                .to_string(),
        );
        lines
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
