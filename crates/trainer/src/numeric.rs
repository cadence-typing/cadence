//! Numeric track generators (P-04): number-row and numpad content.
//!
//! Fresh amounts/sequences from the same generator class, never the
//! identical list every day (L-03 numeric transfer). String identity is
//! preserved (`005.20` stays exactly that string). Number-row exercises
//! embed digits in text-like contexts; numpad exercises are pure entry
//! drills (digits, decimals, zero, repetition, alternation) where Enter
//! submits.

use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar};

/// Generator failure (malformed template — an authoring bug).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumericError {
    /// A produced exercise contained a character outside the training
    /// alphabet (should be impossible: templates are ASCII).
    InvalidCharacter,
}

impl std::fmt::Display for NumericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "numeric generator produced an invalid character")
    }
}

impl std::error::Error for NumericError {}

/// Deterministic entropy source abstraction over the seeded PRNG: the
/// plan builder owns the RNG instance and hands out bounded integers.
pub trait DigitSource {
    /// A uniformly random value in `0..bound` (bound must be positive).
    fn below(&mut self, bound: u64) -> u64;
}

/// Builds one number-row exercise: digit groups embedded in short
/// neutral identifier contexts (route/room/model numbers).
///
/// # Errors
/// Returns [`NumericError`] only on an internal authoring bug.
pub fn number_row_exercise(
    rng: &mut dyn DigitSource,
    used: &mut std::collections::HashSet<String>,
) -> Result<Vec<TrainingChar>, NumericError> {
    const CONTEXTS: [&str; 8] = [
        "route", "room", "model", "gate", "box", "unit", "line", "case",
    ];
    let mut parts: Vec<String> = Vec::new();
    for _ in 0..4 {
        let context = CONTEXTS[usize::try_from(rng.below(CONTEXTS.len() as u64)).unwrap_or(0)];
        let digit_count = 2 + rng.below(3);
        let digits = fresh_digits(rng, digit_count, used);
        parts.push(format!("{context} {digits}"));
    }
    let text = parts.join("  ");
    to_units(&text)
}

/// Builds one numpad entry exercise: amounts, decimals, leading zeros,
/// repetition and alternation, joined by single spaces. Enter submits
/// the whole entry sequence in the engine.
///
/// # Errors
/// Returns [`NumericError`] only on an internal authoring bug.
pub fn numpad_exercise(
    rng: &mut dyn DigitSource,
    used: &mut std::collections::HashSet<String>,
) -> Result<Vec<TrainingChar>, NumericError> {
    let mut entries: Vec<String> = Vec::new();
    for _ in 0..5 {
        let entry = match rng.below(5) {
            0 => {
                // Integer amount.
                let digit_count = 2 + rng.below(4);
                fresh_digits(rng, digit_count, used)
            }
            1 => {
                // Decimal amount; leading zeros are significant
                // (string identity preserved).
                let leading = rng.below(3);
                let mut text = String::new();
                for _ in 0..leading {
                    text.push('0');
                }
                if text.is_empty() {
                    text.push('0');
                }
                let fraction_digits = 1 + rng.below(2);
                text.push_str(&fresh_digits(rng, fraction_digits, used));
                text.push('.');
                text.push_str(&fresh_digits(rng, 2, used));
                register(used, &text)
            }
            2 => {
                // Repetition drill.
                let digit = char::from(b'0' + u8::try_from(rng.below(10)).unwrap_or(0));
                let repeat = 3 + rng.below(3);
                let text: String =
                    std::iter::repeat_n(digit, usize::try_from(repeat).unwrap_or(3)).collect();
                register(used, &text)
            }
            3 => {
                // Alternation drill.
                let left = char::from(b'0' + u8::try_from(rng.below(10)).unwrap_or(0));
                let right = char::from(b'0' + u8::try_from(rng.below(10)).unwrap_or(1));
                let mut text = String::new();
                for pair in 0..4 {
                    text.push(if pair % 2 == 0 { left } else { right });
                }
                register(used, &text)
            }
            _ => {
                // Zero-leading entry.
                let mut text = String::from("00");
                text.push_str(&fresh_digits(rng, 2, used));
                text
            }
        };
        entries.push(entry);
    }
    let text = entries.join(" ");
    to_units(&text)
}

/// Fresh digit string not reused within the session (`used` guards).
fn fresh_digits(
    rng: &mut dyn DigitSource,
    length: u64,
    used: &mut std::collections::HashSet<String>,
) -> String {
    for _ in 0..64 {
        let mut candidate = String::new();
        for _ in 0..length {
            let digit = char::from(b'0' + u8::try_from(rng.below(10)).unwrap_or(0));
            candidate.push(digit);
        }
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    // Collision after 64 attempts (tiny spaces like length-1): accept a
    // repeat rather than looping; freshness is best-effort for drills.
    let mut fallback = String::new();
    for _ in 0..length {
        let digit = char::from(b'0' + u8::try_from(rng.below(10)).unwrap_or(0));
        fallback.push(digit);
    }
    fallback
}

fn register(used: &mut std::collections::HashSet<String>, text: &str) -> String {
    used.insert(text.to_string());
    text.to_string()
}

fn to_units(text: &str) -> Result<Vec<TrainingChar>, NumericError> {
    text.chars()
        .map(|character| {
            TrainingChar::new(character, LayoutSupportsPound::US)
                .map_err(|_| NumericError::InvalidCharacter)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    // Test assertions may use expect/unwrap/panic to fail loudly; the
    // workspace denies them in production code only.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// Fixed-seed digit source for deterministic tests.
    struct Fixed(u64);

    impl DigitSource for Fixed {
        fn below(&mut self, bound: u64) -> u64 {
            // xorshift-style deterministic walk.
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
            (self.0 >> 33) % bound.max(1)
        }
    }

    #[test]
    fn generators_stay_in_alphabet_and_shape() {
        let mut rng = Fixed(42);
        let mut used = std::collections::HashSet::new();
        let row = number_row_exercise(&mut rng, &mut used).unwrap();
        let text: String = row.iter().map(|unit| unit.get()).collect();
        assert!(text.contains("route") || text.contains("room") || text.contains("gate"));
        assert!(text.chars().any(|c| c.is_ascii_digit()));

        let pad = numpad_exercise(&mut rng, &mut used).unwrap();
        let pad_text: String = pad.iter().map(|unit| unit.get()).collect();
        assert!(
            pad_text
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == ' '),
            "numpad stays pure entry content: {pad_text}"
        );
    }

    #[test]
    fn different_seeds_produce_different_content() {
        let mut used_a = std::collections::HashSet::new();
        let mut used_b = std::collections::HashSet::new();
        let a = numpad_exercise(&mut Fixed(1), &mut used_a).unwrap();
        let b = numpad_exercise(&mut Fixed(2), &mut used_b).unwrap();
        assert_ne!(a, b);
    }
}
