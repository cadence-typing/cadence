//! The `[keypad]` policy section (policy v5, #103): how many entries of each
//! keypad family one cycle of a numpad phase serves, and how long an operand
//! of an operation or a chain is. Policies v1 to v4 have no table and parse
//! to the mix that keeps their digits-and-decimals content (3 plain : 2
//! decimal), with operations when operators are switched on.
use cadence_core::keypad::Weights;

use crate::policy::PolicyError;

/// Longest cycle of numpad entries: keeps the per-cycle permutation small.
pub const MAXIMUM_KEYPAD_CYCLE: u64 = 20;
/// Longest operand of an operation or a chain, in digits.
pub const MAXIMUM_OPERAND_DIGITS: usize = 6;

/// Validated numpad content mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeypadPolicy {
    /// Entries of each family in one cycle.
    pub weights: Weights,
    /// Fewest digits in an operand of an operation or a chain.
    pub minimum_operand_digits: usize,
    /// Most digits in an operand of an operation or a chain.
    pub maximum_operand_digits: usize,
}

impl KeypadPolicy {
    /// The cycle length: the sum of the weights (validated to fit).
    #[must_use]
    pub fn cycle(&self) -> u64 {
        let w = self.weights;
        [w.decimal, w.signed, w.operation, w.chain]
            .iter()
            .fold(w.plain, |sum, weight| sum.saturating_add(*weight))
    }
}

/// The mix of policies without a `[keypad]` table.
const LEGACY: KeypadPolicy = KeypadPolicy {
    weights: Weights {
        plain: 3,
        decimal: 2,
        signed: 0,
        operation: 5,
        chain: 0,
    },
    minimum_operand_digits: 1,
    maximum_operand_digits: 4,
};

fn integer<T: TryFrom<i64>>(table: &toml::Table, key: &str) -> Result<T, PolicyError> {
    let raw = table
        .get(key)
        .ok_or_else(|| PolicyError::MissingKey(key.into()))?
        .as_integer()
        .ok_or_else(|| PolicyError::WrongType(key.into()))?;
    T::try_from(raw).map_err(|_unrepresentable| PolicyError::OutOfRange {
        key: key.into(),
        reason: "integer outside representable range",
    })
}

/// Decodes `[keypad]` (required from v5).
///
/// # Errors
/// Missing, mistyped or out-of-range values.
pub(crate) fn parse_keypad(
    document: &toml::Table,
    version: u8,
) -> Result<KeypadPolicy, PolicyError> {
    if version < 5 {
        return Ok(LEGACY);
    }
    let t = document
        .get("keypad")
        .and_then(toml::Value::as_table)
        .ok_or(PolicyError::MissingTable("keypad"))?;
    let policy = KeypadPolicy {
        weights: Weights {
            plain: integer(t, "plain_entries_per_cycle")?,
            decimal: integer(t, "decimal_entries_per_cycle")?,
            signed: integer(t, "signed_entries_per_cycle")?,
            operation: integer(t, "operation_entries_per_cycle")?,
            chain: integer(t, "chain_entries_per_cycle")?,
        },
        minimum_operand_digits: integer(t, "minimum_operand_digits")?,
        maximum_operand_digits: integer(t, "maximum_operand_digits")?,
    };
    let w = policy.weights;
    let each_fits = [w.plain, w.decimal, w.signed, w.operation, w.chain]
        .iter()
        .all(|weight| *weight <= MAXIMUM_KEYPAD_CYCLE);
    let checks = [
        (
            each_fits && (1..=MAXIMUM_KEYPAD_CYCLE).contains(&policy.cycle()),
            "keypad entries per cycle must total 1..=20",
        ),
        (
            w.plain.saturating_add(w.decimal) > 0,
            "plain_entries_per_cycle + decimal_entries_per_cycle must be positive, so operators off still serves entries",
        ),
        (
            (1..=MAXIMUM_OPERAND_DIGITS).contains(&policy.minimum_operand_digits)
                && (policy.minimum_operand_digits..=MAXIMUM_OPERAND_DIGITS)
                    .contains(&policy.maximum_operand_digits),
            "operand digits must be 1..=6 with minimum <= maximum",
        ),
    ];
    for (ok, what) in checks {
        if !ok {
            return Err(PolicyError::Inconsistent { what });
        }
    }
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V5: &str = include_str!("../../../policies/trainer-v5.toml");

    fn with(edit: (&str, &str)) -> Result<crate::policy::TrainerPolicy, PolicyError> {
        crate::policy::parse_policy(V5.replace(edit.0, edit.1).as_bytes())
    }

    #[test]
    fn v5_ships_the_documented_mix_and_older_policies_keep_digits_first() {
        let v5 = with(("", "")).unwrap();
        assert_eq!(v5.keypad.cycle(), 10);
        assert_eq!(v5.keypad.weights.operation, 3);
        assert_eq!(v5.learning.numpad_focus_skill_count, 4);
        let v4 = crate::policy::parse_policy(include_bytes!("../../../policies/trainer-v4.toml"))
            .unwrap();
        assert_eq!(v4.keypad, LEGACY);
    }

    #[test]
    fn a_bad_keypad_table_is_refused_by_name() {
        for edit in [
            (
                "plain_entries_per_cycle = 3",
                "plain_entries_per_cycle = 30",
            ),
            (
                "plain_entries_per_cycle = 3",
                "plain_entries_per_cycle = 0\nx = 0",
            ),
            ("maximum_operand_digits = 4", "maximum_operand_digits = 7"),
            ("minimum_operand_digits = 1", "minimum_operand_digits = 5"),
        ] {
            let edited = if edit.1.contains("x = 0") {
                V5.replace(edit.0, "plain_entries_per_cycle = 0").replace(
                    "decimal_entries_per_cycle = 2",
                    "decimal_entries_per_cycle = 0",
                )
            } else {
                V5.replace(edit.0, edit.1)
            };
            assert!(
                matches!(
                    crate::policy::parse_policy(edited.as_bytes()),
                    Err(PolicyError::Inconsistent { .. })
                ),
                "{edit:?}"
            );
        }
        assert_eq!(
            with(("[keypad]", "[keypad_typo]")),
            Err(PolicyError::MissingTable("keypad"))
        );
    }
}
