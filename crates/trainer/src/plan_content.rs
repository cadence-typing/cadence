//! Startup-only content preparation per phase (plan 0003). Word phases use
//! [`crate::plan_words`]; prose rotates through passages not typed within the
//! no-repeat window, labelling any unavoidable repeat. Numeric phases remain
//! seeded cold-start drills. The engine records exhaustion when it cycles.
use cadence_core::label;
use std::cmp::Reverse;

use crate::numeric::{number_row_exercise, numpad_exercise};
use crate::plan::{ExerciseKind, PlanError, PlanInputs, PlanRng, PlannedExercise};
use crate::plan_words::WordSelector;
use crate::policy::{Phase, TrainerPolicy};
use rand::seq::SliceRandom;

pub(super) fn exercises(
    policy: &TrainerPolicy,
    inputs: PlanInputs<'_>,
    rng: &mut PlanRng,
    phase: Phase,
    seconds: u64,
) -> Result<Vec<PlannedExercise>, PlanError> {
    match phase {
        Phase::Review => Ok(Vec::new()),
        Phase::WarmUp => {
            let selector = WordSelector::warm_up(policy, inputs.pack, inputs.learner, rng);
            words(policy, selector, rng, seconds, ExerciseKind::WarmUp)
        }
        Phase::AdaptiveWords => {
            let selector = WordSelector::adaptive(policy, inputs.pack, inputs.learner, rng);
            words(policy, selector, rng, seconds, ExerciseKind::Words)
        }
        Phase::NumberRow | Phase::Numpad => {
            // One prepared entry per second, not one five-entry string per 20s.
            let count = usize::try_from(seconds)
                .map_err(|_unrepresentable| content("exercise count overflow"))?;
            let mut used = std::collections::HashSet::new();
            let mut result = Vec::with_capacity(count);
            for _ in 0..count {
                let (units, kind) = if phase == Phase::Numpad {
                    (numpad_exercise(rng, &mut used), ExerciseKind::Numpad)
                } else {
                    (number_row_exercise(rng, &mut used), ExerciseKind::NumberRow)
                };
                result.push(PlannedExercise::plain(
                    units.map_err(|error| content(&error.to_string()))?,
                    kind,
                ));
            }
            Ok(result)
        }
        Phase::Prose => prose(policy, inputs, rng, seconds),
    }
}

fn content(message: &str) -> PlanError {
    PlanError::Content(message.into())
}

fn words(
    policy: &TrainerPolicy,
    mut selector: WordSelector<'_>,
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
        });
    }
    Ok(result)
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
            result.push(PlannedExercise {
                units,
                kind: ExerciseKind::Prose,
                words: Vec::new(),
                passage: Some(passage.id),
                caption: Some(caption.clone()),
            });
        }
    }
    if result.is_empty() {
        return Err(content("prose bank empty"));
    }
    Ok(result)
}
