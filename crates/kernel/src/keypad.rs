//! The numeric keypad: its alphabet, the shape of an entry, and which entry
//! family a numpad slot serves (#101, #102; RFC
//! `2026-11-27-full-numeric-keypad-practice`).
//!
//! `class` is the single definition of the keypad alphabet;
//! `core::alphabet::TrainingChar::is_digit` and `trainer::learner::group_of`
//! call it. `well_formed` gates every entry the trainer serves
//! (`trainer::numeric::KeypadEntry::new`); `family_at` schedules a phase's
//! entries (`trainer::plan_content`); `carries` and `focus_admissible`
//! decide which focus skills an entry or a session can serve.
//!
//! Codes are Unicode scalar values (`char as u32` at the caller).

/// `0`.
pub const DIGIT_ZERO: u32 = 0x30;
/// `9`.
pub const DIGIT_NINE: u32 = 0x39;
/// `.`, the default decimal separator.
pub const POINT: u32 = 0x2e;
/// `,`, the configurable decimal separator.
pub const COMMA: u32 = 0x2c;
/// `+`.
pub const PLUS: u32 = 0x2b;
/// `-`.
pub const MINUS: u32 = 0x2d;
/// `*`.
pub const TIMES: u32 = 0x2a;
/// `/`.
pub const DIVIDE: u32 = 0x2f;

/// What a character is on the keypad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// `0` to `9`.
    Digit,
    /// `.` or `,`.
    Separator,
    /// `+ - * /`.
    Operator,
    /// Not a keypad character key.
    Outside,
}

/// The keypad class of a code.
#[must_use]
pub fn class(code: u32) -> Class {
    if code < DIGIT_ZERO {
        class_other(code)
    } else if code <= DIGIT_NINE {
        Class::Digit
    } else {
        class_other(code)
    }
}

/// The keypad class of a code that is not a digit.
#[must_use]
pub fn class_other(code: u32) -> Class {
    if code == POINT || code == COMMA {
        Class::Separator
    } else if code == PLUS || code == MINUS || code == TIMES || code == DIVIDE {
        Class::Operator
    } else {
        Class::Outside
    }
}

/// Whether a code is `+` or `-`, the operators that may lead an entry.
#[must_use]
pub fn is_sign(code: u32) -> bool {
    code == PLUS || code == MINUS
}

/// Where a partly read entry is in `sign? operand (operator operand)*`,
/// `operand = digits (separator digits)?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing read.
    Start,
    /// A leading sign.
    Signed,
    /// Digits of an operand's whole part.
    Whole,
    /// A separator, no fraction digit yet.
    Point,
    /// Digits after the separator.
    Fraction,
    /// An operator between operands.
    Operator,
    /// Not a prefix of any entry.
    Reject,
}

/// The state after one key of class `class`; `sign` says whether an
/// operator key is `+` or `-`.
#[must_use]
pub fn step(state: State, class: Class, sign: bool) -> State {
    match state {
        State::Start => match class {
            Class::Digit => State::Whole,
            Class::Operator => {
                if sign {
                    State::Signed
                } else {
                    State::Reject
                }
            }
            Class::Separator => State::Reject,
            Class::Outside => State::Reject,
        },
        State::Signed => match class {
            Class::Digit => State::Whole,
            Class::Separator => State::Reject,
            Class::Operator => State::Reject,
            Class::Outside => State::Reject,
        },
        State::Whole => match class {
            Class::Digit => State::Whole,
            Class::Separator => State::Point,
            Class::Operator => State::Operator,
            Class::Outside => State::Reject,
        },
        State::Point => match class {
            Class::Digit => State::Fraction,
            Class::Separator => State::Reject,
            Class::Operator => State::Reject,
            Class::Outside => State::Reject,
        },
        State::Fraction => match class {
            Class::Digit => State::Fraction,
            Class::Separator => State::Reject,
            Class::Operator => State::Operator,
            Class::Outside => State::Reject,
        },
        State::Operator => match class {
            Class::Digit => State::Whole,
            Class::Separator => State::Reject,
            Class::Operator => State::Reject,
            Class::Outside => State::Reject,
        },
        State::Reject => State::Reject,
    }
}

/// Whether a whole entry may end in `state`: only after a digit.
#[must_use]
pub fn accepting(state: State) -> bool {
    match state {
        State::Whole => true,
        State::Fraction => true,
        State::Start => false,
        State::Signed => false,
        State::Point => false,
        State::Operator => false,
        State::Reject => false,
    }
}

/// The state after reading `codes` from position `i` in state `state`.
#[must_use]
pub fn run_from(codes: &[u32], i: usize, state: State) -> State {
    let mut i = i;
    let mut state = state;
    while i < codes.len() {
        if let Some(code) = codes.get(i) {
            state = step(state, class(*code), is_sign(*code));
        }
        i = i.saturating_add(1);
    }
    state
}

/// Whether `codes` is a keypad entry. The empty string is not.
#[must_use]
pub fn well_formed(codes: &[u32]) -> bool {
    accepting(run_from(codes, 0, State::Start))
}

/// The shape of a numpad entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// A digit group: `4821`, `3333`, `5656`.
    Plain,
    /// A whole and a fraction: `07.50`.
    Decimal,
    /// A leading `+` or `-`: `-45.20`.
    Signed,
    /// Two operands, one operator: `12+7`.
    Operation,
    /// Three operands, two operators: `6*3/2`.
    Chain,
}

/// Entries of each family in one cycle of a numpad phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Weights {
    /// `Plain` entries per cycle.
    pub plain: u64,
    /// `Decimal` entries per cycle.
    pub decimal: u64,
    /// `Signed` entries per cycle.
    pub signed: u64,
    /// `Operation` entries per cycle.
    pub operation: u64,
    /// `Chain` entries per cycle.
    pub chain: u64,
}

/// The weights a session may serve: operator families count only when
/// `operators` is on.
#[must_use]
pub fn admissible(weights: Weights, operators: bool) -> Weights {
    if operators {
        weights
    } else {
        Weights {
            plain: weights.plain,
            decimal: weights.decimal,
            signed: 0,
            operation: 0,
            chain: 0,
        }
    }
}

/// The cycle length of `w`, or `None` when it does not fit in `u64`.
#[must_use]
pub fn cycle(w: Weights) -> Option<u64> {
    match w.plain.checked_add(w.decimal) {
        None => None,
        Some(a) => match a.checked_add(w.signed) {
            None => None,
            Some(b) => match b.checked_add(w.operation) {
                None => None,
                Some(c) => c.checked_add(w.chain),
            },
        },
    }
}

/// The family of the entry at `position` of a cycle: with
/// `r = position mod cycle`, the family whose run of consecutive positions
/// (in the order plain, decimal, signed, operation, chain) holds `r`. So a
/// whole cycle serves each family exactly its weight. An empty or
/// unrepresentable mix serves `Plain`.
#[must_use]
pub fn family_at(position: u64, weights: Weights, operators: bool) -> Family {
    let w = admissible(weights, operators);
    let total = match cycle(w) {
        None => return Family::Plain,
        Some(t) => t,
    };
    let r = match position.checked_rem(total) {
        None => return Family::Plain,
        Some(r) => r,
    };
    if r < w.plain {
        return Family::Plain;
    }
    let r = r.saturating_sub(w.plain);
    if r < w.decimal {
        return Family::Decimal;
    }
    let r = r.saturating_sub(w.decimal);
    if r < w.signed {
        return Family::Signed;
    }
    let r = r.saturating_sub(w.signed);
    if r < w.operation {
        return Family::Operation;
    }
    Family::Chain
}

/// Whether an entry of `family` can carry the focus skill made of the key
/// `first` (and, when `pair`, the following key `second`), as built by the
/// trainer: digits go anywhere; a separator needs a decimal operand (not
/// `Plain`); an operator needs an operation or a chain, or for `+`/`-` a
/// signed entry; a pair must be adjacent in some entry of that family.
#[must_use]
pub fn carries(family: Family, first: u32, second: u32, pair: bool) -> bool {
    let operated = match family {
        Family::Operation => true,
        Family::Chain => true,
        Family::Plain => false,
        Family::Decimal => false,
        Family::Signed => false,
    };
    let signed = match family {
        Family::Signed => true,
        Family::Plain => false,
        Family::Decimal => false,
        Family::Operation => false,
        Family::Chain => false,
    };
    let fractional = match family {
        Family::Plain => false,
        Family::Decimal => true,
        Family::Signed => true,
        Family::Operation => true,
        Family::Chain => true,
    };
    let a = class(first);
    if !pair {
        return match a {
            Class::Digit => true,
            Class::Separator => fractional,
            Class::Operator => operated || (signed && is_sign(first)),
            Class::Outside => false,
        };
    }
    match a {
        Class::Digit => match class(second) {
            Class::Digit => true,
            Class::Separator => fractional,
            Class::Operator => operated,
            Class::Outside => false,
        },
        Class::Separator => match class(second) {
            Class::Digit => fractional,
            Class::Separator => false,
            Class::Operator => false,
            Class::Outside => false,
        },
        Class::Operator => match class(second) {
            Class::Digit => operated || (signed && is_sign(first)),
            Class::Separator => false,
            Class::Operator => false,
            Class::Outside => false,
        },
        Class::Outside => false,
    }
}

/// Whether a numpad-lane skill may become a numpad focus: some family the
/// session may serve carries it. `Decimal` carries every skill without an
/// operator that any family carries, and `Operation` every skill any
/// family carries.
#[must_use]
pub fn focus_admissible(first: u32, second: u32, pair: bool, operators: bool) -> bool {
    carries(Family::Decimal, first, second, pair)
        || (operators && carries(Family::Operation, first, second, pair))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(s: &str) -> [u32; 16] {
        let mut out = [0u32; 16];
        for (slot, c) in out.iter_mut().zip(s.chars()) {
            *slot = u32::from(c);
        }
        out
    }

    fn entry(s: &str) -> bool {
        let all = codes(s);
        let n = s.chars().count();
        all.get(..n).is_some_and(well_formed)
    }

    #[test]
    fn grammar_accepts_entries_and_refuses_fragments() {
        for ok in ["0", "005.20", "-12.5", "+7", "12+7", "4.5*2/3", "84/2,5"] {
            assert!(entry(ok), "{ok}");
        }
        for bad in [
            "", "12+", "1..2", ".5", "5.", "3*-2", "*3", "1 2", "+", "1.2.3",
        ] {
            assert!(!entry(bad), "{bad}");
        }
    }

    #[test]
    fn classes_are_the_keypad_keys() {
        assert_eq!(class(u32::from('7')), Class::Digit);
        assert_eq!(class(u32::from(',')), Class::Separator);
        for op in ['+', '-', '*', '/'] {
            assert_eq!(class(u32::from(op)), Class::Operator);
        }
        assert_eq!(class(u32::from('=')), Class::Outside);
    }

    #[test]
    fn a_whole_cycle_serves_each_family_its_weight() {
        let w = Weights {
            plain: 3,
            decimal: 2,
            signed: 1,
            operation: 3,
            chain: 1,
        };
        let mut counts = [0u32; 5];
        for p in 0..20 {
            let i = match family_at(p, w, true) {
                Family::Plain => 0,
                Family::Decimal => 1,
                Family::Signed => 2,
                Family::Operation => 3,
                Family::Chain => 4,
            };
            if let Some(c) = counts.get_mut(i) {
                *c += 1;
            }
        }
        assert_eq!(counts, [6, 4, 2, 6, 2]);
        for p in 0..20 {
            assert!(matches!(
                family_at(p, w, false),
                Family::Plain | Family::Decimal
            ));
        }
        let empty = Weights {
            plain: 0,
            decimal: 0,
            ..w
        };
        assert_eq!(family_at(4, empty, false), Family::Plain);
    }

    #[test]
    fn focus_follows_operators_and_families() {
        let star = u32::from('*');
        let four = u32::from('4');
        let minus = u32::from('-');
        assert!(!focus_admissible(star, 0, false, false));
        assert!(focus_admissible(star, 0, false, true));
        assert!(focus_admissible(star, four, true, true));
        assert!(!carries(Family::Plain, star, 0, false));
        assert!(!carries(Family::Signed, star, 0, false));
        assert!(carries(Family::Signed, minus, four, true));
        assert!(!carries(Family::Plain, u32::from('.'), 0, false));
        assert!(!focus_admissible(star, star, true, true));
    }
}
