//! Centered, read-only stage compositions. Counts describe observations, not mastery.

use cadence_core::progress::TrackProgress;
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
    let height = brand
        .saturating_add(gap.saturating_mul(5))
        .saturating_add(6)
        .saturating_add(purpose_rows)
        .saturating_add(baseline);
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
    text(
        buffer,
        take(&mut body, 1),
        format!(
            "Local history · {} completed · {} unfinished",
            view.history.completed_sessions, view.history.interrupted_sessions
        ),
        style(palette, Role::Muted),
    );
    if baseline != 0 {
        text(
            buffer,
            take(&mut body, 1),
            "A baseline in the making. Accuracy before speed.",
            style(palette, Role::Muted),
        );
    }
    take(&mut body, gap);
    action(buffer, take(&mut body, 1), "Begin session", view, palette);
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
            "Time reached · unfinished exercise evidence retained"
        } else {
            "Phase finished · a moment to reset"
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
    Paragraph::new(phase.purpose)
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
    text(
        buffer,
        take(&mut body, 1),
        format!(
            "First-pass {}/{}   Attempts {}/{}",
            report.first_correct,
            report.first_attempted,
            report.correct_attempts,
            report.content_attempts
        ),
        emphasis(palette),
    );
    let seconds = report.active_micros.checked_div(1_000_000).unwrap_or(0);
    text(
        buffer,
        take(&mut body, 1),
        format!(
            "{} corrections · {} correct units · {} active",
            report.deletions,
            report.correct_units,
            Time(seconds)
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

struct Time(u64);

impl std::fmt::Display for Time {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let minutes = self.0.checked_div(60).unwrap_or(0);
        let seconds = self.0.checked_rem(60).unwrap_or(0);
        write!(formatter, "{minutes}:{seconds:02}")
    }
}
