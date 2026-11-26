//! When a changed setting takes effect (U-04, #79). Display settings apply
//! at once; anything that changes what is practised or how it is scored
//! applies from the next session, so a running session is never re-planned
//! or re-scored.

/// Every setting of `config.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `session.minutes`.
    Minutes,
    /// `session.tracks`.
    Tracks,
    /// `session.emphasis`.
    Emphasis,
    /// `practice.accuracy_target`.
    AccuracyTarget,
    /// `practice.live_pace` (a display of the pace while typing).
    LivePace,
    /// `content.breadth`.
    Breadth,
    /// `content.packs`.
    Packs,
    /// `content.numpad_operators` (#83).
    NumpadOperators,
    /// `keyboard.layout`.
    Layout,
    /// `keyboard.numpad`.
    Numpad,
    /// `keyboard.decimal_separator`.
    DecimalSeparator,
    /// `display.theme`.
    Theme,
    /// `display.text_size`.
    TextSize,
    /// `display.key_guide`.
    KeyGuide,
    /// `display.high_contrast`.
    HighContrast,
}

/// When a change applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Repainted immediately.
    Now,
    /// From the next session.
    NextSession,
}

/// When a change to `kind` takes effect.
#[must_use]
pub fn takes_effect(kind: Kind) -> Effect {
    match kind {
        Kind::LivePace | Kind::Theme | Kind::TextSize | Kind::KeyGuide | Kind::HighContrast => {
            Effect::Now
        }
        Kind::Minutes
        | Kind::Tracks
        | Kind::Emphasis
        | Kind::AccuracyTarget
        | Kind::Breadth
        | Kind::Packs
        | Kind::NumpadOperators
        | Kind::Layout
        | Kind::Numpad
        | Kind::DecimalSeparator => Effect::NextSession,
    }
}
