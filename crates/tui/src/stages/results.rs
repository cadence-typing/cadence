//! Session review: headlines, per-track rows that answer "how did I do",
//! a like-for-like change against an earlier session, and what slipped.

use cadence_core::progress::{Track, TrackProgress};
use cadence_engine::summary::{
    percent_label, points_change, rate_change, track_accuracy, track_rate,
};
use cadence_engine::view::{Baseline, PreparedView};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Widget};

use super::{Time, action, centered_height, emphasis, headlines, minimal, style, take, text};
use crate::native_text::NativeText;
use crate::theme::{Palette, SemanticRole as Role};

/// Title and final action must remain visible before optional review detail.
const TITLE_ACTION_ROWS: u16 = 2;
/// Gaps after the heading, after the tiles and before the final action.
const SECTION_GAP_ROWS: u16 = 3;
/// Track, accuracy, pace, corrections, change since the last session.
const SUMMARY_COLUMN_WIDTHS: [u16; 5] = [10, 7, 11, 5, 19];
/// Raw first-pass fractions, added only when the review is wide enough.
const DETAIL_COLUMN_WIDTH: u16 = 13;
const COLUMN_GAP: u16 = 1;

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
        minimal(buffer, area, "Session review", "Save & exit", view, palette);
        return;
    }
    let detailed_height = minimum.saturating_add(table_rows);
    let show_table = area.height >= detailed_height;
    let base = if show_table { detailed_height } else { minimum };
    let mut remaining = area.height.saturating_sub(base);
    let show_slips = reserve(&mut remaining, 1);
    let tile_rows = if reserve(&mut remaining, 1) {
        headlines::ROOMY_ROWS
    } else {
        headlines::COMPACT_ROWS
    };
    let show_active = reserve(&mut remaining, 1);
    let show_focus = reserve(&mut remaining, 1);
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
        let ending = if view.review.ended_early {
            " \u{b7} ended early, saved as unfinished"
        } else {
            ""
        };
        text(
            buffer,
            take(&mut body, 1),
            format!("{} active{ending}", Time(seconds)),
            style(palette, Role::Muted),
        );
    }
    take(&mut body, gap);
    headlines::paint(buffer, take(&mut body, tile_rows), view, palette, native);
    take(&mut body, gap);
    if show_table {
        let detail = body.width >= table_width(true);
        results_header(buffer, take(&mut body, 1), detail, palette);
        if view.summary_reports.is_empty() {
            text(
                buffer,
                take(&mut body, stride),
                "No scored exercises this session.",
                style(palette, Role::Muted),
            );
        } else {
            for result in &view.summary_reports {
                let baseline = view
                    .review
                    .tracks
                    .iter()
                    .find(|b| b.tracks.first().is_some_and(|t| t.track == result.track));
                result_row(
                    buffer,
                    take(&mut body, stride),
                    Row {
                        report: result,
                        baseline,
                        valid_rate: view.summary_rate_valid,
                        detail,
                    },
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
    if show_slips {
        let row = take(&mut body, 1);
        let line = fitting(slips_line(view), row.width);
        text(buffer, row, line, style(palette, Role::Text));
    }
    if show_focus {
        let row = take(&mut body, 1);
        let line = fitting(learning_line(view), row.width);
        text(buffer, row, line, style(palette, Role::Muted));
    }
    take(&mut body, gap);
    action(buffer, take(&mut body, 1), "Save & exit", view, palette);
}

/// The longest variant that fits, else the shortest (which may truncate).
fn fitting(variants: [String; 2], width: u16) -> String {
    let [long, short] = variants;
    if u16::try_from(Span::raw(long.as_str()).width()).is_ok_and(|w| w <= width) {
        long
    } else {
        short
    }
}

/// One observation from recorded first attempts (U-05), never a verdict.
fn slips_line(view: &PreparedView) -> [String; 2] {
    if !view.summary_rate_valid {
        let line = "Measurement interrupted \u{b7} no qualified rates".to_string();
        return [line.clone(), line];
    }
    if view.review.slips.is_empty() {
        let line = "No slips on letter pairs this session.".to_string();
        return [line.clone(), line];
    }
    let slips: Vec<String> = view
        .review
        .slips
        .iter()
        .map(|slip| format!("{} {}/{}", slip.skill, slip.errors, slip.attempts))
        .collect();
    let slips = slips.join(" \u{b7} ");
    [
        format!("Most slips: {slips} \u{2014} future word drills weigh these"),
        format!("Most slips: {slips}"),
    ]
}

fn learning_line(view: &PreparedView) -> [String; 2] {
    let learning = &view.learning;
    let missed = learning.missed_words;
    let plural = if missed == 1 { "" } else { "s" };
    let long = format!("{missed} missed word{plural} scheduled for review");
    let short = format!("{missed} to review");
    if learning.adaptive && !learning.focus.is_empty() {
        let focus = learning.focus.join(" \u{b7} ");
        [
            format!("Targeted today: {focus} \u{b7} {long}"),
            format!("Targeted: {focus} \u{b7} {short}"),
        ]
    } else {
        let keys = learning.observed_keys;
        [
            format!("{keys} keys recorded \u{b7} {long}"),
            format!("{keys} keys \u{b7} {short}"),
        ]
    }
}

fn reserve(remaining: &mut u16, rows: u16) -> bool {
    if let Some(rest) = remaining.checked_sub(rows) {
        *remaining = rest;
        true
    } else {
        false
    }
}

fn table_width(detail: bool) -> u16 {
    let columns = SUMMARY_COLUMN_WIDTHS.iter().fold(0_u16, |sum, width| {
        sum.saturating_add(width.saturating_add(COLUMN_GAP))
    });
    if detail {
        columns.saturating_add(DETAIL_COLUMN_WIDTH)
    } else {
        columns
    }
}

/// Fixed numeric lanes stay aligned across tracks; the table is centred.
fn result_columns(area: Rect, detail: bool) -> ([Rect; 5], Rect) {
    let inset = area
        .width
        .saturating_sub(table_width(detail))
        .checked_div(2)
        .unwrap_or(0);
    let mut x = area.x.saturating_add(inset);
    let columns = SUMMARY_COLUMN_WIDTHS.map(|width| {
        let cell = Rect::new(x, area.y, width, 1).intersection(area);
        x = x.saturating_add(width.saturating_add(COLUMN_GAP));
        cell
    });
    let extra = if detail {
        Rect::new(x, area.y, DETAIL_COLUMN_WIDTH, 1).intersection(area)
    } else {
        Rect::new(x, area.y, 0, 0)
    };
    (columns, extra)
}

fn results_header(buffer: &mut Buffer, area: Rect, detail: bool, palette: Palette) {
    let (columns, extra) = result_columns(area, detail);
    for (cell, label) in columns
        .into_iter()
        .zip(["Track", "Acc", "Pace", "Fixes", "vs last time"])
    {
        Paragraph::new(label)
            .style(style(palette, Role::Muted))
            .render(cell, buffer);
    }
    if detail {
        Paragraph::new("First-pass")
            .style(style(palette, Role::Muted))
            .render(extra, buffer);
    }
}

struct Row<'a> {
    report: &'a TrackProgress,
    baseline: Option<&'a Baseline>,
    valid_rate: bool,
    detail: bool,
}

fn result_row(buffer: &mut Buffer, area: Rect, row: Row<'_>, palette: Palette) {
    let Row {
        report,
        baseline,
        valid_rate,
        detail,
    } = row;
    let ([name, accuracy, pace, fixes, change], extra) = result_columns(area, detail);
    Paragraph::new(track_label(report.track))
        .style(style(palette, Role::Focus))
        .render(name, buffer);
    let current_accuracy = track_accuracy(report).ok().flatten();
    Paragraph::new(current_accuracy.map_or_else(|| "\u{2014}".to_string(), percent_label))
        .style(style(palette, Role::Text))
        .render(accuracy, buffer);
    let unit = if report.track == Track::Numpad {
        "DPM"
    } else {
        "WPM"
    };
    let rate = track_rate(report).filter(|_| valid_rate);
    if let Some(rate) = rate {
        Paragraph::new(format!("{:.1} {unit}", rate.as_f64()))
            .style(style(palette, Role::Text))
            .render(pace, buffer);
    } else {
        Paragraph::new(format!("\u{2014} {unit}"))
            .style(style(palette, Role::Muted))
            .render(pace, buffer);
    }
    Paragraph::new(report.deletions.to_string())
        .style(style(palette, Role::Muted))
        .render(fixes, buffer);
    let earlier = baseline.and_then(|b| b.tracks.first());
    let delta = match earlier {
        // Retyping recent misses is easier than first-pass typing (#60).
        _ if report.track == Track::Review => "not in the headline".to_string(),
        None => "first time".to_string(),
        Some(before) => {
            let pace_change = rate
                .zip(track_rate(before))
                .map(|(now, then)| rate_change(now, then));
            let accuracy_change = current_accuracy
                .zip(track_accuracy(before).ok().flatten())
                .map(|(now, then)| points_change(now, then));
            match (pace_change, accuracy_change) {
                (Some(p), Some(a)) => format!("{p} \u{b7} {a}"),
                (Some(p), None) => p,
                (None, Some(a)) => a,
                (None, None) => "\u{2014}".to_string(),
            }
        }
    };
    Paragraph::new(delta)
        .style(style(palette, Role::Muted))
        .render(change, buffer);
    if detail {
        Paragraph::new(format!(
            "{}/{}",
            report.first_correct, report.first_attempted
        ))
        .style(style(palette, Role::Muted))
        .render(extra, buffer);
    }
}

const fn track_label(track: Track) -> &'static str {
    match track {
        Track::WarmUp => "warm-up",
        Track::Words => "words",
        Track::NumberRow => "number row",
        Track::Numpad => "numpad",
        Track::Prose => "prose",
        Track::Review => "review",
    }
}
