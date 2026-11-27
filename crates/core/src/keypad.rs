//! The numeric keypad (#101): the alphabet, the entry grammar, the family
//! schedule and which focus skills an entry can carry. Every decision is the
//! proved `cadence_kernel::keypad` function of the same name (Lean:
//! `Proofs.Keypad`); this module only turns typed units into the code points
//! the kernel takes (`char as u32`, tested).
use crate::alphabet::TrainingChar;
use crate::learning::Skill;

pub use cadence_kernel::keypad::{Class, Family, Weights};

/// The kernel's code for a unit.
fn code(unit: TrainingChar) -> u32 {
    u32::from(unit.get())
}

/// The keypad class of a unit.
#[must_use]
pub fn class_of(unit: TrainingChar) -> Class {
    cadence_kernel::keypad::class(code(unit))
}

/// Whether `text` is a keypad entry: `sign? operand (operator operand)*`.
#[must_use]
pub fn well_formed(text: &[TrainingChar]) -> bool {
    let codes: Vec<u32> = text.iter().map(|unit| code(*unit)).collect();
    cadence_kernel::keypad::well_formed(&codes)
}

/// The family of the entry at `position` of a cycle with these weights.
#[must_use]
pub fn family_at(position: u64, weights: Weights, operators: bool) -> Family {
    cadence_kernel::keypad::family_at(position, weights, operators)
}

/// The cycle of the weights a session may serve (operator families only
/// with `operators`), or `None` when it does not fit in `u64`.
#[must_use]
pub fn cycle(weights: Weights, operators: bool) -> Option<u64> {
    cadence_kernel::keypad::cycle(cadence_kernel::keypad::admissible(weights, operators))
}

/// A skill as the kernel's `(first, second, pair)`.
fn parts(skill: Skill) -> (u32, u32, bool) {
    match skill {
        Skill::Unit(unit) => (code(unit), 0, false),
        Skill::Bigram(first, second) => (code(first), code(second), true),
    }
}

/// Whether an entry of `family` can carry `skill`.
#[must_use]
pub fn carries(family: Family, skill: Skill) -> bool {
    let (first, second, pair) = parts(skill);
    cadence_kernel::keypad::carries(family, first, second, pair)
}

/// Whether a numpad-lane `skill` may be a numpad focus.
#[must_use]
pub fn focus_admissible(skill: Skill, operators: bool) -> bool {
    let (first, second, pair) = parts(skill);
    cadence_kernel::keypad::focus_admissible(first, second, pair, operators)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphabet::Layout;

    fn units(text: &str) -> Vec<TrainingChar> {
        text.chars()
            .filter_map(|c| TrainingChar::new(c, Layout::Us).ok())
            .collect()
    }

    #[test]
    fn typed_units_reach_the_kernel_as_their_code_points() {
        assert!(well_formed(&units("-12.5*3")));
        assert!(!well_formed(&units("12+")));
        assert_eq!(
            units("*").first().map(|u| class_of(*u)),
            Some(Class::Operator)
        );
        let star = units("*").first().copied().map(Skill::Unit);
        assert_eq!(star.map(|s| focus_admissible(s, false)), Some(false));
        assert_eq!(star.map(|s| carries(Family::Chain, s)), Some(true));
    }
}
