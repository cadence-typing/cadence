//! Original offline practice prose, not quotations or benchmark probes.
//! `corpus/NOTICE` records authorship and licensing limitations. Preparation
//! normalizes paragraph whitespace and keeps every target within its bound.
use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar};

/// Prose preparation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProseError {
    /// A zero-size exercise is not meaningful.
    InvalidBound,
    /// The bank contains unsupported text.
    Alphabet(cadence_core::AlphabetError),
}
impl std::fmt::Display for ProseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBound => write!(f, "prose target bound must be positive"),
            Self::Alphabet(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for ProseError {}

/// Splits paragraphs into bounded exercises, preferring word boundaries.
///
/// # Errors
/// Rejects a zero bound and unsupported alphabet characters.
pub fn prose_exercises(
    bank: &str,
    target_units: usize,
) -> Result<Vec<Vec<TrainingChar>>, ProseError> {
    if target_units == 0 {
        return Err(ProseError::InvalidBound);
    }
    let space = TrainingChar::new(' ', LayoutSupportsPound::US).map_err(ProseError::Alphabet)?;
    let mut result = Vec::new();
    for paragraph in bank.split("\n\n") {
        let mut current = Vec::new();
        for word in paragraph.split_whitespace() {
            let units = word
                .chars()
                .map(|character| {
                    TrainingChar::new(character, LayoutSupportsPound::US)
                        .map_err(ProseError::Alphabet)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let extra = units
                .len()
                .checked_add(usize::from(!current.is_empty()))
                .ok_or(ProseError::InvalidBound)?;
            if !current.is_empty()
                && current
                    .len()
                    .checked_add(extra)
                    .is_none_or(|length| length > target_units)
            {
                result.push(std::mem::take(&mut current));
            }
            for chunk in units.chunks(target_units) {
                if !current.is_empty() {
                    current.push(space);
                }
                current.extend_from_slice(chunk);
                if current.len() == target_units {
                    result.push(std::mem::take(&mut current));
                }
            }
        }
        if !current.is_empty() {
            result.push(current);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn boundaries_include_spaces_and_overlong_words() {
        for bound in 1..=30 {
            let targets =
                prose_exercises("One two three.\n\nAverylongunbrokenwordthenmore!", bound).unwrap();
            assert!(
                targets
                    .iter()
                    .all(|target| !target.is_empty() && target.len() <= bound)
            );
            let actual: String = targets
                .iter()
                .flatten()
                .map(|unit| unit.get())
                .filter(|c| !c.is_whitespace())
                .collect();
            assert_eq!(actual, "Onetwothree.Averylongunbrokenwordthenmore!");
        }
        assert!(prose_exercises("text", 0).is_err());
    }
    #[test]
    fn embedded_original_bank_is_valid() {
        let targets =
            prose_exercises(include_str!("../../../corpus/prose-en-v1.txt"), 120).unwrap();
        assert!(targets.len() >= 8);
        assert!(targets.iter().all(|target| target.len() <= 120));
    }
}
