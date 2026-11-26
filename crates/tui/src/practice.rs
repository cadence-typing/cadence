//! Immutable, word-aware target layout. Wrapping follows expected units, never errors.

use std::ops::Range;

use cadence_engine::commands::paused_bindings;
use cadence_engine::view::{
    PauseReason, PreparedView, ScreenView, StatusFigures, TextSize, UnitSpan, ViewMode,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Widget};

use crate::native_text::{CELL_HEIGHT, CELL_WIDTH, NativeText};
use crate::theme::{Palette, SemanticRole as Role};

const AUTO_MIN_ROWS: u16 = 2;
const LARGE_ROW_GAP: u16 = 1;
const FOOTER_ROWS: u16 = 2;

/// The actual canvas mode, including a visible failed large-size preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResolvedTextSize {
    Large,
    Normal,
    Fallback(FallbackReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FallbackReason {
    Terminal,
    Geometry,
}

impl ResolvedTextSize {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Large => "LARGE 2x",
            Self::Normal => "NORMAL",
            Self::Fallback(_) => "NORMAL*",
        }
    }
}

/// Draw without retaining scroll state or modifying target, input or counters.
pub(super) fn paint(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    palette: Palette,
    native: &mut NativeText<'_>,
) -> ResolvedTextSize {
    let ScreenView::Practice {
        spans,
        status,
        paused,
    } = &view.screen
    else {
        return ResolvedTextSize::Normal;
    };
    let preferred = view.text_size;
    let area = area.intersection(buffer.area);
    // Native letters need only two cells per dimension; the former bitmap
    // threshold is unnecessary. Auto still requires two complete text rows.
    let wants_large = preferred != TextSize::Normal;
    // Small forced canvases may omit ancillary lines, but never a partial glyph.
    let large_footer = area.height.saturating_sub(CELL_HEIGHT).min(FOOTER_ROWS);
    let large_height = area.height.saturating_sub(large_footer);
    let large_rows = large_height
        .saturating_add(LARGE_ROW_GAP)
        .checked_div(CELL_HEIGHT.saturating_add(LARGE_ROW_GAP))
        .unwrap_or(0);
    let needs_error_hint = spans
        .iter()
        .any(|span| matches!(span, UnitSpan::Wrong { .. }));
    let geometry_ok = area.width >= CELL_WIDTH
        && (!needs_error_hint || large_footer > 0)
        && large_rows
            >= if preferred == TextSize::Auto {
                AUTO_MIN_ROWS
            } else {
                1
            };
    let resolved = if wants_large && !native.available() {
        ResolvedTextSize::Fallback(FallbackReason::Terminal)
    } else if wants_large && geometry_ok {
        ResolvedTextSize::Large
    } else if wants_large {
        ResolvedTextSize::Fallback(FallbackReason::Geometry)
    } else {
        ResolvedTextSize::Normal
    };
    if area.is_empty() {
        return resolved;
    }
    // A stopped clock must not become free reading time: the controls
    // replace the target until practice resumes.
    if view.mode == ViewMode::Paused {
        paused_controls(buffer, area, view, palette);
        return resolved;
    }
    let large = resolved == ResolvedTextSize::Large;
    let footer_height = if large {
        large_footer
    } else {
        area.height.saturating_sub(1).min(FOOTER_ROWS)
    };
    let canvas = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(footer_height),
    );
    let width = if large {
        usize::from(canvas.width.checked_div(CELL_WIDTH).unwrap_or(0))
    } else {
        usize::from(canvas.width)
    };
    let rows = wrap_rows(spans, width.max(1));
    // With no cursor (notably a complete numpad entry), follow the last unit,
    // not a fictitious extra row after it.
    let caret = spans
        .iter()
        .position(|span| matches!(span, UnitSpan::Cursor { .. }))
        .unwrap_or_else(|| spans.len().saturating_sub(1));
    let mut row_count = 0;
    let mut caret_row = 0;
    for (index, row) in rows.clone().enumerate() {
        row_count = index.saturating_add(1);
        if row.contains(&caret) {
            caret_row = index;
        }
    }
    let row_height = if large { CELL_HEIGHT } else { 1 };
    let stride = if large {
        CELL_HEIGHT.saturating_add(LARGE_ROW_GAP)
    } else {
        1
    };
    let capacity = if large {
        usize::from(
            canvas
                .height
                .saturating_add(LARGE_ROW_GAP)
                .checked_div(stride)
                .unwrap_or(0),
        )
    } else {
        usize::from(canvas.height)
    };
    let count = capacity.min(row_count);
    let top = caret_row
        .saturating_sub(capacity.checked_div(2).unwrap_or(0))
        .min(row_count.saturating_sub(count));
    let block_height = u16::try_from(count)
        .unwrap_or(0)
        .saturating_mul(stride)
        .saturating_sub(stride.saturating_sub(row_height));
    let y = canvas.y.saturating_add(
        canvas
            .height
            .saturating_sub(block_height)
            .checked_div(2)
            .unwrap_or(0),
    );
    for (offset, range) in rows.skip(top).take(count).enumerate() {
        let row = Rect::new(
            canvas.x,
            y.saturating_add(u16::try_from(offset).unwrap_or(0).saturating_mul(stride)),
            canvas.width,
            row_height,
        );
        let visible_spans = spans.get(range).unwrap_or_default();
        if large {
            if let Some(halves) = native.pair(buffer, row) {
                for half in halves {
                    paint_normal_line(buffer, half, visible_spans, palette);
                }
            }
        } else {
            paint_normal_line(buffer, row, visible_spans, palette);
        }
    }
    let footer = Rect::new(area.x, canvas.bottom(), area.width, footer_height);
    paint_footer(
        buffer,
        footer,
        Footer {
            spans,
            status,
            paused: *paused,
            resolved,
            first_session: view.history.completed_sessions == 0,
        },
        palette,
    );
    resolved
}

/// Width of the controls panel; keys and labels are short by design.
const CONTROLS_WIDTH: u16 = 44;
/// Column where a binding's label starts inside the panel.
const CONTROLS_KEY_COLUMN: u16 = 14;

fn paused_controls(buffer: &mut Buffer, area: Rect, view: &PreparedView, palette: Palette) {
    let reason = match view.pause_reason {
        Some(PauseReason::FocusLost) => "The terminal lost focus; the clock stopped.",
        Some(PauseReason::PasteRejected) => "Paste ignored; nothing was inserted.",
        Some(PauseReason::TooSmall) => "The terminal was too small; the clock stopped.",
        Some(PauseReason::SavingSlow) => "Saving is slow; the clock is stopped.",
        Some(PauseReason::Requested) | None => "The clock is stopped; your line is kept.",
    };
    let keys = paused_bindings(view.pause_reason);
    let rows = u16::try_from(keys.len())
        .unwrap_or(u16::MAX)
        .saturating_add(4);
    let width = CONTROLS_WIDTH.min(area.width);
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(width).checked_div(2).unwrap_or(0));
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(rows).checked_div(2).unwrap_or(0));
    let panel = Rect::new(x, y, width, rows).intersection(area);
    let heading = Style::default()
        .fg(palette.color(Role::Focus))
        .bg(palette.color(Role::Background))
        .add_modifier(Modifier::BOLD);
    let mut line = panel.y;
    let mut next = |text: &str, text_style: Style, buffer: &mut Buffer| {
        if line < panel.bottom() {
            centered(
                buffer,
                Rect::new(panel.x, line, panel.width, 1),
                text,
                text_style,
            );
        }
        line = line.saturating_add(1);
    };
    next("Paused", heading, buffer);
    next(reason, style(palette, Role::Muted), buffer);
    next("", style(palette, Role::Muted), buffer);
    for binding in keys {
        if line >= panel.bottom() {
            break;
        }
        let key_area = Rect::new(panel.x, line, CONTROLS_KEY_COLUMN.min(panel.width), 1);
        Paragraph::new(binding.keys)
            .style(style(palette, Role::Text).add_modifier(Modifier::BOLD))
            .render(key_area, buffer);
        let label_x = panel.x.saturating_add(CONTROLS_KEY_COLUMN);
        if label_x < panel.right() {
            Paragraph::new(binding.label)
                .style(style(palette, Role::Muted))
                .render(
                    Rect::new(label_x, line, panel.right().saturating_sub(label_x), 1),
                    buffer,
                );
        }
        line = line.saturating_add(1);
    }
}

fn expected(span: UnitSpan) -> char {
    match span {
        UnitSpan::Correct { unit } => unit,
        UnitSpan::Extra { typed } => typed,
        UnitSpan::Wrong { expected, .. }
        | UnitSpan::Cursor { expected }
        | UnitSpan::Upcoming { expected } => expected,
    }
}

/// Preserve every unit, including boundary spaces. Newlines terminate a row;
/// oversized words split at unit boundaries. A restartable iterator avoids a
/// range allocation on every redraw without cloning the target.
fn wrap_rows(spans: &[UnitSpan], width: usize) -> impl Clone + Iterator<Item = Range<usize>> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= spans.len() {
            return None;
        }
        let mut end = start;
        let mut used: usize = 0;
        let mut last_break = None;
        while let Some(span) = spans.get(end) {
            let unit = expected(*span);
            let unit_width = char_width(display_char(unit));
            if used.saturating_add(unit_width) > width && end > start {
                if let Some(boundary) = last_break {
                    end = boundary;
                }
                break;
            }
            used = used.saturating_add(unit_width);
            end = end.saturating_add(1);
            if unit == '\n' {
                break;
            }
            if unit.is_whitespace() {
                last_break = Some(end);
            }
        }
        let row = start..end;
        start = end;
        Some(row)
    })
}

/// Use Ratatui's Unicode-width-aware text rendering, not raw wide-cell writes.
/// A stack UTF-8 buffer lets the common normal path avoid per-unit allocations.
fn paint_normal_line(buffer: &mut Buffer, area: Rect, spans: &[UnitSpan], palette: Palette) {
    let width = spans
        .iter()
        .map(|span| char_width(display_char(span.cell_char())))
        .fold(0_usize, usize::saturating_add);
    let caret = spans
        .iter()
        .position(|span| matches!(span, UnitSpan::Cursor { .. }))
        .unwrap_or_else(|| spans.len().saturating_sub(1));
    // A wrong wide character may widen a line, but never change target wrapping.
    // Scroll only this actual-text line enough to retain the caret.
    let caret_end = spans
        .iter()
        .take(caret.saturating_add(1))
        .map(|span| char_width(display_char(span.cell_char())))
        .fold(0_usize, usize::saturating_add);
    let mut first = 0;
    let mut skipped_width: usize = 0;
    while first < caret && caret_end.saturating_sub(skipped_width) > usize::from(area.width) {
        let Some(span) = spans.get(first) else {
            break;
        };
        skipped_width = skipped_width.saturating_add(char_width(display_char(span.cell_char())));
        first = first.saturating_add(1);
    }
    let mut x = area.x.saturating_add(
        area.width
            .saturating_sub(u16::try_from(width).unwrap_or(u16::MAX))
            .checked_div(2)
            .unwrap_or(0),
    );
    for span in spans.iter().skip(first) {
        let unit = display_char(span.cell_char());
        let unit_width = char_width(unit);
        if x >= area.right() {
            break;
        }
        let mut utf8 = [0; 4];
        let available = area.right().saturating_sub(x);
        // A one-cell viewport cannot hold a wide grapheme. Show a replacement,
        // rather than silently discarding the target/caret.
        let visible = if unit_width > usize::from(available) {
            '�'
        } else {
            unit
        };
        let text: &str = visible.encode_utf8(&mut utf8);
        let cell_width = u16::try_from(char_width(visible)).unwrap_or(1);
        if cell_width == 1 {
            if let Some(cell) = buffer.cell_mut((x, area.y)) {
                cell.set_char(visible).set_style(unit_style(*span, palette));
            }
        } else {
            Paragraph::new(text)
                .style(unit_style(*span, palette))
                .render(Rect::new(x, area.y, cell_width, 1), buffer);
        }
        x = x.saturating_add(cell_width);
    }
}

fn display_char(unit: char) -> char {
    match unit {
        '\n' => '↵',
        '\t' => '⇥',
        unit if unit.is_control() => '�',
        unit => unit,
    }
}

fn char_width(unit: char) -> usize {
    let mut utf8 = [0; 4];
    let text: &str = unit.encode_utf8(&mut utf8);
    Span::raw(text).width().max(1)
}

struct Footer<'a> {
    spans: &'a [UnitSpan],
    status: &'a StatusFigures,
    paused: bool,
    resolved: ResolvedTextSize,
    first_session: bool,
}

fn paint_footer(buffer: &mut Buffer, area: Rect, footer: Footer<'_>, palette: Palette) {
    let Footer {
        spans,
        status,
        paused,
        resolved,
        first_session,
    } = footer;
    if area.is_empty() {
        return;
    }
    let mut y = area.y;
    let error = spans.iter().find_map(|span| match span {
        UnitSpan::Wrong { expected, .. } => Some(*expected),
        UnitSpan::Correct { .. }
        | UnitSpan::Cursor { .. }
        | UnitSpan::Upcoming { .. }
        | UnitSpan::Extra { .. } => None,
    });
    let past_end = spans
        .iter()
        .any(|span| matches!(span, UnitSpan::Extra { .. }));
    if past_end || error.is_some() {
        let message = match error {
            _ if past_end => "Past the end · Backspace to the first error".to_string(),
            Some(' ') => "Correction needed · expected space".to_string(),
            Some(expected) => format!("Correction needed · expected ‘{}’", expected.escape_debug()),
            None => String::new(),
        };
        centered(
            buffer,
            Rect::new(area.x, y, area.width, 1),
            &message,
            style(palette, Role::Error),
        );
        y = y.saturating_add(1);
    } else if let ResolvedTextSize::Fallback(reason) = resolved {
        centered(
            buffer,
            Rect::new(area.x, y, area.width, 1),
            match reason {
                FallbackReason::Geometry => "Normal text · large needs more room",
                FallbackReason::Terminal => "Normal text · native scaling unavailable",
            },
            style(palette, Role::Warning),
        );
        y = y.saturating_add(1);
    }
    if y < area.bottom() {
        let instruction = if paused {
            ""
        } else if status.enter_submits {
            "Enter submits"
        } else if let Some(context) = status.context.as_deref() {
            context
        } else if first_session {
            "Follow the caret · Backspace corrects"
        } else {
            ""
        };
        let fitted = fit_whole(instruction, usize::from(area.width));
        centered(
            buffer,
            Rect::new(area.x, y, area.width, 1),
            &fitted,
            style(palette, Role::Muted),
        );
    }
}

/// Fits a context line to `width` without cutting a token (#7, #50):
/// whole " · " parts drop from the right; when the first part alone is too
/// wide, whole list items drop from its end, marked with "…".
pub(crate) fn fit_whole(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let parts: Vec<&str> = text.split(" \u{b7} ").collect();
    for keep in (1..parts.len()).rev() {
        let line = parts.get(..keep).unwrap_or_default().join(" \u{b7} ");
        if line.chars().count() <= width {
            return line;
        }
    }
    let first = parts.first().copied().unwrap_or_default();
    let items: Vec<&str> = first.split(", ").collect();
    for keep in (1..items.len()).rev() {
        let line = format!("{}, \u{2026}", items.get(..keep).unwrap_or_default().join(", "));
        if line.chars().count() <= width {
            return line;
        }
    }
    String::new()
}

fn centered(buffer: &mut Buffer, area: Rect, text: &str, style: Style) {
    Paragraph::new(text)
        .style(style)
        .centered()
        .render(area, buffer);
}

fn unit_style(span: UnitSpan, palette: Palette) -> Style {
    match span {
        UnitSpan::Correct { .. } => style(palette, Role::Correct),
        UnitSpan::Wrong { .. } | UnitSpan::Extra { .. } => {
            style(palette, Role::Error).add_modifier(Modifier::UNDERLINED | Modifier::REVERSED)
        }
        UnitSpan::Cursor { .. } => {
            style(palette, Role::Focus).add_modifier(Modifier::REVERSED | Modifier::BOLD)
        }
        // Upcoming text is what the eyes read ahead: quieter than typed text,
        // but never faint (DIM roughly halves its contrast on many terminals).
        UnitSpan::Upcoming { .. } => style(palette, Role::Muted),
    }
}

fn style(palette: Palette, role: Role) -> Style {
    Style::default()
        .fg(palette.color(role))
        .bg(palette.color(Role::Background))
}

#[cfg(test)]
mod fit_tests {
    use super::fit_whole;

    #[test]
    fn context_lines_never_cut_a_token() {
        let line = "Targeting n' \u{b7} om \u{b7} ne \u{b7} la \u{b7} de \u{b7} ad \u{b7} due review \u{b7} again: truly, really, nearly";
        for width in [10, 20, 30, 40, 58, 60, 100] {
            let fitted = fit_whole(line, width);
            assert!(fitted.chars().count() <= width, "{width}: {fitted}");
            for token in fitted.split([' ', ',']) {
                let token = token.trim_end_matches('\u{2026}');
                assert!(token.is_empty() || line.contains(token), "{width}: cut token {token:?}");
            }
        }
        assert_eq!(fit_whole("again: truly, really, nearly", 22), "again: truly, \u{2026}");
    }
}
