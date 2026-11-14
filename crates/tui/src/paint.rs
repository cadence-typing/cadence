//! Shared pure composition for live output and deterministic cell captures.

use cadence_engine::view::{LineRendition, MINIMUM_COLUMNS, MINIMUM_ROWS, PhaseState, ViewMode};
use cadence_engine::{PreparedView, ScreenView};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use crate::native_text::NativeText;
use crate::practice::ResolvedTextSize;
use crate::theme::{Palette, SemanticRole as Role};

/// Quiet breadcrumb and separator; the writing canvas gets the remaining space.
const HEADER_ROWS: u16 = 2;
/// The modeline and its context-specific hints always occupy two complete rows.
const MODELINE_ROWS: u16 = 2;
/// Errors retain a multi-line diagnostic area independent of decorative content.
const NOTICE_ROWS: u16 = 3;

/// Paints the immutable engine snapshot without I/O, timing or state mutation.
pub fn paint(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    palette: Palette,
    rows: &mut [LineRendition],
) {
    let area = area.intersection(buffer.area);
    let mut native = NativeText::new(view.text_support, area, buffer.area, rows);
    Clear.render(area, buffer);
    buffer.set_style(area, style(palette, Role::Text));
    if area.width < MINIMUM_COLUMNS || area.height < MINIMUM_ROWS {
        Paragraph::new(
            "Terminal too small — practice paused. Resize to at least 60 × 18, then Esc to resume.",
        )
        .style(style(palette, Role::Warning))
        .wrap(Wrap { trim: false })
        .render(area, buffer);
        return;
    }
    let notice_rows = if view.notice.is_some() {
        NOTICE_ROWS
    } else {
        0
    };
    let [header, body, notice, modeline] = Layout::vertical([
        Constraint::Length(HEADER_ROWS),
        Constraint::Min(1),
        Constraint::Length(notice_rows),
        Constraint::Length(MODELINE_ROWS),
    ])
    .areas(area);
    header_bar(buffer, header, view, palette);
    let body = body.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let actual_text_size = match &view.screen {
        ScreenView::Practice { .. } => {
            crate::practice::paint(buffer, body, view, palette, &mut native)
        }
        _ => {
            crate::stages::paint(buffer, body, view, palette, &mut native);
            ResolvedTextSize::Normal
        }
    };
    if let Some(message) = &view.notice {
        let role = if view.durability.is_failed() || view.mode == ViewMode::Invalid {
            Role::Error
        } else {
            Role::Warning
        };
        Paragraph::new(message.as_str())
            .style(style(palette, role))
            .wrap(Wrap { trim: false })
            .render(
                notice.inner(Margin {
                    horizontal: 2,
                    vertical: 0,
                }),
                buffer,
            );
    }
    crate::modeline::paint(buffer, modeline, view, palette, actual_text_size);
}

fn header_bar(buffer: &mut Buffer, area: Rect, view: &PreparedView, palette: Palette) {
    let label = if matches!(view.screen, ScreenView::Today { .. }) {
        "  DAILY PRACTICE"
    } else {
        "  cadence"
    };
    Block::default()
        .borders(Borders::BOTTOM)
        .border_style(style(palette, Role::Border))
        .render(area, buffer);
    let [left, right] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(22)]).areas(area);
    // During practice the header answers "where am I in today's session".
    let route = matches!(view.screen, ScreenView::Practice { .. })
        .then(|| route_line(view, palette))
        .filter(|line| u16::try_from(line.width()).is_ok_and(|w| w <= left.width));
    if let Some(line) = route {
        Paragraph::new(line).render(left, buffer);
    } else {
        Paragraph::new(label)
            .style(style(palette, Role::Focus).add_modifier(Modifier::BOLD))
            .render(left, buffer);
    }
    Paragraph::new(concat!(env!("CARGO_PKG_VERSION"), "  "))
        .alignment(Alignment::Right)
        .style(style(palette, Role::Muted))
        .render(right, buffer);
}

fn route_line(view: &PreparedView, palette: Palette) -> Line<'static> {
    let mut spans = vec![Span::styled("  ", style(palette, Role::Muted))];
    for (index, phase) in view.phases.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", style(palette, Role::Muted)));
        }
        let (marker, role) = match phase.state {
            PhaseState::Complete => ("\u{2713}", Role::Correct),
            PhaseState::Active => ("\u{25cf}", Role::Focus),
            PhaseState::Upcoming => ("\u{25cb}", Role::Muted),
        };
        let mut text_style = style(palette, role);
        if phase.state == PhaseState::Active {
            text_style = text_style.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(
            format!("{marker} {}", phase.label),
            text_style,
        ));
    }
    Line::from(spans)
}

fn style(palette: Palette, role: Role) -> Style {
    Style::default()
        .fg(palette.color(role))
        .bg(palette.color(Role::Background))
}
