---
rfc: "2026-11-26-formal-gates-and-production-mapping"
title: "Formal gates with production mappings"
status: accepted
implementation: not-started
authors:
  - tknawara
  - claude-fable (review agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/25
milestone: "0.1.0-alpha.7"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Formal gates with production mappings

First of three RFCs from the 2026-11-26 review. This one is the foundation: it makes the existing formal work bind to shipped code and run as a gate. [Practice decisions in the kernel](2026-11-26-practice-decisions-in-the-kernel.md) and [Deliberate practice in every phase](2026-11-26-deliberate-practice-in-every-phase.md) build on it and must land after it.

## Summary and motivation

Cadence has 19 Lean claims over `crates/kernel` and three TLA+ modules. The review found four places where they do not reach the application:

1. **TLC never runs.** `tools/run_models.py` is not called by `./ci`, `xtask` or either workflow. The committed `evidence/tla-results.json` is from 2026-10-08. All 14 rows of `formal/claims.json` are `pending` with empty `production_symbols`, including VK-01 and VK-02, which have since been proved in Lean. (The review ran all eight configurations by hand with a newer, unpinned TLC: three pass, five mutants fail on the named invariant. That is not gate evidence.)
2. **Two of the three models have no production counterpart.** `AsyncAdmission` (epoch/request tickets) and `CausalBarrier` (headless barriers) describe a job registry and a headless driver that are not shipped (#4). Only `DurableCheckpoint` describes real code, and it omits the parts that exist: single-flight dispatch, failure and retry, the final flush and quit.
3. **Proved kernel functions that production does not call.** `time::in_window`, `time::active_time`, `time::pause_total` and `time::pause_total_loop` carry `proved` claims. Their only callers are `MonoTick::within_half_open` and `metrics::active_time`, which are called only by tests. The shipped phase deadline is `active.as_us() >= deadline.as_us()` in `engine/src/practice.rs::advance_clock`, and shipped active time is `core::session::TimingSegments`. `docs/TAUTEN.md` states "every function below is called by production code; there is no second copy"; for these four that is false (AGENTS.md: "Do not substitute a verified toy copy for the function used by the application"). The same holds for the tested session table `core::session::next_state`, which the engine never calls: it assigns `session_state` directly in thirteen places.
4. **The proved surface stops short of the decisions that matter.** `AttemptLedger::apply` (buffer, per-word flags) and `TrackProgress::add` / `merge_track` (exercise credit merged once) are tested only; these are unresolved questions 3 and 4 of the [tauten RFC](2026-11-24-tauten-and-lean-kernel.md).

The files under `formal/`, `evidence/` and `tools/run_models.py` are hash-pinned by `MANIFEST.sha256` as the frozen handover packet, so they cannot be edited. That is why an issue alone is not enough: a living model location, registry and gate have to be decided.

Outside this decision: TLAPS or `Machine`/`inv_of_step` unbounded proofs of the protocols (#12 keeps that question), headless models (#4), and running the Lean gate offline (tauten RFC question 2).

## Observable behaviour

No score, timing rule or definition of an error changes. Observable differences:

- `./ci quality` and the `check` workflow fail when a model fails, a mutant configuration passes, TLC is missing or its digest differs, a model times out, or a claim names a production symbol that does not exist.
- `docs/TAUTEN.md` gains a protocol table generated from the registry: model, invariant, mutant, production symbols, badge (`model-proved` at most; never `proved`).
- A key arriving exactly at a phase deadline is still outside the scored slice, but that is now decided by the proved `time::in_window`.

## Proof obligations

### Lean (kernel; theorems over the Aeneas translation)

| Rust entry point (after this RFC) | Theorem | Mutant that must break it | Production caller |
|---|---|---|---|
| `time::in_window` (existing claim) | `Time.in_window_spec` (existing) | existing two | `Engine::advance_clock` decides "this event is inside the phase" through it. No `>=` on deadlines remains in `engine`. |
| `time::segment_active(completed, open_start, now) -> Active` (new) | `Time.segment_active_spec`: result is `completed + (now - start)` exactly, `Overflow` and `Backward` are exact; `segment_active_closed`: with no open segment the result is `completed`; `segment_active_monotone` in `now` | `segment-ignores-open`, `segment-saturates` | `TimingSegments::active_through` and `pause`. |
| `time::active_time`, `pause_total(_loop)` | unchanged | unchanged | Either called by `TimingSegments` or retired by an `[[amend]]` entry stating "no production caller; superseded by `segment_active`". The implementer picks one; leaving them `proved` and unused is not allowed. |
| `ledger::apply_content`, `ledger::apply_backspace` (new; today `core::ledger::AttemptLedger::apply`) | `Ledger.first_write_once`: a slot that is not `Open` is unchanged by any step; `Ledger.backspace_keeps_slots`; `Ledger.buffer_bounded`: `len <= target_len + allowance`; `Ledger.over_bound_rejects`: at the bound, content changes nothing but the interrupted flag; `Ledger.word_flag_sticky`: `any_error`, `interrupted`, `pasted` never clear; `Ledger.complete_iff_exact` | `backspace-reopens-slot`, `flag-clears-on-correction`, `bound-off-by-one`, `complete-ignores-tail` | `AttemptLedger::apply` becomes an adapter over these. Representation is the implementer's choice within what Aeneas accepts (owned `Vec<u8>` slots and flags are known to translate). |
| `progress::merge(total, exercise) -> Merge` (new; today `TrackProgress::add`) and `progress::credit_once(recorded_exercise, closing_exercise) -> bool` | `Progress.merge_exact`: each of the nine counters is the unbounded sum or the result is `Overflow`; `Progress.merge_keeps_valid`: `track_valid` inputs give a `track_valid` output; `Progress.credit_once_spec`: an exercise id is credited iff it is greater than the last credited id | `merge-drops-deletions`, `merge-saturates`, `credit-allows-equal` | `TrackProgress::add`, `engine::practice::merge_track`, `Engine::record_exercise`. This is the engine half of `NoDoubleCredit`. |
| `session::step(state, event) -> Step` (new; replaces `core::session::next_state`) | `Session.clock_runs_only_running`: the step's `clock` command is `Run` only when the next state is `Running`; `Session.review_is_terminal_for_typing`: no step from `Review`, `Finishing` or `Saved` reaches `Running`; `Session.completed_only_after_last_phase`; total | `pause-keeps-clock`, `review-resumes`, `early-end-completes` | Every `self.session_state = ...` in `engine` goes through it (rule 2: typed reducer). |

### TLA+ (TLC; bounded; each model has at least one mutant configuration that must fail on the named invariant)

Living models go in a new `models/` directory with a registry `models/claims.toml`; `formal/` stays frozen.

| Model | Invariants | Mutant configurations | Production symbols the registry must name |
|---|---|---|---|
| `models/CheckpointSave.tla` (extends the idea of `DurableCheckpoint`) | `AckIsDurable` (saved ≤ durable); `NoDoubleCredit`; `SingleFlight` (at most one checkpoint outstanding); `CleanExitIsDurable` (exit status success ⇒ the final-status checkpoint is durable); `FailedTailIsNamed` (after a failure, the UI watermark equals the last acknowledged sequence) | `early-ack`, `double-credit`, `two-in-flight`, `quit-before-ack` | `cadence_engine::storage::{dispatch_checkpoint, save_result, retry_save, request_exit, watchdog}`, `cadence_runtime::writer::SaveWorker`, `cadence_data::store::ProgressStore::apply_checkpoint`, `cadence_kernel::checkpoint::admit` |
| `models/SessionFlow.tla` | `ClockStoppedUnlessRunning`; `NoScoreOutsidePhase` (a content event is scored only in `Running` and inside the half-open window); `InterludeGrace` (no phase advance within the grace of the interlude opening); `EndEarlyIsUnfinished` (ending before the last typing deadline never yields `Completed`); `PauseKeepsReason` | `score-while-paused`, `deadline-inclusive`, `enter-skips-interlude`, `early-end-completed` | `cadence_kernel::session::step`, `cadence_engine::{flow, practice}` entry points, `cadence_kernel::time::in_window` |
| `models/FeedbackLoop.tla` | specified in the [third RFC](2026-11-26-deliberate-practice-in-every-phase.md) | there | there |

`AsyncAdmission` and `CausalBarrier` are recorded in the registry as `design-probe`, `production_symbols = []`, `blocked_on = "#4"`. The gate still runs them from `formal/` unchanged (they are cheap) but they are never shown as mapped.

### Tested, not proved

- That the Rust adapters pass the right facts to the kernel (reducer plumbing, SQL, the worker thread). Behaviour tests stay.
- That a TLA+ model corresponds to the Rust it names. The registry check proves only that the named symbols exist; correspondence is by review plus one runtime test per invariant, named in the registry (`runtime_tests = [...]`).
- TLC results are bounded: badge `model-proved`, constants recorded.

## Design and boundaries

**Registry (`models/claims.toml`).** One `[[model]]` per module with `module`, `config`, `constants`, `invariants`, `production_symbols`, `runtime_tests`, `status` (`mapped` | `design-probe`), and `[[mutant]]` entries (`config`, `violates`). It only grows; weakening an entry needs an `[[amend]]` with a reason, mirroring `proofs/claims.toml`.

**Gate (`tools/tlc_gate.py`, edge tooling, no product code).** Reads the registry; for each positive configuration requires exit 0 and "No error has been found"; for each mutant requires a non-zero exit and "Invariant X is violated" for the registered X. Verifies the TLC jar digest before running anything. Resolves every production symbol against the workspace (`cargo metadata` plus a source search for the item) and every runtime test name against `cargo test -- --list`. Missing tool, digest mismatch, timeout, parse failure, zero models or an unresolved symbol is a failure. Writes `evidence-out/models.json` (not committed).

**CI.** `./ci prepare` fetches the pinned TLC jar by digest into the gate image (the only networked step). `./ci quality` runs the gate offline. `check.yml` needs no new job. The pinned packet runner `tools/run_models.py` is left alone.

**Kernel.** `crates/kernel` gains `ledger`, `session` and the additions to `time` and `progress`. `cadence-core` keeps the typed wrappers (`AttemptLedger`, `TimingSegments`, `TrackProgress`) as adapters; `core::session::next_state` is deleted with its callers migrated (no shim). Dependency direction is unchanged: engine → core → kernel.

**Order.** Gate first with the existing models, then one model or kernel area per PR; each PR adds its registry rows, mutants and runtime tests together.

## Compatibility and reversibility

No persisted format changes. Checkpoint digests are unchanged because counters and their encoding are unchanged; the existing digest fixtures are the regression. Reverting a kernel move means restoring the core implementation and amending the claim, which the registry rules make visible.

## Alternatives

- **Edit `formal/` in place.** Breaks the manifest that pins the handover packet; rejected.
- **Skip TLA+, prove protocols with `Machine` + `inv_of_step` in Lean only.** Stronger, but the save protocol spans a thread and SQLite, and the session flow interleaves five event sources; a bounded model with mutants is the cheaper first check and is what the owner asked for. #12 keeps the unbounded route open.
- **Leave the unused time functions proved.** Costs nothing today, but the badge then claims more than the binary does.

## Evidence and rollout

1. #28 TLC gate and registry over the existing models (finish line: a deliberately broken config fails `./ci quality`).
2. #29 time kernel wired or retired (bug).
3. #30 `CheckpointSave.tla` with mapping and mutants.
4. #31 `SessionFlow.tla` and the kernel session step.
5. #32 ledger `apply` in the kernel.
6. #33 credit merge in the kernel.

Finish line: `docs/TAUTEN.md` lists no claim whose production caller is test-only, `models/claims.toml` has no `mapped` row without symbols, mutants and runtime tests, and the proof gate and TLC gate both pass on the release commit.

## Unresolved questions

None blocking. For the owner to amend if desired: whether `time::active_time` is wired or retired (default: retired by amend, with `segment_active` replacing it), and whether the Lean gate should also join `./ci quality` (tauten RFC question 2; default: stays a separate workflow).

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
