//! Training alphabet (L-02).
//!
//! V1 training alphabet: printable ASCII (`0x20..=0x7E`), explicit
//! newline, and `£` where the configured layout supports it. Targets are
//! rejected before a scored session when they contain anything else,
//! with line/column diagnostics. UI text itself is not restricted; this
//! module governs only scored practice content.

use core::fmt;

/// A validated character that may appear in a target or typed input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrainingChar(char);

/// The keyboard layout (#77): it decides which units are typeable (UK
/// admits `£`) and partitions the learner's history (L-06).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// US QWERTY: no `£`.
    #[default]
    Us,
    /// UK QWERTY: `£` on its own key.
    Uk,
    /// Another layout, typed as text only.
    Custom,
}

impl Layout {
    /// Whether `£` is admissible.
    #[must_use]
    pub const fn supports_pound(self) -> bool {
        matches!(self, Self::Uk)
    }

    /// The persisted and configured name.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Us => "us",
            Self::Uk => "uk",
            Self::Custom => "custom",
        }
    }

    /// Parses a persisted or configured name.
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        [Self::Us, Self::Uk, Self::Custom]
            .into_iter()
            .find(|l| l.key() == key)
    }
}

/// A rejection with source position, reported before any scoring starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlphabetError {
    /// One-based line of the offending character.
    pub line: u32,
    /// One-based column of the offending character.
    pub column: u32,
    /// The offending character (kept for the diagnostic; targets are
    /// short bounded strings).
    pub character: char,
    /// Why it was rejected.
    pub reason: &'static str,
}

impl fmt::Display for AlphabetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unsupported training character {:?} at line {} column {}: {}",
            self.character, self.line, self.column, self.reason
        )
    }
}

impl std::error::Error for AlphabetError {}

impl TrainingChar {
    /// Validates a character against the V1 training alphabet.
    ///
    /// # Errors
    /// Returns [`AlphabetError`] diagnostics with position `0,0`; use
    /// [`validate_target`] for positioned diagnostics.
    pub fn new(character: char, pound: Layout) -> Result<Self, AlphabetError> {
        let ok =
            matches!(character, '\n' | ' '..='~') || (character == '£' && pound.supports_pound());
        if ok {
            Ok(Self(character))
        } else {
            Err(AlphabetError {
                line: 0,
                column: 0,
                character,
                reason: if character == '£' {
                    "the active layout does not provide this character"
                } else {
                    "outside the V1 training alphabet (printable ASCII, newline, £)"
                },
            })
        }
    }

    /// The wrapped character.
    #[must_use]
    pub const fn get(self) -> char {
        self.0
    }

    /// Whether this is a decimal digit (numpad metrics count these).
    #[must_use]
    pub const fn is_digit(self) -> bool {
        self.0.is_ascii_digit()
    }
}

impl fmt::Display for TrainingChar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Validates a whole target string, producing positioned diagnostics for
/// the first unsupported character.
///
/// # Errors
/// Returns the first [`AlphabetError`] with one-based line/column.
pub fn validate_target(text: &str, pound: Layout) -> Result<Vec<TrainingChar>, AlphabetError> {
    let mut units = Vec::with_capacity(text.len());
    let mut line = 1u32;
    let mut column = 1u32;
    for character in text.chars() {
        let ok =
            matches!(character, '\n' | ' '..='~') || (character == '£' && pound.supports_pound());
        if !ok {
            return Err(AlphabetError {
                line,
                column,
                character,
                reason: if character == '£' {
                    "the active layout does not provide this character"
                } else {
                    "outside the V1 training alphabet (printable ASCII, newline, £)"
                },
            });
        }
        units.push(TrainingChar(character));
        if character == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else {
            column = column.saturating_add(1);
        }
    }
    Ok(units)
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.

    use super::*;

    #[test]
    fn accepts_printable_ascii_and_newline() {
        assert_eq!(
            validate_target("cat 005.20\n;?", Layout::Us).unwrap().len(),
            13
        );
    }

    #[test]
    fn pound_depends_on_layout() {
        assert!(validate_target("£", Layout::Uk).is_ok());
        let error = validate_target("a£b", Layout::Us).unwrap_err();
        assert_eq!((error.line, error.column), (1, 2));
    }

    #[test]
    fn rejects_non_ascii_with_line_column() {
        let error = validate_target("café", Layout::Uk).unwrap_err();
        assert_eq!((error.line, error.column, error.character), (1, 4, 'é'));
        let tab = validate_target("a\tb", Layout::Uk).unwrap_err();
        assert_eq!(tab.character, '\t');
    }
}
