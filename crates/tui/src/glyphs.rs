//! Portable 8×8 font data packed into 4×4 standard terminal quadrant cells.

use font8x8::{BASIC_FONTS, LATIN_FONTS, UnicodeFonts};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Paragraph, Widget, Wrap};

pub(super) const CELL_WIDTH: u16 = 4;
pub(super) const CELL_HEIGHT: u16 = 4;
const PIXELS_PER_CELL: u16 = 2;
const PIXEL_PAIR_MASK: u8 = 0b11;

/// Reject control/empty glyphs rather than silently losing unsupported input.
pub(super) fn bitmap(unit: char) -> Option<[u8; 8]> {
    if unit.is_control() {
        return None;
    }
    BASIC_FONTS
        .get(unit)
        .or_else(|| LATIN_FONTS.get(unit))
        .filter(|bits| unit == ' ' || bits.iter().any(|row| *row != 0))
}

/// Paint a single established font glyph, without allocating a string per cell.
pub(super) fn paint_bitmap(
    buffer: &mut Buffer,
    clip: Rect,
    x: u16,
    y: u16,
    bits: &[u8; 8],
    style: Style,
) {
    // Bit order: upper-left, upper-right, lower-left, lower-right.
    const QUADRANTS: [char; 16] = [
        ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
    ];
    let clip = clip.intersection(buffer.area);
    for row in 0..CELL_HEIGHT {
        for column in 0..CELL_WIDTH {
            let Some(cell_x) = x.checked_add(column) else {
                continue;
            };
            let Some(cell_y) = y.checked_add(row) else {
                continue;
            };
            if cell_x < clip.x
                || cell_x >= clip.right()
                || cell_y < clip.y
                || cell_y >= clip.bottom()
            {
                continue;
            }
            let shift = u32::from(column.saturating_mul(PIXELS_PER_CELL));
            let pixel_row = usize::from(row.saturating_mul(PIXELS_PER_CELL));
            let top = bits
                .get(pixel_row)
                .copied()
                .unwrap_or(0)
                .checked_shr(shift)
                .unwrap_or(0)
                & PIXEL_PAIR_MASK;
            let bottom = bits
                .get(pixel_row.saturating_add(1))
                .copied()
                .unwrap_or(0)
                .checked_shr(shift)
                .unwrap_or(0)
                & PIXEL_PAIR_MASK;
            if let Some(cell) = buffer.cell_mut((cell_x, cell_y)) {
                let index =
                    usize::from(top | bottom.checked_shl(u32::from(PIXELS_PER_CELL)).unwrap_or(0));
                cell.set_char(QUADRANTS.get(index).copied().unwrap_or('�'));
                cell.set_style(style);
            }
        }
    }
}

/// Center a complete large label; unsupported or cramped labels stay readable.
pub(super) fn paint_label(buffer: &mut Buffer, area: Rect, text: &str, style: Style) {
    let area = area.intersection(buffer.area);
    if area.is_empty() {
        return;
    }
    let width = text.chars().count().saturating_mul(usize::from(CELL_WIDTH));
    if area.height < CELL_HEIGHT
        || width > usize::from(area.width)
        || text.chars().any(|unit| bitmap(unit).is_none())
    {
        let height = area
            .height
            .min(text.lines().count().max(1).try_into().unwrap_or(u16::MAX));
        let normal = Rect::new(
            area.x,
            area.y.saturating_add(
                area.height
                    .saturating_sub(height)
                    .checked_div(2)
                    .unwrap_or(0),
            ),
            area.width,
            height,
        );
        Paragraph::new(text)
            .style(style)
            .centered()
            .wrap(Wrap { trim: false })
            .render(normal, buffer);
        return;
    }
    let width = u16::try_from(width).unwrap_or(area.width);
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(width).checked_div(2).unwrap_or(0));
    let y = area.y.saturating_add(
        area.height
            .saturating_sub(CELL_HEIGHT)
            .checked_div(2)
            .unwrap_or(0),
    );
    for (index, unit) in text.chars().enumerate() {
        if let Some(bits) = bitmap(unit) {
            let offset = u16::try_from(index).unwrap_or(0).saturating_mul(CELL_WIDTH);
            paint_bitmap(buffer, area, x.saturating_add(offset), y, &bits, style);
        }
    }
}
