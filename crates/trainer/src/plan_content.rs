//! Startup-only content preparation per phase (plan 0003). Word phases use
//! [`crate::plan_words`]; prose rotates through passages not typed within the
//! no-repeat window, labelling any unavoidable repeat. Numeric phases remain
//! seeded cold-start drills. The engine records exhaustion when it cycles.
use cadence_core::alphabet::TrainingChar;
use cadence_core::label;
use std::cmp::Reverse;

use crate::learner::FocusGroup;
use crate::numeric::number_row_exercise;
use crate::numeric_keypad::numpad_exercise;
use crate::plan::{ExerciseKind, PlanError, PlanInputs, PlanRng, PlannedExercise};
use crate::plan_words::WordSelector;
use crate::policy::{Phase, TrainerPolicy};
use cadence_core::keypad;
use rand::seq::SliceRandom;

/// Per-session options for building a phase's content.
#[derive(Debug, Clone, Copy)]
pub(super) struct PhaseOptions {
    /// Seed of the prose stream (#48).
    pub prose_seed: u64,
    /// Share of word target slots from the core band (#62).
    pub familiar: (u32, u32),
}

pub(super) fn exercises(
    policy: &TrainerPolicy,
    inputs: PlanInputs<'_>,
    rng: &mut PlanRng,
    phase: Phase,
    seconds: u64,
    served_due: &mut std::collections::HashSet<cadence_core::WordId>,
    options: PhaseOptions,
) -> Result<Vec<PlannedExercise>, PlanError> {
    let PhaseOptions {
        prose_seed,
        familiar,
    } = options;
    match phase {
        // The closing review (#49): the reducer fills its slots with the
        // session's misses; this is the fallback when none are left —
        // leftover due words, then focus targets, then core words. Every
        // slot is a non-due review slot, so every slot is substitutable.
        // Probes are carved out of prose after allocation (#64).
        Phase::Probe => Ok(Vec::new()),
        Phase::Review => {
            let mut selector =
                WordSelector::adaptive(policy, inputs.pack, inputs.learner, rng, served_due);
            let mut exercises = words(policy, &mut selector, rng, seconds, ExerciseKind::Words)?;
            for slot in exercises.iter_mut().flat_map(|e| e.words.iter_mut()) {
                slot.lane = crate::policy::Lane::Review;
                slot.due_review = false;
            }
            Ok(exercises)
        }
        Phase::WarmUp => {
            let mut selector = WordSelector::warm_up(policy, inputs.pack, inputs.learner, rng);
            let mut exercises = words(policy, &mut selector, rng, seconds, ExerciseKind::WarmUp)?;
            served_due.extend(selector.served_due().iter().copied());
            // #47: say how many retention checks each warm-up exercise holds.
            for exercise in &mut exercises {
                let checks = exercise.words.iter().filter(|word| word.due_review).count();
                if checks > 0 {
                    exercise.caption = Some(format!(
                        "Familiar words \u{b7} {checks} retention check{}",
                        if checks == 1 { "" } else { "s" }
                    ));
                }
            }
            Ok(exercises)
        }
        Phase::AdaptiveWords => {
            let mut selector =
                WordSelector::adaptive(policy, inputs.pack, inputs.learner, rng, served_due);
            selector.familiar(familiar, rng);
            let exercises = words(policy, &mut selector, rng, seconds, ExerciseKind::Words)?;
            served_due.extend(selector.served_due().iter().copied());
            Ok(exercises)
        }
        Phase::NumberRow => number_row(policy, inputs, rng, seconds),
        Phase::Numpad => numpad(policy, inputs, rng, seconds),
        // Prose has its own stream (#48): its passages never depend on
        // what the adaptive phases drew, nor on the learner's focus.
        Phase::Prose => {
            use rand::SeedableRng as _;
            prose(
                policy,
                inputs,
                &mut PlanRng::seed_from_u64(prose_seed),
                seconds,
            )
        }
    }
}

/// One prepared entry per second, not one five-entry string per 20s.
fn entry_count(seconds: u64) -> Result<usize, PlanError> {
    usize::try_from(seconds).map_err(|_unrepresentable| content("exercise count overflow"))
}

/// Whether entry `index` is one of the `numeric_target_share` entries of
/// its cycle that carry a focus skill (#5).
fn targeted_slot(policy: &TrainerPolicy, index: usize) -> bool {
    let (share, cycle) = policy.learning.numeric_target_share;
    let cycle = usize::try_from(cycle.max(1)).unwrap_or(1);
    let share = usize::try_from(share).unwrap_or(0);
    index.checked_rem(cycle).is_some_and(|i| i < share)
}

/// Number-row entries: the row group's focus (L-01: never the numpad's)
/// in turn in the targeted share, the rest uniform.
fn number_row(
    policy: &TrainerPolicy,
    inputs: PlanInputs<'_>,
    rng: &mut PlanRng,
    seconds: u64,
) -> Result<Vec<PlannedExercise>, PlanError> {
    let count = entry_count(seconds)?;
    let mut used = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(count);
    let focus = inputs.learner.focus_in(FocusGroup::Row);
    let mut turn = 0_usize;
    for index in 0..count {
        let target = if !focus.is_empty() && targeted_slot(policy, index) {
            let key = focus
                .get(turn.checked_rem(focus.len()).unwrap_or(0))
                .copied();
            turn = turn.saturating_add(1);
            key
        } else {
            None
        };
        let units = number_row_exercise(rng, &mut used, target.map(|key| key.skill))
            .map_err(|error| content(&error.to_string()))?;
        let mut exercise = PlannedExercise::plain(units, ExerciseKind::NumberRow);
        exercise.target = target;
        result.push(exercise);
    }
    Ok(result)
}

/// Numpad entries over the whole keypad (#101): each cycle of the
/// policy's `[keypad]` mix serves every family exactly its weight, in a
/// seeded order (the proved `keypad::family_at` over a per-cycle
/// permutation). A targeted entry keeps its family and carries the next
/// focus skill, in turn, that the family can carry (`keypad::carries`);
/// when none can, it is untargeted.
fn numpad(
    policy: &TrainerPolicy,
    inputs: PlanInputs<'_>,
    rng: &mut PlanRng,
    seconds: u64,
) -> Result<Vec<PlannedExercise>, PlanError> {
    let count = entry_count(seconds)?;
    let weights = policy.keypad.weights;
    let operators = inputs.numeric.operators;
    let cycle = keypad::cycle(weights, operators)
        .filter(|cycle| *cycle > 0)
        .ok_or_else(|| content("empty keypad mix"))?;
    let cycle_len =
        usize::try_from(cycle).map_err(|_unrepresentable| content("keypad cycle overflow"))?;
    let mut used = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(count);
    let focus = inputs.learner.focus_in(FocusGroup::Numpad);
    let mut order: Vec<u64> = Vec::with_capacity(cycle_len);
    let mut turn = 0_usize;
    for index in 0..count {
        let within = index.checked_rem(cycle_len).unwrap_or(0);
        if within == 0 {
            order = (0..cycle).collect();
            order.shuffle(rng);
        }
        let position = order.get(within).copied().unwrap_or(0);
        let family = keypad::family_at(position, weights, operators);
        let mut target = None;
        if targeted_slot(policy, index) {
            for offset in 0..focus.len() {
                let at = turn.saturating_add(offset).checked_rem(focus.len());
                let key = at.and_then(|at| focus.get(at)).copied();
                if let Some(key) = key.filter(|key| keypad::carries(family, key.skill)) {
                    target = Some(key);
                    turn = turn.saturating_add(offset).saturating_add(1);
                    break;
                }
            }
        }
        let entry = numpad_exercise(
            rng,
            &mut used,
            family,
            target.map(|key| key.skill),
            inputs.numeric,
            &policy.keypad,
        )
        .map_err(|error| content(&error.to_string()))?;
        let mut exercise = PlannedExercise::plain(entry.into_units(), ExerciseKind::Numpad);
        exercise.target = target;
        result.push(exercise);
    }
    Ok(result)
}

fn content(message: &str) -> PlanError {
    PlanError::Content(message.into())
}

fn words(
    policy: &TrainerPolicy,
    selector: &mut WordSelector<'_>,
    rng: &mut PlanRng,
    seconds: u64,
    kind: ExerciseKind,
) -> Result<Vec<PlannedExercise>, PlanError> {
    let count = usize::try_from(seconds.div_ceil(policy.learning.seconds_per_word_exercise))
        .map_err(|_unrepresentable| content("exercise count overflow"))?;
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let words = selector.block(rng)?;
        result.push(PlannedExercise {
            units: selector.units(&words)?,
            kind,
            words,
            passage: None,
            caption: None,
            target: None,
            spans: Vec::new(),
        });
    }
    Ok(result)
}

/// Pack words inside a prose target (#48): each space-separated token,
/// lower-cased with surrounding punctuation stripped, that is a pack form.
pub(crate) fn word_spans(
    units: &[TrainingChar],
    pack: &crate::corpus::WordPack,
) -> Vec<crate::plan::WordSpan> {
    let mut spans = Vec::new();
    let mut start = 0_usize;
    for (token, piece) in units.split(|unit| unit.get() == ' ').enumerate() {
        let inner = |unit: &TrainingChar| unit.get().is_ascii_alphanumeric();
        let lead = piece.iter().take_while(|unit| !inner(unit)).count();
        let trail = piece.iter().rev().take_while(|unit| !inner(unit)).count();
        let core = piece
            .get(lead..piece.len().saturating_sub(trail))
            .unwrap_or_default();
        let form: String = core
            .iter()
            .map(|unit| unit.get().to_ascii_lowercase())
            .collect();
        if let Some(entry) = (!form.is_empty()).then(|| pack.by_form(&form)).flatten() {
            spans.push(crate::plan::WordSpan {
                id: entry.id,
                token,
                start: start.saturating_add(lead),
                len: core.len(),
                units: entry.units.clone(),
            });
        }
        start = start.saturating_add(piece.len()).saturating_add(1);
    }
    spans
}

/// Seeded passage order: never-recently-typed passages first, then the
/// least recently typed. Enough passages are prepared for the planning rate.
fn prose(
    policy: &TrainerPolicy,
    inputs: PlanInputs<'_>,
    rng: &mut PlanRng,
    seconds: u64,
) -> Result<Vec<PlannedExercise>, PlanError> {
    let learning = &policy.learning;
    let needed = seconds
        .checked_mul(learning.prose_units_per_second)
        .and_then(|units| usize::try_from(units).ok())
        .ok_or_else(|| content("prose volume overflow"))?;
    let passages = inputs.prose.passages();
    let mut order: Vec<usize> = (0..passages.len()).collect();
    order.shuffle(rng);
    let age = |index: &usize| {
        passages
            .get(*index)
            .and_then(|passage| inputs.learner.passage_age(passage.id))
    };
    let (fresh, mut stale): (Vec<usize>, Vec<usize>) =
        order.into_iter().partition(|index| age(index).is_none());
    stale.sort_by_key(|index| Reverse(age(index)));
    let mut prepared = 0_usize;
    let mut result = Vec::new();
    for index in fresh.into_iter().chain(stale) {
        if prepared >= needed {
            break;
        }
        let passage = passages
            .get(index)
            .ok_or_else(|| content("passage index out of range"))?;
        let source = inputs
            .prose
            .source(passage)
            .ok_or_else(|| content("passage source missing"))?;
        let mut caption = format!("{} \u{2014} {}", source.title, source.author);
        let age = inputs.learner.passage_age(passage.id);
        // Whether the passage is shown as a repeat is the proved
        // `cadence_kernel::label::of`.
        let shown = label::of(label::Facts {
            served_before: 0,
            reinforcement: false,
            due_review: false,
            wrapped: false,
            recent_passage: age.is_some(),
        });
        if let (label::Label::Repeat, Some(sessions)) = (shown, age) {
            let ago = sessions.saturating_add(1);
            caption.push_str(&format!(
                " \u{b7} repeat, typed {ago} session{} ago",
                if ago == 1 { "" } else { "s" }
            ));
        }
        for units in crate::prose::chunk_passage(&passage.units, learning.prose_exercise_units)
            .map_err(|error| content(&error.to_string()))?
        {
            prepared = prepared.saturating_add(units.len());
            let spans = word_spans(&units, inputs.pack);
            result.push(PlannedExercise {
                units,
                kind: ExerciseKind::Prose,
                words: Vec::new(),
                passage: Some(passage.id),
                caption: Some(caption.clone()),
                target: None,
                spans,
            });
        }
    }
    if result.is_empty() {
        return Err(content("prose bank empty"));
    }
    Ok(result)
}
