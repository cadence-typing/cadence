---
rfc: "2026-11-24-tauten-and-lean-kernel"
title: "Adopt the tauten strictness profile and verify the scoring kernel in Lean"
status: accepted
implementation: complete
authors:
  - tknawara
created: 2026-11-24
updated: 2026-11-26
issue: "https://github.com/cadence-typing/cadence/issues/2"
milestone: "0.1.0-alpha.6"
supersedes: []
superseded_by: []
blocked_on: []
---

# Adopt the tauten strictness profile and verify the scoring kernel in Lean

## Summary and motivation

Cadence's product claim is honest scoring: a corrected mistake stays a
first-attempt error, ordinary hesitation stays in active time, and no
checkpoint is credited twice (`AGENTS.md`, rule 1). Until now that rested on
ordinary Rust with property tests; the handover planned to prove the decision
kernels with Verus, and none of that proof work had been done
(`STATUS.md`: "These are not same-source Verus proofs").

This RFC adopts [tauten](https://crates.io/crates/cargo-tauten) 0.2.0 as the
strictness profile and replaces the Verus plan for kernel proofs with Lean 4
theorems over the Charon/Aeneas translation of a small `no_std` crate,
`cadence-kernel`, which production calls. TLA+ models stay for the
interleaving protocols. The adoption record, with the proof inventory and
badges, is `docs/TAUTEN.md`.

The branch `tauten-lean` implements it; this document is the proposal the
owner decides on. Status stays `draft`.

## Observable behaviour

None for the user. The same 188 tests pass unchanged apart from mechanical
migration of three test fixtures (a ledger slot type, a counters field
path, a cast), plus 5 kernel tests and one `BasisPoints` boundary test (193).
Scores are computed exactly as before. Two deliberate differences exist and
are listed under "Compatibility and reversibility": release builds check
integer overflow, and a few `const fn` became `fn`.

For contributors: `cargo +1.98.0 tauten check` must pass, `AGENTS.md` carries
the tauten agent profile, and edits to `crates/kernel` need the proof gate.

## Proof obligations

Shipped Rust entry points (all in `crates/kernel`, all called by production,
see the call-site table in `docs/TAUTEN.md`): `first::settle`,
`first::count_first`, `counters::matched`, `counters::content_step`,
`counters::deletion_step`, `ratio::fraction`, `ratio::wpm`,
`ratio::digits_per_minute`, `ratio::basis_points`, `time::pause_total`,
`time::active_time`, `time::in_window`, `checkpoint::admit`,
`progress::in_range`, `progress::ordered`, `progress::split_fits`,
`progress::track_valid`.

Lean theorems over the Aeneas translation (`proofs/Proofs/*.lean`), by
property:

- Honesty rule: a settled slot is never changed by a later insertion; a
  mismatching first insertion followed by any later insertion is still
  `Incorrect` (`First.settle_write_once`, `mistake_stays_an_error`); a mistyped
  unit counts as an attempt and not as a correct one; an overflow insertion is
  never correct (`Counters.*`).
- Accounting: `first_correct <= first_attempted <= length` (`First.count_first_spec`);
  `correct + overflow <= content` is preserved and the step cannot overflow
  below the `u64` limit (`Counters.content_step_keeps_inv`,
  `content_step_counts_below_limit`); deletions grow by exactly the removed
  count and refuse exactly on overflow.
- Pace and accuracy: exactly `units * 60e6 / (5 * active)` with no `u128`
  overflow; more units never lower pace, idle time never raises it; basis
  points are `floor(num*10000/den)`, never over 100% (`Ratio.*`).
- Time: `active + sum(pauses) = elapsed` in unbounded arithmetic; with no
  pauses nothing is excluded (`Time.*`); the window is half-open.
- Credit: apply iff no receipt and strictly newer; replay, conflict and stale
  never apply; an applied sequence is never applied again; applied sequences
  strictly increase (`Checkpoint.*`), the decision-level `NoDoubleCredit`
  of `formal/DurableCheckpoint.tla`.
- Validity: which track counters are accepted, and that a valid track's
  accuracy is within `[0, 100%]` (`Progress.*`).

Mutants that must break them: 34 textual mutants in `proofs/claims.toml`
(each a one-line change to the Rust that must fail at least one theorem of its
claim; e.g. `settle-forgives-correction`, `admit-equal-sequence-applies`,
`wpm-minute-is-ten-seconds`). The gate fails a claim whose mutant survives
or does not build.

Host assumptions (trusted, not proved): Charon, Aeneas, its Lean library,
Lean, mathlib, rustc's MIR, Docker and the pinned image; the Rust that calls
the kernel passes the right facts (ledger plumbing, the store's receipt and
latest-sequence reads, the SQL transaction, the field copy into
`TrackCounters`).

Stays tested, not proved: the ledger's buffer and word flags, that Backspace
never touches slots, engine merging of exercise progress, the acknowledgement
protocol (TLA+), f64 display, the learner model, planner, corpus, UI.
Handover IDs without a claim (VK-03..06, VL-01, VP-01/02/04/05, VR-01) stay
`pending`.

## Design and boundaries

Tiers: `kernel` for `cadence-kernel`; `core` for `cadence-core`,
`cadence-trainer`, `cadence-engine`; `edge` for `cadence-data`, `cadence-tui`,
`cadence-runtime`, `cadence-driver`, `cadence`, `xtask`. Edges point inward and
`cargo xtask boundary` enforces them.

The kernel is `no_std` with no dependencies, no floats, no iterator chains
and no `?` on `Option` (Aeneas translates that as an axiom the audit refuses);
loops are `while` over `get`. Results that production must act on are small
enums (`Step`, `Pace`, `Total`, `Active`, `Admission`), not `Option`, so Clippy
and the translation agree. Production owns I/O and types; the kernel owns the
decisions. Where a decision used to be inline (`BasisPoints`,
`TrackProgress` validation, `Store::apply_checkpoint`, `AttemptLedger`), the
inline copy was deleted, not kept beside the kernel call.

The proof tier follows tauten's template: `proofs/` with `claims.toml`,
`claims.json`, committed `Extracted/`, `Audit.lean`, `gate.py`, and a Compose
file for the pinned image. Lint exceptions in tests moved to `clippy.toml`
(the four test allowances tauten's own profile makes) so plain Clippy and
`cargo tauten check` agree.

## Compatibility and reversibility

- Public API: `cadence_core::ledger::Outcome` and `first_outcomes() ->
  &[Option<Outcome>]` became `Slot` (`Open`/`Correct`/`Incorrect`) and `&[Slot]`;
  all callers migrated. `BasisPoints::checked_sub`, `UtcDay::days_since` and
  `MonoTick::within_half_open` are no longer `const fn` (they now call
  non-const `From`/`try_from` or the kernel). No persisted or wire format
  changed: digest bytes for `Track`, `KeyLane` and `PlannerMode` come from
  explicit `digest_tag` matches with the same values as the former enum casts.
- Release builds now check integer overflow (`[profile.release] overflow-checks =
  true`, rule TAU-H001). An overflow that previously wrapped would now panic.
  Production arithmetic is checked, and `arithmetic_side_effects` is denied,
  so this should never trigger, but it is a release-behaviour decision.
- Workspace lints gained `as_conversions`, `cast_sign_loss` and promoted
  `cast_possible_truncation`, `cast_possible_wrap`, `integer_division` from
  `warn` to `deny` (already effectively deny under `-D warnings`).
- Reversal: remove `crates/kernel` and inline its functions (each is under 40
  lines and has the old code in git history), delete `proofs/`, the tiers and
  `clippy.toml`. No data migration. The Verus plan in the handover was never
  started, so there is nothing to restore.
- Handover documents are hash-pinned and still describe Verus
  (`docs/07-assurance.md`, `docs/08-build-release.md`, `README.md`); this RFC
  and `AGENTS.md` supersede them for kernel proofs, and they are not edited
  here. `AGENTS.md` itself changed: `cargo xtask verify-contracts` reports it
  against `MANIFEST.sha256` until the branch that unpins it merges.

## Alternatives

- **Verus (the handover's choice).** Same-source proofs: the verified function
  is the compiled one, ghost-erased, with an SMT solver (Z3). Its advantages
  are no translation step and automation. Why not now: none of the Verus work
  existed; it needs a bespoke nightly toolchain and `vstd`; proofs live in
  annotated production source, so every kernel edit mixes proof code with
  product code; and tauten's profile, claim registry, mutant calibration,
  ratchet and agent rules are built around Lean and Aeneas. The cost of the
  Lean route is the trusted translation (Charon/Aeneas) and a 20 GB image;
  the Verus route trusts Verus and Z3 instead. Neither gives a foundational
  guarantee; this RFC claims only what the gate checks.
- **Hand-written Lean models of the Rust.** Rejected: that proves a copy, which
  `AGENTS.md` forbids.
- **Property tests only (status quo).** Kept for everything outside the kernel;
  they do not cover all inputs, which is exactly what totality, overflow-freedom
  and monotonicity claims need.
- **Adopt only the lint profile.** Possible and cheaper (the advisory trial and
  `check` need no Docker). It leaves the honesty rule asserted by tests alone.
- **Prove more of the ledger** (the whole `apply`, the buffer and word flags)
  by moving it into the kernel. Deferred: it needs a bigger translatable
  surface and would drag types such as `TrainingChar` into the kernel.

## Evidence and rollout

- Advisory trial output: `evidence/tauten/advisory-trial.jsonl` (55 findings under
  assumed tiers). `cargo tauten check` after the change: exit 0, no errors, one
  warning (a wildcard arm over a `#[non_exhaustive]` rusqlite enum). No baseline.
- Tests: 193 pass (188 before); `cargo clippy --workspace --all-targets --locked`
  clean; release build works.
- Proof gate: result and per-claim badges are recorded in `proofs/claims.json`
  and summarised in `docs/TAUTEN.md`, "Result of the last gate run".
- Rollout: merge `tauten-lean`; add the proof gate as a networked Compose
  service next to `verify` (the existing `verify` and `prove` services are
  placeholders for Verus and TLAPS); decide whether CI blocks on
  `cargo tauten check` and on `cargo tauten proofs` (nightly or per change to
  `crates/kernel`, `proofs/`).

## Unresolved questions

1. Accept `overflow-checks = true` in release (TAU-H001), or record it as a
   baseline entry and keep wrapping?
2. Should the proof gate join the offline product gates? It currently needs
   network access inside a fresh container (Charon's MIR sysroot) and a 20 GB
   image that is not yet in the pinned image set (`docs/decisions/0001`).
3. Extend the kernel to the ledger's buffer and word flags so the whole
   `AttemptLedger::apply` is covered, or keep the plumbing tested?
4. Move `TrackProgress::add` and `merge_track` into the kernel to prove that
   exercise credit is merged exactly once, which is the engine-level half of
   `NoDoubleCredit`?
5. Should the pinned handover documents (`docs/07-assurance.md`, `README.md`)
   be amended to say Lean replaced Verus for kernel proofs, given they are
   hash-pinned?
6. Which of VK-03 (allocation), VK-04 (candidate selection) and VK-06 (review
   queue) is next, and are they small enough for Aeneas?
7. Is `cadence-tui` better as `core` (it only renders a snapshot)? It is `edge`
   today because it is the presentation adapter; nothing in the checker depends
   on it.

## Decision log

- 2026-11-24: drafted on branch `tauten-lean`. Decision by the owner pending;
  status remains `draft`.
- 2026-11-26: Accepted by default under the owner's rule adopted from Writfold (AGENTS.md, "Issues, milestones and RFCs"); the owner may amend. Delivered on `main`: 19 of 19 claims proved with all mutants killed (`791aaee`), proofs workflow green on `main`.
