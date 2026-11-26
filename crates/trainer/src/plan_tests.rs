use super::*;
use crate::corpus::WordBand;
use cadence_core::learning::{
    HistoricSession, KeyLane, LearnerHistory, PlanRecord, PlannerMode, SessionLearning, Skill,
    SkillTally, WordTally,
};
use cadence_core::{LayoutSupportsPound, SessionId, UtcDay};

fn policy() -> TrainerPolicy {
    crate::policy::parse_policy(include_bytes!("../../../policies/trainer-v2.toml")).unwrap()
}

/// Distinct lowercase words with varied transitions: `w` plus base-26 digits.
fn word(n: u64) -> String {
    let mut text = String::from("w");
    let mut value = n;
    loop {
        text.push(char::from(b'a' + u8::try_from(value % 26).unwrap()));
        value /= 26;
        if value == 0 {
            return text;
        }
    }
}

fn pack(word_count: u64) -> WordPack {
    let entries = (1..=word_count)
        .map(|n| {
            let band = if n <= 2_000 {
                WordBand::Core
            } else if n <= 10_000 {
                WordBand::Common
            } else {
                WordBand::Tail
            };
            (n, word(n), band)
        })
        .collect();
    WordPack::new(
        entries,
        ContentDigest::from_bytes([9; 32]),
        1,
        LayoutSupportsPound::US,
    )
    .unwrap()
}

const SOURCES: &str = "s\tA Work\tAn Author\n";

fn prose(count: usize) -> ProseBank {
    let rows: String = (0..count)
        .map(|n| format!("s\tPassage number {n} has a few words. It ends here.\n"))
        .collect();
    ProseBank::from_tsv(&rows, SOURCES, 1).unwrap()
}

fn build(
    learner: &LearnerModel,
    pack: &WordPack,
    bank: &ProseBank,
    seed: u64,
    disabled: &[Phase],
    budget: u64,
) -> Result<SessionPlan, PlanError> {
    let inputs = PlanInputs {
        pack,
        prose: bank,
        learner,
    };
    build_session_plan(&policy(), inputs, seed, disabled, budget)
}

fn cold() -> LearnerModel {
    LearnerModel::cold(&policy())
}

fn phase(plan: &SessionPlan, wanted: Phase) -> &PlannedPhase {
    plan.phases.iter().find(|p| p.phase == wanted).unwrap()
}

fn text(units: &[TrainingChar]) -> String {
    units.iter().map(|unit| unit.get()).collect()
}

fn bigram(pair: &str) -> SkillKey {
    let mut chars = pair
        .chars()
        .map(|c| TrainingChar::new(c, LayoutSupportsPound::US).unwrap());
    SkillKey {
        lane: KeyLane::Main,
        skill: Skill::Bigram(chars.next().unwrap(), chars.next().unwrap()),
    }
}

fn weak_on(pair: &str, words: &[(WordId, u64)], day: i64) -> LearnerModel {
    let mut learning = SessionLearning::new(PlanRecord {
        seed: 1,
        mode: PlannerMode::ColdStart,
        focus: Vec::new(),
    });
    let tally = |errors| SkillTally {
        attempts: 40,
        errors,
        timed: 0,
        timed_micros: 0,
    };
    learning.skills.insert(bigram(pair), tally(14));
    for other in ["wa", "ab", "ba", "ca", "wc", "wd"] {
        learning.skills.insert(bigram(other), tally(0));
    }
    for (id, errors) in words {
        learning.words.insert(
            *id,
            WordTally {
                attempts: 1,
                errors: *errors,
            },
        );
    }
    let history = LearnerHistory {
        sessions: vec![HistoricSession {
            session: SessionId::FIRST,
            day,
            learning,
        }],
    };
    let today = UtcDay::from_days_since_epoch(day + 1).unwrap();
    LearnerModel::from_history(&history, &policy(), today).unwrap()
}

fn slots(plan: &SessionPlan) -> impl Iterator<Item = &PlannedWord> {
    phase(plan, Phase::AdaptiveWords)
        .exercises
        .iter()
        .flat_map(|exercise| exercise.words.iter())
}

#[test]
fn default_plan_is_deterministic_and_replayable() {
    let (pack, bank) = (pack(3_000), prose(50));
    let first = build(&cold(), &pack, &bank, 42, &[], 900).unwrap();
    let second = build(&cold(), &pack, &bank, 42, &[], 900).unwrap();
    assert_eq!(first, second, "same inputs produce the same plan");
    let seconds: Vec<u64> = first.phases.iter().map(|phase| phase.seconds).collect();
    assert_eq!(seconds, vec![60, 300, 120, 180, 180, 60]);
    assert_eq!(first.record.mode, PlannerMode::ColdStart);
    assert_eq!(first.record.seed, 42);
}

#[test]
fn different_seeds_change_words_and_prose_order() {
    let (pack, bank) = (pack(3_000), prose(50));
    let first = build(&cold(), &pack, &bank, 1, &[], 900).unwrap();
    let second = build(&cold(), &pack, &bank, 2, &[], 900).unwrap();
    assert_ne!(
        phase(&first, Phase::AdaptiveWords).exercises[0].units,
        phase(&second, Phase::AdaptiveWords).exercises[0].units
    );
    assert_ne!(
        phase(&first, Phase::Prose).exercises[0].passage,
        phase(&second, Phase::Prose).exercises[0].passage
    );
}

#[test]
fn disabled_tracks_are_omitted_and_budget_stays_exact() {
    let (pack, bank) = (pack(3_000), prose(50));
    let disabled = vec![Phase::Numpad, Phase::NumberRow];
    let plan = build(&cold(), &pack, &bank, 7, &disabled, 900).unwrap();
    assert_eq!(
        plan.phases
            .iter()
            .map(|phase| phase.phase)
            .collect::<Vec<_>>(),
        vec![
            Phase::WarmUp,
            Phase::AdaptiveWords,
            Phase::Prose,
            Phase::Review
        ]
    );
    assert_eq!(plan.phases.iter().map(|p| p.seconds).sum::<u64>(), 900);
}

#[test]
fn shorter_budget_still_serves_every_enabled_track() {
    let (pack, bank) = (pack(3_000), prose(50));
    let plan = build(&cold(), &pack, &bank, 9, &[], 300).unwrap();
    assert_eq!(plan.phases.len(), 6);
    assert_eq!(plan.phases.iter().map(|p| p.seconds).sum::<u64>(), 300);
}

#[test]
fn infeasible_budget_and_tiny_corpus_fail_explicitly() {
    let bank = prose(5);
    assert!(matches!(
        build(&cold(), &pack(100), &bank, 1, &[], 100),
        Err(PlanError::Infeasible { .. })
    ));
    assert!(matches!(
        build(&cold(), &pack(4), &bank, 1, &[], 900),
        Err(PlanError::NoEligibleCandidate { .. })
    ));
}

#[test]
fn word_slots_describe_their_target_and_avoid_the_tail_band() {
    let (pack, bank) = (pack(12_000), prose(50));
    let plan = build(&cold(), &pack, &bank, 5, &[], 900).unwrap();
    for exercise in &phase(&plan, Phase::AdaptiveWords).exercises {
        let joined: Vec<String> = exercise
            .words
            .iter()
            .map(|slot| text(&pack.word(slot.id).unwrap().units))
            .collect();
        assert_eq!(text(&exercise.units), joined.join(" "));
        for slot in exercise.words.iter().filter(|s| s.lane == Lane::Target) {
            assert_ne!(pack.word(slot.id).unwrap().band, WordBand::Tail);
        }
    }
}

#[test]
fn recorded_weakness_steers_target_slots() {
    let (pack, bank) = (pack(3_000), prose(50));
    let containing = |plan: &SessionPlan| {
        slots(plan)
            .filter(|slot| text(&pack.word(slot.id).unwrap().units).contains("wb"))
            .count()
    };
    // About 1 in 26 fixture words contains `wb`.
    let learner = weak_on("wb", &[], 10);
    let adaptive = build(&learner, &pack, &bank, 3, &[], 900).unwrap();
    let baseline = build(&cold(), &pack, &bank, 3, &[], 900).unwrap();
    assert_eq!(adaptive.record.mode, PlannerMode::Adaptive);
    assert_eq!(adaptive.record.focus, vec![bigram("wb")]);
    let total = slots(&adaptive).count();
    assert!(
        containing(&adaptive) * 4 >= total && containing(&adaptive) > 3 * containing(&baseline),
        "adaptive {} of {total}, baseline {}",
        containing(&adaptive),
        containing(&baseline)
    );
    assert!(slots(&adaptive).any(|slot| slot.focus == Some(bigram("wb"))));
}

#[test]
fn per_session_cap_keeps_target_words_varied() {
    let (pack, bank) = (pack(3_000), prose(50));
    let plan = build(&weak_on("qa", &[], 10), &pack, &bank, 3, &[], 900).unwrap();
    let mut counts = std::collections::HashMap::new();
    for slot in slots(&plan).filter(|slot| slot.lane == Lane::Target) {
        *counts.entry(slot.id).or_insert(0_u32) += 1;
    }
    let cap = policy()
        .learning
        .maximum_target_word_occurrences_per_session;
    assert!(counts.values().all(|count| *count <= cap));
}

#[test]
fn due_missed_words_fill_review_slots_first() {
    let (pack, bank) = (pack(3_000), prose(50));
    let missed = WordId::new(321).unwrap();
    let learner = weak_on("qa", &[(missed, 1)], 10);
    assert_eq!(learner.due_words(), &[missed]);
    let plan = build(&learner, &pack, &bank, 3, &[], 900).unwrap();
    let first_review = slots(&plan).find(|slot| slot.lane == Lane::Review).unwrap();
    assert_eq!(first_review.id, missed);
    assert!(first_review.due_review);
}

#[test]
fn prose_prefers_unseen_passages_and_labels_repeats() {
    let (pack, bank) = (pack(3_000), prose(40));
    let seen: Vec<_> = bank.passages().iter().map(|p| p.id).collect();
    // Everything but the last passage was typed in the previous session.
    let mut learning = SessionLearning::new(PlanRecord {
        seed: 1,
        mode: PlannerMode::ColdStart,
        focus: Vec::new(),
    });
    learning
        .passages
        .extend(seen.iter().take(seen.len() - 1).copied());
    let history = LearnerHistory {
        sessions: vec![HistoricSession {
            session: SessionId::FIRST,
            day: 1,
            learning,
        }],
    };
    let today = UtcDay::from_days_since_epoch(2).unwrap();
    let learner = LearnerModel::from_history(&history, &policy(), today).unwrap();
    let plan = build(&learner, &pack, &bank, 11, &[], 900).unwrap();
    let exercises = &phase(&plan, Phase::Prose).exercises;
    let first_caption = exercises[0].caption.as_deref().unwrap();
    assert_eq!(exercises[0].passage, seen.last().copied());
    assert!(!first_caption.contains("repeat"));
    assert!(first_caption.contains("A Work"));
    let repeated = exercises
        .iter()
        .find(|exercise| exercise.passage != seen.last().copied())
        .unwrap();
    assert!(
        repeated
            .caption
            .as_deref()
            .unwrap()
            .contains("repeat, typed 1 session ago")
    );
}

#[test]
fn prose_volume_covers_the_phase() {
    let (pack, bank) = (pack(3_000), prose(400));
    let plan = build(&cold(), &pack, &bank, 4, &[], 900).unwrap();
    let prose = phase(&plan, Phase::Prose);
    let units: u64 = prose
        .exercises
        .iter()
        .map(|exercise| u64::try_from(exercise.units.len()).unwrap())
        .sum();
    assert!(units >= prose.seconds * policy().learning.prose_units_per_second);
    let distinct: std::collections::HashSet<_> =
        prose.exercises.iter().map(|e| e.passage).collect();
    assert!(distinct.len() > 10);
}

/// #44: sampled slots draw without replacement: a cold-start words phase
/// plans no word twice (60 exercises of a 25,000-word pack), and the
/// warm-up repeats none within its core stream.
#[test]
fn cold_start_samples_without_replacement() {
    let bank = prose(40);
    let words = pack(25_000);
    for seed in [1, 42, 7_777] {
        let plan = build(&cold(), &words, &bank, seed, &[], 1_800).unwrap();
        for wanted in [Phase::AdaptiveWords, Phase::WarmUp] {
            let planned: Vec<WordId> = phase(&plan, wanted)
                .exercises
                .iter()
                .flat_map(|exercise| exercise.words.iter().map(|word| word.id))
                .collect();
            let distinct: std::collections::HashSet<_> = planned.iter().collect();
            assert_eq!(distinct.len(), planned.len(), "{wanted:?} seed {seed}");
            if wanted == Phase::AdaptiveWords {
                assert!(
                    phase(&plan, wanted).exercises.len() >= 60,
                    "{} exercises",
                    phase(&plan, wanted).exercises.len()
                );
            }
        }
    }
}
