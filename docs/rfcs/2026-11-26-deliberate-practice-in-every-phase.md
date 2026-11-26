---
rfc: "2026-11-26-deliberate-practice-in-every-phase"
title: "Deliberate practice in every phase"
status: accepted
implementation: complete
authors:
  - tknawara
  - claude-fable (review agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/27
milestone: "0.1.0-alpha.7"
supersedes: []
superseded_by: []
blocked_on:
  - https://github.com/cadence-typing/cadence/issues/26
---

# RFC: Deliberate practice in every phase

Third of three RFCs from the 2026-11-26 review. It wires the feedback loop that alpha.4 gave the words phase through the whole 900-second plan. It calls the decisions proved under [Practice decisions in the kernel](2026-11-26-practice-decisions-in-the-kernel.md) and is gated by the model added under [Formal gates with production mappings](2026-11-26-formal-gates-and-production-mapping.md). It extends [Learn from mistakes](2026-11-13-learning-from-mistakes.md); the section "Prose stays unadapted" there is kept and sharpened, not replaced.

## Summary and motivation

**Scores for the five existing tracks are computed exactly as before; a sixth track, `review`, is added and shown separately. Two fixes change learner evidence only (never a score): intervals no longer span a pause, and a transition counts as correct only if both of its keys were.**

The owner reports that targeted words and remembered mistakes helped, and wants every minute of the session to work that way. Traced through the code at `45d9a57`, the loop closes for 300 of 900 seconds:

| Phase (default s) | Captured | Persisted | Read by the learner model | Changes the next session | Changes later exercises this session | Spaced re-review | What the UI says |
|---|---|---|---|---|---|---|---|
| Warm-up (60) | Key and transition first attempts, clean intervals (lane `main`); word outcomes | Yes | Skills yes. Word misses enter the ladder | Only through the words phase. Warm-up content is a seeded draw of core words | No. `Learning::close` enqueues reinforcement only for `ExerciseKind::Words` | Its misses are scheduled, but served only in the words phase | "Settle into an easy rhythm." Nothing about targeting |
| Words (300) | Same, plus reinforcement count | Yes | Yes: focus (letters and apostrophe only), due words | Yes: target slots follow focus skills, review slots serve due words | Yes: a missed word returns once in a later review slot. Defects: it can duplicate a word in the exercise and overwrite a due review (#43); top-ranked words recur in consecutive exercises without a label | Yes: 1/3/7/14/30 days | Focus named on the start screen and per exercise; "again: word". Cycled lists unlabelled (#7) |
| Number row (120) | Key and transition attempts (digits and context letters, lane `main`). No entry outcome | Yes | **No.** `choose_focus` keeps only `is_word_skill` (lowercase letters, apostrophe); digits are dropped | No. `number_row_exercise` is a uniform seeded draw | No | No | "Digits in short text contexts." |
| Numpad (180) | Key and transition attempts (lane `numpad`). No entry outcome | Yes | **No.** `choose_focus` keeps only lane `main` | No. `numpad_exercise` is a uniform seeded draw | No | No | "Exact entries. Leading zeros count." The review's "Most slips" line lists only main-lane letter pairs |
| Prose (180) | Key and transition attempts (lane `main`), passages typed. No word outcome | Yes | Skills yes (prose slips already move the letter focus). Passages feed the no-repeat window | Indirectly, through the words phase | No | Passages are not re-served within 200 sessions; missed words are not scheduled | Title and author; "repeat" when unavoidable. Nothing says prose is untargeted |
| Review (60) | Nothing. `Phase::Review` has no exercises; `open_exercise` shows the summary. The 60 s is not clocked | — | — | — | — | — | "Session review". With `--track numpad --minutes 5` the plan shows 3:45 numpad and 1:15 review |

So 300 s adapt, 540 s record evidence that is partly or wholly unused, and 60 s (17–25% of a single-track 5-minute session) is a results screen with a budget line. Issues #5 and #7 name two of these gaps.

Why an RFC: this changes the learner model, the session plan policy and a persisted format.

Outside this decision: pace hysteresis and benchmark probes (#6), layout configuration (#9), imports (#10), mid-session re-ranking of focus skills (see Alternatives).

## Observable behaviour

**Start screen.** One line per kind of target, omitting the empty ones:

```
Today targets  th · ou · w   number row 7 · 48   numpad 0. · 55   5 due reviews
```

With no evidence for a group it says so ("number row: no digit history yet").

**Warm-up.** Still familiar core words, still easy. Its two review slots per block now serve *retention checks*: due words that are already above the first rung of the ladder (missed once, typed clean since). Caption: "Familiar words · 2 retention checks". A word missed in warm-up returns once in the words phase ("again: word").

**Words.** As today, with these differences: a missed word never duplicates a word already in the exercise and never replaces a due review (it takes a non-due review slot, else a broad slot); a target word does not recur within four exercises; any word served before in the session is labelled ("repeat: weight"); a wrapped prepared list is labelled.

**Number row and numpad.** Each has its own focus digits and digit transitions (numpad also the decimal point), chosen by the same proved rule as letters from that group's own evidence and baseline. Two of every three prepared entries contain a focus skill; the third is uniform, for breadth. Caption: "Targeting 7 · 48". A missed entry returns once, at least two entries later in the same phase ("again"). Leading zeros, decimals and Enter-to-submit are unchanged.

**Prose.** Passage choice stays independent of the learner model. The purpose line says so: "Not targeted: this measures how accuracy carries into sentences. Slips here feed your closing review and tomorrow's words." Words missed in prose that exist in the word pack are recorded as word outcomes: they enter the ladder and this session's closing review.

**Closing review.** The last phase is typed and clocked like the others. It serves, in order: words missed anywhere this session that have not yet come back; then due words not reached earlier; then focus-target words; then core words, captioned "no misses left · extra words". Its figures are a separate `review` row in the results and are **excluded from the headline tiles and from "same track set" comparisons**, because retyping a word missed minutes ago is an easier task than first-pass typing and must not lift the headline (rule 1). It is dismissible: ending the session during or before it still records the session as completed, as today. The results screen follows and is no longer part of the time budget.

**Results.** "Most slips" covers every lane (for a numpad-only session it shows numpad slips). The focus line names what each practised track targeted.

**Refusals.** An alpha.5 binary opening an alpha.7 profile reports `SchemaTooNew` (schema 4).

## Proof obligations

### Lean

No new kernel decision is introduced here. This RFC is the caller of the claims in the second RFC, and each wiring PR must show the call site:

| Behaviour above | Kernel decision it must call | Theorem that carries the guarantee |
|---|---|---|
| Per-group focus (letters, number-row digits, numpad) | `focus::pick`, `focus::qualifies`, `focus::decay_weight` | `Focus.sparse_never_qualifies`, `Focus.quiet_never_qualifies`, `Focus.pick_maximal` |
| Missed word or entry returns; never duplicates; never displaces a due review | `queue::admit`, `slots::substitute` | `Queue.admit_no_duplicate`, `Queue.per_word_bound`, `Slots.never_displaces_due`, `Slots.no_duplicate_in_exercise` |
| Retention checks in warm-up; ladder for prose misses; closing review never promotes | `review::ladder_step`, `review::is_due` | `Review.promotion_needs_two_days`, `Review.early_clean_is_practice`, `Review.error_resets` |
| Every repeat is labelled | `label::of`, `cycle::advance` | `Label.fresh_iff_unseen`, `Cycle.wrap_exact` |
| Review seconds are typing seconds; the budget is exact | `plan::allocate` | `Plan.allocate_sum_exact`, `Plan.allocate_keeps_minimums` |
| Review credit is merged once, in its own track | `progress::merge`, `progress::credit_once` (first RFC) | `Progress.merge_exact`, `Progress.credit_once_spec` |

Scoring kernels (`first`, `counters`, `ratio`) are untouched. `progress::track_valid` gains the `review` track tag; its existing theorems must still build and its mutants still fail.

### TLA+ — `models/FeedbackLoop.tla`

A bounded model (2 words, 3 days, 2 sessions per day, queue capacity 1) of observe → checkpoint → kill or finish → next-session plan → serve.

| Invariant | Meaning | Mutant configuration that must violate it |
|---|---|---|
| `NoLostMiss` | a word with a durably recorded miss and no later qualifying clean recall always has a due day | `drop-miss-when-queue-full` |
| `NoFalsePromotion` | a rung rises only after clean recalls on two distinct days, each on or after its due day | `promote-same-day` |
| `SameSessionNeverPromotes` | reinforcement and closing-review retypes never raise a rung | `closing-review-promotes` |
| `EvidenceCountedOnce` | the model's tally for a session equals that session's last durable checkpoint, however many checkpoints or replays occurred | `append-checkpoints` |
| `DueNotDisplaced` | a planned due review is never replaced by a reinforcement | `reinforce-any-review-slot` |
| `QueueBounded` | queue length ≤ capacity, no duplicates | `admit-duplicate` |
| Property `DueEventuallyServed` (weak fairness on day advance, session start and slot service) | a continuously due word is eventually served (VL-01, under the stated assumptions) | `starve-due` |

Production symbols for the registry: `cadence_kernel::{review::ladder_step, queue::admit, slots::substitute}`, `cadence_engine::learning::Learning::{open, close}`, `cadence_trainer::learner::due_words`, `cadence_data::learning_rows::save`, `cadence_data::store::ProgressStore::bootstrap`.

### Tested, not proved

- Schema 3 → 4 migration of a populated profile, digests of existing receipts intact; export round-trip; `SchemaTooNew` on older binaries.
- Numeric generators: every prepared entry is valid for its track; the targeted share is two in three when a focus exists; cold start is uniform; leading zeros and decimals still occur.
- Prose token mapping (which tokens match a pack word).
- Reducer tests per phase: a miss in warm-up, words, prose, number row and numpad each comes back as specified; nothing comes back twice; headline figures are identical with and without the review track present.
- Two-session PTY run on a temporary profile showing, for each phase, session 1 slips reflected in session 2 content and labels.
- Whether any of this improves learning is empirical and not claimed.

## Design and boundaries

**Evidence (`cadence-core`, `cadence-engine`; `core` tier).** `PlannedExercise` gains `spans: Vec<WordSpan { id, start, len }>` replacing the "words separated by single spaces" assumption in `Learning::close`; prose exercises carry spans for tokens that match a pack word (lower-cased, surrounding punctuation stripped) and `close` records their outcomes. `SessionLearning` keeps its shape and digest encoding. Numeric entry outcomes are session-local: they drive the same-phase "again" and are not persisted, because a random digit string has no retention value; the digit and transition tallies, which are persisted, carry the memory across sessions.

**Queues (`cadence-engine`).** One word queue shared by warm-up, words, prose and the closing review; one entry queue per numeric phase, discarded at that phase's end. Admission is `queue::admit`; placement is `slots::substitute`. All work stays O(exercise) at an exercise boundary (rule 3).

**Learner model (`cadence-trainer`).** Three focus groups: letters (lane `main`, letters and apostrophe), row digits (lane `main`, digits and digit pairs), numpad (lane `numpad`, digits, the decimal point and their pairs). Each has its own error-rate and interval baseline. `PlanRecord.focus` holds all groups in rank order within group; the persisted bound of 16 is kept (6 + 3 + 3 by default).

**Planner (`cadence-trainer`, startup only).** `WordSelector::warm_up` receives the due words above rung 0; `adaptive` receives the rest. Sampled slots take the head of the shuffled stream instead of a random pick from a sliding 256-word window (#44). `numeric.rs` generators take an optional focus list. A prepared fallback list for the closing review is built from leftover due words, focus targets and core words. Prose selection is unchanged.

**Policy.** New `policies/trainer-v3.toml` (v2 stays for provenance): `row_focus_skill_count`, `numpad_focus_skill_count`, `numeric_target_share` (2 of 3), `minimum_exercises_between_word_repeats` (4), `numeric_reinforcement_gap_entries` (2). Phase seconds are unchanged: 60/300/120/180/180/60.

**Schema 4 (`cadence-data`, `edge`).** `session_tracks.track` admits `review`; `Track::Review` gets digest tag 5. Migration 3 → 4 is one transaction that only raises the version (no row changes). Export writes `schema_version: 4`. `word_outcomes` now also holds prose and warm-up words (its columns do not change). No backfill.

**Session flow.** The review phase follows the same `kernel::session::step` path as other phases. `ProgressStatus::Completed` is still set when the last non-review phase reaches its deadline.

### Why prose stays unadapted

1. **It is the only measurement not selected by the learner's weaknesses.** Words, and after this RFC numerics, are deliberately harder where the learner is weak, so their accuracy is not comparable across sessions as the focus moves. Prose from an independent rotation is what makes "am I improving" answerable. Choosing passages dense in weak transitions would change difficulty session to session inside the one track that is supposed to detect transfer.
2. **Rule 1.** Difficulty must not change without being recorded. A passage-difficulty model would have to be recorded, displayed and controlled for; a seeded rotation with a no-repeat window needs none of that.
3. **Unadapted is not open loop.** Prose is where slips are most representative of real typing. This RFC makes prose a *source* for the loop (skill tallies already; word misses and the closing review now) while leaving it untouched as a *target*.

Evidence that would change this: several weeks of history showing prose accuracy flat while targeted tracks improve would argue for a targeted-prose phase *in addition to* an untargeted probe, not instead of it. The owner can amend.

## Compatibility and reversibility

An existing schema-3 profile migrates on first open; all sessions, receipts and digests are unchanged, and the learner model for the first alpha.7 session is built from the evidence already recorded (number-row and numpad tallies have been stored since alpha.4, so numeric targeting starts immediately). Older sessions have no `review` row and compare with new sessions on the five shared tracks. Downgrade: alpha.5 refuses a schema-4 profile; take a backup first (`cadence backup`). Undoing the decision later means leaving `review` rows in place and stopping serving the phase; the schema does not need to go back.

## Alternatives

- **Leave warm-up and numerics cold and only fix labels (#7).** Cheapest, but 360 s keep recording evidence nothing reads.
- **Re-rank focus skills mid-session.** Would let a slip in warm-up retarget the words phase. It needs corpus scans on the reduce path or a pre-built plan per possible focus set; item-level reinforcement gets most of the benefit within rule 3. Not now.
- **Ladder for numeric strings.** Rejected: a random string is not a retention item; skill-level memory is the right grain.
- **Count the closing review in the headline.** Rejected under rule 1 (above). Owner may amend.
- **Keep the review phase as the results screen and give its 60 s to words.** Simpler, but loses the immediate correction of misses from prose and numerics, which have no later slot in the session.

## Evidence and rollout

Order (each one PR unless noted): #40 `FeedbackLoop.tla`; evidence fixes #41 (pause), #42 (transition); defects #43 (reinforcement placement) and #44 (sampling window); #45 schema 4 and the `review` track; #46 per-group focus; #5 numeric content and same-phase reinforcement; #47 warm-up; #48 prose as a source; #49 closing review; #7 repeat labels; #50 per-phase "what was targeted"; #53 release evidence and ledger. #51 (acknowledgement watchdog) and #52 (UTC day) are independent and need an owner answer.

Finish line: the phase table above, re-traced on the release commit, has no "No" in the columns from "Read by the learner model" to "What the UI says" except prose "Changes the next session: indirectly" by design.

## Unresolved questions

None blocking; defaults are stated. Owner may amend: review-track exclusion from the headline; the two-in-three numeric target share; warm-up retention checks versus leaving warm-up untouched.

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
- 2026-11-26: regroomed. The rollout order now lives in tracker #27; #40–#44 moved to 0.1.0-alpha.6 with the kernel work; schema 4 also gains the nullable `sessions.utc_offset_minutes` column (local-calendar days, #52).
