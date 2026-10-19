//! Compact per-track results, separate from landing and transition composition.

use cadence_core::Micros;
use cadence_core::metrics::{correct_wpm, digits_per_minute};
use cadence_core::progress::{Track, TrackProgress};
use cadence_engine::view::PreparedView;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Widget};

use super::{
    SPACIOUS_STAGE_ROWS, Time, WORDMARK_ROWS, WORDMARK_WIDTH, action, centered_height, emphasis,
    minimal, style, take, text, wordmark,
};
use crate::theme::{Palette, SemanticRole as Role};

const SUMMARY_BRAND_ROWS: u16 = 23;
const SUMMARY_SPACED_ROWS: u16 = 25;
const SUMMARY_COLUMN_WIDTHS: [u16; 6] = [10, 9, 9, 5, 6, 12];
const SUMMARY_MIN_WIDTH: u16 = 56;
pub(super) fn paint(buffer: &mut Buffer, area: Rect, view: &PreparedView, palette: Palette) {
    let count = u16::try_from(view.summary_reports.len()).unwrap_or(u16::MAX);
    let table_rows = count.max(1);
    if area.height < table_rows.saturating_add(3) {
        minimal(
            buffer,
            area,
            "Session review",
            "Close session",
            view,
            palette,
        );
        return;
    }
    let stride = if area.height >= SUMMARY_SPACED_ROWS {
        2
    } else {
        1
    };
    let brand = if area.height >= SUMMARY_BRAND_ROWS && area.width >= WORDMARK_WIDTH {
        WORDMARK_ROWS
    } else {
        0
    };
    let gap = u16::from(area.height >= SPACIOUS_STAGE_ROWS);
    let show_active = area.height >= table_rows.saturating_add(4);
    let show_legend = area.height >= table_rows.saturating_add(5);
    let show_baseline = area.height >= table_rows.saturating_add(6);
    let height = brand
        .saturating_add(gap.saturating_mul(3))
        .saturating_add(3)
        .saturating_add(table_rows.saturating_mul(stride))
        .saturating_add(u16::from(show_active))
        .saturating_add(u16::from(show_legend))
        .saturating_add(u16::from(show_baseline));
    let mut body = centered_height(area, height);
    wordmark(buffer, take(&mut body, brand), palette);
    take(&mut body, gap);
    text(
        buffer,
        take(&mut body, 1),
        "Session review",
        emphasis(palette),
    );
    if show_active {
        let seconds = view.active.as_us().checked_div(1_000_000).unwrap_or(0);
        text(
            buffer,
            take(&mut body, 1),
            format!(
                "{} active · observations, not a mastery score",
                Time(seconds)
            ),
            style(palette, Role::Muted),
        );
    }
    take(&mut body, gap);
    results_header(buffer, take(&mut body, 1), palette);
    if view.summary_reports.is_empty() {
        text(
            buffer,
            take(&mut body, stride),
            "No scored exercises this session.",
            style(palette, Role::Muted),
        );
    } else {
        for result in &view.summary_reports {
            result_row(
                buffer,
                take(&mut body, stride),
                result,
                view.summary_rate_valid,
                palette,
            );
        }
    }
    if show_legend {
        text(
            buffer,
            take(&mut body, 1),
            "First / attempt: correct / tried · Del: corrections",
            style(palette, Role::Muted),
        );
    }
    if show_baseline {
        text(
            buffer,
            take(&mut body, 1),
            if view.summary_rate_valid {
                "Frozen plan · sparse evidence, not a qualified baseline"
            } else {
                "Measurement interrupted · no qualified rates"
            },
            style(palette, Role::Muted),
        );
    }
    take(&mut body, gap);
    action(buffer, take(&mut body, 1), "Close session", view, palette);
}
/// Fixed numeric lanes remain aligned across tracks; surplus width goes to names.
fn result_columns(area: Rect) -> [Rect; 6] {
    let mut extra_width = area.width.saturating_sub(SUMMARY_MIN_WIDTH);
    let mut x = area.x;
    SUMMARY_COLUMN_WIDTHS.map(|width| {
        let width = width.saturating_add(extra_width);
        extra_width = 0;
        let cell = Rect::new(x, area.y, width, 1).intersection(area);
        x = x.saturating_add(width.saturating_add(1));
        cell
    })
}

fn results_header(buffer: &mut Buffer, area: Rect, palette: Palette) {
    for (cell, label) in result_columns(area).into_iter().zip([
        "Track",
        "First",
        "Attempt",
        "Del",
        "Units",
        "Correct pace",
    ]) {
        Paragraph::new(label)
            .style(style(palette, Role::Muted))
            .render(cell, buffer);
    }
}

fn result_row(
    buffer: &mut Buffer,
    area: Rect,
    report: &TrackProgress,
    valid_rate: bool,
    palette: Palette,
) {
    let [name, first, attempts, deletions, units, pace] = result_columns(area);
    let label = match report.track {
        Track::WarmUp => "warm-up",
        Track::Words => "words",
        Track::NumberRow => "number row",
        Track::Numpad => "numpad",
        Track::Prose => "prose",
    };
    Paragraph::new(label)
        .style(style(palette, Role::Focus))
        .render(name, buffer);
    Paragraph::new(format!(
        "{}/{}",
        report.first_correct, report.first_attempted
    ))
    .style(style(palette, Role::Text))
    .render(first, buffer);
    Paragraph::new(format!(
        "{}/{}",
        report.correct_attempts, report.content_attempts
    ))
    .style(style(palette, Role::Text))
    .render(attempts, buffer);
    Paragraph::new(report.deletions.to_string())
        .style(style(palette, Role::Muted))
        .render(deletions, buffer);
    Paragraph::new(report.correct_units.to_string())
        .style(style(palette, Role::Text))
        .render(units, buffer);
    let active = Micros::from_us(report.active_micros);
    let (rate, unit) = if report.track == Track::Numpad {
        (digits_per_minute(report.correct_digits, active), "DPM")
    } else {
        (correct_wpm(report.correct_units, active), "WPM")
    };
    if let Some(rate) = rate.filter(|_| valid_rate) {
        Paragraph::new(format!("{:.1} {unit}", rate.as_f64()))
            .style(style(palette, Role::Text))
            .render(pace, buffer);
    } else {
        Paragraph::new(format!("— {unit}"))
            .style(style(palette, Role::Muted))
            .render(pace, buffer);
    }
}
