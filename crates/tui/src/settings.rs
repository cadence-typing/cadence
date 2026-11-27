//! The settings overlay (U-04, #79): a bordered panel over the body with one
//! row per key, scrolled to keep the selected row visible, the draft value,
//! when a saved change applies, and the controls. "Saved" is never shown
//! here; the engine names the outcome only after the worker acknowledges.

use cadence_engine::view::SettingsView;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};

use crate::theme::{Palette, SemanticRole as Role};

/// The panel's widest useful width, in cells.
const PANEL_COLUMNS: u16 = 72;
/// Rows below the list: one blank, the status and the controls.
const FOOTER_ROWS: u16 = 3;
/// The top and bottom border.
const BORDER_ROWS: u16 = 2;
/// The key label column: the longest label (`decimal_separator`) and a gap.
const LABEL_COLUMNS: usize = 18;
/// The value column; longer values (lists) are cut and edited in the file.
const VALUE_COLUMNS: usize = 16;

fn style(palette: Palette, foreground: Role, background: Role) -> Style {
    Style::default()
        .fg(palette.color(foreground))
        .bg(palette.color(background))
}

/// The first row to show so that `cursor` is inside a window of `visible`.
fn first_row(cursor: usize, visible: usize, total: usize) -> usize {
    let last_start = total.saturating_sub(visible);
    cursor
        .saturating_sub(visible.saturating_sub(1))
        .min(last_start)
}

/// The settings overlay's title and controls.
pub const SETTINGS: (&str, &str) = (
    " Settings ",
    // Fits the panel at 60 columns.
    "↑↓ choose  ←→ change  Enter save  r reload  Esc close",
);
/// First-run setup's title and controls (#78).
pub const SETUP: (&str, &str) = (
    " Set up Cadence ",
    "↑↓ choose · ←→ change · Enter start (as shown)",
);

/// Paints the overlay into `area`.
pub fn paint(
    buffer: &mut Buffer,
    area: Rect,
    view: &SettingsView,
    palette: Palette,
    (title, controls): (&str, &str),
) {
    let [panel] = Layout::horizontal([Constraint::Length(area.width.min(PANEL_COLUMNS))])
        .flex(Flex::Center)
        .areas(area);
    // As tall as its rows, the footer and the border need, at most the body.
    let wanted = u16::try_from(view.rows.len())
        .unwrap_or(u16::MAX)
        .saturating_add(FOOTER_ROWS)
        .saturating_add(BORDER_ROWS);
    let [panel] = Layout::vertical([Constraint::Length(wanted.min(panel.height))])
        .flex(Flex::Center)
        .areas(panel);
    Clear.render(panel, buffer);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(style(palette, Role::Border, Role::Panel))
        .style(style(palette, Role::Text, Role::Panel));
    let inner = block.inner(panel);
    block.render(panel, buffer);
    let [list, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(FOOTER_ROWS)]).areas(inner);
    let visible = usize::from(list.height).max(1);
    let start = first_row(view.cursor, visible, view.rows.len());
    let lines: Vec<Line> = view
        .rows
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(index, row)| {
            let selected = index == view.cursor;
            let background = if selected {
                Role::SelectedBackground
            } else {
                Role::Panel
            };
            let marker = if selected { "› " } else { "  " };
            let when = match (row.editable, row.applies_now) {
                (false, true | false) => "edit config.toml",
                (true, true) => "applies now",
                (true, false) => "next session",
            };
            let value_role = if row.changed { Role::Focus } else { Role::Text };
            Line::from(vec![
                Span::styled(
                    format!(
                        "{marker}{:<LABEL_COLUMNS$}",
                        row.key.split_once('.').map_or(row.key, |(_, key)| key)
                    ),
                    style(palette, Role::Text, background),
                ),
                Span::styled(
                    format!("{:<VALUE_COLUMNS$.VALUE_COLUMNS$} ", row.value),
                    style(palette, value_role, background).add_modifier(if row.changed {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
                Span::styled(when, style(palette, Role::Muted, background)),
            ])
        })
        .collect();
    Paragraph::new(lines).render(list, buffer);
    let status = if view.saving {
        "Saving…".to_string()
    } else {
        view.notice.clone().unwrap_or_default()
    };
    Paragraph::new(vec![
        Line::default(),
        Line::styled(status, style(palette, Role::Warning, Role::Panel)),
        Line::styled(controls, style(palette, Role::Muted, Role::Panel)),
    ])
    .wrap(Wrap { trim: true })
    .render(footer, buffer);
}

#[cfg(test)]
mod tests {
    use super::first_row;

    #[test]
    fn the_window_keeps_the_cursor_visible() {
        assert_eq!(first_row(0, 5, 14), 0);
        assert_eq!(first_row(4, 5, 14), 0);
        assert_eq!(first_row(5, 5, 14), 1);
        assert_eq!(first_row(13, 5, 14), 9);
        assert_eq!(first_row(3, 20, 14), 0);
    }
}
