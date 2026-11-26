//! Centered, read-only stage compositions. Counts describe observations, not mastery.

use cadence_core::progress::{Track, TrackProgress};
use cadence_engine::summary::{SummaryMetrics, percent_label, track_accuracy, track_rate};
use cadence_engine::view::RateUnit;
use cadence_engine::view::{PhaseOverview, PhaseState, PreparedView, ScreenView, ViewMode};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::native_text::NativeText;
use crate::theme::{Palette, SemanticRole as Role};
mod headlines;
mod results;

const STAGE_MAX_WIDTH: u16 = 92;
const WORDMARK_WIDTH: u16 = 14;
const WORDMARK_ROWS: u16 = 2;
const SPACIOUS_STAGE_ROWS: u16 = 20;
const SPACIOUS_INTERLUDE_ROWS: u16 = 21;

/// All rectangles are intersected with the caller's body; chrome belongs to paint.rs.
pub(super) fn paint(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    palette: Palette,
    native: &mut NativeText<'_>,
) {
    let area = area.intersection(buffer.area);
    if area.is_empty() || matches!(view.screen, ScreenView::Practice { .. }) {
        return;
    }
    let area = centered_width(area, STAGE_MAX_WIDTH);
    match &view.screen {
        ScreenView::Today { total_seconds } => {
            landing(buffer, area, view, *total_seconds, palette, native);
        }
        ScreenView::PhaseComplete {
            label,
            partial,
            next,
        } => {
            interlude(
                buffer,
                area,
                view,
                (label, *partial, next.as_deref()),
                palette,
                native,
            );
        }
        ScreenView::Summary => results::paint(buffer, area, view, palette, native),
        ScreenView::Practice { .. } => {}
    }
}

fn landing(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    seconds: u64,
    palette: Palette,
    native: &mut NativeText<'_>,
) {
    if area.height < 7 {
        minimal(
            buffer,
            area,
            "Make room for a little practice.",
            "Begin session",
            view,
            palette,
        );
        return;
    }
    let spacious = area.height >= SPACIOUS_STAGE_ROWS;
    let brand = if spacious && area.width >= WORDMARK_WIDTH {
        WORDMARK_ROWS
    } else {
        u16::from(area.height >= 11)
    };
    let gap = u16::from(spacious);
    let purpose_rows = if area.height >= 11 { 2 } else { 1 };
    let baseline = u16::from(area.height >= 12);
    let goals = u16::from(area.height >= 12 && !view.learning.goals.is_empty());
    let height = brand
        .saturating_add(gap.saturating_mul(5))
        .saturating_add(6)
        .saturating_add(purpose_rows)
        .saturating_add(baseline)
        .saturating_add(goals);
    let mut body = centered_height(area, height);
    wordmark(buffer, take(&mut body, brand), palette, native);
    take(&mut body, gap);
    text(
        buffer,
        take(&mut body, 1),
        "Make room for a little practice.",
        emphasis(palette),
    );
    text(
        buffer,
        take(&mut body, 1),
        format!("{} allocated · practice at your pace", Time(seconds)),
        style(palette, Role::Muted),
    );
    take(&mut body, gap);
    route(buffer, take(&mut body, 2), &view.phases, palette);
    take(&mut body, gap);
    purpose(
        buffer,
        take(&mut body, purpose_rows),
        view.phases.first(),
        "UP FIRST",
        palette,
    );
    take(&mut body, gap);
    let history = view.last_session.as_ref().map_or_else(
        || {
            format!(
                "Local history · {} completed · {} unfinished",
                view.history.completed_sessions, view.history.interrupted_sessions
            )
        },
        |last| {
            let metrics = SummaryMetrics::from_reports(&last.tracks, true).ok();
            let speed = metrics
                .and_then(|m| m.speed.map(|rate| (rate, m.speed_unit)))
                .map_or_else(String::new, |(rate, unit)| {
                    format!(" · {:.1} {}", rate.as_f64(), short_unit(unit))
                });
            let accuracy = metrics
                .and_then(|m| m.first_pass_accuracy)
                .map_or_else(String::new, |a| format!(" · {}", percent_label(a)));
            let ending = if last.completed { "" } else { " (ended early)" };
            format!("Last session{speed}{accuracy} · {}{ending}", last.when())
        },
    );
    text(
        buffer,
        take(&mut body, 1),
        history,
        style(palette, Role::Muted),
    );
    if baseline != 0 {
        let row = take(&mut body, 1);
        let line = targets_line(&view.learning, row.width);
        text(buffer, row, line, style(palette, Role::Muted));
        if goals != 0 {
            let row = take(&mut body, 1);
            if let Some(line) = goals_line(&view.learning, row.width, false) {
                text(buffer, row, line, style(palette, Role::Muted));
            }
        }
    }
    take(&mut body, gap);
    action(buffer, take(&mut body, 1), "Begin session", view, palette);
}

/// "Words goal 52 WPM · raised   numpad 140 DPM · steady" (#62): each
/// typing track's pace goal and how it moved; whole goals drop from the
/// right when the line does not fit. `results` explains a lowering.
pub(crate) fn goals_line(
    learning: &cadence_engine::LearningView,
    width: u16,
    results: bool,
) -> Option<String> {
    use cadence_core::learning::PaceMove;
    use cadence_core::progress::Track;
    let parts: Vec<String> = learning
        .goals
        .iter()
        .map(|goal| {
            let (name, per_minute, unit) = match goal.track {
                Track::Numpad => ("numpad", 1, "DPM"),
                Track::NumberRow => ("number row", 5, "WPM"),
                Track::Prose => ("prose", 5, "WPM"),
                Track::Words | Track::WarmUp | Track::Review | Track::Benchmark => {
                    ("Words", 5, "WPM")
                }
            };
            match (goal.target_cpm, goal.moved) {
                (None, _) | (_, PaceMove::NoBaseline) => {
                    format!("{name} goal: building your baseline")
                }
                (Some(cpm), moved) => {
                    let value = cpm.checked_div(per_minute).unwrap_or(0);
                    let how = match moved {
                        PaceMove::Raised => "raised",
                        PaceMove::Lowered if results => "lowered: more familiar words next time",
                        PaceMove::Lowered => "lowered",
                        PaceMove::Steady | PaceMove::NoBaseline => "steady",
                    };
                    format!("{name} goal {value} {unit} \u{b7} {how}")
                }
            }
        })
        .collect();
    let width = usize::from(width);
    (1..=parts.len())
        .rev()
        .map(|keep| parts.get(..keep).unwrap_or_default().join("   "))
        .find(|line| line.chars().count() <= width)
}

/// "Today targets  th · ou · w   number row 7 · 48   numpad 0. · 55   5 due
/// reviews" from the plan record (#50); a group with nothing to target
/// says so; whole groups drop from the right with "+N more" when the line
/// does not fit.
pub(crate) fn targets_line(learning: &cadence_engine::LearningView, width: u16) -> String {
    let mut groups: Vec<String> = Vec::new();
    for group in &learning.targets {
        let skills = group.skills.join(" \u{b7} ");
        match (group.name, skills.is_empty()) {
            ("letters", false) => groups.push(skills),
            ("letters", true) => {}
            (name, false) => groups.push(format!("{name} {skills}")),
            (name, true) if !learning.adaptive => {
                groups.push(format!("{name}: no digit history yet"));
            }
            (_, true) => {}
        }
    }
    if learning.due_reviews > 0 {
        let plural = if learning.due_reviews == 1 { "" } else { "s" };
        groups.push(format!("{} due review{plural}", learning.due_reviews));
    }
    if groups.is_empty()
        || (!learning.adaptive && learning.targets.iter().all(|g| g.skills.is_empty()))
    {
        let long = "No per-key history yet: this session teaches Cadence your slips.";
        return if long.chars().count() <= usize::from(width) {
            long.to_string()
        } else {
            "No per-key history yet.".to_string()
        };
    }
    let width = usize::from(width);
    let compose = |shown: &[String], hidden: usize| {
        let mut line = format!("Today targets  {}", shown.join("   "));
        if hidden > 0 {
            line.push_str(&format!("   +{hidden} more"));
        }
        line
    };
    for keep in (1..=groups.len()).rev() {
        let (shown, hidden) = groups.split_at(keep);
        let line = compose(shown, hidden.len());
        if line.chars().count() <= width {
            return line;
        }
    }
    compose(
        groups.get(..1).unwrap_or_default(),
        groups.len().saturating_sub(1),
    )
}

fn interlude(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    phase: (&str, bool, Option<&str>),
    palette: Palette,
    native: &mut NativeText<'_>,
) {
    let (label, partial, next) = phase;
    if area.height < 9 {
        minimal(
            buffer,
            area,
            &format!("{label} complete"),
            if next.is_some() {
                "Continue"
            } else {
                "Review session"
            },
            view,
            palette,
        );
        return;
    }
    let spacious = area.height >= SPACIOUS_INTERLUDE_ROWS;
    let brand = if spacious && area.width >= WORDMARK_WIDTH {
        WORDMARK_ROWS
    } else {
        u16::from(area.height >= 11)
    };
    let gap = u16::from(spacious);
    let report_rows = if spacious { 4 } else { 2 };
    let purpose_rows = if spacious { 3 } else { 2 };
    let height = brand
        .saturating_add(gap.saturating_mul(5))
        .saturating_add(5)
        .saturating_add(report_rows)
        .saturating_add(purpose_rows);
    let mut body = centered_height(area, height);
    wordmark(buffer, take(&mut body, brand), palette, native);
    take(&mut body, gap);
    text(
        buffer,
        take(&mut body, 1),
        format!("{label} complete"),
        emphasis(palette),
    );
    text(
        buffer,
        take(&mut body, 1),
        if partial {
            "Time's up \u{2014} the partial line still counts"
        } else {
            "Phase finished \u{b7} a moment to reset"
        },
        style(palette, Role::Muted),
    );
    take(&mut body, gap);
    route(buffer, take(&mut body, 2), &view.phases, palette);
    take(&mut body, gap);
    report(
        buffer,
        take(&mut body, report_rows),
        view.phase_report.as_ref(),
        palette,
    );
    take(&mut body, gap);
    let next_phase = view
        .phases
        .iter()
        .find(|phase| phase.state == PhaseState::Upcoming);
    if next.is_some() {
        purpose(
            buffer,
            take(&mut body, purpose_rows),
            next_phase,
            "UP NEXT",
            palette,
        );
    } else {
        text(
            buffer,
            take(&mut body, purpose_rows),
            "UP NEXT · Session review\nYour actual results, one track at a time.",
            style(palette, Role::Text),
        );
    }
    take(&mut body, gap);
    action(
        buffer,
        take(&mut body, 1),
        if next.is_some() {
            "Continue"
        } else {
            "Review session"
        },
        view,
        palette,
    );
}

/// Two rows preserve the entire ordered route, even in the compact body.
fn route(buffer: &mut Buffer, area: Rect, phases: &[PhaseOverview], palette: Palette) {
    if area.is_empty() || phases.is_empty() {
        return;
    }
    let compact = area.width < 72;
    let columns = if compact {
        phases.len().div_ceil(2)
    } else {
        phases.len()
    };
    let columns = u16::try_from(columns).unwrap_or(u16::MAX).max(1);
    for (index, phase) in phases.iter().enumerate() {
        let index = u16::try_from(index).unwrap_or(u16::MAX);
        let column = index.checked_rem(columns).unwrap_or(0);
        let row = if compact {
            index.checked_div(columns).unwrap_or(0)
        } else {
            0
        };
        let left = u32::from(area.width)
            .saturating_mul(u32::from(column))
            .checked_div(u32::from(columns))
            .unwrap_or(0);
        let right = u32::from(area.width)
            .saturating_mul(u32::from(column.saturating_add(1)))
            .checked_div(u32::from(columns))
            .unwrap_or(0);
        let left = u16::try_from(left).unwrap_or(area.width);
        let right = u16::try_from(right).unwrap_or(area.width);
        let cell = Rect::new(
            area.x.saturating_add(left),
            area.y.saturating_add(row),
            right.saturating_sub(left),
            1,
        )
        .intersection(area);
        let (marker, role) = match phase.state {
            PhaseState::Complete => ("✓", Role::Correct),
            PhaseState::Active => ("●", Role::Focus),
            PhaseState::Upcoming => ("○", Role::Muted),
        };
        text(
            buffer,
            cell,
            format!("{marker} {}", phase.label),
            style(palette, role),
        );
        if !compact && area.height > 1 {
            let duration = Rect::new(cell.x, area.y.saturating_add(1), cell.width, 1);
            text(
                buffer,
                duration,
                Time(phase.seconds).to_string(),
                style(palette, Role::Muted),
            );
        }
    }
}

fn purpose(
    buffer: &mut Buffer,
    area: Rect,
    phase: Option<&PhaseOverview>,
    heading: &str,
    palette: Palette,
) {
    let Some(phase) = phase else { return };
    let mut body = area;
    if area.height > 1 {
        text(
            buffer,
            take(&mut body, 1),
            format!("{heading} · {}", phase.label),
            style(palette, Role::Focus),
        );
    }
    Paragraph::new(phase.purpose.as_str())
        .alignment(Alignment::Center)
        .style(style(palette, Role::Text))
        .wrap(Wrap { trim: true })
        .render(body, buffer);
}

fn report(buffer: &mut Buffer, area: Rect, report: Option<&TrackProgress>, palette: Palette) {
    let Some(report) = report else {
        text(
            buffer,
            area,
            "No scored attempts in this phase.",
            style(palette, Role::Muted),
        );
        return;
    };
    let mut body = area;
    if area.height >= 4 {
        let block = Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
            .border_style(style(palette, Role::Border))
            .style(style(palette, Role::Text).bg(palette.color(Role::Panel)));
        body = block.inner(area);
        block.render(area, buffer);
    }
    let accuracy = track_accuracy(report).ok().flatten().map_or_else(
        || "no attempts".to_string(),
        |a| format!("{} first-pass", percent_label(a)),
    );
    let unit = if report.track == Track::Numpad {
        RateUnit::DigitsPerMinute
    } else {
        RateUnit::WordsPerMinute
    };
    let pace = track_rate(report).map_or_else(
        || format!("\u{2014} {}", short_unit(unit)),
        |rate| format!("{:.0} {}", rate.as_f64(), short_unit(unit)),
    );
    text(
        buffer,
        take(&mut body, 1),
        format!(
            "{accuracy} \u{b7} {pace} \u{b7} {} correction{}",
            report.deletions,
            if report.deletions == 1 { "" } else { "s" }
        ),
        emphasis(palette),
    );
    let seconds = report.active_micros.checked_div(1_000_000).unwrap_or(0);
    text(
        buffer,
        take(&mut body, 1),
        format!(
            "{} active \u{b7} {} correct characters",
            Time(seconds),
            report.correct_units
        ),
        style(palette, Role::Muted),
    );
}

fn minimal(
    buffer: &mut Buffer,
    area: Rect,
    title: &str,
    cta: &str,
    view: &PreparedView,
    palette: Palette,
) {
    let mut body = area;
    text(buffer, take(&mut body, 1), title, emphasis(palette));
    if !body.is_empty() {
        action(
            buffer,
            Rect::new(body.x, body.bottom().saturating_sub(1), body.width, 1),
            cta,
            view,
            palette,
        );
    }
}

fn action(buffer: &mut Buffer, area: Rect, label: &str, view: &PreparedView, palette: Palette) {
    let label = if view.durability.is_failed() {
        std::borrow::Cow::Borrowed("r  Retry local save")
    } else if view.mode == ViewMode::Invalid {
        std::borrow::Cow::Borrowed("Ctrl-C  Finish with partial evidence")
    } else {
        std::borrow::Cow::Owned(format!("Enter  {label}"))
    };
    let width = u16::try_from(label.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4);
    let area = centered_width(area, width);
    text(
        buffer,
        area,
        label,
        style(palette, Role::Focus)
            .bg(palette.color(Role::SelectedBackground))
            .add_modifier(Modifier::BOLD),
    );
}

fn wordmark(buffer: &mut Buffer, area: Rect, palette: Palette, native: &mut NativeText<'_>) {
    native.paint_label(buffer, area, "cadence", emphasis(palette));
}

fn text<'a>(
    buffer: &mut Buffer,
    area: Rect,
    value: impl Into<ratatui::text::Text<'a>>,
    style: Style,
) {
    if !area.is_empty() {
        Paragraph::new(value)
            .alignment(Alignment::Center)
            .style(style)
            .render(area, buffer);
    }
}

fn take(area: &mut Rect, height: u16) -> Rect {
    let height = height.min(area.height);
    let result = Rect::new(area.x, area.y, area.width, height);
    area.y = area.y.saturating_add(height);
    area.height = area.height.saturating_sub(height);
    result
}

fn centered_width(area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    let inset = area.width.saturating_sub(width).checked_div(2).unwrap_or(0);
    Rect::new(area.x.saturating_add(inset), area.y, width, area.height)
}

fn centered_height(area: Rect, height: u16) -> Rect {
    let height = height.min(area.height);
    let inset = area
        .height
        .saturating_sub(height)
        .checked_div(2)
        .unwrap_or(0);
    Rect::new(area.x, area.y.saturating_add(inset), area.width, height)
}

fn style(palette: Palette, role: Role) -> Style {
    Style::default().fg(palette.color(role))
}

fn emphasis(palette: Palette) -> Style {
    style(palette, Role::Focus).add_modifier(Modifier::BOLD)
}

/// Compact rate unit for one-line summaries.
const fn short_unit(unit: RateUnit) -> &'static str {
    match unit {
        RateUnit::WordsPerMinute => "WPM",
        RateUnit::DigitsPerMinute => "digits/min",
    }
}

struct Time(u64);

impl std::fmt::Display for Time {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let minutes = self.0.checked_div(60).unwrap_or(0);
        let seconds = self.0.checked_rem(60).unwrap_or(0);
        write!(formatter, "{minutes}:{seconds:02}")
    }
}
