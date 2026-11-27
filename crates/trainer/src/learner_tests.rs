use super::*;
use cadence_core::Layout;
use cadence_core::SessionId;
use cadence_core::learning::{PlanRecord, SessionLearning, SkillTally, WordTally};

fn policy() -> TrainerPolicy {
    crate::policy::parse_policy(include_bytes!("../../../policies/trainer-v2.toml")).unwrap()
}

fn unit(character: char) -> TrainingChar {
    TrainingChar::new(character, Layout::Us).unwrap()
}

fn bigram(pair: &str) -> SkillKey {
    let mut chars = pair.chars();
    SkillKey {
        lane: KeyLane::Main,
        skill: Skill::Bigram(unit(chars.next().unwrap()), unit(chars.next().unwrap())),
    }
}

fn tally(attempts: u64, errors: u64, timed: u64, mean_ms: u64) -> SkillTally {
    SkillTally {
        attempts,
        errors,
        timed,
        timed_micros: timed * mean_ms * 1_000,
    }
}

fn session(id: u64, day: i64, skills: &[(&str, SkillTally)]) -> HistoricSession {
    let mut learning = SessionLearning::new(PlanRecord {
        seed: id,
        mode: PlannerMode::ColdStart,
        focus: Vec::new(),
    });
    for (pair, value) in skills {
        learning.skills.insert(bigram(pair), *value);
    }
    HistoricSession {
        session: SessionId::new(id).unwrap(),
        day,
        learning,
    }
}

fn day(value: i64) -> UtcDay {
    UtcDay::from_days_since_epoch(value).unwrap()
}

#[test]
fn empty_history_is_cold_start() {
    let model = LearnerModel::from_history(
        &LearnerHistory::default(),
        &policy(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert_eq!(model.mode(), PlannerMode::ColdStart);
    assert!(model.focus().is_empty());
    let unseen = model.score(bigram("qz"));
    assert_eq!(unseen.uncertainty, BasisPoints::MAX);
}

#[test]
fn frequent_errors_become_the_first_focus() {
    let history = LearnerHistory {
        sessions: vec![session(
            1,
            10,
            &[
                ("th", tally(40, 12, 0, 0)),
                ("er", tally(40, 1, 0, 0)),
                ("an", tally(40, 0, 0, 0)),
            ],
        )],
    };
    let model = LearnerModel::from_history(
        &history,
        &policy(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert_eq!(model.mode(), PlannerMode::Adaptive);
    assert_eq!(model.focus().first(), Some(&bigram("th")));
    assert!(!model.focus().contains(&bigram("an")));
    assert!(model.score(bigram("th")).error > model.score(bigram("er")).error);
}

#[test]
fn slow_transitions_are_a_focus_even_without_errors() {
    let history = LearnerHistory {
        sessions: vec![session(
            1,
            10,
            &[
                ("ve", tally(40, 0, 30, 400)),
                ("in", tally(40, 0, 30, 100)),
                ("ng", tally(40, 0, 30, 100)),
            ],
        )],
    };
    let model = LearnerModel::from_history(
        &history,
        &policy(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert_eq!(model.focus().first(), Some(&bigram("ve")));
    assert!(model.score(bigram("ve")).hesitation.get() > 5_000);
    assert_eq!(model.score(bigram("in")).hesitation, BasisPoints::ZERO);
}

#[test]
fn sparse_evidence_is_uncertain_not_mastered() {
    let history = LearnerHistory {
        sessions: vec![session(1, 10, &[("qu", tally(2, 0, 0, 0))])],
    };
    let model = LearnerModel::from_history(
        &history,
        &policy(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert!(model.focus().is_empty());
    assert!(model.score(bigram("qu")).uncertainty.get() > 8_000);
}

#[test]
fn recent_sessions_outweigh_old_ones() {
    let old_weak = session(
        1,
        1,
        &[("ou", tally(40, 15, 0, 0)), ("ck", tally(40, 0, 0, 0))],
    );
    let mut sessions = vec![old_weak];
    for id in 2..=30 {
        sessions.push(session(
            id,
            i64::try_from(id).unwrap(),
            &[("ou", tally(40, 0, 0, 0)), ("ck", tally(40, 6, 0, 0))],
        ));
    }
    let model = LearnerModel::from_history(
        &LearnerHistory { sessions },
        &policy(),
        day(30),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert_eq!(model.focus().first(), Some(&bigram("ck")));
}

#[test]
fn missed_words_follow_the_review_ladder() {
    let word = WordId::new(77).unwrap();
    let with_word = |id: u64, on: i64, errors: u64| {
        let mut historic = session(id, on, &[]);
        historic.learning.words.insert(
            word,
            WordTally {
                attempts: 1,
                errors,
            },
        );
        historic
    };
    let policy = policy();
    let build = |sessions: Vec<HistoricSession>, today: i64| {
        LearnerModel::from_history(
            &LearnerHistory { sessions },
            &policy,
            day(today),
            crate::numeric::NumericOptions::default(),
        )
        .unwrap()
    };
    // Missed on day 10: due on day 11, not before.
    assert!(build(vec![with_word(1, 10, 1)], 10).due_words().is_empty());
    assert_eq!(build(vec![with_word(1, 10, 1)], 11).due_words(), &[word]);
    // Two clean due reviews on different days promote to the 3-day rung.
    let promoted = vec![
        with_word(1, 10, 1),
        with_word(2, 11, 0),
        with_word(3, 12, 0),
    ];
    assert!(build(promoted.clone(), 14).due_words().is_empty());
    assert_eq!(build(promoted, 15).due_words(), &[word]);
    // A new error resets to the 1-day rung.
    let reset = vec![
        with_word(1, 10, 1),
        with_word(2, 11, 0),
        with_word(3, 12, 1),
    ];
    assert_eq!(build(reset, 13).due_words(), &[word]);
}

#[test]
fn passages_report_their_age_within_the_window() {
    let passage = PassageId::new(5).unwrap();
    let mut first = session(1, 1, &[]);
    first.learning.passages.insert(passage);
    let history = LearnerHistory {
        sessions: vec![first, session(2, 2, &[]), session(3, 3, &[])],
    };
    let model = LearnerModel::from_history(
        &history,
        &policy(),
        day(3),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert_eq!(model.passage_age(passage), Some(2));
    assert_eq!(model.passage_age(PassageId::new(6).unwrap()), None);
}

#[test]
fn word_score_takes_the_weakest_transition() {
    let history = LearnerHistory {
        sessions: vec![session(1, 10, &[("th", tally(40, 12, 0, 0))])],
    };
    let model = LearnerModel::from_history(
        &history,
        &policy(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    let the: Vec<_> = "the".chars().map(unit).collect();
    let cat: Vec<_> = "cat".chars().map(unit).collect();
    assert!(model.word_score(&the).error > model.word_score(&cat).error);
}

#[test]
fn noise_never_becomes_a_focus() {
    // Zero errors and only 10% slower than the lane: below both thresholds.
    let history = LearnerHistory {
        sessions: vec![session(
            1,
            10,
            &[
                ("pe", tally(30, 0, 30, 110)),
                ("an", tally(30, 1, 30, 100)),
                ("st", tally(30, 1, 30, 100)),
            ],
        )],
    };
    let model = LearnerModel::from_history(
        &history,
        &policy(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert!(!model.focus().contains(&bigram("pe")));
}

#[test]
fn a_key_explained_by_a_focused_transition_is_skipped() {
    let mut historic = session(
        1,
        10,
        &[("er", tally(30, 12, 0, 0)), ("an", tally(30, 0, 0, 0))],
    );
    let r = SkillKey {
        lane: KeyLane::Main,
        skill: Skill::Unit(unit('r')),
    };
    historic.learning.skills.insert(r, tally(60, 12, 0, 0));
    let model = LearnerModel::from_history(
        &LearnerHistory {
            sessions: vec![historic],
        },
        &policy(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert_eq!(model.focus().first(), Some(&bigram("er")));
    assert!(!model.focus().contains(&r));
}

/// #56: the focus list does not depend on hash-map iteration order: models
/// built from the same history (each with its own randomly seeded maps)
/// and from the same skills in another session order agree.
#[test]
fn focus_is_independent_of_map_order() {
    let skills = [
        ("th", tally(40, 9, 0, 0)),
        ("he", tally(40, 9, 0, 0)),
        ("er", tally(40, 8, 0, 0)),
        ("re", tally(40, 8, 0, 0)),
        ("an", tally(40, 7, 0, 0)),
        ("nd", tally(40, 7, 0, 0)),
        ("ou", tally(40, 6, 0, 0)),
        ("st", tally(40, 6, 0, 0)),
        ("in", tally(400, 0, 0, 0)),
        ("on", tally(400, 0, 0, 0)),
        ("at", tally(400, 0, 0, 0)),
    ];
    let mut reversed = skills;
    reversed.reverse();
    let build = |skills: &[(&str, SkillTally)]| {
        LearnerModel::from_history(
            &LearnerHistory {
                sessions: vec![session(1, 10, skills)],
            },
            &policy(),
            day(10),
            crate::numeric::NumericOptions::default(),
        )
        .unwrap()
        .focus()
        .to_vec()
    };
    let first = build(&skills);
    assert_eq!(first.len(), 6);
    for _ in 0..16 {
        assert_eq!(build(&skills), first);
        assert_eq!(build(&reversed), first);
    }
}

/// #52: after a change of UTC offset a later session's local day can be
/// one earlier; replay keeps (day, session) order and planning never fails.
#[test]
fn an_offset_change_between_sessions_keeps_replay_order() {
    let word = WordId::new(7).unwrap();
    let with_word = |id: u64, day_value: i64, errors: u64| {
        let mut s = session(id, day_value, &[]);
        s.learning.words.insert(
            word,
            WordTally {
                attempts: 1,
                errors,
            },
        );
        s
    };
    // Session 2 (clean, on day 11) then session 3, recorded later but on
    // local day 10 after travelling west.
    let history = LearnerHistory {
        sessions: vec![
            with_word(1, 10, 1),
            with_word(2, 11, 0),
            with_word(3, 10, 0),
        ],
    };
    let model = LearnerModel::from_history(
        &history,
        &policy(),
        day(12),
        crate::numeric::NumericOptions::default(),
    );
    assert!(model.is_ok(), "{model:?}");
}

fn policy_v3() -> TrainerPolicy {
    crate::policy::parse_policy(include_bytes!("../../../policies/trainer-v3.toml")).unwrap()
}

fn key(lane: KeyLane, text: &str) -> SkillKey {
    let units: Vec<TrainingChar> = text.chars().map(unit).collect();
    SkillKey {
        lane,
        skill: match units.as_slice() {
            [one] => Skill::Unit(*one),
            [first, second] => Skill::Bigram(*first, *second),
            _ => panic!("one or two units"),
        },
    }
}

fn session_with(id: u64, skills: &[(SkillKey, SkillTally)]) -> HistoricSession {
    let mut s = session(id, 10, &[]);
    for (k, value) in skills {
        s.learning.skills.insert(*k, *value);
    }
    s
}

fn letters_history() -> Vec<(SkillKey, SkillTally)> {
    vec![
        (bigram("th"), tally(40, 12, 0, 0)),
        (bigram("er"), tally(40, 1, 0, 0)),
        (bigram("an"), tally(400, 0, 0, 0)),
    ]
}

/// #46: a history weak on numpad `7` and `48` yields them as the numpad
/// focus and leaves the letters focus unchanged; row digits stay out of
/// the numpad group and vice versa (L-01).
#[test]
fn numpad_weakness_is_a_numpad_focus_and_letters_are_unchanged() {
    let numpad = KeyLane::Numpad;
    let main = KeyLane::Main;
    let mut skills = letters_history();
    skills.extend([
        (key(numpad, "7"), tally(60, 18, 0, 0)),
        (key(numpad, "48"), tally(60, 15, 0, 0)),
        (key(numpad, "5"), tally(600, 0, 0, 0)),
        (key(numpad, "56"), tally(600, 0, 0, 0)),
        (key(main, "3"), tally(60, 20, 0, 0)),
        (key(main, "1"), tally(600, 0, 0, 0)),
    ]);
    let policy = policy_v3();
    let model = |skills: &[(SkillKey, SkillTally)]| {
        LearnerModel::from_history(
            &LearnerHistory {
                sessions: vec![session_with(1, skills)],
            },
            &policy,
            day(10),
            crate::numeric::NumericOptions::default(),
        )
        .unwrap()
    };
    let with = model(&skills);
    let letters_only = model(&letters_history());
    let numpad_focus = with.focus_in(FocusGroup::Numpad);
    assert!(numpad_focus.contains(&key(numpad, "7")), "{numpad_focus:?}");
    assert!(
        numpad_focus.contains(&key(numpad, "48")),
        "{numpad_focus:?}"
    );
    assert!(numpad_focus.iter().all(|k| k.lane == numpad));
    let row_focus = with.focus_in(FocusGroup::Row);
    assert_eq!(row_focus, &[key(main, "3")]);
    assert_eq!(
        with.focus_in(FocusGroup::Letters),
        letters_only.focus_in(FocusGroup::Letters)
    );
    assert_eq!(
        with.focus().len(),
        with.focus_in(FocusGroup::Letters).len() + 3
    );
    assert_eq!(with.mode(), PlannerMode::Adaptive);
}

/// #46: sparse digit evidence yields no focus; policy v2 keeps the digit
/// groups off.
#[test]
fn sparse_digits_and_policy_v2_give_no_digit_focus() {
    let sparse = [(key(KeyLane::Numpad, "7"), tally(3, 3, 0, 0))];
    let history = LearnerHistory {
        sessions: vec![session_with(1, &sparse)],
    };
    let model = LearnerModel::from_history(
        &history,
        &policy_v3(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert!(model.focus_in(FocusGroup::Numpad).is_empty());
    let strong = [
        (key(KeyLane::Numpad, "7"), tally(60, 30, 0, 0)),
        (key(KeyLane::Numpad, "5"), tally(600, 0, 0, 0)),
    ];
    let history = LearnerHistory {
        sessions: vec![session_with(1, &strong)],
    };
    let v2 = LearnerModel::from_history(
        &history,
        &policy(),
        day(10),
        crate::numeric::NumericOptions::default(),
    )
    .unwrap();
    assert!(v2.focus_in(FocusGroup::Numpad).is_empty());
}

/// #46: group membership: letters, row digits and numpad keys.
#[test]
fn groups_keep_the_number_row_and_numpad_apart() {
    assert_eq!(
        group_of(key(KeyLane::Main, "th")),
        Some(FocusGroup::Letters)
    );
    assert_eq!(group_of(key(KeyLane::Main, "48")), Some(FocusGroup::Row));
    assert_eq!(
        group_of(key(KeyLane::Numpad, "48")),
        Some(FocusGroup::Numpad)
    );
    assert_eq!(
        group_of(key(KeyLane::Numpad, "4.")),
        Some(FocusGroup::Numpad)
    );
    assert_eq!(group_of(key(KeyLane::Main, "4.")), None);
    assert_eq!(group_of(key(KeyLane::Main, "t4")), None);
}

/// #83: the numpad focus group treats the decimal separator, either one, as
/// a skill of its own.
#[test]
fn the_separator_is_a_numpad_skill() {
    use cadence_core::learning::{KeyLane, Skill, SkillKey};
    for separator in ['.', ','] {
        let key = SkillKey {
            lane: KeyLane::Numpad,
            skill: Skill::Unit(
                cadence_core::TrainingChar::new(separator, cadence_core::Layout::Us).unwrap(),
            ),
        };
        assert_eq!(
            crate::learner::group_of(key),
            Some(crate::learner::FocusGroup::Numpad)
        );
    }
}

/// #101: every keypad key and pair in the numpad lane is a numpad skill,
/// operators included; outside keys and operator pairs are not.
#[test]
fn keypad_operators_join_the_numpad_group() {
    for text in ["+", "*", "/", "-", "7+", "*4", ".5"] {
        assert_eq!(
            group_of(key(KeyLane::Numpad, text)),
            Some(FocusGroup::Numpad),
            "{text}"
        );
    }
    for text in ["=", "**", "+.", "a"] {
        assert_eq!(group_of(key(KeyLane::Numpad, text)), None, "{text}");
    }
    assert_eq!(group_of(key(KeyLane::Main, "*")), None);
}
