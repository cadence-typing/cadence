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
| `time::active_time` (with `pause_total`) | `metrics::active_time` |
| `time::in_window` | `MonoTick::within_half_open` (the phase deadline, A-05) |
| `progress::track_valid` (with `in_range`, `ordered`, `split_fits`) | `SessionProgress::validate`, before a checkpoint is built or a stored one decoded |
| `checkpoint::admit` | `Store::apply_checkpoint` (`crates/data/src/store.rs`): apply, replay, conflict or stale |

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
| `time::pause_total_loop`, `time::pause_total` | `Time.pause_total_loop_spec`, `pause_total_spec` | The loop returns the exact sum of the pauses, or `Overflow` exactly when the exact sum exceeds `u64::MAX`. | `pauses-saturate`, `pauses-skip-last` | proved |
| `time::active_time` | `Time.active_time_spec`, `active_time_le_elapsed`, `active_time_no_pauses` | `active + sum(pauses) = elapsed` in unbounded arithmetic; the overflow and pauses-exceed cases are exact; with no pauses nothing is excluded (hesitation stays in active time). Total. | `active-ignores-pauses`, `pauses-exceeding-clamp-to-zero`, `pauses-saturate` | proved |
| `time::in_window` | `Time.in_window_spec` | `start <= tick < deadline`; a key at the exact deadline is outside the scored slice. Total. | `window-deadline-inclusive`, `window-start-exclusive` | proved |
| `checkpoint::admit` | `Checkpoint.admit_spec`, `admit_apply_iff`, `refused_never_applies`, `applications_strictly_increase`, `applied_sequence_never_reapplies` | The full decision table. A checkpoint is applied iff no receipt exists and it is strictly newer than the latest applied one; replay, conflict and stale are never applied; once a sequence is applied (the store then holds it as its latest sequence, written in the same transaction: trusted, tested), offering it again in any receipt state is never applied (no double credit). Total. | `admit-equal-sequence-applies`, `admit-replay-applies`, `admit-conflict-replays` | proved |
| `progress::in_range`, `ordered`, `split_fits` | `Progress.in_range_spec`, `ordered_spec`, `split_fits_spec` | Exactly which counters are accepted. Total. | `range-skips-active`, `ordered-skips-first-pass`, `split-ignores-overflow-attempts` | proved |
| `progress::track_valid` | `Progress.track_valid_spec`, `valid_accuracy_at_most_full`, `valid_track_accuracy_in_range` | A valid track has `first_correct <= first_attempted <= content`, and its first-pass accuracy basis points are computed, never over-full and at most 10000 (when the scaled numerator fits `u64`). Total. | the three above and `valid-skips-ordering` | proved |

The "proved" badges above are the gate's; if the table and `proofs/claims.json`
ever disagree, `proofs/claims.json` wins.

### Not established (and why)

These are not proved, and nothing in this record should be read as claiming
them.

| Property | Badge | State |
|---|---|---|
| The ledger calls `settle` for every insertion and Backspace never touches the slots (`AttemptLedger::apply` plumbing, the buffer, per-word flags) | tested | `crates/core/tests/ledger_properties.rs` (write-once, correction cannot erase first errors, counters monotone) and the fixtures. Proving the whole `apply` would need a bigger, Aeneas-translatable ledger in the kernel. |
| The store reads the receipt and latest sequence correctly, the SQL transaction is atomic, the receipt insert and the session update commit together | tested | `crates/data/src/tests/checkpoints.rs` and `disk.rs`. The kernel only decides from facts it is given. |
| Durable acknowledgement ordering: an acknowledgement is never observed before the commit (`AckIsDurable`) and loss of replies | not-established | Model-checked in `formal/DurableCheckpoint.tla`, which the TLC gate runs as a `design-probe` (see *Protocol models*); the shipped save protocol gets its own mapped model in #30. The kernel proves the credit half (`NoDoubleCredit` at the decision level), not the acknowledgement protocol. |
| `TrackProgress::add`, merging exercise progress into a track and into the session (`merge_track`), the engine's per-exercise credit | tested | Checked addition in `cadence-core`; no kernel claim. Double counting across exercises is covered by engine tests, not proofs. |
| Display rounding and formatting (`as_f64`, percent labels), the UI comparison with earlier sessions | tested | f64 is outside the kernel by construction. |
| VK-03 allocation, VK-04 candidate selection, VK-05 quotas, VK-06 review queue, VL-01 fairness, VP-01/02/04/05 protocols, VR-01 replay equivalence | pending | Unchanged from `STATUS.md`; no Lean claim yet. |
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
| `formal/DurableCheckpoint.tla` | TypeOK, AckIsDurable, NoDoubleCredit | `DurableCheckpoint-early-ack.cfg` → `AckIsDurable`<br>`DurableCheckpoint-double-credit.cfg` → `NoDoubleCredit` | none; blocked on #30: the shipped save protocol is modelled by models/CheckpointSave.tla, which carries the production mapping | design-probe (not mapped) |
| `formal/AsyncAdmission.tla` | TypeOK, AcceptedFresh, AtMostOnce | `AsyncAdmission-no-epoch.cfg` → `AcceptedFresh`<br>`AsyncAdmission-no-pending.cfg` → `AtMostOnce` | none; blocked on #4: no job registry with epoch/request tickets is shipped | design-probe (not mapped) |
| `formal/CausalBarrier.tla` | TypeOK, NoEarlyReturn | `CausalBarrier-early-return.cfg` → `NoEarlyReturn` | none; blocked on #4: the headless driver and its barriers are not shipped | design-probe (not mapped) |
<!-- models:end -->

## Result of the last gate run

Recorded by `cargo +1.98.0 tauten proofs` into `proofs/claims.json`; the
summary line, the per-claim outcomes and the mutant results are copied below
from that run (see the commit that adds `proofs/claims.json`).

Command: `cargo +1.98.0 tauten proofs` (full level, every mutant), gating a
snapshot of commit `5e1c4fa4c62aee34a2bfb9ea0bf9e3ba4a54979a`; exit 0:

```text
tauten: 19 of 19 claim(s) proved (recorded in proofs/claims.json)
```

Stages: tools ok, extract ok, drift ok (committed `proofs/Extracted/` equals a
fresh translation), `lake build` ok, axiom and coverage audit ok (only
`propext`, `Classical.choice`, `Quot.sound`; 90 audited theorems including
tauten's vendored machine lemmas), mutant calibration ok: all 34 mutants
`killed`, each by at least one directly failing theorem. Every claim has
outcome `proved` and badge `proved`; none is `proved-under` (the claims take
no assumptions). Two mutant anchors had been reflowed by rustfmt in an earlier
run and reported `error`; they were fixed and re-run before this record.

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
  | VP-03 durable checkpoint | `checkpoint::admit` (credit half) | Decision proved; acknowledgement protocol stays in TLA+ |

- Floating-point display and every f64 in the learner model are outside the
  kernel and unverified.
- `cargo xtask quality` fails in this checkout at the handover-contract step
  because `evidence/model-checks/*.cfg.log` are absent here (they are
  generated by the model gate); it also reports `AGENTS.md` as changed
  against `MANIFEST.sha256`, which resolves when this branch merges with the
  branch that unpins it.
