//! The settings catalogue (U-04, #76, #79): every `config.toml` key, what it
//! accepts, its default and the kernel `Kind` that decides when a change to
//! it takes effect (`cadence_kernel::settings::takes_effect`, proved: a
//! score-affecting setting never applies now). Parsing the file is
//! `cadence_data::config`.

use core::fmt;

pub use cadence_kernel::settings::{Effect, Kind, takes_effect};

/// What a key accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accepts {
    /// One of these integers.
    IntegerIn(&'static [i64]),
    /// An integer in this inclusive range.
    IntegerRange(i64, i64),
    /// `true` or `false`.
    Boolean,
    /// One of these strings.
    OneOf(&'static [&'static str]),
    /// A non-empty list of distinct strings from this set.
    SubsetOf(&'static [&'static str]),
    /// A non-empty list of distinct pack names (`[a-z0-9-]`, 1 to 64).
    PackNames,
    /// A built-in theme, or a theme file name in the config directory
    /// (`*.toml`, no path separators).
    Theme,
}

impl fmt::Display for Accepts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IntegerIn(values) => {
                let list: Vec<String> = values.iter().map(ToString::to_string).collect();
                write!(f, "one of {}", list.join(", "))
            }
            Self::IntegerRange(low, high) => write!(f, "an integer from {low} to {high}"),
            Self::Boolean => write!(f, "true or false"),
            Self::OneOf(values) => write!(f, "one of {}", quoted(values)),
            Self::SubsetOf(values) => {
                write!(f, "a list of distinct values from {}", quoted(values))
            }
            Self::PackNames => write!(f, "a list of distinct pack names (a-z, 0-9, -)"),
            Self::Theme => write!(
                f,
                "dark, light, terminal or a theme file name ending in .toml"
            ),
        }
    }
}

fn quoted(values: &[&str]) -> String {
    values
        .iter()
        .map(|v| format!("\"{v}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// One key: its dotted name, what it accepts and its default.
#[derive(Debug, Clone, Copy)]
pub struct Key {
    /// `table.key`.
    pub name: &'static str,
    /// Accepted values.
    pub accepts: Accepts,
    /// The default.
    pub default: DefaultValue,
}

/// A key's built-in default, as a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultValue {
    /// An integer.
    Integer(i64),
    /// A boolean.
    Boolean(bool),
    /// A string.
    Text(&'static str),
    /// A list of strings.
    List(&'static [&'static str]),
}

impl DefaultValue {
    /// The default as a setting.
    #[must_use]
    pub fn setting(self) -> Setting {
        match self {
            Self::Integer(n) => Setting::Integer(n),
            Self::Boolean(b) => Setting::Boolean(b),
            Self::Text(s) => Setting::Text(s.to_string()),
            Self::List(items) => Setting::List(items.iter().map(|s| (*s).to_string()).collect()),
        }
    }
}

/// Tracks a session may include.
pub const TRACKS: &[&str] = &["warmup", "words", "number-row", "numpad", "prose"];

/// Every key of the file.
pub const KEYS: &[Key] = &[
    Key {
        name: "session.minutes",
        accepts: Accepts::IntegerIn(&[5, 10, 15, 20, 30]),
        default: DefaultValue::Integer(15),
    },
    Key {
        name: "session.tracks",
        accepts: Accepts::SubsetOf(TRACKS),
        default: DefaultValue::List(TRACKS),
    },
    Key {
        name: "session.emphasis",
        accepts: Accepts::OneOf(&["balanced", "accuracy", "speed"]),
        default: DefaultValue::Text("balanced"),
    },
    Key {
        name: "keyboard.layout",
        accepts: Accepts::OneOf(&["us", "uk", "custom"]),
        default: DefaultValue::Text("us"),
    },
    Key {
        name: "keyboard.numpad",
        accepts: Accepts::Boolean,
        default: DefaultValue::Boolean(true),
    },
    Key {
        name: "keyboard.decimal_separator",
        accepts: Accepts::OneOf(&[".", ","]),
        default: DefaultValue::Text("."),
    },
    Key {
        name: "practice.accuracy_target",
        accepts: Accepts::IntegerRange(90, 100),
        default: DefaultValue::Integer(98),
    },
    Key {
        name: "practice.live_pace",
        accepts: Accepts::Boolean,
        default: DefaultValue::Boolean(false),
    },
    Key {
        name: "content.breadth",
        accepts: Accepts::OneOf(&["narrow", "balanced", "broad"]),
        default: DefaultValue::Text("balanced"),
    },
    Key {
        name: "content.packs",
        accepts: Accepts::PackNames,
        default: DefaultValue::List(&["standard"]),
    },
    Key {
        name: "content.numpad_operators",
        accepts: Accepts::Boolean,
        default: DefaultValue::Boolean(false),
    },
    Key {
        name: "display.theme",
        accepts: Accepts::Theme,
        default: DefaultValue::Text("dark"),
    },
    Key {
        name: "display.text_size",
        accepts: Accepts::OneOf(&["auto", "large", "normal"]),
        default: DefaultValue::Text("auto"),
    },
    Key {
        name: "display.key_guide",
        accepts: Accepts::OneOf(&["auto", "always", "off"]),
        default: DefaultValue::Text("auto"),
    },
    Key {
        name: "display.high_contrast",
        accepts: Accepts::Boolean,
        default: DefaultValue::Boolean(false),
    },
];

/// A validated value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setting {
    /// An integer.
    Integer(i64),
    /// A boolean.
    Boolean(bool),
    /// A string.
    Text(String),
    /// A list of strings.
    List(Vec<String>),
}

impl fmt::Display for Setting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(n) => write!(f, "{n}"),
            Self::Boolean(b) => write!(f, "{b}"),
            Self::Text(s) => write!(f, "\"{s}\""),
            Self::List(items) => {
                let list: Vec<String> = items.iter().map(|s| format!("\"{s}\"")).collect();
                write!(f, "[{}]", list.join(", "))
            }
        }
    }
}

/// The SHA-256 of a config file's bytes (#79): an edit records the digest
/// it was based on, and a write whose file no longer has that digest is a
/// conflict, never a silent overwrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConfigDigest([u8; 32]);

impl ConfigDigest {
    /// The digest of `bytes`.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        use sha2::{Digest, Sha256};
        Self(Sha256::digest(bytes).into())
    }
}

impl fmt::Display for ConfigDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// The eleven roles a theme file defines, in palette order (#82).
pub const THEME_ROLES: [&str; 11] = [
    "background",
    "panel",
    "text",
    "muted",
    "border",
    "focus",
    "correct",
    "error",
    "error-background",
    "warning",
    "selected-background",
];

/// One role's colour in a theme file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeColor {
    /// `#rrggbb`.
    Rgb(u8, u8, u8),
    /// `default`: the terminal's own colour.
    Default,
}

/// A validated theme: every role's colour, in [`THEME_ROLES`] order.
pub type ThemeColors = [ThemeColor; 11];

/// The kernel kind of a key, by a total match over the catalogue.
#[must_use]
pub fn kind(name: &str) -> Option<Kind> {
    Some(match name {
        "session.minutes" => Kind::Minutes,
        "session.tracks" => Kind::Tracks,
        "session.emphasis" => Kind::Emphasis,
        "keyboard.layout" => Kind::Layout,
        "keyboard.numpad" => Kind::Numpad,
        "keyboard.decimal_separator" => Kind::DecimalSeparator,
        "practice.accuracy_target" => Kind::AccuracyTarget,
        "practice.live_pace" => Kind::LivePace,
        "content.breadth" => Kind::Breadth,
        "content.packs" => Kind::Packs,
        "content.numpad_operators" => Kind::NumpadOperators,
        "display.theme" => Kind::Theme,
        "display.text_size" => Kind::TextSize,
        "display.key_guide" => Kind::KeyGuide,
        "display.high_contrast" => Kind::HighContrast,
        _ => return None,
    })
}

/// When a change to `name` takes effect: the proved kernel decision.
#[must_use]
pub fn effect(name: &str) -> Option<Effect> {
    kind(name).map(takes_effect)
}

#[cfg(test)]
mod tests {
    use super::{Effect, KEYS, effect, kind};

    #[test]
    fn every_key_has_a_kind_and_display_keys_apply_now() {
        assert!(KEYS.iter().all(|key| kind(key.name).is_some()));
        assert_eq!(kind("session.colour"), None);
        for key in KEYS {
            let display = key.name.starts_with("display.") || key.name == "practice.live_pace";
            let expected = if display {
                Effect::Now
            } else {
                Effect::NextSession
            };
            assert_eq!(effect(key.name), Some(expected), "{}", key.name);
        }
    }
}
