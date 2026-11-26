---
rfc: "2026-11-26-practice-decisions-in-the-kernel"
title: "Practice decisions in the kernel"
status: accepted
implementation: not-started
authors:
  - tknawara
  - claude-fable (review agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/26
milestone: "0.1.0-alpha.7"
supersedes: []
superseded_by: []
blocked_on:
  - https://github.com/cadence-typing/cadence/issues/28
---

# RFC: Practice decisions in the kernel

Second of three RFCs from the 2026-11-26 review. It moves the decisions that choose *what the learner practises* into `crates/kernel` with Lean claims, before [the third RFC](2026-11-26-deliberate-practice-in-every-phase.md) wires them into every phase. It depends on the gate from [the first](2026-11-26-formal-gates-and-production-mapping.md).

## Summary and motivation

Since alpha.4 the planner decides which skills are weak, which words fill which slots, when a missed word comes back and whether it is promoted on the review ladder. None of that is proved. All of it lives in `cadence-trainer` and `cadence-engine` as tested code, and part of it is floating point:

- `trainer/src/learner.rs` computes decay as `0.5_f64.powf(age / half_life)` and ranks focus skills on `f64` sums. `powf` is not guaranteed bit-identical across libm implementations, so the "same inputs, same plan" contract (L-05) rests on an unstated platform assumption.
- `trainer/src/allocation.rs` (VK-03), `candidate.rs::scan_select` (VK-04), `lanes.rs::QuotaTracker` (VK-05) and `review.rs` (VK-06) are named proof targets in `docs/07-assurance.md` and question 6 of the [tauten RFC](2026-11-24-tauten-and-lean-kernel.md). They have no claim.
- Two confirmed defects are decision bugs a theorem would have excluded: same-session reinforcement can put a word twice in one exercise and can overwrite a due spaced review (`engine/src/learning.rs::open` replaces any `Lane::Review` slot without looking at `due_review` or at the other words). See #43.

The third RFC extends these decisions to warm-up, number row, numpad, prose and a typed closing review. Proving them once, first, means each new phase calls a proved function instead of growing another tested copy.

Outside this decision: what each phase serves (third RFC), pace hysteresis and benchmark probes (#6), human learning benefit (not a theorem in this project).

## Observable behaviour

Scores, timing and what counts as an error do not change. Two things a learner could notice:

- **Focus choice may differ slightly.** Decay becomes an exact integer function (below) instead of `powf`. Around a threshold a skill may enter or leave the focus list one session earlier or later than under alpha.5. The plan is now identical on every platform for the same history and seed.
- **No unlabelled repeat and no displaced review** once the third RFC wires the slot and label decisions (#43, #7).

## Proof obligations

All entry points are total functions in `crates/kernel` over integers, slices of integers or small `Copy` structs of integers. Signatures below fix the facts in and the decision out; the implementer may reshape them to what the pinned Aeneas accepts (one early-return loop per function, `while` over `get`, no iterator chains, no floats), provided the theorem statements keep their meaning. Every function also carries a `total` theorem. Each claim lists its production caller in `proofs/claims.toml` and `docs/TAUTEN.md`.

### 1. Phase allocation (VK-03) — `plan::allocate`

Moves `trainer::allocation::allocate`. Facts: budget, per-phase minimums and weights (at most `MAX_PHASES`). Decision: seconds per phase, or `Infeasible`, `NoPhases`, `Overflow`.

| Theorem | Statement | Mutant |
|---|---|---|
| `Plan.allocate_sum_exact` | on success the seconds sum to `budget` | `leftover-dropped` |
| `Plan.allocate_keeps_minimums` | each phase gets at least its minimum | `minimum-not-added` |
| `Plan.allocate_infeasible_exact` | `Infeasible` iff the minimums sum above the budget | `infeasible-clamped` |
| `Plan.allocate_tie_to_lower_index` | among equal remainders the extra second goes to the lower index | `tie-to-higher-index` |

Caller: `trainer::plan::build_session_plan`.

### 2. Bounded candidate scan (VK-04) — `select::utility`, `select::scan`

Moves `candidate::utility` and `scan_select`.

| Theorem | Statement | Mutant |
|---|---|---|
| `Select.utility_in_range` | result ≤ 10,000 and is the weighted sum over the sum of weights | `utility-unnormalized` |
| `Select.utility_monotone` | raising any input never lowers the result | `utility-subtracts-hesitation` |
| `Select.scan_only_eligible` | a returned index is eligible | `scan-ignores-eligibility` |
| `Select.scan_optimal` | no eligible candidate has a greater `(utility, tie_break, id)` | `scan-keeps-first` |
| `Select.scan_none_iff_no_eligible` | `None` iff nothing is eligible | `scan-none-on-tie` |
| `Select.scan_bounded` | more than `bound` candidates is `TooLarge`, never a partial scan | `bound-off-by-one` |

Caller: `trainer::plan_words::WordSelector::select`.

### 3. Block quotas and caps (VK-05) — `lanes::admissible`, `lanes::record`

Moves `QuotaTracker` counting (the maps stay in the trainer; the kernel gets counts).

| Theorem | Statement | Mutant |
|---|---|---|
| `Lanes.never_overrun` | after any `record`, served ≤ quota for that lane | `quota-off-by-one` |
| `Lanes.word_cap` | a word at its per-block cap is not admissible | `word-cap-ignored` |
| `Lanes.focus_cap` | a focus at its owned-slot cap is not admissible in the target lane | `focus-cap-ignored` |
| `Lanes.record_refuses_inadmissible` | `record` succeeds iff `admissible` | `record-always-ok` |
| `Lanes.complete_block_exact` | all quotas met iff served counts equal the quotas | `complete-allows-under` |

### 4. Review ladder and queues (VK-06) — `review::ladder_step`, `review::is_due`, `queue::admit`

Moves `trainer::review::DueQueue::next_state` and the admission rule of the engine's reinforcement queue. Days are non-negative day numbers supplied by the caller.

| Theorem | Statement | Mutant |
|---|---|---|
| `Review.error_resets` | an error gives rung 0, no streak, due = day + first interval | `error-keeps-rung` |
| `Review.due_is_future` | every step's due day is strictly after its outcome day | `due-today` |
| `Review.early_clean_is_practice` | a clean outcome before the due day leaves the state unchanged | `early-clean-counts` |
| `Review.promotion_needs_two_days` | the rung rises only when the streak reaches the policy count on distinct days, each on or after its due day | `same-day-promotes`, `first-clean-promotes` |
| `Review.rung_bounded` | the rung never exceeds the ladder top | `rung-wraps` |
| `Review.rollback_refused` | an outcome dated before the recorded streak day is `Backward`, state unchanged | `rollback-accepted` |
| `Queue.admit_no_duplicate` | a queued word is never admitted again | `admit-duplicate` |
| `Queue.admit_bounded` | admission never exceeds capacity; a full queue reports `Full` | `capacity-off-by-one` |
| `Queue.per_word_bound` | a word at its per-session reinforcement count is `Exhausted` | `per-word-ignored` |

Callers: `trainer::learner::due_words`, `engine::learning::Learning::enqueue`.

### 5. Focus selection — `focus::decay_weight`, `focus::posterior_bp`, `focus::hesitation_bp`, `focus::qualifies`, `focus::pick`

Replaces the `f64` model in `trainer::learner`. Evidence becomes fixed point (`WEIGHT_ONE = 2^16` per undecayed attempt).

- `decay_weight(age, half_life)`: `(WEIGHT_ONE >> (age / h)) * (2h - age % h) / (2h)`; exact halving at multiples of the half-life, linear in between.
- `posterior_bp(errors_w, attempts_w, alpha, beta)`: `(errors_w + alpha·ONE)·10000 / (attempts_w + (alpha+beta)·ONE)`.
- `hesitation_bp(skill_timed, skill_micros, group_timed, group_micros, minimum_samples)`: 0 below the sample minimum; otherwise the skill mean over the group mean minus one, clamped to 0..=10,000.
- `qualifies(candidate, thresholds)` and `pick(candidates, chosen)`: the best qualifying candidate not yet chosen, ties to the lower index (the caller sorts candidates by skill key, so the order is deterministic).

| Theorem | Statement | Mutant |
|---|---|---|
| `Focus.decay_one_at_zero`, `Focus.decay_halves`, `Focus.decay_antitone` | weight is `WEIGHT_ONE` at age 0, exactly halves each half-life, never increases with age | `decay-grows`, `decay-skips-halving` |
| `Focus.posterior_in_range`, `Focus.posterior_prior_at_zero`, `Focus.posterior_monotone_errors` | ≤ 10,000; no evidence gives the prior mean; more errors at equal attempts never lowers it | `posterior-drops-prior` |
| `Focus.hesitation_needs_samples` | below the sample minimum the result is 0 | `hesitation-ignores-minimum` |
| `Focus.sparse_never_qualifies` | below the attempt minimum nothing qualifies (sparse is uncertain, never a focus and never "mastered") | `sparse-qualifies` |
| `Focus.quiet_never_qualifies` | zero errors and hesitation below its threshold never qualifies | `threshold-inverted` |
| `Focus.pick_sound` | a picked index qualifies and is not already chosen | `pick-ignores-chosen` |
| `Focus.pick_maximal` | no qualifying unchosen candidate has a greater signal | `pick-first-not-best` |

Caller: `trainer::learner::LearnerModel::from_history` (builds integer evidence, calls `pick` up to the policy count per focus group).

### 6. Slot sources and labels — `slots::substitute`, `label::of`, `cycle::advance`

New decisions; today they are implicit in `engine::learning::open` and `engine::presentation::exercise_context`.

- `substitute(slot, already_in_exercise) -> bool`: whether a queued missed word may replace this slot. Allowed for a review-lane slot that is not a due review, then for a broad slot; never for a target slot or a due review; never when the word is already in the exercise.
- `label::of(facts) -> Label` with `Label ∈ {Fresh, Again, DueReview, Repeat}`; facts are: times this item was served earlier in the session, whether it is a reinforcement, whether it is a due review, whether the prepared list has wrapped, whether a prose passage is inside the no-repeat window.
- `cycle::advance(index, count) -> (next, wrapped)`.

| Theorem | Statement | Mutant |
|---|---|---|
| `Slots.never_displaces_due` | a due-review slot is never substituted | `substitute-any-review` (today's behaviour) |
| `Slots.never_displaces_target` | a target slot is never substituted | `substitute-target` |
| `Slots.no_duplicate_in_exercise` | no substitution when the word is already present | `substitute-ignores-present` (today's behaviour) |
| `Label.fresh_iff_unseen` | `Fresh` iff served-before is 0 and it is not a reinforcement, not a due review, the list has not wrapped and the passage is outside the window | `repeat-shown-fresh`, `cycle-not-labelled` |
| `Label.again_is_reinforcement` | `Again` iff it is a reinforcement | `again-for-due` |
| `Cycle.wrap_exact` | `wrapped` iff `index + 1 == count`; `next < count` | `wrap-off-by-one` |

Callers: `engine::learning::Learning::open`, `engine::presentation`, `engine::practice::finish_exercise`.

### Tested, not proved

- Building the integer evidence from stored rows (which sessions are in the window, which skills belong to which focus group), the corpus index, posting lists, the PRNG and shuffles.
- That adapters call the kernel for every decision of that kind. The boundary test in `trainer/tests/consumer_boundaries.rs` is extended so that `trainer` and `engine` contain no second copy of a moved function.
- The "key already explained by a chosen transition" filter stays in the trainer with its existing test.

## Design and boundaries

`cadence-kernel` (`kernel` tier, `no_std`, no dependencies) gains modules `plan`, `select`, `lanes`, `review`, `queue`, `focus`, `slots`, `label`, `cycle`. `cadence-trainer` and `cadence-engine` (`core`) keep the typed wrappers (`WordId`, `SkillKey`, `UtcDay`, `BasisPoints`) and collections, convert to integers at the call, and delete their own arithmetic in the same PR (no shim). `xtask boundary` gains the edge `trainer -> kernel` and `engine -> kernel` if not already allowed.

Each move is one PR: kernel function, `sh proofs/refresh.sh`, claims and mutants, caller migration, existing behaviour tests unchanged. Where an existing test pins a float-derived number (`learner_tests.rs`), the PR states the old and new value and why; a test is never deleted or weakened to pass.

## Compatibility and reversibility

No persisted format changes. Plans built by alpha.5 are not replayed by any shipped tool, so the decay change invalidates nothing stored. Reverting a move is a code revert plus an `[[amend]]` on the claim.

## Alternatives

- **Keep the float model, prove only the integer parts.** Leaves the ranking that picks the learner's focus unproved and platform-dependent. Rejected.
- **Exact exponential decay by a lookup table.** Possible, but a table is harder to state a theorem about than halving plus linear interpolation, and the learner model is a stated hypothesis either way.
- **Prove the whole selector (`WordSelector::block`).** It owns hash maps, streams and the PRNG; Aeneas would not accept it. The decisions it makes are covered by 2, 3 and 6.

## Evidence and rollout

In order, each independently mergeable: #34 allocation, #35 candidate scan, #36 quotas, #37 review ladder and queues, #38 focus selection, #39 slot sources and labels. Finish line: the proof gate records every claim above as `proved` with all mutants killed, and `docs/TAUTEN.md` lists each with its production caller.

## Unresolved questions

None blocking. Owner may amend: the interpolation used for decay, and the label precedence (`Again` over `DueReview` over `Repeat`).

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
