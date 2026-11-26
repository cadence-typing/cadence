# tauten and the Lean kernel: adoption record

Status: adopted 2026-11-24. RFC:
[2026-11-24-tauten-and-lean-kernel](rfcs/2026-11-24-tauten-and-lean-kernel.md)
(draft; the owner decides acceptance). Working rules: `AGENTS.md`, section
"Strictness profile (tauten)".

This page says what is proved, what is only tested, and what is not
established. Read the badge column literally.

## Pins

| What | Pin | Why |
|---|---|---|
| `cargo-tauten` | crates.io `0.2.0`, installed with `cargo +1.98.0 install --locked --version 0.2.0 cargo-tauten` | The version `tauten/docs/ADOPTING.md` documents for the advisory trial and CI check; a released, checksummed crate rather than a git revision. |
| Rust for the tool only | `1.98.0` (already present via rustup; not the default) | cargo-tauten 0.2.0 declares `rust-version = 1.98`. The product keeps `rust-toolchain.toml` at 1.97.1 and its Docker image unchanged. Run the tool as `cargo +1.98.0 tauten ...`. |
| Proof image | `ghcr.io/tauten-dev/proofs-toolchain:lean-4.35.0-aeneas-c5eb4a2-charon-8cae8fd`, local image id `sha256:5c1fd168f399e028db85e4a76c8e742abc7bdf93207e5270e54b36636b40a2d8` | The image tauten's own recipe pins: Lean v4.35.0, Aeneas nightly-2026.11.07-c5eb4a2, Charon 8cae8fd on nightly-2026-09-17, mathlib per `proofs/lake-manifest.json`. linux/amd64, about 20 GB unpacked. |
| Machine-model lemmas | `proofs/Proofs/TautenMachine.lean`, `TautenAssumption.lean`, copied from the 0.2.0 templates | Unused by the Cadence claims today; the gate audits them with everything else. |

The proof gate needs network access in a fresh container (Charon builds its
MIR sysroot from crates.io) and Docker. It does not run in the offline
product gates (`docker/docker-compose.yml`); it is a separate, explicitly
networked step. Making it a pinned offline Compose service is an open item
(see "Limits").

## Tiers

`cargo tauten init` proposes `core` for every library crate and `edge` for
binaries; the analysis supports a finer split, and the one structural change
the tiers needed was a new innermost crate.

| Crate | Tier | Reasoning |
|---|---|---|
| `cadence-kernel` | `kernel` | New. `no_std`, no dependencies. The scoring and checkpoint decisions the product's correctness rests on. |
| `cadence-core` | `core` | Domain types, ledger, metrics, progress. No filesystem, clock, environment, randomness or I/O; time and the calendar arrive through the `Clock`/`Calendar` ports. Depends on the kernel. |
| `cadence-trainer` | `core` | Planning and the learner model. Pure over its inputs (policy and corpus bytes are parameters; its only `std::fs` use is in tests). Floats are allowed in `core`. |
| `cadence-engine` | `core` | The typed reducer: `step`-shaped, no I/O. Rule 3 of `AGENTS.md` already required this. |
| `cadence-data` | `edge` | SQLite, files, locks, backup. Calls the kernel's checkpoint admission. |
| `cadence-tui` | `edge` | Ratatui painting. Pure over a snapshot, but it renders to a terminal buffer and is the presentation adapter. |
| `cadence-runtime` | `edge` | Terminal, signals, writer thread, clock. |
| `cadence-driver` | `edge` | Headless/replay driver around the runtime and renderer. |
| `cadence` (app), `xtask` | `edge` | Binaries and tooling. |

Dependencies point inward only (`edge -> core -> kernel`); `cargo xtask
boundary` enforces the exact edges (`cadence-core -> cadence-kernel`,
`cadence-data -> cadence-kernel`, nothing from the kernel).

Nothing was moved to the edge to silence a finding. `cadence-tui` could be
`core` by its effects; it is `edge` because it is an adapter, not because a
finding required it, and `cargo tauten check` reports no effect finding for
it either way.

## Commands

```console
# the tool (Rust 1.98.0 for the tool only)
cargo +1.98.0 install --locked --version 0.2.0 cargo-tauten
cargo +1.98.0 tauten check                          # structure + Clippy; must exit 0
cargo +1.98.0 tauten check --format json            # one diagnostic per line
cargo +1.98.0 tauten explain TAU-N002
# the proof gate (Docker, pinned image)
cargo +1.98.0 tauten proofs extract                 # does the kernel still translate?
cargo +1.98.0 tauten proofs try --mutants settle-forgives-correction
cargo +1.98.0 tauten proofs                         # full gate; records proofs/claims.json
sh proofs/refresh.sh                                # regenerate proofs/Extracted after a kernel edit (run cargo fmt first)
# the product gates, unchanged
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
```

## The kernel and its production call sites

`crates/kernel` (`cadence-kernel`, 470 lines of Rust, no dependencies). Every
function below is called by production code; there is no second copy.

| Kernel function | Production call site |
|---|---|
| `first::settle` | `AttemptLedger::apply` (`crates/core/src/ledger.rs`): the only writer of a first-outcome slot |
| `first::count_first` | `AttemptLedger::first_correct`, `first_attempted` |
| `counters::matched`, `counters::content_step` | `AttemptLedger::apply`, content insertions (counter update is refused before any mutation) |
| `counters::deletion_step` | `AttemptLedger::apply`, `Backspace` and `DeleteWord` |
| `ratio::fraction` | `metrics::first_pass_accuracy`, `attempt_accuracy` (exact pairs) |
| `ratio::wpm`, `ratio::digits_per_minute` | `metrics::correct_wpm`, `digits_per_minute`, hence `engine::summary`, `engine::review`, `engine::presentation` |
| `ratio::basis_points` | `BasisPoints::from_ratio_truncating` (`crates/core/src/id.rs`) for every displayed accuracy percentage |
| `time::segment_active` | `TimingSegments::active_through` (`crates/core/src/session.rs`), hence `pause`, `end_phase` and the engine's phase clock |
| `time::in_window` | `Micros::within_half_open`, called by `Engine::advance_clock` to decide whether active time is still inside the phase (A-05) |
| `progress::track_valid` (with `in_range`, `ordered`, `split_fits`) | `SessionProgress::validate`, before a checkpoint is built or a stored one decoded |
| `ledger::insert`, `ledger::settle_at` | `AttemptLedger::apply` for every content key: admission under the bound, position inside the target, counters, and the write-once slot write |
| `ledger::flags_after_insert`, `flags_after_deletion`, `flags_after_interrupt`, `clean` | `AttemptLedger::apply`, `mark_interrupted`, `is_clean_completion` (per-word flags) |
| `ledger::complete` | `AttemptLedger::is_complete` |
| `progress::merge` | `TrackProgress::add`, hence `engine::practice::merge_track` for every exercise credited to a track |
| `progress::credit_once` | `Engine::record_exercise`: an exercise's ledger is credited only if newer than every credited one, else a typed error |
| `session::step` | `Engine::transition` (`crates/engine/src/flow.rs`), the only writer of the session state, and `Engine::invalidate`; the engine carries out its clock command and status consequence |
| `checkpoint::admit` | `Store::apply_checkpoint` (`crates/data/src/store.rs`): apply, replay, conflict or stale |
| `review::ladder_step` (with `valid_ladder`, `rolled_back`, `promotes`, `next_streak`, `next_rung`), `review::is_due` | `trainer::review::{ladder_step, is_due}`, called by `trainer::learner::due_words` for every recorded word outcome and to pick the due words (L-06, VK-06) |
| `label::of` | `engine::presentation::exercise_context` ("due review", "again: word") and `trainer::plan_content` (the prose repeat caption) |
| `cycle::advance` | `engine::practice::finish_exercise`: the next prepared exercise and the phase's `wrapped` flag |
| `focus::decay_weight`, `posterior_bp`, `uncertainty_bp`, `hesitation_bp` | `trainer::learner::LearnerModel::{from_history, score}`: the recency weight of every session's evidence and each skill's error, uncertainty and hesitation (L-03) |
| `focus::qualifies`, `focus::signal`, `focus::pick` | `trainer::learner::LearnerModel::choose_focus`: the focus skills, in skill-key order (L-05) |
| `evidence::pair_first_correct`, `evidence::clean_interval` | `engine::learning::Learning::first_attempt`: whether a transition's first attempt was correct and whether the gap before a key is a timing sample (L-02, L-03) |
| `slots::substitute` | `engine::learning::Learning::open`: where a queued missed word is re-served |
| `queue::admit` | `engine::learning::Learning::enqueue`: whether a missed word joins the same-session reinforcement queue (L-05, VK-06) |
| `lanes::admissible`, `lanes::record`, `lanes::complete` | `trainer::lanes::QuotaTracker::{word_admissible, record, quotas_exactly_met}`: candidate eligibility, the only writer of the block counts, and the complete-block flag (L-05, VK-05) |
| `select::utility`, `select::scan` | `trainer::candidate::scan_select`, called by `trainer::plan_words::WordSelector::select` for every word slot (L-05, VK-04) |
| `plan::allocate` (with `total`, `shares`, `pick`, `hand_out`) | `cadence_core::plan::allocate`, called by `trainer::plan::build_session_plan` and `SessionPolicy::default_allocation`: the seconds of every phase (P-02, VK-03) |

`fixtures/scoring.json` (independent, hand-calculated) is replayed through the
kernel functions by `crates/kernel/tests/scoring_fixtures.rs` and through the
whole ledger by `crates/core/tests/scoring_fixtures.rs`.

Behaviour is unchanged: the same 188 tests pass, with 5 kernel tests and one
extra `BasisPoints` boundary test added (193 in total).

## Proof inventory

Every claim below is a Lean theorem over `proofs/Extracted/CadenceKernel.lean`
(the committed Charon/Aeneas translation of `crates/kernel`; the gate refuses
drift against a fresh translation). The translation contains no Aeneas
axioms. The audit admits only `propext`, `Classical.choice` and `Quot.sound`.
"Badge" is the gate's badge for the claim, not an opinion.

| Claim (Rust function) | Theorems (`proofs/Proofs/`) | What they say | Mutants that must break it | Badge |
|---|---|---|---|---|
| `first::settle` | `First.settle_spec`, `settle_write_once`, `mistake_stays_an_error`, `first_match_is_kept` | A settled slot is never changed by any later insertion. A mismatching first insertion followed by any later insertion, including the correct retype, is still `Incorrect`: a corrected mistake stays a first-attempt error (AGENTS.md rule 1). Total. | `settle-forgives-correction`, `settle-overwrites-correct`, `settle-open-always-correct` | proved |
| `first::count_first_loop`, `first::count_first` | `First.count_first_loop_spec`, `count_first_spec` | The loop counts exactly the correct and the attempted slots; `first_correct <= first_attempted <= length`, so the accuracy numerator never exceeds its denominator; the saturating additions never saturate. Total. | `count-open-as-attempted`, `count-correct-twice`, `count-skips-last` | proved |
| `counters::matched` | `Counters.matched_spec`, `overflow_never_matches` | An insertion past the target never matches. | `matched-ignores-position` | proved |
| `counters::content_step` | `Counters.content_step_spec`, `overflow_insertion_never_correct`, `mistype_counts_attempt_not_correct`, `content_step_keeps_inv`, `content_step_counts_below_limit` | Exact effect on each counter; a mistyped unit counts as an attempt and not as a correct one; an overflow insertion is never correct; `correct + overflow <= content` is preserved; below the `u64` limit the step is always counted. Total. | `matched-ignores-position`, `mistype-not-an-attempt`, `overflow-not-counted`, `correct-counted-twice` | proved |
| `counters::deletion_step` | `Counters.deletion_step_spec`, `deletion_step_monotone` | The counter grows by exactly the removed count and refuses exactly on `u64` overflow. Total. | `deletion-saturates`, `deletion-forgets-count` | proved |
| `ratio::fraction` | `Ratio.fraction_spec` | The pair is kept exactly; only a zero denominator is refused. Total. | `fraction-allows-zero` | proved |
| `ratio::wpm` | `Ratio.wpm_spec`, `wpm_monotone_in_units`, `wpm_idle_time_never_helps` | `Empty` iff no units or no time, else exactly `units*60e6 / (5*active)` (the fixture identities); the `u128` products never overflow for `u64` inputs; more units never lower the pace; more idle time never raises it. Total. | `wpm-four-units-per-word`, `wpm-minute-is-ten-seconds`, `wpm-zero-time-allowed` | proved |
| `ratio::digits_per_minute` | `Ratio.digits_per_minute_spec` | Same for the numpad rate. Total. | `digits-minute-is-ten-seconds`, `digits-denominator-off-by-one` | proved |
| `ratio::basis_points` | `Ratio.basis_points_spec`, `accuracy_in_range`, `accuracy_computed` | `floor(num*10000/den)`; a ratio of a count to a count at least as large is never over 100% and never `OverFull`; computed whenever `num*10000` fits `u64`. Total. | `basis-allows-over-full`, `basis-remainder-not-quotient`, `basis-full-is-9999` | proved |
| `time::segment_active` | `Time.segment_active_spec`, `segment_active_closed`, `segment_active_monotone` | Completed active time plus the open segment's `now - start`, exactly, in unbounded arithmetic; `Backward` exactly when `now` precedes the open segment, `Overflow` exactly when the sum exceeds `u64::MAX`; with no open segment (paused) it is the completed total; a later clock reading never gives less active time. Total. | `segment-ignores-open`, `segment-saturates`, `segment-backward-is-zero` | proved |
| `ledger::insert` | `Ledger.insert_spec`, `over_bound_rejects` | A key is admitted only below `target + allowance`, so the buffer never exceeds it; at or past the bound it is refused and nothing is counted; it lands inside the target iff the buffer is shorter than the target; only a key inside the target matches. Total. | `bound-off-by-one`, `past-target-counts-as-inside` | proved |
| `ledger::settle_at` | `Ledger.first_write_once` | Over the whole slot vector: the length is kept, no other slot changes, and a settled slot never changes. Total. | `settle-at-overwrites` | proved |
| `ledger::flags_after_*` | `Ledger.word_flag_sticky` | No step clears a set flag; an unmatched key always marks an error; a deletion and an interruption always set their flag. | `flag-clears-on-correction`, `deletion-not-flagged`, `interrupt-not-flagged` | proved |
| `ledger::clean` | `Ledger.clean_spec` | Clean iff no flag is set. Total. | `clean-ignores-paste` | proved |
| `ledger::complete` | `Ledger.complete_iff_exact` | Complete iff the buffer has the target's length and every position matches; a matching prefix with extra input is not complete. Total. | `complete-ignores-tail` | proved |
| `progress::merge` | `Progress.merge_exact`, `merge_keeps_valid` | Each of the nine counters is the exact sum, or `Overflow` exactly when some sum exceeds `u64::MAX`; nothing is dropped, saturated or partly merged. Merging two ordered tracks stays ordered and within split. Total. | `merge-drops-deletions`, `merge-saturates` | proved |
| `progress::credit_once` | `Progress.credit_once_spec` | An exercise is credited iff it is newer than every credited one: never twice. Total. | `credit-allows-equal` | proved |
| `session::step` | `Session.clock_runs_only_running`, `review_is_terminal_for_typing`, `completed_only_after_last_phase`, `running_only_by_start_resume_or_next`, `invalidate_admitted` | Over every state and event: leaving `Running` stops the clock and only entering `Running` resumes it; from the review, finishing or saved no event leads back to typing; only the last typing deadline or reaching the review completes a session (ending early or exiting never does); typing starts only by start, resume or the next phase; an invalid measurement is admitted everywhere. Total. | `pause-keeps-clock`, `review-resumes`, `early-end-completes`, `invalidate-refused-when-saved` | proved |
| `time::in_window` | `Time.in_window_spec` | `start <= tick < deadline`; a key at the exact deadline is outside the scored slice. Total. | `window-deadline-inclusive`, `window-start-exclusive` | proved |
| `checkpoint::admit` | `Checkpoint.admit_spec`, `admit_apply_iff`, `refused_never_applies`, `applications_strictly_increase`, `applied_sequence_never_reapplies` | The full decision table. A checkpoint is applied iff no receipt exists and it is strictly newer than the latest applied one; replay, conflict and stale are never applied; once a sequence is applied (the store then holds it as its latest sequence, written in the same transaction: trusted, tested), offering it again in any receipt state is never applied (no double credit). Total. | `admit-equal-sequence-applies`, `admit-replay-applies`, `admit-conflict-replays` | proved |
| `progress::in_range`, `ordered`, `split_fits` | `Progress.in_range_spec`, `ordered_spec`, `split_fits_spec` | Exactly which counters are accepted. Total. | `range-skips-active`, `ordered-skips-first-pass`, `split-ignores-overflow-attempts` | proved |
| `plan::total` | `Plan.total_spec` | The exact sum when it fits `u64`, `Overflow` exactly when it does not. Total. | `total-drops-sum` | proved |
| `plan::pick` | `Plan.pick_spec`, `allocate_tie_to_lower_index` | The next leftover second goes to the first phase after the previous one in the order largest remainder first, lower index first among equal remainders; `None` only when no phase is left. Total. | `tie-to-higher-index` | proved |
| `plan::allocate` (with `shares`, `hand_out`) | `Plan.allocate_spec`, `allocate_sum_exact`, `allocate_keeps_minimums`, `allocate_infeasible_exact` | On `Done` the seconds sum exactly to the budget and every phase keeps its minimum; `Infeasible` exactly when the shapes agree and the minimums sum (fitting `u64`) exceeds the budget. Total. | `leftover-dropped`, `minimum-not-added`, `infeasible-clamped` | proved |
| `review::ladder_step` | `Review.ladder_step_spec`, `error_resets`, `due_is_future`, `early_clean_is_practice`, `promotion_needs_two_days`, `rung_bounded`, `rollback_refused` | An error puts the word on the first rung, due the next day, with no streak; the next due day is after the outcome's day; only a clean completion on or after the due day moves the word (earlier clean typing is `Practice`); the rung rises by one only after a clean day strictly later than a counted one; the rung stays on the ladder; a completion before the counted clean day never moves the word. Total. | `error-keeps-rung`, `due-today`, `early-clean-counts`, `same-day-promotes`, `first-clean-promotes`, `rung-wraps`, `rollback-accepted` | proved |
| `review::is_due` | `Review.is_due_spec` | Due exactly on or after the due day. Total. | `due-strictly-after` | proved |
| `label::of` | `Label.fresh_iff_unseen`, `again_is_reinforcement` | `Fresh` exactly when the item was not served before, is not a reinforcement or a due review, its list has not wrapped and its passage is outside the no-repeat window; `Again` exactly for a reinforcement. Total. | `repeat-shown-fresh`, `cycle-not-labelled`, `again-for-due` | proved |
| `cycle::advance` | `Label.wrap_exact`, `advance_total` | The list wraps exactly at its last index and the next index is inside the list. Total. | `wrap-off-by-one` | proved |
| `focus::decay_weight` | `Focus.decay_total`, `decay_weight_spec`, `decay_one_at_zero`, `decay_halves`, `decay_antitone` | `WEIGHT_ONE` (2^16) at age 0, exactly `WEIGHT_ONE / 2^k` after `k` half-lives, linear in between, and never more for an older session. Total. | `decay-grows`, `decay-skips-halving` | proved |
| `focus::posterior_bp` | `Focus.posterior_total`, `posterior_spec`, `posterior_in_range`, `posterior_prior_at_zero`, `posterior_monotone_errors` | `(errors + α·ONE)·10000 / (attempts + (α+β)·ONE)` exactly; at most 10,000 when errors never exceed attempts; the prior mean with no evidence; more errors never lower it. Total. | `posterior-drops-prior` | proved |
| `focus::hesitation_bp` | `Focus.hesitation_in_range`, `hesitation_needs_samples` | Within 0..=10,000; 0 below the sample minimum. Total. | `hesitation-ignores-minimum` | proved |
| `focus::qualifies` (with `signal`) | `Focus.qualifies_spec`, `signal_spec`, `sparse_never_qualifies`, `quiet_never_qualifies` | Qualifies exactly with enough weighted attempts and a positive signal (the weighted error excess over the group rate and the weighted hesitation, each above its threshold); sparse evidence and a quiet skill never qualify. Total. | `sparse-qualifies`, `threshold-inverted` | proved |
| `focus::pick` | `Focus.pick_spec`, `pick_sound`, `pick_maximal` | A picked candidate is present, not yet chosen and qualifies; no qualifying unchosen candidate has a greater signal, the lower index first among equals. Total. | `pick-ignores-chosen`, `pick-first-not-best` | proved |
| `evidence::pair_first_correct` | `Evidence.pair_correct_iff_both` | A transition is first-correct exactly when the previous key's first attempt was correct and this key matched. Total. | `pair-ignores-previous` | proved |
| `evidence::clean_interval` | `Evidence.interval_reason_exact`, `interval_timed_iff` | `Timed g` exactly when the anchor is the previous position, the key matched, no pause separates them (same segment ordinal) and `0 < g = active - anchor ≤ ceiling`; every other answer exactly under its condition. Total. | `interval-crosses-pause`, `interval-zero-gap`, `interval-nonadjacent`, `interval-on-error`, `ceiling-off-by-one` | proved |
| `slots::substitute` | `Slots.substitute_spec`, `never_displaces_due`, `never_displaces_target`, `no_duplicate_in_exercise` | A due review and a target slot are never replaced, and a word already in the exercise is never placed again; only a review or broad slot without a due review takes it. Total. | `substitute-any-review`, `substitute-target`, `substitute-ignores-present` | proved |
| `queue::admit` | `Queue.admit_spec`, `admit_no_duplicate`, `admit_bounded`, `per_word_bound` | An admitted word is not already queued, the queue stays within capacity, and no word is repeated more than its per-session limit. Total. | `admit-duplicate`, `capacity-off-by-one`, `per-word-ignored` | proved |
| `lanes::admissible` | `Lanes.admissible_spec` | The word-level rules exactly: refused at the per-block occurrence cap, at the consecutive cap for the same word, at the focus-owned cap for a focused target slot. Total. | `word-cap-ignored`, `focus-cap-ignored`, `consecutive-cap-ignored` | proved |
| `lanes::record` | `Lanes.record_spec`, `never_overrun`, `word_cap`, `focus_cap`, `consecutive_cap`, `record_refuses_inadmissible` | A recorded slot never overruns its lane quota, the word cap, the consecutive cap (a different word restarts the run at one) or the focus-owned cap; a word `admissible` refuses is never recorded. Total. | `quota-off-by-one`, `record-always-ok` | proved |
| `lanes::complete` | `Lanes.complete_block_exact` | Complete exactly when every lane met its quota. Total. | `complete-allows-under` | proved |
| `select::utility` | `Select.utility_in_range`, `utility_monotone` | The weighted mean `sum(w*t)/sum(w)` of the four basis-point terms (0 when every weight is 0), never above 10,000; raising any term never lowers it. Total. | `utility-unnormalized`, `utility-subtracts-hesitation` | proved |
| `select::scan` | `Select.scan_spec`, `scan_only_eligible`, `scan_optimal`, `scan_none_iff_no_eligible`, `scan_bounded` | `TooLarge` exactly when the vector exceeds the bound (nothing is scanned); a returned index is eligible and no eligible candidate has a greater `(utility, tie_break, id)`, the first winning among equal keys; `Empty` exactly when nothing is eligible. Total. | `scan-ignores-eligibility`, `scan-keeps-first`, `scan-drops-best`, `scan-bound-off-by-one` | proved |
| `progress::track_valid` | `Progress.track_valid_spec`, `valid_accuracy_at_most_full`, `valid_track_accuracy_in_range` | A valid track has `first_correct <= first_attempted <= content`, and its first-pass accuracy basis points are computed, never over-full and at most 10000 (when the scaled numerator fits `u64`). Total. | the three above and `valid-skips-ordering` | proved |

The "proved" badges above are the gate's; if the table and `proofs/claims.json`
ever disagree, `proofs/claims.json` wins.

### Not established (and why)

These are not proved, and nothing in this record should be read as claiming
them.

| Property | Badge | State |
|---|---|---|
| That `AttemptLedger::apply` passes the buffer length as the position, the right `same`, the number of matching positions and the right word index to the ledger kernel; the buffer push/pop and the character comparison | tested | `crates/core/tests/ledger_properties.rs` and the scoring fixtures. Every decision in the step is proved (`ledger::*`); this is the plumbing around it. |
| The store reads the receipt and latest sequence correctly, the SQL transaction is atomic, the receipt insert and the session update commit together | tested | `crates/data/src/tests/checkpoints.rs` and `disk.rs`. The kernel only decides from facts it is given. |
| Durable acknowledgement ordering: an acknowledgement is never observed before the commit (`AckIsDurable`), single-flight dispatch, retry after failure, and a clean exit only after the final acknowledgement | model-proved (bounded) | `models/CheckpointSave.tla`, mapped to `engine::storage`, `SaveWorker`, `ProgressStore::apply_checkpoint` and `checkpoint::admit` (see *Protocol models*). Correspondence to the Rust is by review and the named runtime tests, not by proof. The kernel proves the credit decision (`admit`). |
| Display rounding and formatting (`as_f64`, percent labels), the UI comparison with earlier sessions | tested | f64 is outside the kernel by construction. |
| That a feasible allocation never ends in `Overflow` (`pick` always finds a phase because the leftover is below the phase count) | tested | `crates/core/src/plan.rs` compares the kernel with the sort-based largest-remainder rule on every small input. |
| VL-01 fairness, VP-01/02/04/05 protocols, VR-01 replay equivalence | pending | Unchanged from `STATUS.md`; no Lean claim yet. |
| That `cargo tauten check`'s findings are all the bugs there are | not-established | A finding is a departure from a profile, not a defect count (tauten's own benchmark: 0 of 120 sampled findings were reachable defects). |

### Trust boundary

Trusted, not proved: Charon, Aeneas and its Lean library, Lean, mathlib, rustc
(that the MIR Charon reads means what the Rust means), Docker and the pinned
image; the claim that a `settled`/`Counted` result is used as the ledger uses
it (the Rust callers); the hand-copied field conversions
`TrackProgress -> TrackCounters` and `LedgerCounters` accessors; the OS and
SQLite for the store. The proofs are about a `no_std` translation of the
kernel with `usize` read as the platform word; `count_first` needs no bound
beyond the slice length.

`proofs/TRUST.md` lists the same items in the generated template's form.

## Protocol models (TLC)

`tools/tlc_gate.py` runs every model in `models/claims.toml` with the pinned
TLC 1.7.4 as part of `cargo xtask quality` (and so `./ci quality` and the
`check` workflow): each positive configuration must find no error and each
mutant must fail on exactly the invariant it names. A missing or wrong TLC
jar, a timeout, an unresolved production symbol or runtime test, or a stale
registry snapshot fails it. These are **bounded** checks of an abstract
model: the strongest badge is `model-proved`, and only for a `mapped` model,
whose correspondence to the named Rust is by review plus the named runtime
tests, not by proof. The table is generated from the registry.

<!-- models:begin (generated by tools/tlc_gate.py --update) -->
| Model | Invariants | Mutants that must fail | Production symbols | Badge |
|---|---|---|---|---|
| `formal/DurableCheckpoint.tla` | TypeOK, AckIsDurable, NoDoubleCredit | `DurableCheckpoint-early-ack.cfg` → `AckIsDurable`<br>`DurableCheckpoint-double-credit.cfg` → `NoDoubleCredit` | none; blocked on superseded for production by models/CheckpointSave.tla (#30); kept as the handover's design probe for crash and reopen | design-probe (not mapped) |
| `formal/AsyncAdmission.tla` | TypeOK, AcceptedFresh, AtMostOnce | `AsyncAdmission-no-epoch.cfg` → `AcceptedFresh`<br>`AsyncAdmission-no-pending.cfg` → `AtMostOnce` | none; blocked on #75: no job registry with epoch/request tickets is shipped | design-probe (not mapped) |
| `formal/CausalBarrier.tla` | TypeOK, NoEarlyReturn | `CausalBarrier-early-return.cfg` → `NoEarlyReturn` | none; blocked on #4: the headless driver and its barriers are not shipped | design-probe (not mapped) |
| `models/CheckpointSave.tla` | TypeOK, AckIsDurable, NoDoubleCredit, SingleFlight, CleanExitIsDurable, FailedTailIsNamed | `CheckpointSave-two-in-flight.cfg` → `SingleFlight`<br>`CheckpointSave-early-ack.cfg` → `AckIsDurable`<br>`CheckpointSave-double-credit.cfg` → `NoDoubleCredit`<br>`CheckpointSave-quit-before-ack.cfg` → `CleanExitIsDurable` | `cadence_engine::storage::{dispatch_checkpoint, save_result, retry_save, request_exit, watchdog}`<br>`cadence_runtime::writer::SaveWorker`<br>`cadence_data::store::ProgressStore::apply_checkpoint`<br>`cadence_kernel::checkpoint::admit` | model-proved (bounded) |
| `models/SessionFlow.tla` | TypeOK, ClockStoppedUnlessRunning, NoScoreOutsidePhase, InterludeGrace, EndEarlyIsUnfinished, PauseKeepsReason | `SessionFlow-score-while-paused.cfg` → `ClockStoppedUnlessRunning`<br>`SessionFlow-deadline-inclusive.cfg` → `NoScoreOutsidePhase`<br>`SessionFlow-enter-skips-interlude.cfg` → `InterludeGrace`<br>`SessionFlow-early-end-completed.cfg` → `EndEarlyIsUnfinished` | `cadence_kernel::session::step`<br>`cadence_kernel::time::in_window`<br>`cadence_engine::flow::{transition, pause_for, resume, end_early, interlude_settled}`<br>`cadence_engine::practice::{advance_clock, advance_phase}` | model-proved (bounded) |
| `models/FeedbackLoop.tla` | TypeOK, NoLostMiss, NoFalsePromotion, SameSessionNeverPromotes, EvidenceCountedOnce, DueNotDisplaced, QueueBounded, DueEventuallyServed (temporal) | `FeedbackLoop-drop-refused-miss.cfg` → `NoLostMiss`<br>`FeedbackLoop-promote-same-day.cfg` → `NoFalsePromotion`<br>`FeedbackLoop-closing-review-promotes.cfg` → `SameSessionNeverPromotes`<br>`FeedbackLoop-append-checkpoints.cfg` → `EvidenceCountedOnce`<br>`FeedbackLoop-reinforce-any-review-slot.cfg` → `DueNotDisplaced`<br>`FeedbackLoop-admit-duplicate.cfg` → `QueueBounded`<br>`FeedbackLoop-starve-due.cfg` → `DueEventuallyServed` | `cadence_kernel::review::ladder_step`<br>`cadence_kernel::queue::admit`<br>`cadence_kernel::slots::substitute`<br>`cadence_engine::learning::{open, close, enqueue}`<br>`cadence_trainer::learner::due_words`<br>`cadence_data::learning_rows::save`<br>`cadence_data::store::bootstrap` | model-proved (bounded) |
<!-- models:end -->

## Result of the last gate run

Recorded by `cargo +1.98.0 tauten proofs` into `proofs/claims.json`; the
summary line, the per-claim outcomes and the mutant results are copied below
from that run (see the commit that adds `proofs/claims.json`).

Command: `cargo +1.98.0 tauten proofs --live` (full level, every mutant),
gating the working tree of the commit that records this run (#32); exit 0:

```text
tauten: 27 of 27 claim(s) proved (recorded in proofs/claims.json)
```

All 48 mutants were `killed`, each by the theorem it targets. The previous
run (#29, 17 of 17 with 33 mutants) carried temporary `[[amend]]` entries for
the retired time claims, dropped after that record as tauten requires. The description of the earlier run follows.

Stages: tools ok, extract ok, drift ok (committed `proofs/Extracted/` equals a
fresh translation), `lake build` ok, axiom and coverage audit ok (only
`propext`, `Classical.choice`, `Quot.sound`; 90 audited theorems including
tauten's vendored machine lemmas), mutant calibration ok: all 34 mutants
`killed`, each by at least one directly failing theorem. Every claim has
outcome `proved` and badge `proved`; none is `proved-under` (the claims take
no assumptions). Two mutant anchors had been reflowed by rustfmt in an earlier
run and reported `error`; they were fixed and re-run before this record.

On 2026-11-26 (#29) the claims for `time::active_time`, `pause_total` and
`pause_total_loop` were retired by `[[amend]]`: production never called them
(active time is `TimingSegments`), so they proved a second copy. The proved
`time::segment_active` replaced them and is what `TimingSegments` calls.

A claim for `checkpoint::latest_after`, a helper with no production caller,
was removed (with an `[[amend]]` that was dropped again after this record);
no-double-credit is stated over `admit` alone.

## Findings before and after

| Run | Command | Result |
|---|---|---|
| Advisory trial (disposable copy, nothing written) | `cargo +1.98.0 tauten check --structure-only --no-baseline --assume-tier '*=core' --assume-tier cadence=edge --assume-tier xtask=edge --format json` | exit 1: 55 findings (34 errors, 21 warnings): `TAU-X001` 23, `TAU-X002` 10 (lint exceptions without a reason or crate-wide), `TAU-X003` 21 (manifest lint table), `TAU-H001` 1 (release overflow checks). Saved as `evidence/tauten/advisory-trial.jsonl`. |
| First full check with the tiers above | `cargo +1.98.0 tauten check --format json` | exit 1: 77 findings (68 errors, 9 warnings), all in `cadence-core`, `cadence-trainer`, `cadence-tui`. Clippy-decided rules in downstream crates were masked until `cadence-core` compiled, so more surfaced as each layer was fixed. |
| After | `cargo +1.98.0 tauten check` | exit 0: 0 errors, 1 warning (`TAU-M001`, `crates/data/src/transfer.rs:123`: `rusqlite::backup::StepResult` is `#[non_exhaustive]`, so a wildcard arm is required). |

`cargo tauten obligations`: 0 obligations in kernel crates (the kernel uses
checked and saturating operations only).

Findings fixed at the cause (none were baselined, no baseline file exists):

- `TAU-N002` (lossy `as`): `From`/`try_from` throughout, `Track`, `KeyLane` and
  `PlannerMode` digest bytes come from explicit `digest_tag` matches (same
  bytes as before), `UnitCount::from_len` for slot counts, `i64::MAX.unsigned_abs()`
  replaced by the kernel's `COUNTER_MAXIMUM`. Three `as f64`/`as u16` casts that
  have no lossless alternative carry a reasoned `#[expect(clippy::as_conversions)]`
  (`Ratio::as_f64`, two helpers and `basis_points` in `learner.rs`).
- `TAU-E001` (`map_err(|_| ..)`): 36 closures bind a named parameter. The
  discarded errors are `TryFromIntError`, `Utf8Error`, a thread-join payload
  and similar values that carry no information the typed domain error does not
  already state; no source field was added to any error enum.
- `TAU-M001` (wildcard arms): `CoreError`, `EventKind`, `Key`, `Applied`,
  `Screen`, `ScreenView` and `UnitSpan` matches list their variants.
- `TAU-P002`, `TAU-E003`: `SessionId::FIRST` uses `NonZeroU64::MIN`; the hex
  rendering no longer discards a `fmt::Write` result.
- `TAU-X001/X002`: test-module `#![allow(..)]` became `clippy.toml` test
  allowances (the same four the tauten profile makes: unwrap, expect, panic,
  indexing, in tests) plus `#![cfg(test)]` on integration-test crates; the two
  test files that need arithmetic carry a reasoned `#[expect]` on the module
  declaration.
- `TAU-X003`: workspace `cast_possible_truncation`, `cast_possible_wrap`,
  `integer_division` are `deny` (they were `warn` under `-D warnings`, so
  nothing new fires); `as_conversions` and `cast_sign_loss` are added as `deny`.
- `TAU-H001`: `[profile.release] overflow-checks = true`. This is an owner
  decision that changes release behaviour (an overflow panics instead of
  wrapping); production arithmetic is checked, so it should never trigger. It
  is listed under "Unresolved questions" in the RFC.

## Baseline

None. There were no reviewed legacy findings that could not reasonably be
fixed; `tauten-baseline.json` is not created. The one remaining warning
(`TAU-M001` on a `#[non_exhaustive]` third-party enum) is not error-level and
needs no baseline entry.

## Limits

- The proofs are about `crates/kernel`, 470 lines. The reducer, the ledger's
  buffer and word flags, the store, the corpus/learner/planner and the UI are
  not proved.
- The proof gate is not part of the offline product gate. It needs Docker, the
  20 GB image and network access in a fresh container; a pinned offline
  Compose service for it, and a Buildbot step, are not done.
- `docs/07-assurance.md`, `docs/08-build-release.md` and `README.md` still
  describe Verus. They are hash-pinned handover documents (`MANIFEST.sha256`);
  this record and `AGENTS.md` supersede them for kernel proofs. `formal/claims.json`
  still lists VK-01/VK-02 and VP-03 as pending; it is pinned too, so the
  mapping from those IDs to the claims above is recorded here, not there:

  | Handover ID | Covered by | Coverage |
  |---|---|---|
  | VK-01 first outcomes write-once | `first::settle`, `count_first`, `counters::*` | Decision proved; ledger plumbing tested |
  | VK-02 checked metric arithmetic | `ratio::*`, `time::*`, `progress::*` | Proved for the listed functions |
  | VP-03 durable checkpoint | `checkpoint::admit` (store credit), `progress::credit_once` and `progress::merge` (engine credit) | Decisions proved; the save protocol is model-checked (`models/CheckpointSave.tla`) |

- Floating-point display and every f64 in the learner model are outside the
  kernel and unverified.
- `cargo xtask quality` fails in this checkout at the handover-contract step
  because `evidence/model-checks/*.cfg.log` are absent here (they are
  generated by the model gate); it also reports `AGENTS.md` as changed
  against `MANIFEST.sha256`, which resolves when this branch merges with the
  branch that unpins it.
