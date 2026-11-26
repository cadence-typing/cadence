//! Policy v3 (#46): it parses, its new fields are bounded, and v2 still
//! parses with them switched off.
use cadence_trainer::policy::parse_policy;

const V2: &str = include_str!("../../../policies/trainer-v2.toml");
const V3: &str = include_str!("../../../policies/trainer-v3.toml");

#[test]
fn v3_parses_with_the_deliberate_practice_fields() {
    let policy = parse_policy(V3.as_bytes()).unwrap();
    let learning = &policy.learning;
    assert_eq!(learning.row_focus_skill_count, 3);
    assert_eq!(learning.numpad_focus_skill_count, 3);
    assert_eq!(learning.numeric_target_share, (2, 3));
    assert_eq!(learning.minimum_exercises_between_word_repeats, 4);
    assert_eq!(learning.numeric_reinforcement_gap_entries, 2);
}

#[test]
fn v2_still_parses_with_the_new_fields_off() {
    let learning = parse_policy(V2.as_bytes()).unwrap().learning;
    assert_eq!(learning.row_focus_skill_count, 0);
    assert_eq!(learning.numpad_focus_skill_count, 0);
    assert_eq!(learning.numeric_target_share, (0, 1));
    assert_eq!(learning.minimum_exercises_between_word_repeats, 0);
    assert_eq!(learning.numeric_reinforcement_gap_entries, 0);
}

#[test]
fn each_new_field_is_bounded() {
    for (from, to) in [
        ("row_focus_skill_count = 3", "row_focus_skill_count = 9"),
        (
            "numpad_focus_skill_count = 3",
            "numpad_focus_skill_count = 9",
        ),
        (
            "numeric_target_share = [2, 3]",
            "numeric_target_share = [4, 3]",
        ),
        (
            "numeric_target_share = [2, 3]",
            "numeric_target_share = [2, 0]",
        ),
        (
            "numeric_target_share = [2, 3]",
            "numeric_target_share = [2]",
        ),
        (
            "minimum_exercises_between_word_repeats = 4",
            "minimum_exercises_between_word_repeats = 101",
        ),
        (
            "numeric_reinforcement_gap_entries = 2",
            "numeric_reinforcement_gap_entries = 101",
        ),
    ] {
        assert!(V3.contains(from), "{from}");
        let changed = V3.replace(from, to);
        assert!(parse_policy(changed.as_bytes()).is_err(), "{to}");
    }
    for key in [
        "row_focus_skill_count = 3\n",
        "numeric_target_share = [2, 3]\n",
        "numeric_reinforcement_gap_entries = 2\n",
    ] {
        assert!(
            parse_policy(V3.replace(key, "").as_bytes()).is_err(),
            "missing {key}"
        );
    }
}
