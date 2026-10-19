//! Prepared immutable view models (A-01).
//!
//! View preparation is an explicit engine operation. Ratatui paint
//! receives `&PreparedView` and immutable theme data; paint cannot
//! correct scroll state, initiate I/O, alter counters or advance time.
//! These types are deliberately free of terminal-renderer types: spans
//! are typed domain data.

use cadence_core::{CommitSeq, FrameRevision, Micros};

/// U-02 minimum viewport width for scored practice, in terminal cells.
pub const MINIMUM_COLUMNS: u16 = 60;
/// U-02 minimum viewport height for scored practice, in terminal cells.
pub const MINIMUM_ROWS: u16 = 18;

/// Display preference only; never changes targets, difficulty or scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextSize {
    /// Use large cells when the practice viewport can fit them comfortably.
    #[default]
    Auto,
    /// Prefer large cells, falling back rather than hiding unsupported text.
    Large,
    /// Center ordinary terminal text.
    Normal,
}

impl TextSize {
    /// Label for the available display preference.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Large => "LARGE",
            Self::Normal => "NORMAL",
        }
    }

    /// F2 cycles the three display-only choices.
    pub const fn next(self) -> Self {
        match self {
            Self::Auto => Self::Large,
            Self::Large => Self::Normal,
            Self::Normal => Self::Auto,
        }
    }
}

/// Explicit presentation mode; painters must not parse diagnostic text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// Before the session.
    Ready,
    /// Accepting scored input.
    Typing,
    /// Explicitly paused.
    Paused,
    /// A phase ended; the next awaits confirmation.
    Next,
    /// Session summary.
    Review,
    /// Final durable acknowledgement awaited.
    Saving,
    /// Retryable storage failure.
    SaveError,
    /// Scoring stopped because measurement was invalidated.
    Invalid,
}

/// Durable facts, independent of transient input or screen lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    /// No persistence port (isolated fixture).
    Transient,
    /// Local writer ready; no checkpoint acknowledged yet.
    Local,
    /// A checkpoint is in flight; prior watermark remains authoritative.
    Saving {
        /// Last acknowledged sequence, if any.
        last_ack: Option<CommitSeq>,
    },
    /// The last acknowledged watermark, not a claim that every later key is saved.
    Checkpointed {
        /// Last acknowledged sequence.
        sequence: CommitSeq,
    },
    /// Retryable failure; prior watermark remains authoritative.
    Failed {
        /// Last acknowledged sequence, if any.
        last_ack: Option<CommitSeq>,
    },
}

impl Durability {
    /// A terminal outcome is still awaited.
    pub const fn is_saving(self) -> bool {
        matches!(self, Self::Saving { .. })
    }
    /// Saving failed and retry controls are available.
    pub const fn is_failed(self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// A phase's place on the frozen session route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseState {
    /// Not started.
    Upcoming,
    /// Current practice phase.
    Active,
    /// Its deadline has closed the scored slice.
    Complete,
}

/// Bounded presentation of one phase in policy order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseOverview {
    /// Stable human-readable track label.
    pub label: &'static str,
    /// Concise practice purpose, without efficacy claims.
    pub purpose: &'static str,
    /// Frozen allocated seconds.
    pub seconds: u64,
    /// Actual position in the session.
    pub state: PhaseState,
}

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
    /// Counted corrections in the current exercise.
    pub corrections: u64,
}

/// Which screen is prepared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenView {
    /// Today/start destination: the planned session summary.
    Today {
        /// Total allocated seconds, including dismissible review.
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
    Summary,
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
    /// Display-only size preference.
    pub text_size: TextSize,
    /// Reducer-derived input/presentation mode.
    pub mode: ViewMode,
    /// Last durable watermark or outstanding storage condition.
    pub durability: Durability,
    /// The frozen session route (bounded by validated phase inventory).
    pub phases: Vec<PhaseOverview>,
    /// Actual aggregate of the completed track, on an interlude only.
    pub phase_report: Option<cadence_core::progress::TrackProgress>,
    /// Exact per-track aggregates for the summary, never parsed from prose.
    pub summary_reports: Vec<cadence_core::progress::TrackProgress>,
    /// Whether throughput remains interpretable after measurement admission.
    pub summary_rate_valid: bool,
}
