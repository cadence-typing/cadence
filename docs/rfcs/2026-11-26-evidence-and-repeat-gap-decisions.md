---
rfc: "2026-11-26-evidence-and-repeat-gap-decisions"
title: "Evidence attribution and repeat-gap decisions in the kernel"
status: accepted
implementation: in-progress
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/54
milestone: "0.1.0-alpha.6"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Evidence attribution and repeat-gap decisions in the kernel

A section-level amendment to [Practice decisions in the kernel](2026-11-26-practice-decisions-in-the-kernel.md). That RFC stays in force; this one adds four decisions it missed.

## Summary and motivation

**Scores do not change. Learner-model evidence does: a transition counts as correct only if both of its keys were, and a clean interval never spans a pause or has zero length.**

Grooming the backlog against `main` (`fc79d2e`) found decisions that the kernel RFC does not list and that are still tested-only in `cadence-engine` and `cadence-trainer`, although AGENTS.md puts "what counts as an error" and "a repeat" in the kernel:

1. **Transition outcome.** `engine::learning::Learning::first_attempt` records `Skill::Bigram(previous, unit)` with `error = !matched` for the second key only. L-02 says a bigram is first-correct only if both positions were. This is bug #42.
2. **Clean interval.** The same function decides which inter-key gaps are timing evidence. It admits a gap that spans a pause (bug #41; `Engine::pause_for` never breaks the chain) and a gap of zero microseconds, which L-03 calls insufficient timing resolution.
3. **Consecutive-word cap.** `trainer::lanes::QuotaTracker` enforces `maximum_consecutive_word_occurrences`, but section 3 of the kernel RFC has no theorem for it.
4. **Repeat gap.** #58 (split from #44) adds "a target word may not recur within N exercises". That is a new admissibility decision.

Fixing 1 and 2 in the engine as tested code would add two more unproved decisions. They move to the kernel first, and the bug fix is the caller migration.

## Observable behaviour

Nothing on screen changes directly. Focus skills may differ from alpha.5 for the same history going forward: transitions out of a mistyped key now count as errors, and a pause in the middle of a word no longer produces a short fabricated interval for that transition. Evidence already stored is not rewritten.

## Proof obligations

New module `cadence_kernel::evidence`; additions to `cadence_kernel::lanes` (created by #36). All functions are total, over integers, `bool` and the existing `first::Slot`.

| Rust entry point | Theorem | Mutant that must break it | Production caller |
|---|---|---|---|
| `evidence::pair_first_correct(previous: Slot, matched: bool) -> bool` | `Evidence.pair_correct_iff_both`: true iff `previous` is `Correct` and `matched` | `pair-ignores-previous` (today's behaviour) | `Learning::first_attempt`, which reads the previous position's first outcome from the ledger (#42) |
| `evidence::clean_interval(mark, position, matched, active_us, segment, ceiling_us) -> Interval` where `mark` is `(present, position, active_us, segment)` and `Interval` is `Timed(us)`, `NoAnchor`, `NotAdjacent`, `Error`, `CrossesPause`, `Unresolved`, `Backward` or `AboveCeiling` | `Evidence.interval_timed_iff`: `Timed(g)` iff a mark is present, `mark.position + 1 = position`, `matched`, `mark.segment = segment`, and `0 < g = active_us - mark.active_us <= ceiling_us`; `Evidence.interval_reason_exact`: each other variant is returned exactly under its condition, in the order listed | `interval-crosses-pause` (today's behaviour), `interval-zero-gap` (today's behaviour), `interval-nonadjacent`, `interval-on-error`, `ceiling-off-by-one` | `Learning::first_attempt` (#41). `segment` is the ordinal of the open timing segment from `TimingSegments`; it rises at every resume, so the proof does not depend on the engine remembering to clear the mark |
| `lanes::admissible` (from #36), consecutive-run fact | `Lanes.consecutive_cap`: a word whose current run equals the cap is not admissible | `consecutive-cap-ignored` | `QuotaTracker::word_admissible` (#36) |
| `lanes::gap_admissible(served_before: bool, last_exercise: u32, current_exercise: u32, minimum_gap: u32) -> bool` | `Lanes.repeat_gap_spec`: true iff not served before, or `current_exercise >= last_exercise + minimum_gap` without overflow | `gap-off-by-one`, `gap-ignored` | `WordSelector::candidates` for target slots (#58) |

Each function also has a `total` theorem.

Tested, not proved: that the engine passes the right previous slot, segment ordinal and active time; that the ledger's first outcomes are the ones the kernel wrote (covered by `Ledger.first_write_once`).

## Design and boundaries

`cadence-kernel` (`kernel` tier) owns the decisions. `cadence-engine` (`core`) keeps the mark and the evidence maps and deletes its own conditions in the same change (no second copy; `trainer/tests/consumer_boundaries.rs` is extended). `Interval` reasons other than `Timed` are not persisted in this RFC; the engine counts an attempt and records no interval, as today.

## Compatibility and reversibility

No persisted format changes. Stored tallies keep their old attribution; the recency decay ages them out. Reverting is a code revert plus an `[[amend]]` on the claims.

## Alternatives

- **Fix #41 and #42 in the engine only.** Smaller, but leaves "what counts as an error for a transition" unproved. Rejected under AGENTS.md "Lean proofs first".
- **Break the chain by clearing the mark on pause, without a segment ordinal.** Works, but the property then depends on every pause path calling one method, which is how #41 happened.

## Evidence and rollout

#42 (pair outcome), then #41 (interval), each with its kernel function, claims and the corrected reducer test. `Lanes.consecutive_cap` lands with #36. `Lanes.repeat_gap_spec` lands with #58 in alpha.7, because its threshold is a policy v3 field.

## Unresolved questions

None blocking. Owner may amend: whether excluded-interval reasons should be persisted (L-03 "store named reasons"); default no, until a reader exists.

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
- 2026-11-26: #42 and #41 implemented on `main` (`4a1cc1f`); `Lanes.consecutive_cap` landed with #36. The segment ordinal rises at every pause (not at every resume); the effect is the same. `lanes::gap_admissible` remains for #58 (alpha.7).
