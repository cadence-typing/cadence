//! The numpad phase over the whole keypad (#101, #103): the policy v5 mix
//! per cycle, operators off, and operator focus skills.
use super::*;
use crate::numeric::{NumericOptions, OPERATORS, contains_skill};
use cadence_core::keypad::Family;
use cadence_core::learning::{
    HistoricSession, KeyLane, LearnerHistory, PlanRecord, PlannerMode, SessionLearning, Skill,
    SkillKey, SkillTally,
};
use cadence_core::{ContentDigest, Layout, SessionId, TrainingChar, UtcDay};

fn v5() -> TrainerPolicy {
    crate::policy::parse_policy(include_bytes!("../../../policies/trainer-v5.toml")).unwrap()
}

fn pack() -> WordPack {
    let entries = (1..=3_000_u64)
        .map(|n| {
            let mut text = String::from("w");
            let mut value = n;
            loop {
                let letter = u8::try_from(value % 26).unwrap();
                text.push(char::from(b'a' + letter));
                value /= 26;
                if value == 0 {
                    break;
                }
            }
            let band = if n <= 2_000 {
                crate::corpus::WordBand::Core
            } else {
                crate::corpus::WordBand::Common
            };
            (n, text, band)
        })
        .collect();
    WordPack::new(entries, ContentDigest::from_bytes([9; 32]), 1, Layout::Us).unwrap()
}

fn prose() -> ProseBank {
    let rows: String = (0..40)
        .map(|n| format!("s\tPassage number {n} has a few words. It ends here.\n"))
        .collect();
    ProseBank::from_tsv(&rows, "s\tA Work\tAn Author\n", 1).unwrap()
}

fn numpad_entries(learner: &LearnerModel, numeric: NumericOptions, seed: u64) -> Vec<String> {
    numpad_plan(learner, numeric, seed)
        .into_iter()
        .map(|exercise| exercise.units.iter().map(|unit| unit.get()).collect())
        .collect()
}

fn numpad_plan(learner: &LearnerModel, numeric: NumericOptions, seed: u64) -> Vec<PlannedExercise> {
    let (words, bank) = (pack(), prose());
    let plan = build_session_plan(
        &v5(),
        PlanInputs {
            pack: &words,
            prose: &bank,
            learner,
            probes: None,
            pace: None,
            numeric,
        },
        seed,
        &[],
        900,
    )
    .unwrap();
    plan.phases
        .into_iter()
        .find(|phase| phase.phase == Phase::Numpad)
        .map(|phase| phase.exercises)
        .unwrap()
}

fn family_of(text: &str) -> Family {
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
        _ if text.contains('.') => Family::Decimal,
        _ => Family::Plain,
    }
}

const OPERATORS_OFF: NumericOptions = NumericOptions {
    operators: false,
    separator: crate::numeric::Separator::Point,
};

const OPERATORS_ON: NumericOptions = NumericOptions {
    operators: true,
    separator: crate::numeric::Separator::Point,
};

/// #103: every whole cycle of numpad entries serves exactly the policy's
/// counts (3 plain, 2 decimal, 1 signed, 3 operation, 1 chain), in an order
/// that differs between seeds.
#[test]
fn each_cycle_serves_the_policy_mix_in_a_seeded_order() {
    let cold = LearnerModel::cold(&v5());
    let entries = numpad_entries(&cold, OPERATORS_ON, 9);
    let cycle = 10;
    assert!(entries.len() >= cycle * 2, "{} entries", entries.len());
    for chunk in entries.chunks_exact(cycle) {
        let count = |family| chunk.iter().filter(|t| family_of(t) == family).count();
        assert_eq!(
            [
                count(Family::Plain),
                count(Family::Decimal),
                count(Family::Signed),
                count(Family::Operation),
                count(Family::Chain),
            ],
            [3, 2, 1, 3, 1],
            "{chunk:?}"
        );
    }
    let other = numpad_entries(&cold, OPERATORS_ON, 10);
    let families = |e: &[String]| {
        e.iter()
            .take(cycle)
            .map(|t| family_of(t))
            .collect::<Vec<_>>()
    };
    assert_ne!(families(&entries), families(&other));
}

/// #103: with operators off, every entry is digits and at most one point.
#[test]
fn operators_off_serves_digits_and_decimals_only() {
    let cold = LearnerModel::cold(&v5());
    let entries = numpad_entries(&cold, OPERATORS_OFF, 9);
    assert!(!entries.is_empty());
    for text in &entries {
        assert!(
            text.chars().all(|c| c.is_ascii_digit() || c == '.'),
            "{text}"
        );
        assert!(text.matches('.').count() <= 1, "{text}");
    }
    for chunk in entries.chunks_exact(5) {
        let plain = chunk
            .iter()
            .filter(|t| family_of(t) == Family::Plain)
            .count();
        assert_eq!(plain, 3, "{chunk:?}");
    }
}

fn history_slow_on(skills: &[(&str, u64, u64)]) -> LearnerHistory {
    let unit = |c| TrainingChar::new(c, Layout::Us).unwrap();
    let mut learning = SessionLearning::new(PlanRecord {
        seed: 1,
        mode: PlannerMode::ColdStart,
        focus: Vec::new(),
    });
    for (text, attempts, errors) in skills {
        let chars: Vec<char> = text.chars().collect();
        let skill = match chars.as_slice() {
            [a] => Skill::Unit(unit(*a)),
            [a, b] => Skill::Bigram(unit(*a), unit(*b)),
            _ => panic!("skill"),
        };
        learning.skills.insert(
            SkillKey {
                lane: KeyLane::Numpad,
                skill,
            },
            SkillTally {
                attempts: *attempts,
                errors: *errors,
                timed: 0,
                timed_micros: 0,
            },
        );
    }
    LearnerHistory {
        sessions: vec![HistoricSession {
            session: SessionId::FIRST,
            day: 10,
            learning,
        }],
    }
}

/// #101: a weak `*` becomes a numpad focus when operators are on, never
/// when they are off; targeted entries carry it only in families that can.
#[test]
fn a_weak_operator_is_targeted_only_when_operators_are_served() {
    let history = history_slow_on(&[("*", 60, 25), ("5", 600, 0), ("8", 600, 0)]);
    let today = UtcDay::from_days_since_epoch(11).unwrap();
    let star = Skill::Unit(TrainingChar::new('*', Layout::Us).unwrap());
    let on = LearnerModel::from_history(&history, &v5(), today, OPERATORS_ON).unwrap();
    let focus = on.focus_in(crate::learner::FocusGroup::Numpad);
    assert!(focus.iter().any(|key| key.skill == star), "{focus:?}");
    let plan = numpad_plan(&on, OPERATORS_ON, 4);
    let mut carried = 0;
    for exercise in &plan {
        let text: String = exercise.units.iter().map(|unit| unit.get()).collect();
        if let Some(key) = exercise.target {
            assert!(contains_skill(&text, key.skill), "{text}");
            assert!(cadence_core::keypad::carries(family_of(&text), key.skill));
            carried += usize::from(key.skill == star);
        }
    }
    assert!(carried > 0);
    let off = LearnerModel::from_history(&history, &v5(), today, OPERATORS_OFF).unwrap();
    assert!(
        off.focus_in(crate::learner::FocusGroup::Numpad)
            .iter()
            .all(|key| key.skill != star)
    );
}
