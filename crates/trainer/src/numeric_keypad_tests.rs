use super::*;
use crate::numeric::{Separator, contains_skill};
use crate::plan::PlanRng;
use cadence_core::alphabet::Layout;
use rand::SeedableRng;

const FAMILIES: [Family; 5] = [
    Family::Plain,
    Family::Decimal,
    Family::Signed,
    Family::Operation,
    Family::Chain,
];

fn shape() -> KeypadPolicy {
    KeypadPolicy {
        weights: keypad::Weights {
            plain: 3,
            decimal: 2,
            signed: 1,
            operation: 3,
            chain: 1,
        },
        minimum_operand_digits: 1,
        maximum_operand_digits: 4,
    }
}

fn text_of(entry: &KeypadEntry) -> String {
    entry.units().iter().map(|unit| unit.get()).collect()
}

/// The family an entry's text belongs to, read from its characters.
fn family_of(text: &str, separator: char) -> Family {
    let signed = text.starts_with(['+', '-']);
    let operators = text
        .chars()
        .skip(usize::from(signed))
        .filter(|c| OPERATORS.contains(c))
        .count();
    match (signed, operators) {
        (true, _) => Family::Signed,
        (false, 1) => Family::Operation,
        (false, 2) => Family::Chain,
        _ if text.contains(separator) => Family::Decimal,
        _ => Family::Plain,
    }
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

/// #103: every family builds entries of that family, accepted by the
/// keypad grammar, with the configured separator only.
#[test]
fn each_family_builds_grammatical_entries_of_its_own_shape() {
    for separator in [Separator::Point, Separator::Comma] {
        let options = NumericOptions {
            operators: true,
            separator,
        };
        let other = if separator == Separator::Point {
            ','
        } else {
            '.'
        };
        for family in FAMILIES {
            let mut rng = PlanRng::seed_from_u64(11);
            let mut used = HashSet::new();
            for _ in 0..300 {
                let entry =
                    numpad_exercise(&mut rng, &mut used, family, None, options, &shape()).unwrap();
                let text = text_of(&entry);
                assert!(keypad::well_formed(entry.units()), "{text}");
                assert!(!text.contains(other), "{text}");
                assert_eq!(family_of(&text, separator.char()), family, "{text}");
            }
        }
    }
}

/// #103: leading zeros and decimals still occur, every keypad operator is
/// served, and operands stay within the policy's digit bounds.
#[test]
fn the_whole_keypad_is_served() {
    let mut rng = PlanRng::seed_from_u64(5);
    let mut used = HashSet::new();
    let mut seen = HashSet::new();
    let options = NumericOptions {
        operators: true,
        separator: Separator::Point,
    };
    for family in FAMILIES {
        for _ in 0..200 {
            let entry =
                numpad_exercise(&mut rng, &mut used, family, None, options, &shape()).unwrap();
            let text = text_of(&entry);
            seen.extend(text.chars());
            if matches!(family, Family::Operation | Family::Chain) {
                for operand in text.split(OPERATORS) {
                    let whole = operand.split('.').next().unwrap_or("");
                    assert!((1..=4).contains(&whole.len()), "{text}");
                }
            }
        }
    }
    for c in "0123456789.+-*/".chars() {
        assert!(seen.contains(&c), "{c} never served");
    }
}

/// #103: a focus skill sits in every entry built around it, for every
/// family that carries it.
#[test]
fn targeted_entries_contain_their_focus() {
    let options = NumericOptions {
        operators: true,
        separator: Separator::Point,
    };
    let mut rng = PlanRng::seed_from_u64(7);
    let mut used = HashSet::new();
    let skills = [
        "7", "48", "00", ".", "4.", ".5", "+", "-", "*", "/", "7+", "*4", "-1", "5/",
    ];
    for text in skills {
        let focus = skill(text);
        for family in FAMILIES {
            if !keypad::carries(family, focus) {
                continue;
            }
            for _ in 0..30 {
                let entry =
                    numpad_exercise(&mut rng, &mut used, family, Some(focus), options, &shape())
                        .unwrap();
                let built = text_of(&entry);
                assert!(contains_skill(&built, focus), "{text} in {built}");
                assert_eq!(family_of(&built, '.'), family, "{built}");
            }
        }
    }
    // A comma separator focus is targeted in comma form.
    let comma = NumericOptions {
        operators: false,
        separator: Separator::Comma,
    };
    let entry = numpad_exercise(
        &mut rng,
        &mut used,
        Family::Decimal,
        Some(skill(",")),
        comma,
        &shape(),
    )
    .unwrap();
    assert!(text_of(&entry).contains(','));
}

/// #103: a focus the family cannot carry is refused, never served untargeted.
#[test]
fn a_focus_the_family_cannot_carry_is_refused() {
    let options = NumericOptions::default();
    for (family, focus) in [
        (Family::Plain, "*"),
        (Family::Plain, "."),
        (Family::Decimal, "+"),
        (Family::Signed, "*"),
        (Family::Operation, "**"),
    ] {
        assert_eq!(
            numpad_exercise(
                &mut PlanRng::seed_from_u64(1),
                &mut HashSet::new(),
                family,
                Some(skill(focus)),
                options,
                &shape(),
            ),
            Err(NumericError::FocusNotCarried),
            "{focus} in {family:?}"
        );
    }
}

#[test]
fn only_grammatical_entries_are_admitted() {
    let units = |text: &str| crate::numeric::units(text).unwrap();
    assert!(KeypadEntry::new(units("-005.20")).is_ok());
    for bad in ["12+", "1..2", "3*-2", ".5", ""] {
        assert_eq!(
            KeypadEntry::new(units(bad)),
            Err(NumericError::Malformed),
            "{bad}"
        );
    }
}

/// The same seed, family and focus give the same entries.
#[test]
fn entries_replay() {
    let run = |seed| {
        let mut rng = PlanRng::seed_from_u64(seed);
        let mut used = HashSet::new();
        FAMILIES
            .iter()
            .cycle()
            .take(20)
            .map(|family| {
                numpad_exercise(
                    &mut rng,
                    &mut used,
                    *family,
                    Some(skill("4")),
                    NumericOptions::default(),
                    &shape(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(run(3), run(3));
    assert_ne!(run(3), run(4));
}
