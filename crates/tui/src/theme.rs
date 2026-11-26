//! Semantic theme roles (U-02).
//!
//! Colours remain in this adapter: no other TUI module names a colour
//! (tested). Error meaning also has underline and inversion. Palettes are
//! built in, or come whole from a validated theme file (#82); high contrast
//! uses the terminal's own colours and pairs every colour signal with a
//! modifier, so meaning survives with colours disabled.

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
    colors: [Color; 11],
    high_contrast: bool,
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

    /// A built-in palette by its `display.theme` name (#79); theme files
    /// are read by the app.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "dark" => Some(Self::cadence_dark()),
            "light" => Some(Self::cadence_light()),
            "terminal" => Some(Self::terminal()),
            _ => None,
        }
    }

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
        Self {
            colors,
            high_contrast: false,
        }
    }

    /// A validated theme file's palette (#82).
    #[must_use]
    pub fn from_theme(colours: &cadence_core::settings::ThemeColors) -> Self {
        let mut colors = [Color::Reset; 11];
        for (slot, colour) in colors.iter_mut().zip(colours) {
            *slot = match *colour {
                cadence_core::settings::ThemeColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
                cadence_core::settings::ThemeColor::Default => Color::Reset,
            };
        }
        Self::from_ordered(colors)
    }

    /// The palette the display settings ask for: high contrast, a theme
    /// file, a built-in name, or `fallback` (the palette chosen at startup).
    #[must_use]
    pub fn for_display(display: &cadence_engine::view::DisplayView, fallback: Self) -> Self {
        if display.high_contrast {
            return Self {
                high_contrast: true,
                ..Self::terminal()
            };
        }
        display
            .theme_colors
            .as_ref()
            .map(Self::from_theme)
            .or_else(|| Self::named(&display.theme))
            .unwrap_or(fallback)
    }

    /// High contrast: every colour signal is also a modifier (#82).
    #[must_use]
    pub const fn high_contrast(self) -> bool {
        self.high_contrast
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
