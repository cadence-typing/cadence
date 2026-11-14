//! Typed key bindings per presentation mode (U-03). The reducer's dispatch in
//! `lib.rs`/`practice.rs` and this table are reviewed together; footer hints
//! and the pause controls are rendered only from here, so a hint never names
//! a binding the reducer does not honour in that mode.

use crate::view::ViewMode;

/// What a binding does, independent of how it is labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start the planned session.
    Begin,
    /// Leave before starting; nothing is recorded.
    Quit,
    /// Pause measurement and open the practice controls.
    Pause,
    /// Resume measurement.
    Resume,
    /// Stop practising, keep the partial evidence and show the review.
    EndAndReview,
    /// Open the next phase.
    Continue,
    /// Save and exit after the review.
    SaveAndExit,
    /// Retry a failed durable save.
    RetrySave,
    /// Exit although the latest tail was not acknowledged.
    ForceExit,
    /// Finish after measurement was invalidated, keeping partial evidence.
    FinishInvalid,
    /// Submit a complete numeric keypad entry.
    SubmitEntry,
    /// Cycle the display-only text size.
    TextSize,
}

/// One documented key binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    /// Key names as shown to the learner.
    pub keys: &'static str,
    /// Short description of the effect.
    pub label: &'static str,
    /// The typed action.
    pub action: Action,
}

const fn bind(keys: &'static str, label: &'static str, action: Action) -> Binding {
    Binding {
        keys,
        label,
        action,
    }
}

const READY: &[Binding] = &[
    bind("Enter", "begin", Action::Begin),
    bind("q", "quit", Action::Quit),
    bind("F2", "text size", Action::TextSize),
];
const TYPING: &[Binding] = &[
    bind("Esc", "pause", Action::Pause),
    bind("Ctrl-C", "end & review", Action::EndAndReview),
    bind("F2", "text size", Action::TextSize),
];
const TYPING_NUMPAD: &[Binding] = &[
    bind("Enter", "submit", Action::SubmitEntry),
    bind("Esc", "pause", Action::Pause),
    bind("Ctrl-C", "end & review", Action::EndAndReview),
    bind("F2", "text size", Action::TextSize),
];
const PAUSED: &[Binding] = &[
    bind("Esc / Enter", "resume", Action::Resume),
    bind("e", "end session & review", Action::EndAndReview),
    bind("F2", "text size", Action::TextSize),
];
const NEXT: &[Binding] = &[
    bind("Enter", "continue", Action::Continue),
    bind("Ctrl-C", "end & review", Action::EndAndReview),
];
const REVIEW: &[Binding] = &[bind("Enter", "save & exit", Action::SaveAndExit)];
const SAVE_ERROR: &[Binding] = &[
    bind("r", "retry save", Action::RetrySave),
    bind("Ctrl-C x2", "exit (tail at risk)", Action::ForceExit),
];
const INVALID: &[Binding] = &[bind(
    "Ctrl-C",
    "finish with partial evidence",
    Action::FinishInvalid,
)];

/// Bindings honoured in `mode`, most important first.
#[must_use]
pub const fn bindings(mode: ViewMode, enter_submits: bool) -> &'static [Binding] {
    match mode {
        ViewMode::Ready => READY,
        ViewMode::Typing if enter_submits => TYPING_NUMPAD,
        ViewMode::Typing => TYPING,
        ViewMode::Paused => PAUSED,
        ViewMode::Next => NEXT,
        ViewMode::Review => REVIEW,
        ViewMode::Saving => &[],
        ViewMode::SaveError => SAVE_ERROR,
        ViewMode::Invalid => INVALID,
    }
}
