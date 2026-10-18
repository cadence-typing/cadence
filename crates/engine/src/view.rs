//! Prepared immutable view models (A-01).
//!
//! View preparation is an explicit engine operation. Ratatui paint
//! receives `&PreparedView` and immutable theme data; paint cannot
//! correct scroll state, initiate I/O, alter counters or advance time.
//! These types are deliberately free of terminal-renderer types: spans
//! are typed domain data.

use cadence_core::{FrameRevision, Micros};

/// One visible training unit with its presentation meaning (U-01).
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

/// Conventional word throughput and numeric digit throughput are not comparable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RateUnit {
    /// Correct visible units divided by five, per minute.
    #[default]
    WordsPerMinute,
    /// Correct matching digits per minute; punctuation is excluded.
    DigitsPerMinute,
}

impl RateUnit {
    /// Honest display label for the current track.
    pub const fn label(self) -> &'static str {
        match self {
            Self::WordsPerMinute => "Correct WPM",
            Self::DigitsPerMinute => "Correct digits/min",
        }
    }
}

/// Status figures shown during practice; exact ratios are formatted only
/// at this edge (L-02).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusFigures {
    /// Live correct throughput; absent until measurable.
    pub rate: Option<String>,
    /// Units for the throughput figure.
    pub rate_unit: RateUnit,
    /// First-pass accuracy percentage, truncated for display only.
    pub first_pass_percent: Option<u16>,
    /// Active seconds so far in the whole session.
    pub active_seconds: u64,
    /// Whether the current measurement is paused.
    pub paused: bool,
    /// The current phase label (U-01: answer what am I practising).
    pub phase_label: String,
    /// Exercise index within the phase (one-based).
    pub exercise_index: u32,
    /// Exercises planned in this phase.
    pub exercise_total: u32,
    /// Seconds remaining in the phase (active-time basis).
    pub phase_seconds_remaining: u64,
    /// Whether Enter submits (numpad phase).
    pub enter_submits: bool,
}

/// Which screen is prepared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenView {
    /// Today/start destination: the planned session summary.
    Today {
        /// Short reasons for today's work (phase seconds + purpose).
        plan_hint: String,
        /// Total planned active seconds.
        total_seconds: u64,
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
    /// A phase reached its exact active deadline; offer the next phase.
    PhaseComplete {
        /// The label of the finished phase.
        label: String,
        /// Whether the last exercise was left partial at the boundary.
        partial: bool,
        /// The label of the next phase, when one remains.
        next: Option<String>,
    },
    /// End-of-session summary: honest aggregate figures per track.
    Summary {
        /// Human summary lines.
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
    /// Session active-time figure included in this preparation.
    pub active: Micros,
    /// Visible timing, durability or interruption notice.
    pub notice: Option<String>,
    /// Previously durable history (no mid-word restore implication).
    pub history: cadence_core::progress::HistorySummary,
    /// An exact checkpoint is awaiting acknowledgement.
    pub saving: bool,
    /// Saving failed and paused retry controls are available.
    pub save_failed: bool,
}
