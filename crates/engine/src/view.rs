//! Prepared immutable view models (A-01).
//!
//! View preparation is an explicit engine operation. Ratatui paint
//! receives `&PreparedView` and immutable theme data; paint cannot
//! correct scroll state, initiate I/O, alter counters or advance time.
//! These types are deliberately free of terminal-renderer types: spans
//! are typed domain data.

use cadence_core::{FrameRevision, Micros};

/// One visible training unit with its presentation meaning (U-01):
/// correct typed, wrong typed (with the expected target available),
/// cursor position, or upcoming text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitSpan {
    /// Correctly typed unit at this position.
    Correct {
        /// The unit as typed (equals the target unit).
        unit: char,
    },
    /// Mistyped unit: the typed input remains highlighted with the
    /// expected target available.
    Wrong {
        /// What was typed.
        typed: char,
        /// What was expected.
        expected: char,
    },
    /// The next expected position (cursor).
    Cursor {
        /// The expected unit.
        expected: char,
    },
    /// Upcoming, not yet reached.
    Upcoming {
        /// The expected unit.
        expected: char,
    },
}

impl UnitSpan {
    /// The unit this span should render as a cell.
    #[must_use]
    pub const fn cell_char(self) -> char {
        match self {
            Self::Correct { unit } => unit,
            Self::Wrong { typed, .. } => typed,
            Self::Cursor { expected } | Self::Upcoming { expected } => expected,
        }
    }
}

/// Status figures shown during practice; exact ratios are formatted only
/// at this edge (L-02).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusFigures {
    /// Live correct WPM, formatted once; absent until measurable.
    pub wpm: Option<String>,
    /// First-pass accuracy percentage, truncated for display only.
    pub first_pass_percent: Option<u16>,
    /// Active seconds so far.
    pub active_seconds: u64,
    /// Whether the current measurement is paused.
    pub paused: bool,
}

/// Which screen is prepared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenView {
    /// Today/start destination (P1 slice: one drill offer).
    Today {
        /// The short reason shown for today's work.
        plan_hint: String,
    },
    /// The quiet practice screen.
    Practice {
        /// Spans of the inline target region.
        spans: Vec<UnitSpan>,
        /// Live status figures.
        status: StatusFigures,
        /// Whether practice is paused (overlay).
        paused: bool,
    },
    /// Result of the last exercise.
    Result {
        /// Completed or partial.
        completed: bool,
        /// Human summary lines (one observation, numbers).
        lines: Vec<String>,
    },
}

/// An immutable prepared view snapshot. The frame revision advances with
/// every preparation; a stale generation never paints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedView {
    /// Revision of this prepared lineage.
    pub revision: FrameRevision,
    /// The screen content.
    pub screen: ScreenView,
    /// Active-time figure included in this preparation (for honest
    /// elapsed/active labelling, P-02).
    pub active: Micros,
}
