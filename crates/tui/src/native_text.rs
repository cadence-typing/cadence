//! Terminal-native double-height text, with normal text on unsupported surfaces.

use cadence_engine::view::{LineRendition, TextSupport};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;

pub(crate) const CELL_WIDTH: u16 = 2;
pub(crate) const CELL_HEIGHT: u16 = 2;

/// Per-frame line rendition metadata; native rows require a full-width viewport.
pub(crate) struct NativeText<'a> {
    viewport: Rect,
    buffer_area: Rect,
    rows: &'a mut [LineRendition],
    enabled: bool,
}

impl<'a> NativeText<'a> {
    pub(crate) fn new(
        support: TextSupport,
        viewport: Rect,
        buffer_area: Rect,
        rows: &'a mut [LineRendition],
    ) -> Self {
        rows.fill(LineRendition::Normal);
        let enabled = support == TextSupport::DoubleHeight
            && viewport.x == 0
            && buffer_area.x == 0
            && viewport.width == buffer_area.width
            && contains(buffer_area, viewport)
            && viewport
                .y
                .checked_add(viewport.height)
                .is_some_and(|bottom| usize::from(bottom) <= rows.len());
        Self {
            viewport,
            buffer_area,
            rows,
            enabled,
        }
    }

    pub(crate) fn available(&self) -> bool {
        self.enabled
    }

    /// Reserve both physical rows atomically, returning their logical text lanes.
    pub(crate) fn pair(&mut self, buffer: &Buffer, area: Rect) -> Option<[Rect; 2]> {
        let [top, bottom] = self.geometry(buffer, area)?;
        let end = bottom.y.checked_add(1)?;
        let rows = self.rows.get_mut(usize::from(top.y)..usize::from(end))?;
        rows.copy_from_slice(&[
            LineRendition::DoubleHeightTop,
            LineRendition::DoubleHeightBottom,
        ]);
        Some([top, bottom])
    }

    /// Center a label in native text lanes or use ordinary single-height text.
    pub(crate) fn paint_label(
        &mut self,
        buffer: &mut Buffer,
        area: Rect,
        text: &str,
        style: Style,
    ) {
        let width = Span::raw(text).width();
        let logical_width = area
            .x
            .checked_add(area.width)
            .and_then(|right| right.checked_div(CELL_WIDTH))
            .and_then(|right| right.checked_sub(area.x.div_ceil(CELL_WIDTH)));
        if logical_width.is_some_and(|available| width <= usize::from(available))
            && let Some(pair) = self.pair(buffer, area)
        {
            for row in pair {
                centered_label(buffer, row, text, width, style);
            }
            return;
        }
        let area = area.intersection(self.viewport).intersection(buffer.area);
        if !area.is_empty() {
            let y = area
                .y
                .saturating_add(area.height.saturating_sub(1).checked_div(2).unwrap_or(0));
            centered_label(
                buffer,
                Rect::new(area.x, y, area.width, 1),
                text,
                width,
                style,
            );
        }
    }

    fn geometry(&self, buffer: &Buffer, area: Rect) -> Option<[Rect; 2]> {
        if !self.enabled
            || area.height < CELL_HEIGHT
            || !contains(self.viewport, area)
            || !contains(self.buffer_area, area)
            || !contains(buffer.area, area)
        {
            return None;
        }
        let right = area.x.checked_add(area.width)?.checked_div(CELL_WIDTH)?;
        let left = area.x.div_ceil(CELL_WIDTH);
        let width = right.checked_sub(left)?;
        if width == 0 {
            return None;
        }
        let bottom = area.y.checked_add(1)?;
        let pair = [
            Rect::new(left, area.y, width, 1),
            Rect::new(left, bottom, width, 1),
        ];
        if pair.iter().any(|row| {
            !contains(buffer.area, *row)
                || !contains(self.buffer_area, *row)
                || !contains(self.viewport, *row)
        }) {
            return None;
        }
        let end = bottom.checked_add(1)?;
        self.rows.get(usize::from(area.y)..usize::from(end))?;
        Some(pair)
    }
}

fn contains(outer: Rect, inner: Rect) -> bool {
    let Some(outer_right) = outer.x.checked_add(outer.width) else {
        return false;
    };
    let Some(outer_bottom) = outer.y.checked_add(outer.height) else {
        return false;
    };
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner
            .x
            .checked_add(inner.width)
            .is_some_and(|right| right <= outer_right)
        && inner
            .y
            .checked_add(inner.height)
            .is_some_and(|bottom| bottom <= outer_bottom)
}

fn centered_label(buffer: &mut Buffer, area: Rect, text: &str, width: usize, style: Style) {
    let width = u16::try_from(width).unwrap_or(u16::MAX).min(area.width);
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(width).checked_div(2).unwrap_or(0));
    buffer.set_stringn(x, area.y, text, usize::from(width), style);
}
