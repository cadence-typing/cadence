//! Compact editor-style chrome, painted from admitted facts only.
//!
//! Width priority: mode, durability, clock, phase, first-pass accuracy,
//! correct throughput, corrections, exercise position. Each segment is atomic;
//! optional numbers are formatted only after the higher-priority groups fit.

use std::fmt::{self, Write};

use cadence_engine::view::{Durability, PreparedView, RateUnit, ScreenView, ViewMode};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Clear, Widget};

use super::practice::ResolvedTextSize;
use crate::theme::{Palette, SemanticRole as Role};

const MICROS_PER_SECOND: u64 = 1_000_000;
const SECONDS_PER_MINUTE: u64 = 60;
const ROW_HEIGHT: u16 = 1;
const CONTEXT_PADDING: u16 = 2;
const SEGMENT_CHROME: u16 = 3;
const SEGMENT_TEXT_OFFSET: u16 = 2;
const HINT_GAP: u16 = 2;
const LABEL_CAPACITY: usize = 96;

pub(super) fn paint(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    palette: Palette,
    text_size: ResolvedTextSize,
) {
    let area = area.intersection(buffer.area);
    if area.is_empty() {
        return;
    }
    Clear.render(area, buffer);
    buffer.set_style(area, style(palette, Role::Text, Role::Panel));
    let (badge, short, role) = match view.mode {
        ViewMode::Ready => (" READY ", "R", Role::Focus),
        ViewMode::Typing => (" TYPE ", "T", Role::Focus),
        ViewMode::Paused => (" PAUSED ", "P", Role::Warning),
        ViewMode::Next => (" NEXT ", "N", Role::Focus),
        ViewMode::Review => (" REVIEW ", "R", Role::Focus),
        ViewMode::Saving => (" SAVING ", "S", Role::Warning),
        ViewMode::SaveError => (" ERROR ", "!", Role::Error),
        ViewMode::Invalid => (" INVALID ", "!", Role::Error),
    };
    // Reversal also distinguishes the badge with the all-default terminal palette.
    let badge_style =
        style(palette, role, Role::Panel).add_modifier(Modifier::BOLD | Modifier::REVERSED);
    let badge = if width(badge) <= area.width {
        badge
    } else {
        short
    };
    buffer.set_stringn(area.x, area.y, badge, usize::from(area.width), badge_style);
    let mut left = area.x.saturating_add(width(badge));
    let mut right = area.right();

    let mut durable = Label::new();
    let durable_role = match &view.durability {
        Durability::Transient => {
            durable.set(format_args!("VOLATILE"));
            Role::Warning
        }
        Durability::Local => {
            durable.set(format_args!("LOCAL"));
            Role::Muted
        }
        Durability::Saving { .. } => {
            durable.set(format_args!("SAVING"));
            Role::Warning
        }
        Durability::Checkpointed { sequence } => {
            durable.set(format_args!("CKPT {}", sequence.get()));
            Role::Focus
        }
        Durability::Failed { .. } => {
            durable.set(format_args!("SAVE ERROR"));
            Role::Error
        }
    };
    right_segment(
        buffer,
        area.y,
        left,
        &mut right,
        durable.as_str(),
        style(palette, durable_role, Role::SelectedBackground).add_modifier(Modifier::BOLD),
        palette,
    );

    let mut clock = Label::new();
    let (seconds, suffix) = match &view.screen {
        ScreenView::Today { total_seconds } => (*total_seconds, "plan"),
        ScreenView::Practice { status, .. } => (status.phase_seconds_remaining, "left"),
        _ => (
            view.active
                .as_us()
                .checked_div(MICROS_PER_SECOND)
                .unwrap_or(0),
            "active",
        ),
    };
    let minutes = seconds.checked_div(SECONDS_PER_MINUTE).unwrap_or(0);
    let remainder = seconds.checked_rem(SECONDS_PER_MINUTE).unwrap_or(0);
    clock.set(format_args!("{minutes}:{remainder:02} {suffix}"));
    right_segment(
        buffer,
        area.y,
        left,
        &mut right,
        clock.as_str(),
        style(palette, Role::Text, Role::Panel),
        palette,
    );

    let context = match &view.screen {
        ScreenView::Today { .. } if view.history.completed_sessions == 0 => "today / baseline",
        ScreenView::Today { .. } => "today",
        ScreenView::Practice { status, .. } => status.phase_label.as_str(),
        ScreenView::PhaseComplete { next, .. } => next.as_deref().unwrap_or("session summary"),
        ScreenView::Summary => "session review",
    };
    let context_width = width(context).saturating_add(CONTEXT_PADDING);
    if context_width <= right.saturating_sub(left) {
        buffer.set_stringn(
            left.saturating_add(1),
            area.y,
            context,
            usize::from(context_width.saturating_sub(CONTEXT_PADDING)),
            style(palette, Role::Text, Role::Panel),
        );
        left = left.saturating_add(context_width);
    }

    if let ScreenView::Practice { status, .. } = &view.screen {
        // Accuracy has priority over speed, regardless of the training track.
        if right.saturating_sub(left) >= 8 {
            let mut accuracy = Label::new();
            if let Some(percent) = status.first_pass_percent {
                accuracy.set(format_args!("FP {percent}%"));
            } else {
                accuracy.set(format_args!("FP --"));
            }
            right_segment(
                buffer,
                area.y,
                left,
                &mut right,
                accuracy.as_str(),
                style(palette, Role::Text, Role::Panel),
                palette,
            );
        }
        if right.saturating_sub(left) >= 9 {
            let mut pace = Label::new();
            let unit = match status.rate_unit {
                RateUnit::WordsPerMinute => "WPM",
                RateUnit::DigitsPerMinute => "digits/min",
            };
            pace.set(format_args!(
                "{} {unit}",
                status.rate.as_deref().unwrap_or("--")
            ));
            right_segment(
                buffer,
                area.y,
                left,
                &mut right,
                pace.as_str(),
                style(palette, Role::Focus, Role::Panel),
                palette,
            );
        }
        if right.saturating_sub(left) >= 8 {
            let mut corrections = Label::new();
            corrections.set(format_args!("fix {}", status.corrections));
            right_segment(
                buffer,
                area.y,
                left,
                &mut right,
                corrections.as_str(),
                style(palette, Role::Muted, Role::Panel),
                palette,
            );
        }
        if right.saturating_sub(left) >= 9 {
            let mut exercise = Label::new();
            exercise.set(format_args!(
                "ex {}/{}",
                status.exercise_index, status.exercise_total
            ));
            right_segment(
                buffer,
                area.y,
                left,
                &mut right,
                exercise.as_str(),
                style(palette, Role::Muted, Role::Panel),
                palette,
            );
        }
    } else if let Durability::Saving {
        last_ack: Some(sequence),
    }
    | Durability::Failed {
        last_ack: Some(sequence),
    } = &view.durability
        && right.saturating_sub(left) >= 9
    {
        let mut acknowledged = Label::new();
        acknowledged.set(format_args!("CKPT {}", sequence.get()));
        right_segment(
            buffer,
            area.y,
            left,
            &mut right,
            acknowledged.as_str(),
            style(palette, Role::Muted, Role::Panel),
            palette,
        );
    }
    if area.height > ROW_HEIGHT {
        hints(
            buffer,
            Rect::new(
                area.x,
                area.y.saturating_add(ROW_HEIGHT),
                area.width,
                ROW_HEIGHT,
            ),
            view,
            palette,
            text_size,
        );
    }
}

fn hints(
    buffer: &mut Buffer,
    area: Rect,
    view: &PreparedView,
    palette: Palette,
    text_size: ResolvedTextSize,
) {
    buffer.set_style(area, style(palette, Role::Muted, Role::Background));
    let (primary, exit) = match view.mode {
        ViewMode::Ready => ("Enter begin", "Ctrl-C quit"),
        ViewMode::Typing => ("Esc pause", "Ctrl-C finish"),
        ViewMode::Paused => ("Esc resume", "Ctrl-C finish"),
        ViewMode::Next => ("Enter next", "Ctrl-C finish"),
        ViewMode::Review => ("Enter exit", ""),
        ViewMode::Saving => ("", ""),
        ViewMode::SaveError => ("r retry", "Ctrl-C x2 exit (tail at risk)"),
        ViewMode::Invalid => ("", "Ctrl-C finish"),
    };
    let mut x = area.x;
    let key_style = style(palette, Role::Text, Role::Background);
    let quiet_style = style(palette, Role::Muted, Role::Background);
    hint(buffer, area, &mut x, primary, key_style);
    hint(buffer, area, &mut x, exit, quiet_style);
    let mut size = Label::new();
    size.set(format_args!(
        "F2 {}>{}",
        view.text_size.label(),
        view.text_size.next().label()
    ));
    if !hint(buffer, area, &mut x, size.as_str(), quiet_style) {
        hint(buffer, area, &mut x, "F2 size", quiet_style);
    }
    if view.mode == ViewMode::Typing
        && let ScreenView::Practice { status, .. } = &view.screen
        && status.enter_submits
    {
        hint(buffer, area, &mut x, "Enter submit", key_style);
    }
    if matches!(view.screen, ScreenView::Practice { .. }) {
        let mut resolved = Label::new();
        resolved.set(format_args!("text {}", text_size.label()));
        hint(buffer, area, &mut x, resolved.as_str(), quiet_style);
    }
}

/// A whole segment fits, or it contributes no cells at all.
fn right_segment(
    buffer: &mut Buffer,
    y: u16,
    left: u16,
    right: &mut u16,
    text: &str,
    text_style: Style,
    palette: Palette,
) -> bool {
    let cells = width(text).saturating_add(SEGMENT_CHROME);
    if cells > right.saturating_sub(left) {
        return false;
    }
    *right = right.saturating_sub(cells);
    buffer.set_style(Rect::new(*right, y, cells, ROW_HEIGHT), text_style);
    buffer.set_string(*right, y, "│", style(palette, Role::Border, Role::Panel));
    buffer.set_stringn(
        right.saturating_add(SEGMENT_TEXT_OFFSET),
        y,
        text,
        usize::from(cells.saturating_sub(SEGMENT_CHROME)),
        text_style,
    );
    true
}

fn hint(buffer: &mut Buffer, area: Rect, x: &mut u16, text: &str, text_style: Style) -> bool {
    if text.is_empty() {
        return true;
    }
    let gap = if *x == area.x { 1 } else { HINT_GAP };
    let cells = width(text).saturating_add(gap);
    if cells > area.right().saturating_sub(*x) {
        return false;
    }
    *x = x.saturating_add(gap);
    buffer.set_stringn(
        *x,
        area.y,
        text,
        usize::from(cells.saturating_sub(gap)),
        text_style,
    );
    *x = x.saturating_add(cells.saturating_sub(gap));
    true
}

fn width(text: &str) -> u16 {
    u16::try_from(Span::raw(text).width()).unwrap_or(u16::MAX)
}

fn style(palette: Palette, foreground: Role, background: Role) -> Style {
    Style::default()
        .fg(palette.color(foreground))
        .bg(palette.color(background))
}

/// Bounded numeric labels need no heap allocation and never outlive this paint.
struct Label {
    bytes: [u8; LABEL_CAPACITY],
    len: usize,
}

impl Label {
    const fn new() -> Self {
        Self {
            bytes: [0; LABEL_CAPACITY],
            len: 0,
        }
    }

    fn set(&mut self, value: fmt::Arguments<'_>) {
        self.len = 0;
        if self.write_fmt(value).is_err() {
            // A failed numeric label is visible, never a truncated false value.
            const OVERFLOW: &[u8] = b"VALUE TOO LONG";
            for (destination, source) in self.bytes.iter_mut().zip(OVERFLOW) {
                *destination = *source;
            }
            self.len = OVERFLOW.len();
        }
    }

    fn as_str(&self) -> &str {
        // Only complete UTF-8 strings enter through fmt::Write.
        std::str::from_utf8(self.bytes.get(..self.len).unwrap_or_default()).unwrap_or_default()
    }
}

impl Write for Label {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
        let destination = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        destination.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}
