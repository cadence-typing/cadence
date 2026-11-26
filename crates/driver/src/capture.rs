//! Canonical frame capture (H-04, #70): the shared renderer
//! (`cadence_tui::paint`, the live painter) into an in-memory buffer of the
//! current viewport, read back as `contracts/frame.schema.json`: row-major
//! cells (symbol, width or continuation, colours, modifiers), the native
//! row renditions, the cursor and the prepared-view revision. `validate`
//! checks what the schema cannot: the cell count, an in-bounds visible
//! cursor, and wide-cell continuation pairs that never cross a row.

use cadence_core::MonoTick;
use cadence_engine::Engine;
use cadence_engine::view::LineRendition;
use cadence_tui::paint::paint;
use cadence_tui::theme::Palette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use serde_json::{Value, json};
use unicode_width::UnicodeWidthStr;

/// One cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The grapheme drawn (empty under a wide glyph).
    pub symbol: String,
    /// Display width: 1 or 2, 0 for a continuation.
    pub width: u8,
    /// The right half of a wide glyph.
    pub continuation: bool,
    /// Resolved foreground colour.
    pub foreground: String,
    /// Resolved background colour.
    pub background: String,
    /// Modifiers, in a fixed order.
    pub modifiers: Vec<&'static str>,
}

/// The hardware cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Shown. The live app hides it: the caret is a styled cell.
    pub visible: bool,
    /// Column.
    pub column: u16,
    /// Row.
    pub row: u16,
}

/// A canonical frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Columns.
    pub columns: u16,
    /// Rows.
    pub rows: u16,
    /// The prepared view's revision.
    pub revision: u64,
    /// Row-major cells, `columns * rows` of them.
    pub cells: Vec<Cell>,
    /// The cursor.
    pub cursor: Cursor,
    /// Native line rendition per row.
    pub renditions: Vec<LineRendition>,
}

fn colour(color: Color) -> String {
    match color {
        Color::Reset => "reset".to_string(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(index) => format!("indexed:{index}"),
        named => format!("{named:?}").to_lowercase(),
    }
}

fn modifiers(set: Modifier) -> Vec<&'static str> {
    [
        (Modifier::BOLD, "bold"),
        (Modifier::DIM, "dim"),
        (Modifier::ITALIC, "italic"),
        (Modifier::UNDERLINED, "underline"),
        (Modifier::REVERSED, "reverse"),
        (Modifier::HIDDEN, "hidden"),
        (Modifier::CROSSED_OUT, "crossed_out"),
    ]
    .into_iter()
    .filter(|(flag, _)| set.contains(*flag))
    .map(|(_, name)| name)
    .collect()
}

const fn rendition(rendition: LineRendition) -> &'static str {
    match rendition {
        LineRendition::Normal => "normal",
        LineRendition::DoubleHeightTop => "double_height_top",
        LineRendition::DoubleHeightBottom => "double_height_bottom",
    }
}

impl Frame {
    /// The readable text: continuation cells omitted, trailing spaces
    /// trimmed.
    #[must_use]
    pub fn text(&self) -> String {
        self.cells
            .chunks(usize::from(self.columns).max(1))
            .map(|row| {
                row.iter()
                    .filter(|cell| !cell.continuation)
                    .map(|cell| cell.symbol.as_str())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The cross-field invariants of H-04.
    ///
    /// # Errors
    /// The first violated invariant.
    pub fn validate(&self) -> Result<(), String> {
        let columns = usize::from(self.columns);
        let expected = columns
            .checked_mul(usize::from(self.rows))
            .ok_or("frame size overflow")?;
        if self.cells.len() != expected {
            return Err(format!("{} cells, expected {expected}", self.cells.len()));
        }
        if self.renditions.len() != usize::from(self.rows) {
            return Err("one rendition per row".to_string());
        }
        if self.cursor.visible
            && (self.cursor.column >= self.columns || self.cursor.row >= self.rows)
        {
            return Err("visible cursor out of bounds".to_string());
        }
        for (row_index, row) in self.cells.chunks(columns.max(1)).enumerate() {
            let mut wide_open = false;
            for (column, cell) in row.iter().enumerate() {
                if cell.continuation != wide_open {
                    return Err(format!(
                        "row {row_index}, column {column}: broken wide-cell continuation pair"
                    ));
                }
                if cell.continuation && cell.width != 0 {
                    return Err(format!(
                        "row {row_index}, column {column}: continuation with width"
                    ));
                }
                wide_open = cell.width == 2;
            }
            if wide_open {
                return Err(format!(
                    "row {row_index}: a wide glyph crosses the row boundary"
                ));
            }
        }
        Ok(())
    }

    /// The frame as `contracts/frame.schema.json`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": 1,
            "columns": self.columns,
            "rows": self.rows,
            "prepared_view_revision": self.revision.to_string(),
            "cells": self.cells.iter().map(|cell| json!({
                "symbol": cell.symbol,
                "width": cell.width,
                "continuation": cell.continuation,
                "foreground": cell.foreground,
                "background": cell.background,
                "modifiers": cell.modifiers,
            })).collect::<Vec<_>>(),
            "cursor": {"visible": self.cursor.visible, "column": self.cursor.column, "row": self.cursor.row},
            "row_renditions": self.renditions.iter().map(|r| rendition(*r)).collect::<Vec<_>>(),
        })
    }

    /// A compact golden form: per row, the text, then the style runs as
    /// `count×foreground/background/modifiers`.
    #[must_use]
    pub fn golden(&self) -> String {
        let mut out = String::new();
        for (index, row) in self
            .cells
            .chunks(usize::from(self.columns).max(1))
            .enumerate()
        {
            let text: String = row
                .iter()
                .filter(|cell| !cell.continuation)
                .map(|cell| cell.symbol.as_str())
                .collect();
            out.push_str(&format!(
                "{index:02} {} |{}\n",
                rendition(self.renditions.get(index).copied().unwrap_or_default()),
                text
            ));
            let mut runs: Vec<(String, usize)> = Vec::new();
            for cell in row {
                let style = format!(
                    "{}/{}/{}",
                    cell.foreground,
                    cell.background,
                    cell.modifiers.join("+")
                );
                match runs.last_mut() {
                    Some((last, count)) if *last == style => *count = count.saturating_add(1),
                    Some(_) | None => runs.push((style, 1)),
                }
            }
            let runs: Vec<String> = runs
                .iter()
                .map(|(style, count)| format!("{count}×{style}"))
                .collect();
            out.push_str(&format!("   {}\n", runs.join(" ")));
        }
        out.push_str(&format!(
            "cursor visible={} column={} row={}\n",
            self.cursor.visible, self.cursor.column, self.cursor.row
        ));
        out
    }
}

/// Paints the engine's prepared view at `at` into a `columns` x `rows`
/// buffer with the shared painter. Painting never changes engine state.
#[must_use]
pub fn capture(engine: &Engine, at: MonoTick, columns: u16, rows: u16) -> Frame {
    let view = engine.prepare(at);
    let mut buffer = Buffer::empty(Rect::new(0, 0, columns, rows));
    let area = buffer.area;
    let mut renditions = vec![LineRendition::Normal; usize::from(rows)];
    paint(
        &mut buffer,
        area,
        &view,
        Palette::cadence_dark(),
        &mut renditions,
    );
    let mut cells = Vec::with_capacity(usize::from(columns).saturating_mul(usize::from(rows)));
    for y in 0..rows {
        let mut wide_open = false;
        for x in 0..columns {
            let cell = &buffer[(x, y)];
            let continuation = wide_open;
            let width = if continuation {
                0
            } else {
                u8::try_from(cell.symbol().width()).unwrap_or(1).clamp(1, 2)
            };
            // A wide glyph at the last column cannot continue: draw it as
            // narrow rather than cross the row.
            let width = if width == 2 && x.saturating_add(1) == columns {
                1
            } else {
                width
            };
            wide_open = width == 2;
            cells.push(Cell {
                symbol: if continuation {
                    String::new()
                } else {
                    cell.symbol().to_string()
                },
                width,
                continuation,
                foreground: colour(cell.fg),
                background: colour(cell.bg),
                modifiers: modifiers(cell.modifier),
            });
        }
    }
    Frame {
        columns,
        rows,
        revision: view.revision.get(),
        cells,
        cursor: Cursor {
            visible: false,
            column: 0,
            row: 0,
        },
        renditions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(symbol: &str, width: u8, continuation: bool) -> Cell {
        Cell {
            symbol: symbol.to_string(),
            width,
            continuation,
            foreground: "reset".to_string(),
            background: "reset".to_string(),
            modifiers: Vec::new(),
        }
    }

    fn frame(cells: Vec<Cell>) -> Frame {
        Frame {
            columns: 3,
            rows: 1,
            revision: 0,
            cells,
            cursor: Cursor {
                visible: false,
                column: 0,
                row: 0,
            },
            renditions: vec![LineRendition::Normal],
        }
    }

    #[test]
    fn the_validator_checks_what_the_schema_cannot() {
        let good = frame(vec![
            cell("界", 2, false),
            cell("", 0, true),
            cell("a", 1, false),
        ]);
        assert!(good.validate().is_ok());
        // A wide glyph without its continuation.
        let broken = frame(vec![
            cell("界", 2, false),
            cell("a", 1, false),
            cell("b", 1, false),
        ]);
        assert!(broken.validate().is_err());
        // A continuation without a wide glyph.
        let orphan = frame(vec![
            cell("a", 1, false),
            cell("", 0, true),
            cell("b", 1, false),
        ]);
        assert!(orphan.validate().is_err());
        // A wide glyph crossing the row boundary.
        let crossing = frame(vec![
            cell("a", 1, false),
            cell("b", 1, false),
            cell("界", 2, false),
        ]);
        assert!(crossing.validate().is_err());
        // The cell count and a visible cursor in bounds.
        assert!(frame(vec![cell("a", 1, false)]).validate().is_err());
        let mut cursor = good;
        cursor.cursor = Cursor {
            visible: true,
            column: 3,
            row: 0,
        };
        assert!(cursor.validate().is_err());
    }
}
