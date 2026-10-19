//! Immutable, word-aware target layout. Wrapping follows expected units, never errors.

use std::ops::Range;

use cadence_engine::view::{PreparedView, ScreenView, StatusFigures, TextSize, UnitSpan};
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
        },
        palette,
    );
    resolved
}

fn expected(span: UnitSpan) -> char {
    match span {
        UnitSpan::Correct { unit } => unit,
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
    Span::raw(unit.encode_utf8(&mut utf8) as &str)
        .width()
        .max(1)
}

struct Footer<'a> {
    spans: &'a [UnitSpan],
    status: &'a StatusFigures,
    paused: bool,
    resolved: ResolvedTextSize,
}

fn paint_footer(buffer: &mut Buffer, area: Rect, footer: Footer<'_>, palette: Palette) {
    let Footer {
        spans,
        status,
        paused,
        resolved,
    } = footer;
    if area.is_empty() {
        return;
    }
    let mut y = area.y;
    let error = spans.iter().find_map(|span| match span {
        UnitSpan::Wrong { expected, .. } => Some(*expected),
        _ => None,
    });
    if let Some(expected) = error {
        centered(
            buffer,
            Rect::new(area.x, y, area.width, 1),
            &format!("Correction needed · expected ‘{}’", expected.escape_debug()),
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
            "Paused · Esc resumes"
        } else if status.enter_submits {
            "Keypad origin unverified · Enter submits the entry"
        } else {
            "Follow the caret · Backspace corrects"
        };
        centered(
            buffer,
            Rect::new(area.x, y, area.width, 1),
            instruction,
            style(palette, Role::Muted),
        );
    }
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
        UnitSpan::Wrong { .. } => {
            style(palette, Role::Error).add_modifier(Modifier::UNDERLINED | Modifier::REVERSED)
        }
        UnitSpan::Cursor { .. } => {
            style(palette, Role::Focus).add_modifier(Modifier::REVERSED | Modifier::BOLD)
        }
        UnitSpan::Upcoming { .. } => style(palette, Role::Muted).add_modifier(Modifier::DIM),
    }
}

fn style(palette: Palette, role: Role) -> Style {
    Style::default()
        .fg(palette.color(role))
        .bg(palette.color(Role::Background))
}
