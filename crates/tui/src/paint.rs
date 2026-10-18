//! One pure Ratatui painter for live output and TestBackend captures.

use cadence_engine::{PreparedView, ScreenView, StatusFigures, UnitSpan};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use crate::theme::{Palette, SemanticRole as Role};

/// Minimum full-practice geometry from U-02; the reducer pauses below it.
const MIN_COLUMNS: u16 = 60;
const MIN_ROWS: u16 = 18;
/// Conventional seconds per displayed minute; exact truncation is intentional.
const SECONDS_PER_MINUTE: u64 = 60;

/// Paints only the immutable prepared view. No clock, I/O or state mutation.
pub fn paint(buffer: &mut Buffer, area: Rect, view: &PreparedView, palette: Palette) {
    Clear.render(area, buffer);
    buffer.set_style(area, style(palette, Role::Text));
    if area.width < MIN_COLUMNS || area.height < MIN_ROWS {
        Paragraph::new("Terminal too small — practice paused. Resize to at least 60 x 18.")
            .style(style(palette, Role::Warning))
            .wrap(Wrap { trim: false })
            .render(area, buffer);
        return;
    }
    let [header, body, notice, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(area);
    Paragraph::new(Line::from(vec![
        Span::styled(
            " cadence ",
            style(palette, Role::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" / offline typing practice", style(palette, Role::Muted)),
    ]))
    .block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(style(palette, Role::Border)),
    )
    .render(header, buffer);
    let body = body.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    match &view.screen {
        ScreenView::Today {
            plan_hint,
            total_seconds,
        } => {
            let minutes = total_seconds.checked_div(SECONDS_PER_MINUTE).unwrap_or(0);
            let lines = vec![
                Line::styled("today", style(palette, Role::Focus)),
                Line::raw(""),
                Line::raw(format!("Guided session · {minutes} minutes allocated")),
                Line::raw(""),
                Line::raw(plan_hint.as_str()),
                Line::raw(""),
                Line::raw("Enter to begin. Accuracy comes before speed."),
                Line::raw(""),
                Line::styled(
                    "Building a baseline; no learned adaptation claimed yet.",
                    style(palette, Role::Muted),
                ),
                Line::raw(format!(
                    "Local history: {} completed · {} interrupted/partial",
                    view.history.completed_sessions, view.history.interrupted_sessions
                )),
            ];
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .render(body, buffer);
        }
        ScreenView::Practice {
            spans,
            status,
            paused,
        } => practice(buffer, body, spans, status, *paused, palette),
        ScreenView::PhaseComplete {
            label,
            partial,
            next,
        } => {
            let title = format!("{label} complete");
            let description = if *partial {
                "The deadline ended the scored slice; partial evidence is retained."
            } else {
                "Your observations are retained without averaging percentages."
            };
            let action = next.as_ref().map_or_else(
                || "Enter for the session summary".to_string(),
                |label| format!("Enter to continue with {label}"),
            );
            Paragraph::new(vec![
                Line::styled(title, style(palette, Role::Correct)),
                Line::raw(""),
                Line::raw(description),
                Line::raw(""),
                Line::raw(action),
            ])
            .wrap(Wrap { trim: false })
            .render(body, buffer);
        }
        ScreenView::Summary { lines } => {
            let mut text = vec![
                Line::styled("session summary", style(palette, Role::Focus)),
                Line::raw(""),
            ];
            text.extend(lines.iter().map(|line| Line::raw(line.as_str())));
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .render(body, buffer);
        }
    }
    let message = view.notice.as_deref().unwrap_or(if view.saving {
        "Saving local progress…"
    } else {
        ""
    });
    let role = if view.save_failed {
        Role::Error
    } else if view.saving {
        Role::Warning
    } else {
        Role::Muted
    };
    Paragraph::new(message)
        .style(style(palette, role))
        .wrap(Wrap { trim: false })
        .render(notice, buffer);
    let hints = if view.save_failed {
        " r retry save   Ctrl-C leave with last acknowledged progress "
    } else {
        " Enter continue/submit   Esc pause/resume   Ctrl-C save & quit "
    };
    Paragraph::new(hints)
        .style(style(palette, Role::Muted))
        .render(footer, buffer);
}

fn practice(
    buffer: &mut Buffer,
    area: Rect,
    spans: &[UnitSpan],
    status: &StatusFigures,
    paused: bool,
    palette: Palette,
) {
    let [heading, target, feedback, metrics] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(2),
        Constraint::Length(2),
        Constraint::Length(2),
    ])
    .areas(area);
    let label = if paused {
        "paused — Esc resumes".to_string()
    } else {
        format!(
            "{} · exercise {} · {}s left",
            status.phase_label, status.exercise_index, status.phase_seconds_remaining
        )
    };
    let subline = if status.enter_submits {
        "Keypad origin unverified · Enter submits an exact entry"
    } else {
        "Backspace corrects the buffer, never first-attempt history"
    };
    Paragraph::new(vec![
        Line::styled(label, style(palette, Role::Focus)),
        Line::styled(subline, style(palette, Role::Muted)),
    ])
    .render(heading, buffer);
    inline_target(buffer, target, spans, palette);
    let expected = spans.iter().find_map(|span| match span {
        UnitSpan::Wrong { expected, .. } => Some(*expected),
        _ => None,
    });
    if let Some(expected) = expected {
        Paragraph::new(format!(
            "Correction needed — expected {} at the first highlighted error.",
            expected.escape_default()
        ))
        .style(style(palette, Role::Error))
        .wrap(Wrap { trim: false })
        .render(feedback, buffer);
    }
    let accuracy = status
        .first_pass_percent
        .map_or_else(|| "—".to_string(), |value| format!("{value}%"));
    let pace = status.rate.as_deref().unwrap_or("—");
    Paragraph::new(format!(
        "First-pass {accuracy}   {} {pace}   Active {}s",
        status.rate_unit.label(),
        status.active_seconds
    ))
    .style(style(palette, Role::Muted))
    .wrap(Wrap { trim: false })
    .render(metrics, buffer);
}

fn inline_target(buffer: &mut Buffer, area: Rect, spans: &[UnitSpan], palette: Palette) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let width = usize::from(area.width);
    let caret = spans
        .iter()
        .position(|span| matches!(span, UnitSpan::Cursor { .. }))
        .unwrap_or(spans.len());
    let caret_row = caret.checked_div(width).unwrap_or(0);
    let top_row = caret_row.saturating_sub(usize::from(area.height).saturating_sub(1));
    // One training unit is one cell in the supported alphabet. Explicit
    // newlines have a visible return marker; renderer wraps aren't input.
    let start = top_row.saturating_mul(width);
    let visible = usize::from(area.height).saturating_mul(width);
    for (offset, span) in spans.iter().skip(start).take(visible).enumerate() {
        let Some(x) = offset
            .checked_rem(width)
            .and_then(|x| u16::try_from(x).ok())
        else {
            continue;
        };
        let Some(y) = offset
            .checked_div(width)
            .and_then(|y| u16::try_from(y).ok())
        else {
            continue;
        };
        let (unit, text_style) = match *span {
            UnitSpan::Correct { unit } => (unit, style(palette, Role::Correct)),
            UnitSpan::Wrong { typed, .. } => (
                typed,
                style(palette, Role::Error).add_modifier(Modifier::UNDERLINED | Modifier::REVERSED),
            ),
            UnitSpan::Cursor { expected } => (
                expected,
                style(palette, Role::Focus).add_modifier(Modifier::REVERSED),
            ),
            UnitSpan::Upcoming { expected } => (expected, style(palette, Role::Muted)),
        };
        if let Some(cell) = buffer.cell_mut((area.x.saturating_add(x), area.y.saturating_add(y))) {
            cell.set_char(if unit == '\n' { '↵' } else { unit });
            cell.set_style(text_style);
        }
    }
}

fn style(palette: Palette, role: Role) -> Style {
    Style::default()
        .fg(palette.color(role))
        .bg(palette.color(Role::Background))
}
