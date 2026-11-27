//! Numpad entries over the whole keypad (#101, #103): five families
//! (`Plain`, `Decimal`, `Signed`, `Operation`, `Chain`) built from digits,
//! the decimal separator and `+ - * /`. Which family an entry belongs to is
//! decided by the proved `keypad::family_at` in `plan_content`; every entry
//! passes the proved grammar `keypad::well_formed` in [`KeypadEntry::new`]
//! before it can reach a plan. Nothing is evaluated: entries are strings.
use std::collections::HashSet;

use cadence_core::alphabet::TrainingChar;
use cadence_core::keypad::{self, Class, Family};
use cadence_core::learning::Skill;

use crate::keypad_policy::KeypadPolicy;
use crate::numeric::{
    DigitSource, NumericError, NumericOptions, OPERATORS, count, digits, embed, skill_text, units,
};

/// Attempts at a fresh entry before preparation reports exhaustion.
const DISTINCT_ENTRY_SCAN: usize = 64;
/// Digits after the point in a `Decimal` entry (prices, amounts).
const DECIMAL_FRACTION_DIGITS: usize = 2;
/// One operand in this many of an operation or chain is a decimal.
const DECIMAL_OPERAND_ONE_IN: u64 = 3;

/// A numpad entry that is `sign? operand (operator operand)*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeypadEntry(Vec<TrainingChar>);

impl KeypadEntry {
    /// Admits `text` only when the keypad grammar accepts it.
    ///
    /// # Errors
    /// [`NumericError::Malformed`] for anything else.
    pub fn new(text: Vec<TrainingChar>) -> Result<Self, NumericError> {
        if keypad::well_formed(&text) {
            Ok(Self(text))
        } else {
            Err(NumericError::Malformed)
        }
    }

    /// The entry's units.
    #[must_use]
    pub fn units(&self) -> &[TrainingChar] {
        &self.0
    }

    /// The entry's units, owned.
    #[must_use]
    pub fn into_units(self) -> Vec<TrainingChar> {
        self.0
    }
}

/// One operand: whole digits and an optional fraction.
#[derive(Debug, Clone)]
struct Operand {
    whole: String,
    fraction: Option<String>,
}

/// An entry under construction, written with `.`.
#[derive(Debug, Clone)]
struct Draft {
    sign: Option<char>,
    operands: Vec<Operand>,
    operators: Vec<char>,
}

impl Draft {
    fn single(whole: String, fraction: Option<String>) -> Self {
        Self {
            sign: None,
            operands: vec![Operand { whole, fraction }],
            operators: Vec::new(),
        }
    }

    fn text(&self) -> String {
        let mut text = String::new();
        if let Some(sign) = self.sign {
            text.push(sign);
        }
        for (index, operand) in self.operands.iter().enumerate() {
            if let Some(operator) = index
                .checked_sub(1)
                .and_then(|before| self.operators.get(before))
            {
                text.push(*operator);
            }
            text.push_str(&operand.whole);
            if let Some(fraction) = &operand.fraction {
                text.push('.');
                text.push_str(fraction);
            }
        }
        text
    }
}

fn pick<T: Copy>(rng: &mut dyn DigitSource, items: &[T]) -> Result<T, NumericError> {
    let bound = u64::try_from(items.len()).map_err(|_too_many| NumericError::InvalidCharacter)?;
    let index = usize::try_from(rng.below(bound))
        .map_err(|_unrepresentable| NumericError::InvalidCharacter)?;
    items
        .get(index)
        .copied()
        .ok_or(NumericError::InvalidCharacter)
}

fn operator(rng: &mut dyn DigitSource) -> Result<char, NumericError> {
    pick(rng, &OPERATORS)
}

/// A digit group: a run of 2–6, one digit repeated 3–6 times, or a pair twice.
fn plain(rng: &mut dyn DigitSource) -> Result<Draft, NumericError> {
    let whole = match rng.below(3) {
        0 => {
            let length = count(rng, 2, 5)?;
            digits(rng, length)?
        }
        1 => {
            let digit = digits(rng, 1)?;
            let length = count(rng, 3, 4)?;
            digit.repeat(length)
        }
        _ => digits(rng, 2)?.repeat(2),
    };
    Ok(Draft::single(whole, None))
}

/// A whole of 1–4 digits or `00` and three digits, then a two-digit fraction.
fn decimal(rng: &mut dyn DigitSource) -> Result<Draft, NumericError> {
    let whole = if rng.below(2) == 0 {
        let length = count(rng, 1, 4)?;
        digits(rng, length)?
    } else {
        format!("00{}", digits(rng, 3)?)
    };
    let fraction = digits(rng, DECIMAL_FRACTION_DIGITS)?;
    Ok(Draft::single(whole, Some(fraction)))
}

/// `+` or `-`, then 1–4 digits or a decimal.
fn signed(rng: &mut dyn DigitSource) -> Result<Draft, NumericError> {
    let sign = pick(rng, &['+', '-'])?;
    let length = count(rng, 1, 4)?;
    let whole = digits(rng, length)?;
    let fraction = if rng.below(2) == 0 {
        Some(digits(rng, DECIMAL_FRACTION_DIGITS)?)
    } else {
        None
    };
    let mut draft = Draft::single(whole, fraction);
    draft.sign = Some(sign);
    Ok(draft)
}

/// An operand of the policy's length, one in three with a 1–2 digit fraction.
fn operand(rng: &mut dyn DigitSource, shape: &KeypadPolicy) -> Result<Operand, NumericError> {
    let span = shape
        .maximum_operand_digits
        .checked_sub(shape.minimum_operand_digits)
        .and_then(|d| d.checked_add(1))
        .and_then(|d| u64::try_from(d).ok())
        .ok_or(NumericError::InvalidCharacter)?;
    let length = count(rng, shape.minimum_operand_digits, span)?;
    let whole = digits(rng, length)?;
    let fraction = if rng.below(DECIMAL_OPERAND_ONE_IN) == 0 {
        let length = count(rng, 1, 2)?;
        Some(digits(rng, length)?)
    } else {
        None
    };
    Ok(Operand { whole, fraction })
}

/// `operands` operands joined by seeded operators.
fn operated(
    rng: &mut dyn DigitSource,
    shape: &KeypadPolicy,
    operands: usize,
) -> Result<Draft, NumericError> {
    let mut draft = Draft {
        sign: None,
        operands: Vec::with_capacity(operands),
        operators: Vec::with_capacity(operands.saturating_sub(1)),
    };
    for index in 0..operands {
        if index > 0 {
            draft.operators.push(operator(rng)?);
        }
        draft.operands.push(operand(rng, shape)?);
    }
    Ok(draft)
}

fn build(
    rng: &mut dyn DigitSource,
    family: Family,
    shape: &KeypadPolicy,
) -> Result<Draft, NumericError> {
    match family {
        Family::Plain => plain(rng),
        Family::Decimal => decimal(rng),
        Family::Signed => signed(rng),
        Family::Operation => operated(rng, shape, 2),
        Family::Chain => operated(rng, shape, 3),
    }
}

/// Replaces the last character of `text` with `c` (or makes it `c`).
fn set_last(text: &mut String, c: char) {
    text.pop();
    text.push(c);
}

/// Replaces the first character of `text` with `c` (or makes it `c`).
fn set_first(text: &mut String, c: char) {
    let rest: String = text.chars().skip(1).collect();
    *text = format!("{c}{rest}");
}

fn ensure_fraction(rng: &mut dyn DigitSource, operand: &mut Operand) -> Result<(), NumericError> {
    if operand.fraction.is_none() {
        operand.fraction = Some(digits(rng, DECIMAL_FRACTION_DIGITS)?);
    }
    Ok(())
}

/// Puts the focus skill into `draft`, whose family carries it
/// (`keypad::carries`): digits into the first operand, a separator into a
/// decimal operand, an operator as the first operator or the sign.
fn embed_focus(
    rng: &mut dyn DigitSource,
    draft: &mut Draft,
    family: Family,
    skill: Skill,
) -> Result<(), NumericError> {
    let text = skill_text(skill);
    let chars: Vec<char> = text.chars().collect();
    let class = |c: char| match TrainingChar::new(c, cadence_core::alphabet::Layout::Us) {
        Ok(unit) => keypad::class_of(unit),
        Err(_unrepresentable) => Class::Outside,
    };
    let operated = matches!(family, Family::Operation | Family::Chain);
    let (first, rest) = draft
        .operands
        .split_first_mut()
        .ok_or(NumericError::InvalidCharacter)?;
    match chars.as_slice() {
        [a] => match class(*a) {
            Class::Digit => first.whole = embed(rng, &first.whole, &text)?,
            Class::Separator => ensure_fraction(rng, first)?,
            Class::Operator if operated => {
                let slot = draft
                    .operators
                    .first_mut()
                    .ok_or(NumericError::FocusNotCarried)?;
                *slot = *a;
            }
            Class::Operator => draft.sign = Some(*a),
            Class::Outside => return Err(NumericError::FocusNotCarried),
        },
        [a, b] => match (class(*a), class(*b)) {
            (Class::Digit, Class::Digit) => first.whole = embed(rng, &first.whole, &text)?,
            (Class::Digit, Class::Separator) => {
                set_last(&mut first.whole, *a);
                ensure_fraction(rng, first)?;
            }
            (Class::Separator, Class::Digit) => {
                ensure_fraction(rng, first)?;
                if let Some(fraction) = first.fraction.as_mut() {
                    set_first(fraction, *b);
                }
            }
            (Class::Digit, Class::Operator) => {
                match first.fraction.as_mut() {
                    Some(fraction) => set_last(fraction, *a),
                    None => set_last(&mut first.whole, *a),
                }
                let slot = draft
                    .operators
                    .first_mut()
                    .ok_or(NumericError::FocusNotCarried)?;
                *slot = *b;
            }
            (Class::Operator, Class::Digit) if operated => {
                let second = rest.first_mut().ok_or(NumericError::FocusNotCarried)?;
                set_first(&mut second.whole, *b);
                let slot = draft
                    .operators
                    .first_mut()
                    .ok_or(NumericError::FocusNotCarried)?;
                *slot = *a;
            }
            (Class::Operator, Class::Digit) => {
                draft.sign = Some(*a);
                set_first(&mut first.whole, *b);
            }
            _ => return Err(NumericError::FocusNotCarried),
        },
        _ => return Err(NumericError::FocusNotCarried),
    }
    Ok(())
}

/// One numpad entry of `family`, built around `focus` when given (the
/// caller passes only a focus the family carries). Leading zeros and
/// fractional trailing zeros are significant target units.
///
/// # Errors
/// Invalid entropy, a focus the family cannot carry, a malformed entry
/// (a builder bug, refused rather than served), or bounded distinct-content
/// exhaustion, never silently substituting a repeat for fresh content.
pub fn numpad_exercise(
    rng: &mut dyn DigitSource,
    used: &mut HashSet<String>,
    family: Family,
    focus: Option<Skill>,
    options: NumericOptions,
    shape: &KeypadPolicy,
) -> Result<KeypadEntry, NumericError> {
    if let Some(skill) = focus
        && !keypad::carries(family, skill)
    {
        return Err(NumericError::FocusNotCarried);
    }
    for _ in 0..DISTINCT_ENTRY_SCAN {
        let mut draft = build(rng, family, shape)?;
        if let Some(skill) = focus {
            embed_focus(rng, &mut draft, family, skill)?;
        }
        let text = draft
            .text()
            .replace('.', &options.separator.char().to_string());
        if !used.contains(&text) {
            let entry = KeypadEntry::new(units(&text)?)?;
            used.insert(text);
            return Ok(entry);
        }
    }
    Err(NumericError::Exhausted)
}

#[cfg(test)]
#[path = "numeric_keypad_tests.rs"]
mod tests;
