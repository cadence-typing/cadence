//! Learning evidence (plan 0003): first attempts survive correction, clean
//! intervals exclude pauses and long gaps, missed words return once.
use cadence_core::learning::{
    KeyLane, PlanRecord, PlannerMode, SessionLearning, Skill, SkillKey, SkillTally, WordTally,
};
use cadence_core::progress::{Checkpoint, HistorySummary, SaveResult};
use cadence_core::{
    ContentDigest, LayoutSupportsPound, Micros, MonoTick, SessionId, TrainingChar, WordId,
};
use cadence_engine::{Engine, EventEnvelope, EventKind, Key, ScreenView};
use cadence_trainer::{
    Adaptation, ExerciseKind, Lane, Phase, PlannedExercise, PlannedPhase, PlannedWord, SessionPlan,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Error = Box<dyn std::error::Error>;

const SECOND: u64 = 1_000_000;

fn unit(c: char) -> Result<TrainingChar, Error> {
    Ok(TrainingChar::new(c, LayoutSupportsPound::US)?)
}

fn words_exercise(slots: &[(&str, u64, Lane)]) -> Result<PlannedExercise, Error> {
    let text: Vec<&str> = slots.iter().map(|(word, _, _)| *word).collect();
    Ok(PlannedExercise {
        units: cadence_core::validate_target(&text.join(" "), LayoutSupportsPound::US)?,
        kind: ExerciseKind::Words,
        words: slots
            .iter()
            .map(|(_, id, lane)| {
                Ok(PlannedWord {
                    id: WordId::new(*id)?,
                    lane: *lane,
                    focus: None,
                    due_review: false,
                })
            })
            .collect::<Result<_, cadence_core::CoreError>>()?,
        passage: None,
        caption: None,
    })
}

fn engine(phases: Vec<PlannedPhase>) -> Result<Engine, Error> {
    Ok(Engine::new_plan(SessionPlan {
        budget_seconds: 120,
        policy_digest: ContentDigest::from_bytes([1; 32]),
        corpus_digest: ContentDigest::from_bytes([2; 32]),
        record: PlanRecord {
            seed: 9,
            mode: PlannerMode::Adaptive,
            focus: Vec::new(),
        },
        adaptation: Adaptation {
            hesitation_ceiling: Micros::from_us(1_500_000),
            reinforcement_capacity: 8,
            reinforcements_per_word: 1,
        },
        phases,
    })?)
}

fn words_engine() -> Result<Engine, Error> {
    engine(vec![PlannedPhase {
        phase: Phase::AdaptiveWords,
        seconds: 60,
        exercises: vec![
            words_exercise(&[("cat", 1, Lane::Target), ("dog", 2, Lane::Broad)])?,
            words_exercise(&[("sun", 3, Lane::Target), ("hat", 4, Lane::Review)])?,
        ],
    }])
}

/// Drives the reducer, acknowledging each checkpoint as storage would.
struct Driver {
    engine: Engine,
    last: Option<Checkpoint>,
}

impl Driver {
    fn new(engine: Engine) -> Result<Self, Error> {
        let mut driver = Self { engine, last: None };
        driver.send(
            0,
            EventKind::StorageReady {
                next_session: SessionId::new(1)?,
                day: 100,
                history: HistorySummary::default(),
            },
        )?;
        Ok(driver)
    }

    fn send(&mut self, at: u64, kind: EventKind) -> Result<(), Error> {
        let seq = self.engine.consumed_events().next()?;
        let transition = self.engine.reduce(EventEnvelope {
            seq,
            at: MonoTick::from_us(at),
            kind,
        });
        if let Some(checkpoint) = transition.checkpoint {
            let key = checkpoint.key;
            self.last = Some(checkpoint);
            self.send(
                at,
                EventKind::SaveResult(SaveResult {
                    key,
                    result: Ok(()),
                }),
            )?;
        }
        Ok(())
    }

    fn key(&mut self, at: u64, key: Key) -> Result<(), Error> {
        self.send(at, EventKind::Key(key))
    }

    fn typed(&mut self, at: u64, c: char) -> Result<(), Error> {
        self.key(at, Key::Content(unit(c)?))
    }

    fn finish(mut self, at: u64) -> Result<SessionLearning, Error> {
        self.key(at, Key::Interrupt)?;
        self.last
            .and_then(|checkpoint| checkpoint.learning)
            .ok_or_else(|| "final checkpoint carries learning evidence".into())
    }
}

fn bigram(first: char, second: char, lane: KeyLane) -> Result<SkillKey, Error> {
    Ok(SkillKey {
        lane,
        skill: Skill::Bigram(unit(first)?, unit(second)?),
    })
}

fn tally(learning: &SessionLearning, key: SkillKey) -> SkillTally {
    learning.skills.get(&key).copied().unwrap_or_default()
}

#[test]
fn corrected_mistakes_stay_errors_and_only_clean_intervals_are_timed() -> TestResult {
    let mut driver = Driver::new(words_engine()?)?;
    driver.key(0, Key::Enter)?;
    driver.typed(SECOND, 'c')?;
    driver.typed(1_200_000, 'x')?;
    driver.key(1_300_000, Key::Backspace)?;
    driver.typed(1_500_000, 'a')?;
    driver.typed(1_700_000, 't')?;
    driver.typed(1_900_000, ' ')?;
    driver.typed(2_100_000, 'd')?;
    driver.typed(2_300_000, 'o')?;
    // 2.7 s gap: above the hesitation ceiling, still a first attempt.
    driver.typed(5_000_000, 'g')?;
    let learning = driver.finish(6 * SECOND)?;
    let main = KeyLane::Main;
    let a = SkillKey {
        lane: main,
        skill: Skill::Unit(unit('a')?),
    };
    assert_eq!(
        tally(&learning, a).attempts,
        1,
        "retyping is not a new first attempt"
    );
    assert_eq!(
        tally(&learning, a).errors,
        1,
        "the corrected slip stays an error"
    );
    assert_eq!(tally(&learning, bigram('c', 'a', main)?).errors, 1);
    // The correction broke the timing chain into `t`.
    assert_eq!(tally(&learning, bigram('a', 't', main)?).timed, 0);
    let space = tally(&learning, bigram('t', ' ', main)?);
    assert_eq!((space.timed, space.timed_micros), (1, 200_000));
    assert_eq!(
        tally(&learning, bigram('d', 'o', main)?).timed_micros,
        200_000
    );
    let long = tally(&learning, bigram('o', 'g', main)?);
    assert_eq!((long.attempts, long.errors, long.timed), (1, 0, 0));
    assert_eq!(
        learning.words.get(&WordId::new(1)?),
        Some(&WordTally {
            attempts: 1,
            errors: 1
        })
    );
    assert_eq!(
        learning.words.get(&WordId::new(2)?),
        Some(&WordTally {
            attempts: 1,
            errors: 0
        })
    );
    Ok(())
}

#[test]
fn pauses_are_excluded_from_intervals() -> TestResult {
    let mut driver = Driver::new(words_engine()?)?;
    driver.key(0, Key::Enter)?;
    driver.typed(SECOND, 'c')?;
    driver.key(1_050_000, Key::Escape)?;
    driver.key(9 * SECOND, Key::Escape)?;
    driver.typed(9_100_000, 'a')?;
    let learning = driver.finish(10 * SECOND)?;
    let ca = tally(&learning, bigram('c', 'a', KeyLane::Main)?);
    assert_eq!((ca.timed, ca.timed_micros), (1, 150_000));
    Ok(())
}

#[test]
fn a_missed_word_returns_once_in_a_later_review_slot() -> TestResult {
    let mut driver = Driver::new(words_engine()?)?;
    driver.key(0, Key::Enter)?;
    let mut at = SECOND;
    for c in "cxt dog".chars() {
        if c == 't' {
            driver.key(at, Key::Backspace)?;
            at += 100_000;
            driver.typed(at, 'a')?;
            at += 100_000;
        }
        driver.typed(at, c)?;
        at += 100_000;
    }
    let ScreenView::Practice { spans, status, .. } =
        driver.engine.prepare(MonoTick::from_us(at)).screen
    else {
        return Err("practice screen expected".into());
    };
    let target: String = spans.iter().map(|span| span.cell_char()).collect();
    assert_eq!(target, "sun cat", "the review slot now re-serves the miss");
    assert_eq!(status.context.as_deref(), Some("missed earlier, again"));
    for c in "sun cat".chars() {
        driver.typed(at, c)?;
        at += 100_000;
    }
    let learning = driver.finish(at)?;
    assert_eq!(learning.reinforced, 1);
    assert_eq!(
        learning.words.get(&WordId::new(1)?),
        Some(&WordTally {
            attempts: 2,
            errors: 1
        })
    );
    assert!(!learning.words.contains_key(&WordId::new(4)?));
    Ok(())
}

#[test]
fn numpad_keys_form_a_separate_lane() -> TestResult {
    let mut driver = Driver::new(engine(vec![PlannedPhase {
        phase: Phase::Numpad,
        seconds: 30,
        exercises: vec![PlannedExercise::plain(
            cadence_core::validate_target("12", LayoutSupportsPound::US)?,
            ExerciseKind::Numpad,
        )],
    }])?)?;
    driver.key(0, Key::Enter)?;
    driver.typed(SECOND, '1')?;
    driver.typed(1_100_000, '2')?;
    let learning = driver.finish(2 * SECOND)?;
    assert_eq!(
        tally(&learning, bigram('1', '2', KeyLane::Numpad)?).attempts,
        1
    );
    assert_eq!(
        tally(&learning, bigram('1', '2', KeyLane::Main)?).attempts,
        0
    );
    Ok(())
}
