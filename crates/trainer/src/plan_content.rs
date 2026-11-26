//! Startup-only content preparation per phase (plan 0003). Word phases use
//! [`crate::plan_words`]; prose rotates through passages not typed within the
//! no-repeat window, labelling any unavoidable repeat. Numeric phases remain
//! seeded cold-start drills. The engine records exhaustion when it cycles.
use cadence_core::alphabet::TrainingChar;
use cadence_core::label;
use std::cmp::Reverse;

use crate::learner::FocusGroup;
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
    served_due: &mut std::collections::HashSet<cadence_core::WordId>,
    prose_seed: u64,
) -> Result<Vec<PlannedExercise>, PlanError> {
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
            let exercises = words(policy, &mut selector, rng, seconds, ExerciseKind::Words)?;
            served_due.extend(selector.served_due().iter().copied());
            Ok(exercises)
        }
        Phase::NumberRow | Phase::Numpad => {
            // One prepared entry per second, not one five-entry string per 20s.
            let count = usize::try_from(seconds)
                .map_err(|_unrepresentable| content("exercise count overflow"))?;
            let mut used = std::collections::HashSet::new();
            let mut result = Vec::with_capacity(count);
            // #5: the group's focus (L-01: the number row and the numpad
            // never share one); `numeric_target_share` of the entries are
            // built around a focus skill in turn, the rest stay uniform.
            let group = if phase == Phase::Numpad {
                FocusGroup::Numpad
            } else {
                FocusGroup::Row
            };
            let focus = inputs.learner.focus_in(group);
            let (share, cycle) = policy.learning.numeric_target_share;
            let cycle = usize::try_from(cycle.max(1)).unwrap_or(1);
            let share = usize::try_from(share).unwrap_or(0);
            let mut turn = 0_usize;
            for index in 0..count {
                let targeted =
                    !focus.is_empty() && index.checked_rem(cycle).is_some_and(|i| i < share);
                let target = if targeted {
                    let key = focus
                        .get(turn.checked_rem(focus.len()).unwrap_or(0))
                        .copied();
                    turn = turn.saturating_add(1);
                    key
                } else {
                    None
                };
                let skill = target.map(|key| key.skill);
                let (units, kind) = if phase == Phase::Numpad {
                    (numpad_exercise(rng, &mut used, skill), ExerciseKind::Numpad)
                } else {
                    (
                        number_row_exercise(rng, &mut used, skill),
                        ExerciseKind::NumberRow,
                    )
                };
                let mut exercise = PlannedExercise::plain(
                    units.map_err(|error| content(&error.to_string()))?,
                    kind,
                );
                exercise.target = target;
                result.push(exercise);
            }
            Ok(result)
        }
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
