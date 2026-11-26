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

/// Runtime terminal capability, independent of the requested display size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextSupport {
    /// Use ordinary terminal text until native support has been established.
    #[default]
    NormalOnly,
    /// The terminal supports native double-height, double-width line pairs.
    DoubleHeight,
}

/// Derived native rendition for one physical terminal row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineRendition {
    /// Ordinary single-width, single-height text.
    #[default]
    Normal,
    /// Top half of a native double-height, double-width line.
    DoubleHeightTop,
    /// Bottom half of a native double-height, double-width line.
    DoubleHeightBottom,
}

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
    /// Concise practice purpose naming what it targets (#50), without
    /// efficacy claims.
    pub purpose: String,
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
    /// Typed past the end of the target; always an error to delete.
    Extra {
        /// What was typed.
        typed: char,
    },
}

impl UnitSpan {
    /// The unit this span should render as a cell.
    #[must_use]
    pub const fn cell_char(self) -> char {
        match self {
            Self::Correct { unit } => unit,
            Self::Wrong { typed, .. } | Self::Extra { typed } => typed,
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
    /// Correct throughput of this track's completed exercises, as an integer;
    /// updated at exercise boundaries, not per keystroke (P-05).
    pub rate: Option<String>,
    /// Units for the throughput figure.
    pub rate_unit: RateUnit,
    /// First-pass accuracy of this track's completed exercises, truncated.
    pub first_pass_percent: Option<u16>,
    /// Active seconds so far in the whole session.
    pub active_seconds: u64,
    /// Whether the current measurement is paused.
    pub paused: bool,
    /// The current phase label (U-01: answer what am I practising).
    pub phase_label: String,
    /// One-based position of this phase among the session's typing phases.
    pub phase_position: u32,
    /// Typing phases in the session.
    pub phase_count: u32,
    /// Seconds remaining in the phase (active-time basis).
    pub phase_seconds_remaining: u64,
    /// Whether Enter submits (numpad phase).
    pub enter_submits: bool,
    /// Counted corrections in the current exercise.
    pub corrections: u64,
    /// What this target is for: prose attribution, or the focus skills and
    /// review slots of a word exercise.
    pub context: Option<String>,
}

/// What the plan adapts to and what this session recorded (plan 0003).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LearningView {
    /// Whether the plan used recorded evidence.
    pub adaptive: bool,
    /// Targets per focus group whose phase is in the plan (#50), in
    /// letters, number row, numpad order.
    pub targets: Vec<TargetGroup>,
    /// Each typing track's pace goal for this session (#62).
    pub goals: Vec<cadence_core::learning::PaceRecord>,
    /// Today's session includes a transfer probe (#65).
    pub probe_today: bool,
    /// Focus skills as typed text, strongest first.
    pub focus: Vec<String>,
    /// Due spaced-review words planned this session.
    pub due_reviews: usize,
    /// Missed words re-served later this session.
    pub reinforced: u64,
    /// Distinct planned words completed with any error this session.
    pub missed_words: usize,
    /// First attempts recorded for the learner model this session.
    pub observed_keys: u64,
}

/// What one focus group's phase targets (#50), from the plan record.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TargetGroup {
    /// "letters", "number row" or "numpad".
    pub name: &'static str,
    /// Focus skills as typed text, strongest first; empty when the plan has
    /// nothing to target in this group.
    pub skills: Vec<String>,
    /// The group's track was typed this session.
    pub practised: bool,
}

/// Why practice is paused; the controls explain it in plain words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    /// Esc.
    Requested,
    /// The terminal window lost focus.
    FocusLost,
    /// A paste was refused; nothing was inserted.
    PasteRejected,
    /// The terminal became smaller than the practice minimum.
    TooSmall,
    /// A checkpoint acknowledgement is overdue (#51): the clock is stopped
    /// until the save catches up.
    SavingSlow,
}

/// An earlier session (or one of its tracks) used for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    /// Local days between that session and today.
    pub days_ago: i64,
    /// Whether that session finished its plan.
    pub completed: bool,
    /// Its exact totals.
    pub tracks: Vec<cadence_core::progress::TrackProgress>,
}

impl Baseline {
    /// When, in words.
    #[must_use]
    pub fn when(&self) -> String {
        match self.days_ago {
            0 => "earlier today".to_string(),
            1 => "yesterday".to_string(),
            days => format!("{days} days ago"),
        }
    }
}

/// A skill that slipped this session, from recorded first attempts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slip {
    /// The skill as typed text, with visible space.
    pub skill: String,
    /// Wrong first attempts.
    pub errors: u64,
    /// All first attempts.
    pub attempts: u64,
}

/// Review-only context: how this session compares and what slipped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewContext {
    /// The session stopped before its plan finished.
    pub ended_early: bool,
    /// Most recent earlier session with exactly this track set.
    pub session: Option<Baseline>,
    /// Per track, the most recent earlier session that had it
    /// (one track each).
    pub tracks: Vec<Baseline>,
    /// Most frequent slips, worst first.
    pub slips: Vec<Slip>,
    /// This session's transfer probe, if it had one (#65).
    pub probe: Option<ProbeView>,
}

/// A transfer probe's result line (#65).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeView {
    /// The probe's own totals.
    pub report: cadence_core::progress::TrackProgress,
    /// Why it is not comparable, if it is not ("paused", "ended early", …).
    pub not_comparable: Option<&'static str>,
    /// The latest earlier comparable probe, when this one is comparable.
    pub previous: Option<cadence_core::progress::TrackProgress>,
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
    /// Reducer-admitted terminal capability; normal text before runtime discovery.
    pub text_support: TextSupport,
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
    /// Plan adaptation and recorded evidence.
    pub learning: LearningView,
    /// Why practice is paused, when it is.
    pub pause_reason: Option<PauseReason>,
    /// The most recent session with typing, for the start screen.
    pub last_session: Option<Baseline>,
    /// Comparison and slips, on the review only.
    pub review: ReviewContext,
}
