//! The Progress destination (U-05, #81): per track, the series of
//! comparable sessions with its exact counts, or why there is none; the
//! transfer probes the same way; due reviews and the current focus. No
//! composite score and no streaks.

use cadence_core::progress::Track;
use cadence_engine::PreparedView;
use cadence_engine::progress::{ProgressView, Series};
use cadence_engine::summary::percent_label;
use cadence_engine::view::RateUnit;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};

use crate::theme::{Palette, SemanticRole as Role};

/// The track label column.
const LABEL_COLUMNS: usize = 16;

fn style(palette: Palette, role: Role) -> Style {
    Style::default()
        .fg(palette.color(role))
        .bg(palette.color(Role::Background))
}

const fn label(track: Track) -> &'static str {
    match track {
        Track::WarmUp => "warm-up",
        Track::Words => "words",
        Track::NumberRow => "number row",
        Track::Numpad => "numpad",
        Track::Prose => "prose",
        Track::Review => "review",
        Track::Benchmark => "transfer probe",
    }
}

const fn unit(unit: RateUnit) -> &'static str {
    match unit {
        RateUnit::WordsPerMinute => "WPM",
        RateUnit::DigitsPerMinute => "digits/min",
    }
}

fn series_line(series: &Series, palette: Palette) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{:<LABEL_COLUMNS$}", label(series.track)),
        style(palette, Role::Text).add_modifier(Modifier::BOLD),
    )];
    let noun = if series.track == Track::Benchmark {
        "probes"
    } else {
        "sessions"
    };
    if series.enough {
        let accuracy = series
            .accuracy
            .map_or_else(|| "—".to_string(), percent_label);
        let speed = series.speed.map_or_else(String::new, |rate| {
            format!(" · {:.1} {}", rate.as_f64(), unit(series.unit))
        });
        spans.push(Span::styled(
            format!(
                "{accuracy} first pass ({} / {}){speed} · {} comparable {noun}",
                series.first_correct, series.first_attempted, series.sessions
            ),
            style(palette, Role::Text),
        ));
    } else {
        spans.push(Span::styled(
            format!(
                "not enough comparable {noun} ({} of {})",
                series.sessions,
                cadence_engine::progress::MINIMUM_COMPARABLE
            ),
            style(palette, Role::Muted),
        ));
    }
    if let Some(stopped) = series.stopped {
        spans.push(Span::styled(
            format!(" · {}", stopped.note()),
            style(palette, Role::Warning),
        ));
    }
    Line::from(spans)
}

/// Paints Progress into `area`.
pub fn paint(buffer: &mut Buffer, area: Rect, view: &PreparedView, palette: Palette) {
    let mut lines = vec![
        Line::styled(
            "Progress",
            style(palette, Role::Text).add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            "Only sessions on the same layout and content are compared.",
            style(palette, Role::Muted),
        ),
        Line::default(),
    ];
    match &view.progress {
        None | Some(ProgressView::Loading) => {
            lines.push(Line::styled(
                "Reading your history…",
                style(palette, Role::Muted),
            ));
        }
        Some(ProgressView::Failed(reason)) => {
            lines.push(Line::styled(
                format!("History could not be read: {reason}"),
                style(palette, Role::Error),
            ));
        }
        Some(ProgressView::Ready { tracks, benchmark }) => {
            if tracks.is_empty() && benchmark.is_none() {
                lines.push(Line::styled(
                    "No sessions yet: practise once and your trends start here.",
                    style(palette, Role::Muted),
                ));
            }
            lines.extend(tracks.iter().map(|s| series_line(s, palette)));
            if let Some(benchmark) = benchmark {
                lines.push(Line::default());
                lines.push(series_line(benchmark, palette));
            }
        }
    }
    lines.push(Line::default());
    lines.push(Line::styled(
        format!("Due reviews {}", view.learning.due_reviews),
        style(palette, Role::Text),
    ));
    for group in &view.learning.targets {
        if !group.skills.is_empty() {
            lines.push(Line::styled(
                format!("Focus · {}: {}", group.name, group.skills.join(", ")),
                style(palette, Role::Muted),
            ));
        }
    }
    // Lines wrap at narrow widths rather than lose their counts.
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .render(area, buffer);
}
