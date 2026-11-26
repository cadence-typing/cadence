//! `config.toml` (U-04, D-01, #76): typed, validated preferences.
//!
//! The file lives at `$XDG_CONFIG_HOME/cadence/config.toml` (relative XDG
//! values are ignored, as for data), else `~/.config/cadence/config.toml`.
//! Every key is listed in [`KEYS`] with its accepted values; an unknown key,
//! a wrong type or a value out of range is an error naming the file, the
//! line, the key and what is accepted. Omitted keys take their defaults.
//! Precedence is defaults, then the file, then CLI flags (the caller applies
//! the flags). In-app edits go through [`set`], which keeps comments and
//! formatting (`toml_edit`).

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, ImDocument, Item, Value};

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
    /// The default, as TOML.
    pub default: &'static str,
}

/// Tracks a session may include.
pub const TRACKS: &[&str] = &["warmup", "words", "number-row", "numpad", "prose"];

/// Every key of the file.
pub const KEYS: &[Key] = &[
    Key {
        name: "session.minutes",
        accepts: Accepts::IntegerIn(&[5, 10, 15, 20, 30]),
        default: "15",
    },
    Key {
        name: "session.tracks",
        accepts: Accepts::SubsetOf(TRACKS),
        default: r#"["warmup", "words", "number-row", "numpad", "prose"]"#,
    },
    Key {
        name: "session.emphasis",
        accepts: Accepts::OneOf(&["balanced", "accuracy", "speed"]),
        default: r#""balanced""#,
    },
    Key {
        name: "keyboard.layout",
        accepts: Accepts::OneOf(&["us", "uk", "custom"]),
        default: r#""us""#,
    },
    Key {
        name: "keyboard.numpad",
        accepts: Accepts::Boolean,
        default: "true",
    },
    Key {
        name: "keyboard.decimal_separator",
        accepts: Accepts::OneOf(&[".", ","]),
        default: r#"".""#,
    },
    Key {
        name: "practice.accuracy_target",
        accepts: Accepts::IntegerRange(90, 100),
        default: "98",
    },
    Key {
        name: "practice.live_pace",
        accepts: Accepts::Boolean,
        default: "false",
    },
    Key {
        name: "content.breadth",
        accepts: Accepts::OneOf(&["narrow", "balanced", "broad"]),
        default: r#""balanced""#,
    },
    Key {
        name: "content.packs",
        accepts: Accepts::PackNames,
        default: r#"["standard"]"#,
    },
    Key {
        name: "display.theme",
        accepts: Accepts::Theme,
        default: r#""dark""#,
    },
    Key {
        name: "display.text_size",
        accepts: Accepts::OneOf(&["auto", "large", "normal"]),
        default: r#""auto""#,
    },
    Key {
        name: "display.key_guide",
        accepts: Accepts::OneOf(&["auto", "always", "off"]),
        default: r#""auto""#,
    },
    Key {
        name: "display.high_contrast",
        accepts: Accepts::Boolean,
        default: "false",
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

/// Where an effective value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The built-in default.
    Default,
    /// The config file.
    File,
    /// A command-line flag.
    Flag,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Default => "default",
            Self::File => "file",
            Self::Flag => "flag",
        })
    }
}

/// A configuration problem, located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The file.
    pub file: PathBuf,
    /// The 1-based line (0 when the file could not be read).
    pub line: usize,
    /// The key, when one is concerned.
    pub key: String,
    /// What is wrong and what is accepted.
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.key.is_empty() {
            write!(f, "{}:{}: {}", self.file.display(), self.line, self.message)
        } else {
            write!(
                f,
                "{}:{}: {}: {}",
                self.file.display(),
                self.line,
                self.key,
                self.message
            )
        }
    }
}

impl std::error::Error for ConfigError {}

/// The effective configuration: every key with its value and source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    values: BTreeMap<&'static str, (Setting, Source)>,
}

impl Default for Config {
    fn default() -> Self {
        let mut values = BTreeMap::new();
        for key in KEYS {
            let setting = default_setting(key).unwrap_or(Setting::Boolean(false));
            values.insert(key.name, (setting, Source::Default));
        }
        Self { values }
    }
}

fn default_setting(key: &Key) -> Option<Setting> {
    let document: ImDocument<String> = format!("v = {}", key.default).parse().ok()?;
    let value = document.get("v")?.as_value()?;
    validate(key, value).ok()
}

fn line_of(text: &str, span: Option<std::ops::Range<usize>>) -> usize {
    let offset = span.map_or(0, |range| range.start);
    text.get(..offset)
        .map_or(1, |prefix| prefix.matches('\n').count().saturating_add(1))
}

fn validate(key: &Key, value: &Value) -> Result<Setting, String> {
    let wrong = || format!("expected {}", key.accepts);
    match key.accepts {
        Accepts::IntegerIn(allowed) => {
            let n = value.as_integer().ok_or_else(wrong)?;
            allowed
                .contains(&n)
                .then_some(Setting::Integer(n))
                .ok_or_else(wrong)
        }
        Accepts::IntegerRange(low, high) => {
            let n = value.as_integer().ok_or_else(wrong)?;
            (low..=high)
                .contains(&n)
                .then_some(Setting::Integer(n))
                .ok_or_else(wrong)
        }
        Accepts::Boolean => value.as_bool().map(Setting::Boolean).ok_or_else(wrong),
        Accepts::OneOf(allowed) => {
            let s = value.as_str().ok_or_else(wrong)?;
            allowed
                .contains(&s)
                .then(|| Setting::Text(s.to_string()))
                .ok_or_else(wrong)
        }
        Accepts::SubsetOf(allowed) => {
            let items = strings(value).ok_or_else(wrong)?;
            let distinct = items
                .iter()
                .enumerate()
                .all(|(i, item)| !items.iter().take(i).any(|x| x == item));
            (!items.is_empty() && distinct && items.iter().all(|i| allowed.contains(&i.as_str())))
                .then_some(Setting::List(items))
                .ok_or_else(wrong)
        }
        Accepts::PackNames => {
            let items = strings(value).ok_or_else(wrong)?;
            let name_ok = |s: &String| {
                (1..=64).contains(&s.len())
                    && s.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            };
            let distinct = items
                .iter()
                .enumerate()
                .all(|(i, item)| !items.iter().take(i).any(|x| x == item));
            (!items.is_empty() && distinct && items.iter().all(name_ok))
                .then_some(Setting::List(items))
                .ok_or_else(wrong)
        }
        Accepts::Theme => {
            let s = value.as_str().ok_or_else(wrong)?;
            let builtin = ["dark", "light", "terminal"].contains(&s);
            let file = s.len() > 5
                && s.len() <= 80
                && s.ends_with(".toml")
                && !s.contains(['/', '\\'])
                && !s.starts_with('.');
            (builtin || file)
                .then(|| Setting::Text(s.to_string()))
                .ok_or_else(wrong)
        }
    }
}

fn strings(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}

impl Config {
    /// Parses a file's text strictly.
    ///
    /// # Errors
    /// Every problem found, each located.
    pub fn parse(text: &str, file: &Path) -> Result<Self, Vec<ConfigError>> {
        let error = |line: usize, key: &str, message: String| ConfigError {
            file: file.to_path_buf(),
            line,
            key: key.to_string(),
            message,
        };
        let document: ImDocument<&str> = ImDocument::parse(text).map_err(|e| {
            vec![error(
                line_of(text, e.span()),
                "",
                format!("not valid TOML: {}", e.message()),
            )]
        })?;
        let mut config = Self::default();
        let mut errors = Vec::new();
        for (table_name, item) in document.iter() {
            let Some(table) = item.as_table() else {
                errors.push(error(
                    line_of(text, item.span()),
                    table_name,
                    "expected a table such as [session]".into(),
                ));
                continue;
            };
            for (name, item) in table.iter() {
                let dotted = format!("{table_name}.{name}");
                let line = line_of(
                    text,
                    item.span()
                        .or_else(|| table.key(name).and_then(toml_edit::Key::span)),
                );
                let Some(key) = KEYS.iter().find(|k| k.name == dotted) else {
                    errors.push(error(line, &dotted, "unknown key".into()));
                    continue;
                };
                let Some(value) = item.as_value() else {
                    errors.push(error(line, &dotted, format!("expected {}", key.accepts)));
                    continue;
                };
                match validate(key, value) {
                    Ok(setting) => {
                        config.values.insert(key.name, (setting, Source::File));
                    }
                    Err(message) => errors.push(error(line, &dotted, message)),
                }
            }
        }
        if errors.is_empty() {
            Ok(config)
        } else {
            Err(errors)
        }
    }

    /// Loads `path`; a missing file is the defaults.
    ///
    /// # Errors
    /// An unreadable or invalid file.
    pub fn load(path: &Path) -> Result<Self, Vec<ConfigError>> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(vec![ConfigError {
                file: path.to_path_buf(),
                line: 0,
                key: String::new(),
                message: format!("cannot read: {error}"),
            }]),
        }
    }

    /// A key's effective value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Setting> {
        self.values.get(key).map(|(setting, _)| setting)
    }

    /// A key's source.
    #[must_use]
    pub fn source(&self, key: &str) -> Option<Source> {
        self.values.get(key).map(|(_, source)| *source)
    }

    /// An integer key's value.
    #[must_use]
    pub fn integer(&self, key: &str) -> Option<i64> {
        match self.get(key)? {
            Setting::Integer(n) => Some(*n),
            Setting::Boolean(_) | Setting::Text(_) | Setting::List(_) => None,
        }
    }

    /// A string key's value.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Setting::Text(s) => Some(s),
            Setting::Integer(_) | Setting::Boolean(_) | Setting::List(_) => None,
        }
    }

    /// A boolean key's value.
    #[must_use]
    pub fn boolean(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Setting::Boolean(b) => Some(*b),
            Setting::Integer(_) | Setting::Text(_) | Setting::List(_) => None,
        }
    }

    /// Records a command-line flag's value, which overrides the file.
    pub fn flag(&mut self, key: &'static str, setting: Setting) {
        self.values.insert(key, (setting, Source::Flag));
    }

    /// Every key with its effective value and source, in key order.
    #[must_use]
    pub fn effective(&self) -> Vec<(&'static str, &Setting, Source)> {
        KEYS.iter()
            .filter_map(|key| {
                self.values
                    .get(key.name)
                    .map(|(s, src)| (key.name, s, *src))
            })
            .collect()
    }
}

/// Rewrites one key in a file's text, keeping every comment and all other
/// formatting byte for byte.
///
/// # Errors
/// An unknown key, an invalid value or text that is not TOML.
pub fn set(text: &str, key: &str, value: &str) -> Result<String, String> {
    let spec = KEYS
        .iter()
        .find(|k| k.name == key)
        .ok_or_else(|| format!("unknown key {key}"))?;
    let parsed: ImDocument<String> = format!("v = {value}")
        .parse()
        .map_err(|error: toml_edit::TomlError| format!("not a TOML value: {}", error.message()))?;
    let new = parsed
        .get("v")
        .and_then(Item::as_value)
        .ok_or("not a TOML value")?;
    validate(spec, new)?;
    let mut document: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    let (table, name) = key.split_once('.').ok_or("keys are table.key")?;
    let entry = document
        .as_table_mut()
        .entry(table)
        .or_insert_with(toml_edit::table);
    let table = entry.as_table_like_mut().ok_or("not a table")?;
    // The value's surrounding spaces and comment stay as they were.
    let decor = table
        .get(name)
        .and_then(Item::as_value)
        .map(|value| value.decor().clone());
    let mut replacement = new.clone();
    if let Some(decor) = decor {
        *replacement.decor_mut() = decor;
    }
    // Assigning in place keeps the key's own decor (a comment above it).
    match table.get_mut(name) {
        Some(item) => *item = Item::Value(replacement),
        None => {
            table.insert(name, Item::Value(replacement));
        }
    }
    Ok(document.to_string())
}

/// The config file path: `$XDG_CONFIG_HOME/cadence/config.toml` when that
/// is absolute, else `$HOME/.config/cadence/config.toml`.
#[must_use]
pub fn path(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    let base = xdg_config_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|h| h.join(".config"))
        })?;
    Some(base.join("cadence").join("config.toml"))
}
