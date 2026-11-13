//! Compact per-track results, separate from landing and transition composition.

use cadence_core::Micros;
use cadence_core::metrics::{correct_wpm, digits_per_minute};
use cadence_core::progress::{Track, TrackProgress};
use cadence_engine::view::PreparedView;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{Paragraph, Widget};

use super::{Time, action, centered_height, emphasis, headlines, minimal, style, take, text};
use crate::native_text::NativeText;
use crate::theme::{Palette, SemanticRole as Role};

/// Title and final action must remain visible before optional review detail.
const TITLE_ACTION_ROWS: u16 = 2;
/// Gaps after the heading, after the tiles and before the final action.
const SECTION_GAP_ROWS: u16 = 3;
const SUMMARY_COLUMN_WIDTHS: [u16; 6] = [10, 9, 9, 5, 6, 12];
const SUMMARY_MIN_WIDTH: u16 = 56;
pub(super) fn paint(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    palette: Palette,
    native: &mut NativeText<'_>,
) {
    let table_rows = u16::try_from(view.summary_reports.len())
        .unwrap_or(u16::MAX)
        .max(1);
    let minimum = headlines::COMPACT_ROWS
        .saturating_add(TITLE_ACTION_ROWS)
        .saturating_add(1);
    if area.height < minimum {
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
    let detailed_height = minimum.saturating_add(table_rows);
    let show_table = area.height >= detailed_height;
    let base = if show_table { detailed_height } else { minimum };
    let mut remaining = area.height.saturating_sub(base);
    let tile_rows = if reserve(&mut remaining, 1) {
        headlines::ROOMY_ROWS
    } else {
        headlines::COMPACT_ROWS
    };
    let show_active = reserve(&mut remaining, 1);
    let show_legend = show_table && reserve(&mut remaining, 1);
    let show_baseline = reserve(&mut remaining, 1);
    let gap = u16::from(reserve(&mut remaining, SECTION_GAP_ROWS));
    let stride = if show_table && reserve(&mut remaining, table_rows) {
        2
    } else {
        1
    };
    let height = area.height.saturating_sub(remaining);
    let mut body = centered_height(area, height);
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
            format!("{} active · session observations", Time(seconds)),
            style(palette, Role::Muted),
        );
    }
    take(&mut body, gap);
    headlines::paint(buffer, take(&mut body, tile_rows), view, palette, native);
    take(&mut body, gap);
    if show_table {
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
    } else {
        text(
            buffer,
            take(&mut body, 1),
            "Resize for per-track details",
            style(palette, Role::Muted),
        );
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
                format!(
                    "{} keys recorded · {} missed words scheduled · not a benchmark",
                    view.learning.observed_keys, view.learning.missed_words
                )
            } else {
                "Measurement interrupted · no qualified rates".to_string()
            },
            style(palette, Role::Muted),
        );
    }
    take(&mut body, gap);
    action(buffer, take(&mut body, 1), "Close session", view, palette);
}

fn reserve(remaining: &mut u16, rows: u16) -> bool {
    if let Some(rest) = remaining.checked_sub(rows) {
        *remaining = rest;
        true
    } else {
        false
    }
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
