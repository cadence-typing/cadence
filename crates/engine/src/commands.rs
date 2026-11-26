//! The typed command table (U-01, U-03, #80). Every binding names the keys
//! that trigger it, what it is called and the typed [`Action`] it performs.
//! The reducer resolves keys through this table ([`resolve`]); footer hints,
//! the help overlay and headless `action` validation read the same table, so
//! a hint never names a binding the reducer does not honour in that state,
//! and a command the state does not offer is refused. Typing keeps its own
//! input owner: during practice every printable key is content, Esc pauses
//! and Ctrl-C ends early; Ctrl-C and F2 are global.

use crate::events::{Direction, Key};
use crate::view::ViewMode;

/// What a binding does, independent of how it is labelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start the planned session (Today), or the chosen practice (Train,
    /// Numpad).
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
    /// The next destination (Today, Train, Numpad).
    NextDestination,
    /// The previous destination.
    PreviousDestination,
    /// Open or close the help overlay.
    Help,
    /// Open the settings overlay (#79).
    Settings,
    /// Train: the next track.
    NextTrack,
    /// Train: the previous track.
    PreviousTrack,
    /// Train and Numpad: a longer session.
    Longer,
    /// Train and Numpad: a shorter session.
    Shorter,
}

/// A key a binding answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMatch {
    /// Enter.
    Enter,
    /// Escape.
    Escape,
    /// A printable character.
    Char(char),
    /// Tab.
    Tab,
    /// Shift-Tab.
    BackTab,
    /// An arrow.
    Arrow(Direction),
    /// Ctrl-C: the global quit path, shown but dispatched by its own arm.
    Interrupt,
    /// F2: the global text-size toggle, shown but dispatched by its own arm.
    F2,
}

impl KeyMatch {
    /// Whether `key` is this key. The global keys never match here: they
    /// keep their own dispatch in every state.
    #[must_use]
    pub fn matches(self, key: Key) -> bool {
        match (self, key) {
            (Self::Enter, Key::Enter)
            | (Self::Escape, Key::Escape)
            | (Self::Tab, Key::Tab)
            | (Self::BackTab, Key::BackTab) => true,
            (Self::Char(expected), Key::Content(c)) => c.get() == expected,
            (Self::Arrow(expected), Key::Arrow(direction)) => expected == direction,
            (
                Self::Enter
                | Self::Escape
                | Self::Char(_)
                | Self::Tab
                | Self::BackTab
                | Self::Arrow(_)
                | Self::Interrupt
                | Self::F2,
                _,
            ) => false,
        }
    }

    /// Whether this is a global key (Ctrl-C, F2) dispatched by its own arm.
    #[must_use]
    pub const fn is_global(self) -> bool {
        matches!(self, Self::Interrupt | Self::F2)
    }
}

/// One documented key binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    /// Key names as shown to the learner.
    pub keys: &'static str,
    /// The keys it answers to.
    pub matches: &'static [KeyMatch],
    /// Short description of the effect.
    pub label: &'static str,
    /// The typed action.
    pub action: Action,
}

const fn bind(
    keys: &'static str,
    matches: &'static [KeyMatch],
    label: &'static str,
    action: Action,
) -> Binding {
    Binding {
        keys,
        matches,
        label,
        action,
    }
}

/// The destinations outside practice (#80).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Destination {
    /// The guided daily session.
    #[default]
    Today,
    /// Single-track practice with a chosen length.
    Train,
    /// Numpad-only practice with a chosen length.
    Numpad,
    /// Comparable trends with counts (#81).
    Progress,
}

impl Destination {
    /// Every destination, in tab order.
    pub const ALL: [Self; 4] = [Self::Today, Self::Train, Self::Numpad, Self::Progress];

    /// Its tab label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Train => "Train",
            Self::Numpad => "Numpad",
            Self::Progress => "Progress",
        }
    }
}

const TEXT_SIZE: Binding = bind("F2", &[KeyMatch::F2], "text size", Action::TextSize);
const HELP: Binding = bind("?", &[KeyMatch::Char('?')], "help", Action::Help);
const SETTINGS: Binding = bind("s", &[KeyMatch::Char('s')], "settings", Action::Settings);
const QUIT: Binding = bind("q", &[KeyMatch::Char('q')], "quit", Action::Quit);
const NEXT_DESTINATION: Binding = bind(
    "Tab / →",
    &[KeyMatch::Tab, KeyMatch::Arrow(Direction::Right)],
    "next screen",
    Action::NextDestination,
);
const PREVIOUS_DESTINATION: Binding = bind(
    "Shift-Tab / ←",
    &[KeyMatch::BackTab, KeyMatch::Arrow(Direction::Left)],
    "previous screen",
    Action::PreviousDestination,
);
const LONGER: Binding = bind(
    "+",
    &[KeyMatch::Char('+'), KeyMatch::Char('=')],
    "longer",
    Action::Longer,
);
const SHORTER: Binding = bind("-", &[KeyMatch::Char('-')], "shorter", Action::Shorter);

const READY_TODAY: &[Binding] = &[
    bind("Enter", &[KeyMatch::Enter], "begin", Action::Begin),
    QUIT,
    HELP,
    NEXT_DESTINATION,
    PREVIOUS_DESTINATION,
    SETTINGS,
    TEXT_SIZE,
];
const READY_TRAIN: &[Binding] = &[
    bind("Enter", &[KeyMatch::Enter], "start", Action::Begin),
    bind(
        "↓",
        &[KeyMatch::Arrow(Direction::Down)],
        "next track",
        Action::NextTrack,
    ),
    bind(
        "↑",
        &[KeyMatch::Arrow(Direction::Up)],
        "previous track",
        Action::PreviousTrack,
    ),
    LONGER,
    SHORTER,
    QUIT,
    HELP,
    NEXT_DESTINATION,
    PREVIOUS_DESTINATION,
    SETTINGS,
    TEXT_SIZE,
];
const READY_PROGRESS: &[Binding] = &[
    QUIT,
    HELP,
    NEXT_DESTINATION,
    PREVIOUS_DESTINATION,
    SETTINGS,
    TEXT_SIZE,
];
const READY_NUMPAD: &[Binding] = &[
    bind("Enter", &[KeyMatch::Enter], "start", Action::Begin),
    LONGER,
    SHORTER,
    QUIT,
    HELP,
    NEXT_DESTINATION,
    PREVIOUS_DESTINATION,
    SETTINGS,
    TEXT_SIZE,
];
const TYPING: &[Binding] = &[
    bind("Esc", &[KeyMatch::Escape], "pause", Action::Pause),
    bind(
        "Ctrl-C",
        &[KeyMatch::Interrupt],
        "end & review",
        Action::EndAndReview,
    ),
    TEXT_SIZE,
];
const TYPING_NUMPAD: &[Binding] = &[
    bind("Enter", &[KeyMatch::Enter], "submit", Action::SubmitEntry),
    bind("Esc", &[KeyMatch::Escape], "pause", Action::Pause),
    bind(
        "Ctrl-C",
        &[KeyMatch::Interrupt],
        "end & review",
        Action::EndAndReview,
    ),
    TEXT_SIZE,
];
const PAUSED: &[Binding] = &[
    bind(
        "Esc / Enter",
        &[KeyMatch::Escape, KeyMatch::Enter],
        "resume",
        Action::Resume,
    ),
    bind(
        "e",
        &[KeyMatch::Char('e')],
        "end session & review",
        Action::EndAndReview,
    ),
    SETTINGS,
    TEXT_SIZE,
];
/// A slow save's pause (#51) resumes by itself; Esc does not resume it.
const STALLED: &[Binding] = &[
    bind(
        "e",
        &[KeyMatch::Char('e')],
        "end session & review",
        Action::EndAndReview,
    ),
    TEXT_SIZE,
];
const NEXT: &[Binding] = &[
    bind(
        "Enter / Esc",
        &[KeyMatch::Enter, KeyMatch::Escape],
        "continue",
        Action::Continue,
    ),
    bind(
        "Ctrl-C",
        &[KeyMatch::Interrupt],
        "end & review",
        Action::EndAndReview,
    ),
];
const REVIEW: &[Binding] = &[bind(
    "Enter / Esc",
    &[KeyMatch::Enter, KeyMatch::Escape],
    "save & exit",
    Action::SaveAndExit,
)];
const SAVE_ERROR: &[Binding] = &[
    bind("r", &[KeyMatch::Char('r')], "retry save", Action::RetrySave),
    bind(
        "Ctrl-C x2",
        &[KeyMatch::Interrupt],
        "exit (tail at risk)",
        Action::ForceExit,
    ),
];
const INVALID: &[Binding] = &[bind(
    "Ctrl-C",
    &[KeyMatch::Interrupt],
    "finish with partial evidence",
    Action::FinishInvalid,
)];
const HELP_OPEN: &[Binding] = &[bind(
    "Esc / ?",
    &[KeyMatch::Escape, KeyMatch::Char('?')],
    "close help",
    Action::Help,
)];

/// Bindings honoured while paused for `reason`: a slow save's pause offers
/// no resume, because it ends when the save catches up.
#[must_use]
pub const fn paused_bindings(reason: Option<crate::view::PauseReason>) -> &'static [Binding] {
    match reason {
        Some(crate::view::PauseReason::SavingSlow) => STALLED,
        Some(
            crate::view::PauseReason::Requested
            | crate::view::PauseReason::FocusLost
            | crate::view::PauseReason::PasteRejected
            | crate::view::PauseReason::TooSmall,
        )
        | None => PAUSED,
    }
}

/// The state the table is chosen by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandState {
    /// The presentation mode.
    pub mode: ViewMode,
    /// Enter submits a keypad entry.
    pub enter_submits: bool,
    /// Why practice is paused.
    pub pause_reason: Option<crate::view::PauseReason>,
    /// The selected destination (start screen).
    pub destination: Destination,
    /// The help overlay is open.
    pub help: bool,
}

/// Bindings honoured in `state`, most important first.
#[must_use]
pub const fn bindings_for(state: CommandState) -> &'static [Binding] {
    if state.help {
        return HELP_OPEN;
    }
    match state.mode {
        ViewMode::Ready => match state.destination {
            Destination::Today => READY_TODAY,
            Destination::Train => READY_TRAIN,
            Destination::Numpad => READY_NUMPAD,
            Destination::Progress => READY_PROGRESS,
        },
        ViewMode::Typing if state.enter_submits => TYPING_NUMPAD,
        ViewMode::Typing => TYPING,
        ViewMode::Paused => paused_bindings(state.pause_reason),
        ViewMode::Next => NEXT,
        ViewMode::Review => REVIEW,
        ViewMode::Saving => &[],
        ViewMode::SaveError => SAVE_ERROR,
        ViewMode::Invalid => INVALID,
    }
}

/// The action `key` triggers among `bindings`, if any. Global keys never
/// resolve here.
#[must_use]
pub fn resolve(bindings: &[Binding], key: Key) -> Option<Action> {
    bindings
        .iter()
        .find(|binding| {
            binding
                .matches
                .iter()
                .any(|m| !m.is_global() && m.matches(key))
        })
        .map(|binding| binding.action)
}

#[cfg(test)]
mod stall_tests {
    use super::*;
    use crate::view::PauseReason;

    /// #51: a slow save's pause offers no resume; other pauses do.
    #[test]
    fn a_stall_offers_no_resume() {
        let stalled = paused_bindings(Some(PauseReason::SavingSlow));
        assert!(stalled.iter().all(|b| b.action != Action::Resume));
        assert!(stalled.iter().any(|b| b.action == Action::EndAndReview));
        let requested = paused_bindings(Some(PauseReason::Requested));
        assert!(requested.iter().any(|b| b.action == Action::Resume));
    }
}
