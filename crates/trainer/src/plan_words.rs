//! Word slot selection (plan 0003, L-05). Startup-only and bounded:
//! every slot scans at most `maximum_candidates_per_slot` candidates.
//!
//! * Target slots: rotate over the learner's focus skills; candidates are
//!   core/common words containing the skill, ranked by the existing utility
//!   (error, hesitation, frequency relevance, uncertainty) with a seeded
//!   tie-break. With no focus (cold start) they sample the target bands.
//! * Review slots: due missed words, FIFO; otherwise a broad sample.
//! * Broad slots: seeded sample of all bands, keeping vocabulary breadth.
use std::collections::{HashMap, HashSet, VecDeque};

use cadence_core::alphabet::TrainingChar;
use cadence_core::learning::{Skill, SkillKey};
use cadence_core::{BasisPoints, WordId, lanes};
use rand::seq::SliceRandom;

use crate::candidate::{Candidate, UtilityInputs, scan_select};
use crate::corpus::{WordBand, WordPack};
use crate::lanes::QuotaTracker;
use crate::learner::LearnerModel;
use crate::plan::{PlanError, PlanRng, PlannedWord};
use crate::policy::{Lane, TrainerPolicy};

/// One rotating candidate stream.
struct Stream {
    ids: Vec<WordId>,
    cursor: usize,
}

impl Stream {
    fn new(mut ids: Vec<WordId>, rng: &mut PlanRng) -> Self {
        ids.shuffle(rng);
        Self::ordered(ids)
    }

    const fn ordered(ids: Vec<WordId>) -> Self {
        Self { ids, cursor: 0 }
    }

    /// The bounded window starting at the cursor, wrapping once.
    fn window(&self, bound: usize) -> impl Iterator<Item = WordId> + '_ {
        let (before, after) = self.ids.split_at(self.cursor.min(self.ids.len()));
        after.iter().chain(before).take(bound).copied()
    }

    fn advance(&mut self) {
        self.cursor = match self.cursor.checked_add(1) {
            Some(next) if next < self.ids.len() => next,
            _ => 0,
        };
    }
}

/// Selects words for one phase.
pub(super) struct WordSelector<'a> {
    policy: &'a TrainerPolicy,
    pack: &'a WordPack,
    learner: &'a LearnerModel,
    broad: Stream,
    targets: Stream,
    focus: Vec<(SkillKey, Stream)>,
    due: VecDeque<WordId>,
    session_counts: HashMap<WordId, u32>,
    /// Every word planned by this selector (the phase), so a sampled slot
    /// never repeats one while an unplanned word is admissible (#44).
    planned: HashSet<WordId>,
    /// The exercise (block) ordinal of each target word's last use, for
    /// the proved repeat gap (#58).
    last_target: HashMap<WordId, u32>,
    /// The ordinal of the block being selected.
    exercise: u32,
    focus_turn: usize,
}

impl<'a> WordSelector<'a> {
    /// Adaptive words phase selector.
    pub(super) fn adaptive(
        policy: &'a TrainerPolicy,
        pack: &'a WordPack,
        learner: &'a LearnerModel,
        rng: &mut PlanRng,
    ) -> Self {
        let target_bands = target_bands(policy);
        // The words phase targets the letters group (#46).
        let focus = learner
            .focus_in(crate::learner::FocusGroup::Letters)
            .iter()
            .filter_map(|key| {
                let ids = words_in(pack, target_bands, |units| contains(units, key.skill));
                (!ids.is_empty()).then(|| (*key, Stream::new(ids, rng)))
            })
            .collect();
        Self {
            policy,
            pack,
            learner,
            broad: Stream::ordered(interleaved(
                pack,
                &[WordBand::Core, WordBand::Common, WordBand::Tail],
                rng,
            )),
            targets: Stream::new(words_in(pack, target_bands, |_| true), rng),
            focus,
            due: learner.due_words().iter().copied().collect(),
            session_counts: HashMap::new(),
            planned: HashSet::new(),
            last_target: HashMap::new(),
            exercise: 0,
            focus_turn: 0,
        }
    }

    /// Warm-up selector: familiar core words, no evidence-driven slots.
    pub(super) fn warm_up(
        policy: &'a TrainerPolicy,
        pack: &'a WordPack,
        learner: &'a LearnerModel,
        rng: &mut PlanRng,
    ) -> Self {
        let core = words_in(pack, &[WordBand::Core], |_| true);
        Self {
            policy,
            pack,
            learner,
            broad: Stream::new(core.clone(), rng),
            targets: Stream::new(core, rng),
            focus: Vec::new(),
            due: VecDeque::new(),
            session_counts: HashMap::new(),
            planned: HashSet::new(),
            last_target: HashMap::new(),
            exercise: 0,
            focus_turn: 0,
        }
    }

    /// Selects one block of words in lane order.
    pub(super) fn block(&mut self, rng: &mut PlanRng) -> Result<Vec<PlannedWord>, PlanError> {
        let mut tracker = QuotaTracker::new(self.policy.selection.clone());
        let mut words = Vec::with_capacity(self.policy.selection.block_slots);
        for lane in &self.policy.selection.lane_order {
            let chosen = match lane {
                Lane::Target => match self.focused(&tracker, rng)? {
                    Some(word) => Some(word),
                    None => self.sampled(&tracker, Lane::Target)?,
                },
                Lane::Review => match self.due_review(&tracker) {
                    Some(word) => Some(word),
                    None => self.sampled(&tracker, Lane::Review)?,
                },
                Lane::Broad => self.sampled(&tracker, Lane::Broad)?,
            };
            let word = chosen.ok_or(PlanError::NoEligibleCandidate { lane: *lane })?;
            let focus_id = self.focus_index(word.focus);
            tracker
                .record(word.lane, word.id, focus_id)
                .map_err(|error| PlanError::Content(error.to_string()))?;
            if word.lane == Lane::Target {
                let count = self.session_counts.entry(word.id).or_default();
                *count = count.saturating_add(1);
                self.last_target.insert(word.id, self.exercise);
            }
            self.planned.insert(word.id);
            words.push(word);
        }
        self.exercise = self.exercise.saturating_add(1);
        Ok(words)
    }

    /// The units of a block, space-separated.
    pub(super) fn units(&self, words: &[PlannedWord]) -> Result<Vec<TrainingChar>, PlanError> {
        let space = TrainingChar::new(' ', cadence_core::LayoutSupportsPound::US)
            .map_err(|error| PlanError::Content(error.to_string()))?;
        let mut units = Vec::new();
        for word in words {
            let entry = self
                .pack
                .word(word.id)
                .ok_or_else(|| PlanError::Content("selected word missing".into()))?;
            if !units.is_empty() {
                units.push(space);
            }
            units.extend_from_slice(&entry.units);
        }
        Ok(units)
    }

    fn focus_index(&self, focus: Option<SkillKey>) -> Option<u64> {
        let key = focus?;
        let index = self
            .focus
            .iter()
            .position(|(candidate, _)| *candidate == key)?;
        u64::try_from(index).ok()
    }

    fn focused(
        &mut self,
        tracker: &QuotaTracker,
        rng: &mut PlanRng,
    ) -> Result<Option<PlannedWord>, PlanError> {
        let count = self.focus.len();
        for offset in 0..count {
            let index = self
                .focus_turn
                .checked_add(offset)
                .and_then(|turn| turn.checked_rem(count))
                .ok_or_else(|| PlanError::Content("focus rotation overflow".into()))?;
            let focus_id = u64::try_from(index).ok();
            let Some((key, stream)) = self.focus.get(index) else {
                continue;
            };
            let candidates = self.candidates(stream, tracker, Lane::Target, focus_id, rng);
            if let Some(id) = self.select(&candidates)? {
                let key = *key;
                if let Some((_, stream)) = self.focus.get_mut(index) {
                    stream.advance();
                }
                self.focus_turn = index.saturating_add(1);
                return Ok(Some(PlannedWord {
                    id,
                    lane: Lane::Target,
                    focus: Some(key),
                    due_review: false,
                }));
            }
        }
        Ok(None)
    }

    fn due_review(&mut self, tracker: &QuotaTracker) -> Option<PlannedWord> {
        while let Some(id) = self.due.pop_front() {
            if self.pack.word(id).is_some() && tracker.word_admissible(id, Lane::Review, None) {
                return Some(PlannedWord {
                    id,
                    lane: Lane::Review,
                    focus: None,
                    due_review: true,
                });
            }
        }
        None
    }

    /// Seeded sample without replacement (#44): the next admissible word
    /// at the head of the already shuffled stream; the cursor moves past
    /// it, so no word returns until the stream wraps. Target lanes draw from
    /// the target bands, others broadly. Admissibility is the proved lane
    /// decision; at most `maximum_candidates_per_slot` words are examined.
    fn sampled(
        &mut self,
        tracker: &QuotaTracker,
        lane: Lane,
    ) -> Result<Option<PlannedWord>, PlanError> {
        let bound = self.policy.selection.maximum_candidates_per_slot;
        let stream = if lane == Lane::Target {
            &self.targets
        } else {
            &self.broad
        };
        let len = stream.ids.len();
        let mut found = None;
        let mut fallback = None;
        for offset in 0..bound.min(len) {
            let index = stream
                .cursor
                .checked_add(offset)
                .and_then(|i| i.checked_rem(len))
                .ok_or_else(|| PlanError::Content("stream index overflow".into()))?;
            let Some(id) = stream.ids.get(index).copied() else {
                continue;
            };
            if self.pack.word(id).is_none() || !self.eligible(tracker, lane, None, id) {
                continue;
            }
            if !self.planned.contains(&id) {
                found = Some((index, id));
                break;
            }
            // Planned earlier in the phase: only if nothing unplanned is
            // admissible within the window (a small pack wraps).
            fallback = fallback.or(Some((index, id)));
        }
        let found = found.or(fallback);
        let Some((index, id)) = found else {
            return Ok(None);
        };
        let stream = if lane == Lane::Target {
            &mut self.targets
        } else {
            &mut self.broad
        };
        stream.cursor = index
            .checked_add(1)
            .and_then(|next| next.checked_rem(len))
            .unwrap_or(0);
        Ok(Some(PlannedWord {
            id,
            lane,
            focus: None,
            due_review: false,
        }))
    }

    /// Lane eligibility: the proved quota and cap decision, plus the
    /// per-session cap on target words and the proved repeat gap (#58).
    fn eligible(
        &self,
        tracker: &QuotaTracker,
        lane: Lane,
        focus_id: Option<u64>,
        id: WordId,
    ) -> bool {
        let within_session = self.session_counts.get(&id).copied().unwrap_or(0)
            < self
                .policy
                .learning
                .maximum_target_word_occurrences_per_session;
        let last = self.last_target.get(&id).copied();
        let gap = lanes::gap_admissible(
            last.is_some(),
            last.unwrap_or(0),
            self.exercise,
            self.policy.learning.minimum_exercises_between_word_repeats,
        );
        tracker.word_admissible(id, lane, focus_id)
            && (lane != Lane::Target || (within_session && gap))
    }

    fn candidates(
        &self,
        stream: &Stream,
        tracker: &QuotaTracker,
        lane: Lane,
        focus_id: Option<u64>,
        rng: &mut PlanRng,
    ) -> Vec<Candidate> {
        use crate::numeric::DigitSource as _;
        let bound = self.policy.selection.maximum_candidates_per_slot;
        let learning = &self.policy.learning;
        stream
            .window(bound)
            .filter_map(|id| self.pack.word(id).map(|entry| (id, entry)))
            .map(|(id, entry)| {
                let eligible = self.eligible(tracker, lane, focus_id, id);
                let inputs = if focus_id.is_some() {
                    let score = self.learner.word_score(&entry.units);
                    UtilityInputs {
                        error: score.error,
                        hesitation: score.hesitation,
                        relevance: match entry.band {
                            WordBand::Core => learning.core_relevance,
                            WordBand::Common => learning.common_relevance,
                            WordBand::Tail => BasisPoints::ZERO,
                        },
                        uncertainty: score.uncertainty,
                    }
                } else {
                    UtilityInputs {
                        error: BasisPoints::ZERO,
                        hesitation: BasisPoints::ZERO,
                        relevance: BasisPoints::ZERO,
                        uncertainty: BasisPoints::ZERO,
                    }
                };
                Candidate {
                    id,
                    eligible,
                    inputs,
                    tie_break: rng.below(u64::MAX),
                }
            })
            .collect()
    }

    fn select(&self, candidates: &[Candidate]) -> Result<Option<WordId>, PlanError> {
        scan_select(
            candidates,
            &self.policy.weights,
            self.policy.selection.maximum_candidates_per_slot,
        )
        .map_err(|error| PlanError::Content(error.to_string()))
    }
}

fn target_bands(policy: &TrainerPolicy) -> &'static [WordBand] {
    match policy.learning.target_maximum_band {
        WordBand::Core => &[WordBand::Core],
        WordBand::Common | WordBand::Tail => &[WordBand::Core, WordBand::Common],
    }
}

fn words_in(
    pack: &WordPack,
    bands: &[WordBand],
    keep: impl Fn(&[TrainingChar]) -> bool,
) -> Vec<WordId> {
    bands
        .iter()
        .flat_map(|band| pack.band(*band).iter())
        .filter_map(|index| pack.words().get(*index))
        .filter(|entry| keep(&entry.units))
        .map(|entry| entry.id)
        .collect()
}

/// Independently shuffled bands interleaved round-robin, so small bands stay
/// visible without frequency claims.
fn interleaved(pack: &WordPack, bands: &[WordBand], rng: &mut PlanRng) -> Vec<WordId> {
    let mut streams: Vec<_> = bands
        .iter()
        .map(|band| {
            let mut ids = words_in(pack, &[*band], |_| true);
            ids.shuffle(rng);
            ids.into_iter()
        })
        .collect();
    let mut pool = Vec::with_capacity(pack.len());
    loop {
        let before = pool.len();
        for stream in &mut streams {
            if let Some(id) = stream.next() {
                pool.push(id);
            }
        }
        if before == pool.len() {
            return pool;
        }
    }
}

fn contains(units: &[TrainingChar], skill: Skill) -> bool {
    match skill {
        Skill::Unit(unit) => units.contains(&unit),
        Skill::Bigram(first, second) => units
            .windows(2)
            .any(|pair| matches!(pair, [a, b] if *a == first && *b == second)),
    }
}
