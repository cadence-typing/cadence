//! Two prominent session measures; native rows are shared by both cards.

use cadence_core::BasisPoints;
use cadence_engine::summary::SummaryMetrics;
use cadence_engine::view::{PreparedView, RateUnit};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::native_text::{CELL_HEIGHT, CELL_WIDTH, NativeText};
use crate::theme::{Palette, SemanticRole as Role};

/// Heading, two native number rows and a unit/explanation line.
pub(super) const COMPACT_ROWS: u16 = 4;
/// Add a quiet bottom border when the review has room.
pub(super) const ROOMY_ROWS: u16 = 5;
const CARD_GAP: u16 = 4;
/// Ten logical characters fit inside a native tile at the 60-column minimum.
const MAX_VALUE_CELLS: usize = 10;
const BASIS_POINTS_PER_PERCENT: u16 = 100;
const BASIS_POINTS_PER_TENTH: u16 = 10;

struct Tile {
    heading: &'static str,
    value: Option<String>,
    note: &'static str,
}

pub(super) fn paint(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    palette: Palette,
    native: &mut NativeText<'_>,
) {
    if area.height < COMPACT_ROWS {
        return;
    }
    let metrics = SummaryMetrics::from_reports(&view.summary_reports, view.summary_rate_valid);
    let (speed, accuracy, note) = match metrics {
        Ok(metrics) => {
            let speed = metrics.speed.map(|rate| {
                let value = format!("{:.1}", rate.as_f64());
                if value.len() <= MAX_VALUE_CELLS {
                    value
                } else {
                    format!("{:.1e}", rate.as_f64())
                }
            });
            let note = match metrics.speed_unit {
                RateUnit::WordsPerMinute => "WPM · typing tracks",
                RateUnit::DigitsPerMinute => "Keypad digits / min",
            };
            (speed, accuracy_label(metrics.first_pass_accuracy), note)
        }
        Err(_) => (None, None, "Totals unavailable"),
    };
    let tiles = [
        Tile {
            heading: "CORRECT SPEED",
            value: speed,
            note,
        },
        Tile {
            heading: "FIRST-PASS ACCURACY",
            value: accuracy,
            note: "Mistakes stay counted",
        },
    ];
    let [left, _, right] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(CARD_GAP),
        Constraint::Fill(1),
    ])
    .areas(area);
    let panels = [left, right].map(|panel| {
        if native.available() {
            // Align card edges to the same two-cell grid as their native values.
            let logical = logical_panel(panel, panel.y);
            Rect::new(
                logical.x.saturating_mul(CELL_WIDTH),
                panel.y,
                logical.width.saturating_mul(CELL_WIDTH),
                panel.height,
            )
        } else {
            panel
        }
    });
    let panel_style = style(palette, Role::Text).bg(palette.color(Role::Panel));
    for (panel, tile) in panels.iter().zip(&tiles) {
        buffer.set_style(*panel, panel_style);
        if area.height >= ROOMY_ROWS {
            Block::new()
                .borders(Borders::TOP | Borders::BOTTOM)
                .border_style(style(palette, Role::Border).bg(palette.color(Role::Panel)))
                .title(
                    Line::from(tile.heading)
                        .centered()
                        .style(style(palette, Role::Muted)),
                )
                .render(*panel, buffer);
        } else {
            Paragraph::new(tile.heading)
                .centered()
                .style(panel_style.fg(palette.color(Role::Muted)))
                .render(Rect::new(panel.x, panel.y, panel.width, 1), buffer);
        }
        Paragraph::new(tile.note)
            .centered()
            .style(panel_style.fg(palette.color(Role::Muted)))
            .render(
                Rect::new(
                    panel.x,
                    panel.y.saturating_add(COMPACT_ROWS.saturating_sub(1)),
                    panel.width,
                    1,
                ),
                buffer,
            );
    }
    let values = Rect::new(area.x, area.y.saturating_add(1), area.width, CELL_HEIGHT);
    let fits = panels.iter().zip(&tiles).all(|(panel, tile)| {
        let logical = logical_panel(*panel, values.y).inner(Margin {
            horizontal: 1,
            vertical: 0,
        });
        Span::raw(tile.value.as_deref().unwrap_or("—")).width() <= usize::from(logical.width)
    });
    if fits && let Some(halves) = native.pair(buffer, values) {
        for row in halves {
            // Whole-row scaling changes column coordinates. Remove normal-coordinate
            // panel fills before painting both panels in their logical columns.
            let visible = Rect::new(
                0,
                row.y,
                buffer.area.width.checked_div(CELL_WIDTH).unwrap_or(0),
                1,
            );
            Clear.render(visible, buffer);
            buffer.set_style(
                visible,
                style(palette, Role::Text).bg(palette.color(Role::Background)),
            );
            for (panel, tile) in panels.iter().zip(&tiles) {
                let logical = logical_panel(*panel, row.y);
                buffer.set_style(logical, panel_style);
                number(
                    buffer,
                    logical.inner(Margin {
                        horizontal: 1,
                        vertical: 0,
                    }),
                    tile,
                    palette,
                );
            }
        }
    } else {
        for (panel, tile) in panels.iter().zip(&tiles) {
            number(
                buffer,
                Rect::new(panel.x, values.y, panel.width, 1),
                tile,
                palette,
            );
        }
    }
}

fn logical_panel(panel: Rect, y: u16) -> Rect {
    let left = panel.x.div_ceil(CELL_WIDTH);
    let right = panel.right().checked_div(CELL_WIDTH).unwrap_or(left);
    Rect::new(left, y, right.saturating_sub(left), 1)
}

fn number(buffer: &mut Buffer, area: Rect, tile: &Tile, palette: Palette) {
    let role = if tile.value.is_some() {
        Role::Focus
    } else {
        Role::Muted
    };
    let value = tile.value.as_deref().unwrap_or("—");
    let width = u16::try_from(Span::raw(value).width())
        .unwrap_or(u16::MAX)
        .min(area.width);
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(width).checked_div(2).unwrap_or(0));
    buffer.set_stringn(
        x,
        area.y,
        value,
        usize::from(width),
        style(palette, role)
            .bg(palette.color(Role::Panel))
            .add_modifier(Modifier::BOLD),
    );
}

fn accuracy_label(accuracy: Option<BasisPoints>) -> Option<String> {
    accuracy.map(|accuracy| {
        let value = accuracy.get();
        let whole = value.checked_div(BASIS_POINTS_PER_PERCENT).unwrap_or(0);
        let tenth = value
            .checked_rem(BASIS_POINTS_PER_PERCENT)
            .and_then(|fraction| fraction.checked_div(BASIS_POINTS_PER_TENTH))
            .unwrap_or(0);
        // Never round a corrected mistake up to a displayed perfect 100.0%.
        format!("{whole}.{tenth}%")
    })
}

fn style(palette: Palette, role: Role) -> Style {
    Style::default().fg(palette.color(role))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_remaining_first_pass_error_never_displays_perfect_accuracy()
    -> Result<(), cadence_core::CoreError> {
        assert_eq!(
            accuracy_label(Some(BasisPoints::new(9999)?)).as_deref(),
            Some("99.9%")
        );
        assert_eq!(
            accuracy_label(Some(BasisPoints::MAX)).as_deref(),
            Some("100.0%")
        );
        assert_eq!(accuracy_label(None), None);
        Ok(())
    }
}
