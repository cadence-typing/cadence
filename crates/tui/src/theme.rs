//! Semantic theme roles (U-02).
//!
//! Colours live in the theme adapter, not scattered widget literals. The
//! three shipped palettes are `cadence-dark`, `cadence-light` and
//! `terminal`. A validated custom palette maps each named role to a
//! colour; an invalid theme retains the previous palette and reports one
//! actionable diagnostic.

use ratatui::style::Color;

/// A semantic colouring role. Pairing colour with underline, cursor shape
/// or labels keeps meaning accessible when colours are disabled (U-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticRole {
    /// Panel background.
    Background,
    /// Panel chrome.
    Panel,
    /// Primary readable text.
    Text,
    /// De-emphasised text.
    Muted,
    /// Thin borders.
    Border,
    /// Focused/selected element.
    Focus,
    /// Correctly typed units.
    Correct,
    /// Mistyped units.
    Error,
    /// Error background banding.
    ErrorBackground,
    /// Warnings and uncertainty notes.
    Warning,
    /// Selected row background.
    SelectedBackground,
}

/// A fully resolved palette: every [`SemanticRole`] has a colour. The
/// `terminal` palette defers to terminal defaults and is the accessible
/// fallback usable with colours disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    dark: bool,
    colors: [Color; 11],
}

/// Failures while adopting a user palette (U-02: invalid theme retains the
/// previous palette with one actionable diagnostic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeError {
    /// A named role was missing from the theme file.
    MissingRole {
        /// The absent role name.
        role: String,
    },
    /// A colour value could not be parsed.
    InvalidColor {
        /// The offending value.
        value: String,
        /// Expected colour syntax.
        expected: &'static str,
    },
}

impl Palette {
    /// Canonical role order used by palette construction and validation.
    pub const ROLE_ORDER: [SemanticRole; 11] = [
        SemanticRole::Background,
        SemanticRole::Panel,
        SemanticRole::Text,
        SemanticRole::Muted,
        SemanticRole::Border,
        SemanticRole::Focus,
        SemanticRole::Correct,
        SemanticRole::Error,
        SemanticRole::ErrorBackground,
        SemanticRole::Warning,
        SemanticRole::SelectedBackground,
    ];

    /// The shipped `cadence-dark` palette: neutral readable text with a
    /// restrained mint accent (approved visual direction).
    #[must_use]
    pub const fn cadence_dark() -> Self {
        Self::from_ordered([
            Color::Rgb(10, 13, 16),
            Color::Rgb(16, 20, 24),
            Color::Rgb(214, 220, 224),
            Color::Rgb(124, 134, 142),
            Color::Rgb(58, 66, 74),
            Color::Rgb(133, 208, 180),
            Color::Rgb(133, 208, 180),
            Color::Rgb(237, 135, 128),
            Color::Rgb(64, 32, 32),
            Color::Rgb(224, 178, 106),
            Color::Rgb(40, 52, 58),
        ])
    }

    /// The shipped `cadence-light` palette.
    #[must_use]
    pub const fn cadence_light() -> Self {
        Self::from_ordered([
            Color::Reset,
            Color::Rgb(246, 247, 244),
            Color::Rgb(38, 44, 48),
            Color::Rgb(110, 118, 124),
            Color::Rgb(196, 202, 206),
            Color::Rgb(31, 122, 93),
            Color::Rgb(31, 122, 93),
            Color::Rgb(188, 66, 60),
            Color::Rgb(246, 216, 214),
            Color::Rgb(156, 116, 32),
            Color::Rgb(224, 230, 232),
        ])
    }

    /// The `terminal` palette: terminal defaults everywhere, usable with
    /// colours disabled (accessible high-contrast fallback).
    #[must_use]
    pub const fn terminal() -> Self {
        Self::from_ordered([Color::Reset; 11])
    }

    /// Assembles a palette from colours in [`Palette::ROLE_ORDER`] order.
    /// Internal; public construction goes through named builders or the
    /// validated config path.
    const fn from_ordered(colors: [Color; 11]) -> Self {
        Self { dark: true, colors }
    }

    /// Whether this palette prefers light-on-dark rendering.
    #[must_use]
    pub const fn is_dark(self) -> bool {
        self.dark
    }

    /// The colour bound to a role.
    #[must_use]
    pub const fn color(self, role: SemanticRole) -> Color {
        match role {
            SemanticRole::Background => self.colors[0],
            SemanticRole::Panel => self.colors[1],
            SemanticRole::Text => self.colors[2],
            SemanticRole::Muted => self.colors[3],
            SemanticRole::Border => self.colors[4],
            SemanticRole::Focus => self.colors[5],
            SemanticRole::Correct => self.colors[6],
            SemanticRole::Error => self.colors[7],
            SemanticRole::ErrorBackground => self.colors[8],
            SemanticRole::Warning => self.colors[9],
            SemanticRole::SelectedBackground => self.colors[10],
        }
    }

    /// The canonical role names used in theme files (U-02).
    #[must_use]
    pub const fn role_name(role: SemanticRole) -> &'static str {
        match role {
            SemanticRole::Background => "background",
            SemanticRole::Panel => "panel",
            SemanticRole::Text => "text",
            SemanticRole::Muted => "muted",
            SemanticRole::Border => "border",
            SemanticRole::Focus => "focus",
            SemanticRole::Correct => "correct",
            SemanticRole::Error => "error",
            SemanticRole::ErrorBackground => "error-background",
            SemanticRole::Warning => "warning",
            SemanticRole::SelectedBackground => "selected-background",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_has_a_colour_and_a_name() {
        for role in Palette::ROLE_ORDER {
            let color = Palette::cadence_dark().color(role);
            assert_ne!(
                Palette::cadence_light().color(role),
                color,
                "dark and light must differ for {}",
                Palette::role_name(role)
            );
            assert!(!Palette::role_name(role).is_empty());
        }
    }

    #[test]
    fn terminal_palette_defers_to_defaults() {
        for role in Palette::ROLE_ORDER {
            assert_eq!(Palette::terminal().color(role), Color::Reset);
        }
    }
}
