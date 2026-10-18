//! Startup-only content preparation. No learner evidence is fabricated: lane
//! slots are cold-start sampling, not completed reviews or adapted weaknesses.
//! The engine records exhaustion when it cycles these prepared targets.
use crate::candidate::{Candidate, UtilityInputs, scan_select};
use crate::corpus::{WordBand, WordPack};
use crate::lanes::QuotaTracker;
use crate::numeric::{DigitSource, number_row_exercise, numpad_exercise};
use crate::plan::{ExerciseKind, PlanError, PlanRng, PlannedExercise};
use crate::policy::{Phase, TrainerPolicy};
use cadence_core::alphabet::{LayoutSupportsPound, TrainingChar};
use cadence_core::{BasisPoints, WordId};
use rand::seq::SliceRandom;

pub(super) fn exercises(
    policy: &TrainerPolicy,
    pack: &WordPack,
    bank: &str,
    rng: &mut PlanRng,
    phase: Phase,
    seconds: u64,
) -> Result<Vec<PlannedExercise>, PlanError> {
    match phase {
        Phase::Review => Ok(Vec::new()),
        Phase::WarmUp => words(policy, pack, rng, seconds, true),
        Phase::AdaptiveWords => words(policy, pack, rng, seconds, false),
        Phase::NumberRow | Phase::Numpad => {
            // One prepared entry per second, not one five-entry string per 20s.
            let count = usize::try_from(seconds).map_err(|_| content("exercise count overflow"))?;
            let mut used = std::collections::HashSet::new();
            let mut result = Vec::with_capacity(count);
            for _ in 0..count {
                let (units, kind) = if phase == Phase::Numpad {
                    (numpad_exercise(rng, &mut used), ExerciseKind::Numpad)
                } else {
                    (number_row_exercise(rng, &mut used), ExerciseKind::NumberRow)
                };
                result.push(PlannedExercise {
                    units: units.map_err(|error| content(&error.to_string()))?,
                    kind,
                });
            }
            Ok(result)
        }
        Phase::Prose => crate::prose::prose_exercises(bank, 120)
            .map_err(|error| content(&error.to_string()))?
            .into_iter()
            .map(|units| {
                Ok(PlannedExercise {
                    units,
                    kind: ExerciseKind::Prose,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .and_then(|result| {
                if result.is_empty() {
                    Err(content("prose bank empty"))
                } else {
                    Ok(result)
                }
            }),
    }
}

fn content(message: &str) -> PlanError {
    PlanError::Content(message.into())
}

fn words(
    policy: &TrainerPolicy,
    pack: &WordPack,
    rng: &mut PlanRng,
    seconds: u64,
    warmup: bool,
) -> Result<Vec<PlannedExercise>, PlanError> {
    // Shuffle once at startup, not on each key or slot. Interleaving independently
    // shuffled bands keeps small viable bands visible without frequency claims.
    let core_only = warmup && pack.band(WordBand::Core).len() >= policy.selection.block_slots;
    let bands: &[WordBand] = if core_only {
        &[WordBand::Core]
    } else {
        &[WordBand::Core, WordBand::Common, WordBand::Tail]
    };
    let mut streams: Vec<_> = bands
        .iter()
        .map(|band| {
            let mut ids: Vec<WordId> = pack
                .band(*band)
                .iter()
                .filter_map(|index| pack.words().get(*index).map(|entry| entry.id))
                .collect();
            ids.shuffle(rng);
            ids.into_iter()
        })
        .collect();
    let mut pool = Vec::with_capacity(if core_only {
        pack.band(WordBand::Core).len()
    } else {
        pack.len()
    });
    loop {
        let before = pool.len();
        for stream in &mut streams {
            if let Some(id) = stream.next() {
                pool.push(id);
            }
        }
        if before == pool.len() {
            break;
        }
    }
    if pool.is_empty() {
        return Err(PlanError::NoEligibleCandidate {
            lane: crate::policy::Lane::Broad,
        });
    }
    let count =
        usize::try_from(seconds.div_ceil(5)).map_err(|_| content("exercise count overflow"))?;
    let mut cursor = 0usize;
    let mut result = Vec::with_capacity(count);
    let space = TrainingChar::new(' ', LayoutSupportsPound::US)
        .map_err(|error| content(&error.to_string()))?;
    let mut candidates = Vec::with_capacity(policy.selection.maximum_candidates_per_slot);
    for _ in 0..count {
        let mut tracker = QuotaTracker::new(policy.selection.clone());
        let mut units = Vec::new();
        for lane in &policy.selection.lane_order {
            candidates.clear();
            // At most the configured bound is visited, including ineligible
            // entries. Since bound >= block slots, a viable pool cannot hide
            // entirely behind fewer-than-block-slots already used words.
            let after = pool
                .get(cursor..)
                .ok_or_else(|| content("pool cursor out of bounds"))?;
            let before = pool
                .get(..cursor)
                .ok_or_else(|| content("pool cursor out of bounds"))?;
            for id in after
                .iter()
                .chain(before)
                .take(pool.len().min(policy.selection.maximum_candidates_per_slot))
            {
                candidates.push(Candidate {
                    id: *id,
                    eligible: tracker.word_admissible(*id, *lane, None),
                    inputs: UtilityInputs {
                        error: BasisPoints::ZERO,
                        hesitation: BasisPoints::ZERO,
                        relevance: BasisPoints::ZERO,
                        uncertainty: BasisPoints::ZERO,
                    },
                    tie_break: rng.below(u64::MAX),
                });
            }
            let selected = scan_select(
                &candidates,
                &policy.weights,
                policy.selection.maximum_candidates_per_slot,
            )
            .map_err(|error| content(&error.to_string()))?
            .ok_or(PlanError::NoEligibleCandidate { lane: *lane })?;
            tracker
                .record(*lane, selected, None)
                .map_err(|error| content(&error.to_string()))?;
            let entry = pack
                .word(selected)
                .ok_or_else(|| content("selected word missing"))?;
            if !units.is_empty() {
                units.push(space);
            }
            units.extend_from_slice(&entry.units);
            cursor = cursor
                .checked_add(1)
                .ok_or_else(|| content("pool cursor overflow"))?;
            if cursor == pool.len() {
                cursor = 0;
            }
        }
        result.push(PlannedExercise {
            units,
            kind: if warmup {
                ExerciseKind::WarmUp
            } else {
                ExerciseKind::Words
            },
        });
    }
    Ok(result)
}
