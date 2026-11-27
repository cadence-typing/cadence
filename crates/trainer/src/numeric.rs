//! Seeded numeric practice: number-row entries, and the parts numpad entries
//! share (`crate::numeric_keypad`). Numpad targets are single
//! Enter-submitted entries; decimals and leading zeros remain strings, never
//! parsed numeric values. An entry is built with `.` and the session's
//! decimal separator replaces it last. Operators and separators are typed
//! units, so they count toward accuracy, but never toward the digit
//! numerator (`TrainingChar::is_digit`).
use cadence_core::alphabet::{Layout, TrainingChar};
use cadence_core::learning::Skill;
use std::collections::HashSet;

/// Numeric preparation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumericError {
    /// A template or entropy source produced an invalid value.
    InvalidCharacter,
    /// No new entry was found within the bounded preparation scan.
    Exhausted,
    /// A built numpad entry is not `sign? operand (operator operand)*`.
    Malformed,
    /// The focus skill cannot be carried by the entry's family.
    FocusNotCarried,
}
impl std::fmt::Display for NumericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCharacter => write!(f, "invalid numeric template or entropy value"),
            Self::Exhausted => write!(
                f,
                "numeric preparation exhausted its bounded distinct-entry scan"
            ),
            Self::Malformed => write!(f, "a numpad entry broke the keypad entry grammar"),
            Self::FocusNotCarried => {
                write!(
                    f,
                    "a numpad focus skill was given to a family that cannot carry it"
                )
            }
        }
    }
}
impl std::error::Error for NumericError {}

/// The keypad operators (#83, #101).
pub const OPERATORS: [char; 4] = ['+', '-', '*', '/'];

/// The decimal separator numpad entries use (`keyboard.decimal_separator`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Separator {
    /// `.`
    #[default]
    Point,
    /// `,`
    Comma,
}

impl Separator {
    /// The character.
    #[must_use]
    pub const fn char(self) -> char {
        match self {
            Self::Point => '.',
            Self::Comma => ',',
        }
    }
}

/// How numpad entries are written (#83).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NumericOptions {
    /// Operator families (signed, operation, chain) may be served (#101).
    pub operators: bool,
    /// The decimal separator.
    pub separator: Separator,
}

/// Seeded entropy source owned by the plan builder.
pub trait DigitSource {
    /// Uniform value in `0..bound`; callers pass a positive bound.
    fn below(&mut self, bound: u64) -> u64;
}
pub(crate) fn digits(rng: &mut dyn DigitSource, length: usize) -> Result<String, NumericError> {
    (0..length)
        .map(|_| {
            let value = u8::try_from(rng.below(10))
                .map_err(|_unrepresentable| NumericError::InvalidCharacter)?;
            if value > 9 {
                return Err(NumericError::InvalidCharacter);
            }
            b'0'.checked_add(value)
                .map(char::from)
                .ok_or(NumericError::InvalidCharacter)
        })
        .collect()
}
pub(crate) fn count(
    rng: &mut dyn DigitSource,
    base: usize,
    bound: u64,
) -> Result<usize, NumericError> {
    let value = rng.below(bound);
    if value >= bound {
        return Err(NumericError::InvalidCharacter);
    }
    base.checked_add(
        usize::try_from(value).map_err(|_unrepresentable| NumericError::InvalidCharacter)?,
    )
    .ok_or(NumericError::InvalidCharacter)
}
pub(crate) fn units(text: &str) -> Result<Vec<TrainingChar>, NumericError> {
    text.chars()
        .map(|character| {
            TrainingChar::new(character, Layout::Us)
                .map_err(|_unrepresentable| NumericError::InvalidCharacter)
        })
        .collect()
}

/// The characters of a focus skill (a keypad key or a pair), with either
/// separator written `.`.
pub(crate) fn skill_text(skill: Skill) -> String {
    let canonical = |unit: TrainingChar| if unit.get() == ',' { '.' } else { unit.get() };
    match skill {
        Skill::Unit(unit) => canonical(unit).to_string(),
        Skill::Bigram(first, second) => [canonical(first), canonical(second)].iter().collect(),
    }
}

/// Overwrites a seeded window of the digit run `run` with `skill` (digits
/// only), keeping the run's length. A skill longer than the run replaces it.
pub(crate) fn embed(
    rng: &mut dyn DigitSource,
    run: &str,
    skill: &str,
) -> Result<String, NumericError> {
    let (run_len, skill_len) = (run.chars().count(), skill.chars().count());
    if skill_len >= run_len {
        return Ok(skill.to_string());
    }
    let slack = u64::try_from(run_len.saturating_sub(skill_len))
        .map_err(|_unrepresentable| NumericError::InvalidCharacter)?;
    let start = usize::try_from(rng.below(slack.saturating_add(1)))
        .map_err(|_unrepresentable| NumericError::InvalidCharacter)?;
    let before: String = run.chars().take(start).collect();
    let after: String = run.chars().skip(start.saturating_add(skill_len)).collect();
    Ok(format!("{before}{skill}{after}"))
}

/// Whether `text` contains the focus skill's characters.
#[must_use]
pub fn contains_skill(text: &str, skill: Skill) -> bool {
    text.contains(&skill_text(skill))
}

/// One number-row target with neutral identifier context.
///
/// # Errors
/// Rejects invalid entropy or bounded distinct-content exhaustion.
pub fn number_row_exercise(
    rng: &mut dyn DigitSource,
    used: &mut HashSet<String>,
    focus: Option<Skill>,
) -> Result<Vec<TrainingChar>, NumericError> {
    const CONTEXTS: [&str; 8] = [
        "route", "room", "model", "gate", "box", "unit", "line", "case",
    ];
    for _ in 0..64 {
        let index = usize::try_from(rng.below(8))
            .map_err(|_unrepresentable| NumericError::InvalidCharacter)?;
        let context = CONTEXTS.get(index).ok_or(NumericError::InvalidCharacter)?;
        let length = count(rng, 3, 4)?;
        let mut run = digits(rng, length)?;
        if let Some(skill) = focus {
            // A targeted entry (#5): the focus digit or pair sits in the run.
            run = embed(rng, &run, &skill_text(skill))?;
        }
        let text = format!("{context} {run}");
        if !used.contains(&text) {
            let target = units(&text)?;
            used.insert(text);
            return Ok(target);
        }
    }
    Err(NumericError::Exhausted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::PlanRng;
    use rand::SeedableRng;

    /// #83: an entry is a string: leading and trailing zeros stay exactly.
    #[test]
    fn a_decimal_entry_stays_that_exact_string() {
        let text: String = units("005.20")
            .unwrap()
            .iter()
            .map(|unit| unit.get())
            .collect();
        assert_eq!(text, "005.20");
    }

    #[test]
    fn seeds_replay_and_change_content() {
        let make = |seed| {
            number_row_exercise(&mut PlanRng::seed_from_u64(seed), &mut HashSet::new(), None)
                .unwrap()
        };
        assert_eq!(make(1), make(1));
        assert_ne!(make(1), make(2));
    }

    fn skill(text: &str) -> Skill {
        let units: Vec<TrainingChar> = text
            .chars()
            .map(|c| TrainingChar::new(c, Layout::Us).unwrap())
            .collect();
        match units.as_slice() {
            [one] => Skill::Unit(*one),
            [a, b] => Skill::Bigram(*a, *b),
            _ => panic!("skill"),
        }
    }

    /// #5: a targeted number-row entry is valid and contains its focus.
    #[test]
    fn targeted_number_row_entries_contain_their_focus() {
        let mut rng = PlanRng::seed_from_u64(7);
        let mut used = HashSet::new();
        for focus in ["3", "48"] {
            for _ in 0..40 {
                let target = number_row_exercise(&mut rng, &mut used, Some(skill(focus))).unwrap();
                let text: String = target.iter().map(|unit| unit.get()).collect();
                let run = text.split(' ').nth(1).unwrap();
                assert!(run.chars().all(|c| c.is_ascii_digit()), "{text}");
                assert!(contains_skill(run, skill(focus)), "{focus} in {text}");
            }
        }
    }
}
