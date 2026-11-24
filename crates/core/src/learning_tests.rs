use super::*;
use crate::progress::{Checkpoint, ProgressStatus, SessionProgress, Track, TrackProgress};
use crate::{CommitSeq, LayoutSupportsPound};

fn unit(character: char) -> TrainingChar {
    TrainingChar::new(character, LayoutSupportsPound::US).unwrap()
}

fn bigram(first: char, second: char) -> SkillKey {
    SkillKey {
        lane: KeyLane::Main,
        skill: Skill::Bigram(unit(first), unit(second)),
    }
}

fn progress() -> SessionProgress {
    SessionProgress {
        profile: "default".to_string(),
        day: 1,
        policy_digest: "policy".to_string(),
        corpus_digest: "corpus".to_string(),
        status: ProgressStatus::Running,
        tracks: vec![
            TrackProgress::empty(Track::Words),
            TrackProgress::empty(Track::Numpad),
        ],
        active_micros: 0,
    }
}

fn learning() -> SessionLearning {
    SessionLearning::new(PlanRecord {
        seed: 7,
        mode: PlannerMode::Adaptive,
        focus: vec![bigram('t', 'h')],
    })
}

/// Pinned from the alpha.3 implementation (`0f9e006`): schema-2 receipts
/// must keep verifying after the learning section was introduced.
const ALPHA3_DIGEST: &str = "07df56389ed137468aa95cc3af147f737e33eef60c4ab71aab960153847ed27e";

#[test]
fn absent_learning_keeps_the_alpha3_digest() {
    let checkpoint =
        Checkpoint::new(SessionId::FIRST, CommitSeq::INITIAL, progress(), None).unwrap();
    assert_eq!(checkpoint.key.digest.to_hex(), ALPHA3_DIGEST);
}

#[test]
fn learning_section_is_digest_sensitive() {
    let bare = Checkpoint::new(
        SessionId::FIRST,
        CommitSeq::INITIAL,
        progress(),
        Some(learning()),
    )
    .unwrap();
    assert_ne!(bare.key.digest.to_hex(), ALPHA3_DIGEST);
    let mut observed = learning();
    observed
        .observe_skill(bigram('t', 'h'), true, None)
        .unwrap();
    let changed = Checkpoint::new(
        SessionId::FIRST,
        CommitSeq::INITIAL,
        progress(),
        Some(observed),
    )
    .unwrap();
    assert_ne!(changed.key.digest, bare.key.digest);
}

#[test]
fn corrected_errors_and_timing_accumulate_exactly() {
    let mut evidence = learning();
    let key = bigram('t', 'h');
    evidence.observe_skill(key, true, None).unwrap();
    evidence
        .observe_skill(key, false, Some(Micros::from_us(150_000)))
        .unwrap();
    evidence
        .observe_skill(key, false, Some(Micros::from_us(250_000)))
        .unwrap();
    let tally = evidence.skills.get(&key).copied().unwrap();
    assert_eq!(
        tally,
        SkillTally {
            attempts: 3,
            errors: 1,
            timed: 2,
            timed_micros: 400_000
        }
    );
    evidence.validate().unwrap();
}

#[test]
fn advances_only_cumulatively_under_the_same_plan() {
    let mut old = learning();
    old.observe_skill(bigram('a', 'b'), false, None).unwrap();
    old.observe_word(WordId::new(3).unwrap(), true).unwrap();
    let mut new = old.clone();
    new.observe_skill(bigram('a', 'b'), true, None).unwrap();
    new.observe_passage(PassageId::new(9).unwrap()).unwrap();
    assert!(new.advances_from(&old));
    assert!(!old.advances_from(&new));
    let mut replanned = new.clone();
    replanned.plan.seed = 8;
    assert!(!replanned.advances_from(&old));
}

#[test]
fn persisted_names_round_trip() {
    for skill in [Skill::Unit(unit('q')), Skill::Bigram(unit(' '), unit('"'))] {
        assert_eq!(
            Skill::parse(skill.kind_key(), &skill.text()).unwrap(),
            skill
        );
    }
    assert!(Skill::parse("unit", "ab").is_err());
    assert!(Skill::parse("bigram", "a").is_err());
    assert!(Skill::parse("trigram", "abc").is_err());
    for lane in [KeyLane::Main, KeyLane::Numpad] {
        assert_eq!(KeyLane::parse(lane.key()), Some(lane));
    }
    for mode in [PlannerMode::ColdStart, PlannerMode::Adaptive] {
        assert_eq!(PlannerMode::parse(mode.key()), Some(mode));
    }
}

#[test]
fn malformed_tallies_are_refused() {
    let mut evidence = learning();
    evidence.skills.insert(
        bigram('x', 'y'),
        SkillTally {
            attempts: 1,
            errors: 2,
            timed: 0,
            timed_micros: 0,
        },
    );
    assert!(evidence.validate().is_err());
}
