//! The Train and Numpad destinations and the help overlay (U-01, #80).
//! Both destinations start an ordinary single-track plan; help lists the
//! command table of the state beneath it, nothing else.

use cadence_engine::commands::{Binding, Destination};
use cadence_engine::view::DestinationView;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::theme::{Palette, SemanticRole as Role};

/// The help panel's widest useful width, in cells.
const HELP_COLUMNS: u16 = 64;
/// The key column in help.
const KEY_COLUMNS: usize = 16;
/// Rows a destination's description may gain by wrapping.
const WRAP_ROWS: u16 = 2;
/// Rows around the help list: the border, a blank and the closing hint.
const HELP_CHROME_ROWS: u16 = 4;

fn style(palette: Palette, role: Role) -> Style {
    Style::default()
        .fg(palette.color(role))
        .bg(palette.color(Role::Background))
}

fn clock(minutes: u64) -> String {
    format!("{minutes}:00")
}

/// The header's tabs: the selected destination stands out.
#[must_use]
pub fn tabs(selected: Destination, palette: Palette) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    for destination in Destination::ALL {
        let label = destination.label();
        if destination == selected {
            spans.push(Span::styled(
                label.to_uppercase(),
                style(palette, Role::Focus).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ));
        } else {
            spans.push(Span::styled(label, style(palette, Role::Muted)));
        }
        spans.push(Span::raw("   "));
    }
    Line::from(spans)
}

/// Train, Numpad or Progress, in place of the daily landing.
pub fn paint(
    buffer: &mut Buffer,
    area: Rect,
    full: &cadence_engine::PreparedView,
    palette: Palette,
) {
    let view: &DestinationView = &full.destination;
    let mut lines = Vec::new();
    match view.selected {
        Destination::Progress => {
            crate::progress::paint(buffer, area, full, palette);
            return;
        }
        Destination::Train => {
            lines.push(Line::styled(
                "Train one track",
                style(palette, Role::Text).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::styled(
                "Single-track practice, saved with your history like any session.",
                style(palette, Role::Muted),
            ));
            lines.push(Line::default());
            for (index, label) in view.tracks.iter().enumerate() {
                let selected = index == view.track;
                lines.push(Line::styled(
                    format!("{}{label}", if selected { "› " } else { "  " }),
                    if selected {
                        style(palette, Role::Focus).add_modifier(Modifier::BOLD)
                    } else {
                        style(palette, Role::Text)
                    },
                ));
            }
        }
        Destination::Numpad => {
            lines.push(Line::styled(
                "Numpad practice",
                style(palette, Role::Text).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::styled(
                "Digits, decimals and + - * / on the keypad, with their own history.",
                style(palette, Role::Muted),
            ));
        }
        Destination::Today => return,
    }
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("Length ", style(palette, Role::Muted)),
        Span::styled(
            clock(view.minutes),
            style(palette, Role::Text).add_modifier(Modifier::BOLD),
        ),
        Span::styled("   - shorter · + longer", style(palette, Role::Muted)),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            "Enter ",
            style(palette, Role::Focus).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Start", style(palette, Role::Text)),
    ]));
    // Room for the description to wrap at narrow widths.
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(WRAP_ROWS)
        .min(area.height);
    let [body] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    Paragraph::new(lines)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .render(body, buffer);
}

/// The help overlay: every binding of the state beneath.
pub fn help(buffer: &mut Buffer, area: Rect, bindings: &[Binding], palette: Palette) {
    let rows = u16::try_from(bindings.len())
        .unwrap_or(u16::MAX)
        .saturating_add(HELP_CHROME_ROWS);
    let [panel] = Layout::horizontal([Constraint::Length(area.width.min(HELP_COLUMNS))])
        .flex(Flex::Center)
        .areas(area);
    let [panel] = Layout::vertical([Constraint::Length(rows.min(panel.height))])
        .flex(Flex::Center)
        .areas(panel);
    Clear.render(panel, buffer);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help ")
        .border_style(style(palette, Role::Border))
        .style(style(palette, Role::Text));
    let inner = block.inner(panel);
    block.render(panel, buffer);
    let mut lines: Vec<Line> = bindings
        .iter()
        .map(|binding| {
            Line::from(vec![
                Span::styled(
                    format!("{:<KEY_COLUMNS$}", binding.keys),
                    style(palette, Role::Focus).add_modifier(Modifier::BOLD),
                ),
                Span::styled(binding.label, style(palette, Role::Text)),
            ])
        })
        .collect();
    lines.push(Line::default());
    lines.push(Line::styled(
        "Esc or ? closes help · while typing every key is practice",
        style(palette, Role::Muted),
    ));
    Paragraph::new(lines).render(inner, buffer);
}
