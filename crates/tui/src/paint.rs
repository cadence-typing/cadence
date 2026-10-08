//! Pure paint from prepared view models (U-01, U-02).
//!
//! One shared path serves the live terminal and the headless
//! `TestBackend` (VR-01): both hand the same [`paint`] function a
//! `ratatui` buffer. Paint is a total function of the view, palette and
//! geometry — no I/O, no state mutation, no time. It is total for every
//! nonzero bounded viewport including 1×1 (U-02): all writes are
//! bounds-checked and clipped, never panicking.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;

use cadence_engine::{PreparedView, ScreenView, StatusFigures, UnitSpan};

use crate::theme::{Palette, SemanticRole as Role};

/// Bundled presentation parameters for one cell write.
#[derive(Clone, Copy)]
struct StyleSpec {
    palette: Palette,
    foreground: Role,
    background: Role,
    bold: bool,
}

impl StyleSpec {
    fn new(palette: Palette, foreground: Role, background: Role, bold: bool) -> Self {
        Self {
            palette,
            foreground,
            background,
            bold,
        }
    }

    fn style(self) -> Style {
        let mut style = Style::new()
            .fg(self.palette.color(self.foreground))
            .bg(self.palette.color(self.background));
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }
}

/// Paints the prepared view into `buffer` for the given area.
pub fn paint(buffer: &mut Buffer, area: Rect, view: &PreparedView, palette: Palette) {
    // Every screen clears its area first so no stale cells leak.
    clear(buffer, area, palette);
    match &view.screen {
        ScreenView::Today { plan_hint } => paint_today(buffer, area, plan_hint, palette),
        ScreenView::Practice {
            spans,
            status,
            paused,
        } => paint_practice(buffer, area, spans, status, *paused, palette),
        ScreenView::Result { lines, completed } => {
            paint_result(buffer, area, lines, *completed, palette);
        }
    }
}

fn clear(buffer: &mut Buffer, area: Rect, palette: Palette) {
    let background = palette.color(Role::Background);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.reset();
                cell.set_char(' ');
                cell.set_style(Style::new().bg(background));
            }
        }
    }
}

fn paint_today(buffer: &mut Buffer, area: Rect, plan_hint: &str, palette: Palette) {
    put_line(
        buffer,
        area,
        0,
        1,
        "cadence — today",
        StyleSpec::new(palette, Role::Text, Role::Background, true),
    );
    put_line(
        buffer,
        area,
        0,
        3,
        "enter  start drill",
        StyleSpec::new(palette, Role::Text, Role::Background, false),
    );
    put_line(
        buffer,
        area,
        0,
        4,
        "q      quit",
        StyleSpec::new(palette, Role::Muted, Role::Background, false),
    );
    put_line(
        buffer,
        area,
        0,
        6,
        plan_hint,
        StyleSpec::new(palette, Role::Muted, Role::Background, false),
    );
}

fn paint_practice(
    buffer: &mut Buffer,
    area: Rect,
    spans: &[UnitSpan],
    status: &StatusFigures,
    paused: bool,
    palette: Palette,
) {
    // Inline target region: correct units, wrong units (typed shown,
    // expected available), cursor at the next position, upcoming text.
    let mut column = 2u16;
    let mut row = area.height.div_ceil(2).saturating_sub(1);
    for span in spans {
        let (character, role, underline) = match *span {
            UnitSpan::Correct { unit } => (unit, Role::Correct, false),
            UnitSpan::Wrong { typed, .. } => (typed, Role::Error, false),
            UnitSpan::Cursor { expected } => (expected, Role::Focus, true),
            UnitSpan::Upcoming { expected } => (expected, Role::Muted, false),
        };
        put_unit(
            buffer,
            area,
            column,
            row,
            character,
            StyleSpec::new(palette, role, Role::Background, false),
            underline,
        );
        column = column.saturating_add(1);
        if column >= area.width.saturating_sub(2) {
            // Soft wrap at the region edge; renderer-introduced wraps are
            // never expected input (L-02).
            column = 2;
            row = row.saturating_add(1);
        }
    }

    // Status line: figures that answer a user question (U-01), quiet.
    let mut status_line = String::from("wpm ");
    match &status.wpm {
        Some(wpm) => status_line.push_str(wpm),
        None => status_line.push_str("--"),
    }
    if let Some(percent) = status.first_pass_percent {
        status_line.push_str(&format!("   first-pass {percent}%"));
    }
    status_line.push_str(&format!("   active {}s", status.active_seconds));
    if status.paused {
        status_line.push_str("   [paused]");
    }
    put_line(
        buffer,
        area,
        0,
        area.height.saturating_sub(1),
        &status_line,
        StyleSpec::new(palette, Role::Muted, Role::Background, false),
    );
    if paused {
        put_line(
            buffer,
            area,
            0,
            0,
            "paused — esc resumes",
            StyleSpec::new(palette, Role::Warning, Role::Background, true),
        );
    }
}

fn paint_result(
    buffer: &mut Buffer,
    area: Rect,
    lines: &[String],
    completed: bool,
    palette: Palette,
) {
    let (title, role) = if completed {
        ("result", Role::Correct)
    } else {
        ("result (partial)", Role::Warning)
    };
    put_line(
        buffer,
        area,
        0,
        1,
        title,
        StyleSpec::new(palette, role, Role::Background, true),
    );
    for (index, line) in lines.iter().enumerate() {
        let row = u16::try_from(index).unwrap_or(u16::MAX).saturating_add(3);
        put_line(
            buffer,
            area,
            0,
            row,
            line,
            StyleSpec::new(palette, Role::Text, Role::Background, false),
        );
    }
    put_line(
        buffer,
        area,
        0,
        area.height.saturating_sub(1),
        "esc/enter back   ctrl-c quit",
        StyleSpec::new(palette, Role::Muted, Role::Background, false),
    );
}

fn put_line(buffer: &mut Buffer, area: Rect, column: u16, row: u16, text: &str, spec: StyleSpec) {
    let y = area.y.saturating_add(row);
    if y >= area.bottom() {
        return;
    }
    let mut x = area.x.saturating_add(column);
    for character in text.chars() {
        if x >= area.right() {
            break;
        }
        if let Some(cell) = buffer.cell_mut((x, y)) {
            cell.set_char(character);
            cell.set_style(spec.style());
        }
        x = x.saturating_add(1);
    }
}

fn put_unit(
    buffer: &mut Buffer,
    area: Rect,
    column: u16,
    row: u16,
    character: char,
    spec: StyleSpec,
    underline: bool,
) {
    let y = area.y.saturating_add(row);
    let x = area.x.saturating_add(column);
    if y >= area.bottom() || x >= area.right() {
        return;
    }
    let display = if character == '\n' { ' ' } else { character };
    if let Some(cell) = buffer.cell_mut((x, y)) {
        cell.set_char(display);
        let mut style = spec.style();
        if underline {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        cell.set_style(style);
    }
}

/// Convenience for golden-text assertions: one line as styled spans.
#[must_use]
pub fn line_spans(text: &str, palette: Palette, role: Role) -> Vec<Span<'static>> {
    vec![Span::styled(
        text.to_string(),
        StyleSpec::new(palette, role, Role::Background, false).style(),
    )]
}
