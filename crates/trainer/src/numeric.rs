//! Seeded numeric practice. Numpad targets are single Enter-submitted entries;
//! decimals and leading zeros remain strings, never parsed numeric values.
use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar};
use cadence_core::learning::Skill;
use std::collections::HashSet;

/// Numeric preparation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumericError {
    /// A template or entropy source produced an invalid value.
    InvalidCharacter,
    /// No new entry was found within the bounded preparation scan.
    Exhausted,
}
impl std::fmt::Display for NumericError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCharacter => write!(f, "invalid numeric template or entropy value"),
            Self::Exhausted => write!(
                f,
                "numeric preparation exhausted its bounded distinct-entry scan"
            ),
        }
    }
}
impl std::error::Error for NumericError {}

/// Seeded entropy source owned by the plan builder.
pub trait DigitSource {
    /// Uniform value in `0..bound`; callers pass a positive bound.
    fn below(&mut self, bound: u64) -> u64;
}
fn digits(rng: &mut dyn DigitSource, length: usize) -> Result<String, NumericError> {
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
fn count(rng: &mut dyn DigitSource, base: usize, bound: u64) -> Result<usize, NumericError> {
    let value = rng.below(bound);
    if value >= bound {
        return Err(NumericError::InvalidCharacter);
    }
    base.checked_add(
        usize::try_from(value).map_err(|_unrepresentable| NumericError::InvalidCharacter)?,
    )
    .ok_or(NumericError::InvalidCharacter)
}
fn units(text: &str) -> Result<Vec<TrainingChar>, NumericError> {
    text.chars()
        .map(|character| {
            TrainingChar::new(character, LayoutSupportsPound::US)
                .map_err(|_unrepresentable| NumericError::InvalidCharacter)
        })
        .collect()
}

/// The characters of a focus skill (a digit, `.`, or a pair of them).
fn skill_text(skill: Skill) -> String {
    match skill {
        Skill::Unit(unit) => unit.get().to_string(),
        Skill::Bigram(first, second) => [first.get(), second.get()].iter().collect(),
    }
}

/// Overwrites a seeded window of the digit run `run` with `skill` (digits
/// only), keeping the run's length. A skill longer than the run replaces it.
fn embed(rng: &mut dyn DigitSource, run: &str, skill: &str) -> Result<String, NumericError> {
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

/// One numpad target containing only digits and optionally one decimal point.
/// Leading zeros and fractional trailing zeros are significant target units.
///
/// # Errors
/// Rejects invalid entropy or bounded distinct-content exhaustion, never silently
/// substituting a repeat for fresh content.
pub fn numpad_exercise(
    rng: &mut dyn DigitSource,
    used: &mut HashSet<String>,
    focus: Option<Skill>,
) -> Result<Vec<TrainingChar>, NumericError> {
    for _ in 0..64 {
        let text = match focus {
            Some(skill) => targeted_numpad(rng, skill)?,
            None => uniform_numpad(rng)?,
        };
        if !used.contains(&text) {
            let target = units(&text)?;
            used.insert(text);
            return Ok(target);
        }
    }
    Err(NumericError::Exhausted)
}

/// A numpad entry built around a focus skill (#5): digits are embedded in
/// a digit run; a skill with the decimal point makes a decimal entry.
fn targeted_numpad(rng: &mut dyn DigitSource, skill: Skill) -> Result<String, NumericError> {
    let text = skill_text(skill);
    let length = count(rng, 2, 5)?;
    Ok(match text.split_once('.') {
        None => {
            if rng.below(3) == 0 {
                // Keep decimals in the targeted mix too.
                let whole = digits(rng, length)?;
                format!("{}.{}", embed(rng, &whole, &text)?, digits(rng, 2)?)
            } else {
                let run = digits(rng, length)?;
                embed(rng, &run, &text)?
            }
        }
        Some((before, after)) => {
            let whole = format!("{}{before}", digits(rng, length.saturating_sub(1).max(1))?);
            let fraction = format!(
                "{after}{}",
                digits(rng, 2_usize.saturating_sub(after.len()))?
            );
            format!("{whole}.{fraction}")
        }
    })
}

/// A uniform numpad entry (the cold-start mix).
fn uniform_numpad(rng: &mut dyn DigitSource) -> Result<String, NumericError> {
    {
        let text = match rng.below(5) {
            0 => {
                let length = count(rng, 2, 5)?;
                digits(rng, length)?
            }
            1 => {
                let length = count(rng, 1, 4)?;
                format!("{}.{}", digits(rng, length)?, digits(rng, 2)?)
            }
            2 => {
                let digit = digits(rng, 1)?;
                let length = count(rng, 3, 4)?;
                digit.repeat(length)
            }
            3 => {
                let pair = digits(rng, 2)?;
                pair.repeat(2)
            }
            4 => format!("00{}.{}", digits(rng, 3)?, digits(rng, 2)?),
            _ => return Err(NumericError::InvalidCharacter),
        };
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::PlanRng;
    use rand::SeedableRng;

    #[test]
    fn single_enter_entries_preserve_zeros_and_decimals() {
        let mut rng = PlanRng::seed_from_u64(42);
        let mut used = HashSet::new();
        let mut saw_leading = false;
        let mut saw_decimal = false;
        for _ in 0..1800 {
            let target = numpad_exercise(&mut rng, &mut used, None).unwrap();
            let text: String = target.iter().map(|unit| unit.get()).collect();
            assert!(text.chars().all(|c| c.is_ascii_digit() || c == '.'));
            assert!(text.matches('.').count() <= 1);
            saw_leading |= text.starts_with("00");
            saw_decimal |= text.contains('.');
        }
        assert!(saw_leading && saw_decimal);
        assert_eq!(used.len(), 1800);
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
            .map(|c| TrainingChar::new(c, LayoutSupportsPound::US).unwrap())
            .collect();
        match units.as_slice() {
            [one] => Skill::Unit(*one),
            [a, b] => Skill::Bigram(*a, *b),
            _ => panic!("skill"),
        }
    }

    /// #5: a targeted entry is valid for its track and contains its focus.
    #[test]
    fn targeted_entries_contain_their_focus_and_stay_valid() {
        let mut rng = PlanRng::seed_from_u64(7);
        let mut used = HashSet::new();
        for focus in ["7", "48", ".", "4.", ".5", "00"] {
            for _ in 0..40 {
                let target = numpad_exercise(&mut rng, &mut used, Some(skill(focus))).unwrap();
                let text: String = target.iter().map(|unit| unit.get()).collect();
                assert!(
                    text.chars().all(|c| c.is_ascii_digit() || c == '.'),
                    "{text}"
                );
                assert!(text.matches('.').count() <= 1, "{text}");
                assert!(contains_skill(&text, skill(focus)), "{focus} in {text}");
            }
        }
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

    /// #5: the same seed and focus give the same entries.
    #[test]
    fn targeted_entries_replay() {
        let run = |seed| {
            let mut rng = PlanRng::seed_from_u64(seed);
            let mut used = HashSet::new();
            (0..20)
                .map(|_| numpad_exercise(&mut rng, &mut used, Some(skill("48"))).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(run(3), run(3));
        assert_ne!(run(3), run(4));
    }
}
