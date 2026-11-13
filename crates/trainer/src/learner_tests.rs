#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
use super::*;
use cadence_core::LayoutSupportsPound;
use cadence_core::SessionId;
use cadence_core::learning::{PlanRecord, SessionLearning, SkillTally, WordTally};

fn policy() -> TrainerPolicy {
    crate::policy::parse_policy(include_bytes!("../../../policies/trainer-v2.toml")).unwrap()
}

fn unit(character: char) -> TrainingChar {
    TrainingChar::new(character, LayoutSupportsPound::US).unwrap()
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
    let model = LearnerModel::from_history(&LearnerHistory::default(), &policy(), day(10)).unwrap();
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
    let model = LearnerModel::from_history(&history, &policy(), day(10)).unwrap();
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
    let model = LearnerModel::from_history(&history, &policy(), day(10)).unwrap();
    assert_eq!(model.focus().first(), Some(&bigram("ve")));
    assert!(model.score(bigram("ve")).hesitation.get() > 5_000);
    assert_eq!(model.score(bigram("in")).hesitation, BasisPoints::ZERO);
}

#[test]
fn sparse_evidence_is_uncertain_not_mastered() {
    let history = LearnerHistory {
        sessions: vec![session(1, 10, &[("qu", tally(2, 0, 0, 0))])],
    };
    let model = LearnerModel::from_history(&history, &policy(), day(10)).unwrap();
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
    let model =
        LearnerModel::from_history(&LearnerHistory { sessions }, &policy(), day(30)).unwrap();
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
        LearnerModel::from_history(&LearnerHistory { sessions }, &policy, day(today)).unwrap()
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
    let model = LearnerModel::from_history(&history, &policy(), day(3)).unwrap();
    assert_eq!(model.passage_age(passage), Some(2));
    assert_eq!(model.passage_age(PassageId::new(6).unwrap()), None);
}

#[test]
fn word_score_takes_the_weakest_transition() {
    let history = LearnerHistory {
        sessions: vec![session(1, 10, &[("th", tally(40, 12, 0, 0))])],
    };
    let model = LearnerModel::from_history(&history, &policy(), day(10)).unwrap();
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
    let model = LearnerModel::from_history(&history, &policy(), day(10)).unwrap();
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
    )
    .unwrap();
    assert_eq!(model.focus().first(), Some(&bigram("er")));
    assert!(!model.focus().contains(&r));
}
