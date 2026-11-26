//! Frame capture (H-04): the shared renderer (`cadence_tui::paint`, the
//! same painter as live mode) into an in-memory buffer of the current
//! viewport. #70 extends the frame with styles, the cursor and cross-field
//! invariants.

use cadence_core::MonoTick;
use cadence_engine::Engine;
use cadence_engine::view::LineRendition;
use cadence_tui::paint::paint;
use cadence_tui::theme::Palette;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use serde_json::{Value, json};

/// A captured frame: one string of cell symbols per row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Columns.
    pub columns: u16,
    /// Rows of cell symbols, each exactly `columns` cells wide.
    pub rows: Vec<String>,
}

impl Frame {
    /// The readable text: rows with trailing spaces trimmed.
    #[must_use]
    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(|row| row.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The frame as JSON (`contracts/frame.schema.json` grows with #70).
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "columns": self.columns,
            "rows": self.rows,
        })
    }
}

/// Paints the engine's prepared view at `at` into a `columns` x `rows`
/// buffer with the shared painter.
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
    let lines = (0..rows)
        .map(|y| {
            (0..columns)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect();
    Frame {
        columns,
        rows: lines,
    }
}
