//! Reducer-side learning evidence and same-session reinforcement (plan 0003).
//!
//! Every update is bounded by the current exercise; nothing here touches
//! the corpus, storage or the clock source. A first attempt is recorded at
//! the moment it is typed and is never rewritten by a later correction.
//! Clean intervals use active time, so pauses never count as hesitation;
//! the hesitation ceiling excludes long gaps from the learner model only.
use std::collections::{BTreeMap, VecDeque};

use cadence_core::alphabet::TrainingChar;
use cadence_core::learning::{KeyLane, SessionLearning, Skill, SkillKey};
use cadence_core::ledger::Slot;
use cadence_core::{CoreError, Micros, WordId, evidence, queue, slots};
use cadence_trainer::{Adaptation, ExerciseKind, Lane, PlannedExercise, PlannedWord};

use super::invalid;

/// The last clean first attempt, for the next transition's interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KeyMark {
    position: usize,
    active: Micros,
    /// The timing segment it was typed in (`TimingSegments::segment_ordinal`).
    segment: u64,
}

/// A missed word waiting to be served again this session.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reinforcement {
    word: WordId,
    units: Vec<TrainingChar>,
}

/// Evidence accumulator and reinforcement queue owned by the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Learning {
    pub(super) evidence: SessionLearning,
    adaptation: Adaptation,
    mark: Option<KeyMark>,
    queue: VecDeque<Reinforcement>,
    served: BTreeMap<WordId, u32>,
    /// Word slots of the open exercise after any substitution.
    pub(super) words: Vec<PlannedWord>,
}

fn slot_lane(lane: Lane) -> slots::SlotLane {
    match lane {
        Lane::Target => slots::SlotLane::Target,
        Lane::Review => slots::SlotLane::Review,
        Lane::Broad => slots::SlotLane::Broad,
    }
}

impl Learning {
    pub(super) fn new(evidence: SessionLearning, adaptation: Adaptation) -> Self {
        Self {
            evidence,
            adaptation,
            mark: None,
            queue: VecDeque::with_capacity(adaptation.reinforcement_capacity),
            served: BTreeMap::new(),
            words: Vec::new(),
        }
    }

    /// Forgets the timing anchor (deletion or a new exercise).
    pub(super) fn break_timing(&mut self) {
        self.mark = None;
    }

    /// Records a first attempt at `position` of `target`, typed at `active`
    /// in timing segment `segment`; `previous` is the first outcome of the
    /// position before it.
    #[expect(
        clippy::too_many_arguments,
        reason = "one call site passes the ledger facts the kernel decides from"
    )]
    pub(super) fn first_attempt(
        &mut self,
        target: &[TrainingChar],
        position: usize,
        matched: bool,
        previous: Slot,
        active: Micros,
        segment: u64,
        lane: KeyLane,
    ) -> Result<(), CoreError> {
        let unit = *target
            .get(position)
            .ok_or_else(|| invalid("first attempt beyond target"))?;
        self.evidence.observe_skill(
            SkillKey {
                lane,
                skill: Skill::Unit(unit),
            },
            !matched,
            None,
        )?;
        if let Some(previous_unit) = position.checked_sub(1).and_then(|p| target.get(p)) {
            // The kernel decides both (L-02, L-03): the transition is
            // correct only if both keys were, and its interval is a sample
            // only when adjacent, matched, unpaused, positive and in range.
            let anchor = self.mark.map_or(
                evidence::Mark {
                    present: false,
                    position: 0,
                    active_us: 0,
                    segment: 0,
                },
                |mark| evidence::Mark {
                    present: true,
                    position: mark.position,
                    active_us: mark.active.as_us(),
                    segment: mark.segment,
                },
            );
            let interval = match evidence::clean_interval(
                anchor,
                position,
                matched,
                active.as_us(),
                segment,
                self.adaptation.hesitation_ceiling.as_us(),
            ) {
                evidence::Interval::Timed(gap) => Some(Micros::from_us(gap)),
                evidence::Interval::NoAnchor
                | evidence::Interval::NotAdjacent
                | evidence::Interval::Error
                | evidence::Interval::CrossesPause
                | evidence::Interval::Unresolved
                | evidence::Interval::Backward
                | evidence::Interval::AboveCeiling => None,
            };
            self.evidence.observe_skill(
                SkillKey {
                    lane,
                    skill: Skill::Bigram(*previous_unit, unit),
                },
                !evidence::pair_first_correct(previous, matched),
                interval,
            )?;
        }
        self.mark = matched.then_some(KeyMark {
            position,
            active,
            segment,
        });
        Ok(())
    }

    /// Prepares the target for `exercise`, substituting queued missed words
    /// into its review slots. Bounded by the exercise's slot count.
    pub(super) fn open(
        &mut self,
        exercise: &PlannedExercise,
    ) -> Result<Vec<TrainingChar>, CoreError> {
        self.mark = None;
        self.words.clone_from(&exercise.words);
        if exercise.kind != ExerciseKind::Words || self.queue.is_empty() {
            return Ok(exercise.units.clone());
        }
        let mut pieces: Vec<Vec<TrainingChar>> = exercise
            .units
            .split(|unit| unit.get() == ' ')
            .map(<[TrainingChar]>::to_vec)
            .collect();
        if pieces.len() != self.words.len() {
            return Err(invalid("word slots do not match the target"));
        }
        // Place each queued word with the proved
        // `cadence_kernel::slots::substitute`: a review slot that is not a
        // due review first, then a broad slot; never a target slot or a due
        // review, and never a word already in the exercise (it stays queued
        // for the next exercise). Each slot is replaced at most once.
        let mut replaced = vec![false; self.words.len()];
        let mut waiting = VecDeque::with_capacity(self.queue.len());
        while let Some(next) = self.queue.pop_front() {
            let present = self.words.iter().any(|slot| slot.id == next.word);
            let fits = |wanted: Lane| {
                self.words.iter().zip(&replaced).position(|(slot, done)| {
                    !done
                        && slot.lane == wanted
                        && slots::substitute(slot_lane(slot.lane), slot.due_review, present)
                })
            };
            // Every lane is offered; the kernel refuses target slots.
            let Some(index) = fits(Lane::Review)
                .or_else(|| fits(Lane::Broad))
                .or_else(|| fits(Lane::Target))
            else {
                waiting.push_back(next);
                continue;
            };
            let (Some(slot), Some(piece), Some(done)) = (
                self.words.get_mut(index),
                pieces.get_mut(index),
                replaced.get_mut(index),
            ) else {
                return Err(invalid("substituted slot outside the exercise"));
            };
            *slot = PlannedWord {
                id: next.word,
                lane: Lane::Review,
                focus: None,
                due_review: false,
            };
            *piece = next.units;
            *done = true;
            self.evidence.note_reinforcement()?;
        }
        self.queue = waiting;
        let space = exercise
            .units
            .iter()
            .copied()
            .find(|unit| unit.get() == ' ')
            .ok_or_else(|| invalid("word exercise without separators"))?;
        Ok(pieces.join(&space))
    }

    /// Folds a finished or interrupted exercise: completed word outcomes,
    /// the typed passage, and missed words queued for reinforcement.
    pub(super) fn close(
        &mut self,
        exercise: &PlannedExercise,
        ledger: &cadence_core::ledger::AttemptLedger,
    ) -> Result<(), CoreError> {
        self.mark = None;
        if let Some(passage) = exercise.passage
            && ledger.first_attempted().get() > 0
        {
            self.evidence.observe_passage(passage)?;
        }
        if self.words.is_empty() {
            return Ok(());
        }
        let outcomes = ledger.first_outcomes();
        let flags = ledger.word_flags();
        let mut end = 0_usize;
        for (index, word) in std::mem::take(&mut self.words).into_iter().enumerate() {
            let length = ledger
                .target()
                .get(end..)
                .and_then(|rest| rest.iter().position(|unit| unit.get() == ' '))
                .unwrap_or_else(|| ledger.target().len().saturating_sub(end));
            let last = end.checked_add(length).and_then(|e| e.checked_sub(1));
            end = end
                .checked_add(length)
                .and_then(|e| e.checked_add(1))
                .ok_or_else(|| invalid("word boundary overflow"))?;
            let completed = last
                .and_then(|position| outcomes.get(position))
                .is_some_and(|slot| *slot != Slot::Open);
            let Some(flag) = flags.get(index) else {
                return Err(invalid("word flags do not match slots"));
            };
            if !completed || flag.interrupted || flag.pasted {
                continue;
            }
            self.evidence.observe_word(word.id, flag.any_error)?;
            if flag.any_error && exercise.kind == ExerciseKind::Words {
                self.enqueue(word.id, ledger.target(), end, length)?;
            }
        }
        Ok(())
    }

    fn enqueue(
        &mut self,
        word: WordId,
        target: &[TrainingChar],
        end: usize,
        length: usize,
    ) -> Result<(), CoreError> {
        let served = self.served.get(&word).copied().unwrap_or(0);
        // A full queue or an exhausted per-word allowance only forgoes an
        // optional extra repetition; the miss itself is already recorded.
        // The proved kernel decides (no duplicate, within capacity, within
        // the per-word limit).
        match queue::admit(
            self.queue.iter().any(|queued| queued.word == word),
            self.queue.len(),
            self.adaptation.reinforcement_capacity,
            served,
            self.adaptation.reinforcements_per_word,
        ) {
            queue::Admit::Admitted => {}
            queue::Admit::Duplicate | queue::Admit::Full | queue::Admit::Exhausted => {
                return Ok(());
            }
        }
        let start = end
            .checked_sub(1)
            .and_then(|e| e.checked_sub(length))
            .ok_or_else(|| invalid("word span underflow"))?;
        let units = start
            .checked_add(length)
            .and_then(|stop| target.get(start..stop))
            .ok_or_else(|| invalid("word span outside target"))?
            .to_vec();
        self.served.insert(word, served.saturating_add(1));
        self.queue.push_back(Reinforcement { word, units });
        Ok(())
    }
}
