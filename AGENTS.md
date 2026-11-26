# Working rules for Cadence

These rules express the owner's requested quality constraints for the future project. They do not change instructions in reference repositories.

## Product and architecture

1. Training correctness is the product. Never improve displayed scores by forgiving corrected mistakes, excluding ordinary hesitation, or changing difficulty without recording it.
2. All application and UI state mutations pass through the engine's typed reducer. Workers return facts; widgets render snapshots. No sibling component mutation, interior-mutable global app state, or worker access to `&mut AppState`.
3. Input admission → reduction → presentation preparation → render must not await, perform filesystem/database/network I/O, launch a process, or do corpus-wide work.
4. Every asynchronous result carries the complete identity and dependency versions it was computed against. Check successful and failed outcomes. Cancellation does not prove freshness.
5. Headless, replay and live mode call the same reducer and renderer. Fixtures replace effect ports, never product logic. Do not add a demo-only practice path.
6. Ordered semantic events and durable acknowledgements cannot be silently dropped or coalesced. Only explicitly replaceable notifications/snapshots may coalesce, with their truth retained elsewhere.
7. Offline means the practice process starts and works with networking disabled, after installing the bundle. All normal history is local.

## Rust and maintainability

- Strong domain types for IDs, generations, revisions, units, positions, counts, probabilities and byte/cell/text lengths. No primitive soup, sentinel numbers, boolean mode combinations or stringly typed events.
- No naked policy numbers. Named, typed constants or validated policy fields own budgets, timeouts, limits, weights and thresholds; document units and rationale. Arithmetic identities `0`/`1`, enum encodings with a schema, and explicit test examples are fine. Naming `MAGIC_500` is not compliance.
- Checked arithmetic at all external-input/counter boundaries. Generations and revisions never wrap; retire an exhausted slot or stop the operation with a typed error. Saturation is allowed only when clipping is the specified operation, not to conceal lost data.
- Production libraries return typed errors. `anyhow` is limited to CLI/xtask aggregation. No `unwrap`, `expect`, `panic!`, unchecked indexing, ignored results, or warning suppression in normal application logic. Unreachable paths need a typed invariant and an explicit reviewable justification.
- `unsafe` is forbidden in owned application crates initially. If an unavoidable terminal/platform seam emerges, isolate it in one narrow module with an invariant, safe alternative assessment and tests; do not weaken workspace policy globally.
- Use `cargo fmt --check` and Clippy with warnings denied. Review targeted `#[expect(..., reason = ...)]` only for actual documented false positives; no blanket lint disabling.
- Aim for modules around 300–400 lines; 600 lines triggers review and decomposition; 800 lines requires an explicit exceptional justification in the same change. Tests count. Generated fixed tables are exempt and carry a generator/provenance link.
- Split by responsibility, not arbitrary line ranges. Functions with load-bearing branch order should usually become typed state transitions or decision tables.
- Prefer small concrete interfaces and established dependencies. Do not create a framework, plugin system, home-grown database, CI scheduler, allocator, terminal parser or PRNG.
- No hot-path full-state serialization, whole-corpus scan, unbounded vectors/channels, per-key SQL transaction or per-key allocation of the entire target text.
- No compatibility shim left behind when an internal interface changes: migrate every caller and delete the old implementation together. Persisted/wire schema compatibility is explicit and must never be silently dropped.
- Dependency versions and features are pinned by committed locks. Dependencies must have a named purpose, compatible licensing and documented security/update ownership. Do not use floating `latest` in qualification recipes.

## Verification and evidence

- Test contracts, behaviours and failure boundaries. Prefer the cheapest layer that exercises the real code. No coverage-percentage target, source-text tests pretending to be behaviour tests, or sleeps as deterministic synchronization.
- Tests run with an explicit temporary profile; never read/write the owner's real HOME, history or credentials. No network in product gates.
- Every required formal property has a named production mapping, positive run and negative calibration. Tool absence, parse failure, timeout, skipped proof or zero obligations is a failure/blocked result.
- Do not substitute a verified toy copy for the function used by the application. Kernel proofs are Lean theorems over the Charon/Aeneas translation of `crates/kernel`, which production calls (see the tauten profile below and `docs/TAUTEN.md`; this replaces the earlier plan to prove kernels with Verus). TLA+ models remain for the interleaving protocols. Differential testing is useful evidence, not a universal refinement proof.
- No `assume`, `admit`, `sorry`, unreviewed `external_body`, Lean `axiom`, or circular lemma in claimed theorems. The proof gate's axiom audit admits only `propext`, `Classical.choice` and `Quot.sound`. List tool/library/OS assumptions and approved trusted adapters; a result is shown with the badge its controls admit (`proved`, `pending`, `not-established`) and nothing stronger.
- Preserve failing attempts and exact candidate identities. A green cache entry or successful subset is not complete qualification.
- Docker Compose is the canonical reproducible gate route. Host Rust is optional for iteration. Buildbot schedules the same repository commands; proof/test logic does not live in controller configuration.
- Do not mutate shared CI infrastructure, disk layouts, public releases or other projects as an incidental fix. Implement the Cadence adapter and report missing shared prerequisites concretely.

## Work completion

Maintain a compact status ledger mapping requirement IDs to implementation, tests, proof claims and receipts. Finish all authorised P0–P8 work; do not replace required behaviour with a TODO and describe the feature as complete. Report actual limitations and blocked gates precisely. Human-learning effectiveness is evaluated empirically; software proofs do not certify a guaranteed WPM gain.

## Issues, milestones and RFCs (owner, 2026-11-26, adopted from Writfold)

- **All remaining and future work is a GitHub issue in a milestone.** A commit that does it names the issue (`Refs #N`; `Closes #N` on the last). Work that landed before 2026-11-26 is recorded by closed `backfill` issues in the `0.1.0-dev (P0–P2)` and `0.1.0-alpha.1`…`alpha.5` milestones.
- **RFCs** in `docs/rfcs/` ([process](docs/rfcs/README.md)) cover designs that are costly to reverse and any new or changed decision logic, with proof obligations listed first. **The owner accepts RFCs by default:** an agent marks one `accepted` with a decision-log line, implements it, and the owner reviews and amends afterwards. Do not stall work waiting for acceptance.
- Close an issue only when its work is on `main` and gated; never close one to make a count.
- **Lean proofs first.** Every decision the trainer makes (what counts as an error, a pace, a focus skill, a due review, a phase allocation, a repeat) lives in `crates/kernel`, is extracted with Charon/Aeneas and has its properties proved in Lean, and `cargo tauten proofs` passes (drift, `lake build`, axiom audit, mutants) before it is wired into the product. Existing tested-only decisions are backfilled with proofs. Parsers, IO, rendering and numeric plumbing stay tested-only, and each RFC says which parts those are.
- **One TLC model per real protocol**, with mutant configurations that must fail, a named production mapping per claim in `formal/claims.json`, and the models run as a gate. A model whose claim has no production mapping is `pending`, never `proved`.

## Strictness profile (tauten)

This workspace uses the tauten profile. `cargo tauten check` must pass before
you report a task as done. Run it with `--format json` to get one diagnostic
per line; each has a rule `id`, the `file`/`line`, and a `fix`.
`cargo tauten explain <ID>` prints the full rule.

**The architecture on one page.** Each line says what enforces it, and how
strongly: *proved* (a Lean theorem over the deciding code; the facts it is
given are read by tested, unproved code), *compiler-enforced* (rustc rejects
the code), *lint* (`cargo tauten check` reports it; an exception needs a
reason), *tested*, or *not checked*. Links are to
[tauten](https://github.com/tauten-dev/tauten/blob/main/) sources.

1. **Decisions are total kernel functions with claims.** Put a decision in a
   `kernel` crate as a plain function that returns for every input.
   - The crate is `#![no_std]`, so it cannot do IO: *compiler-enforced*,
     required by TAU-K003 (decided by the proved
     [`layout::k003`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/layout.rs)).
   - No panics, unchecked arithmetic, indexing, floats or std paths:
     TAU-P001–P004, TAU-N001–N003, TAU-K001–K002, *lint*.
   - A claim in `proofs/claims.toml` (a Lean theorem over the function's
     translation, plus mutants that must break it) is recorded `proved` by
     `cargo tauten proofs` only when it builds, passes the axiom audit and
     every mutant fails it: *proved* (gate admission is
     [`control::admit`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/control.rs)).
     A `total` theorem lets `check` treat TAU-N001/P003 findings in that
     function as subsumed: *proved*
     ([`subsume::subsumes`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/subsume.rs)).
   - That a decision has a claim at all: *not checked*. Add one.
2. **Protocols are machines.** Write a protocol as
   [`tauten::machine::Machine`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten/src/machine.rs):
   `step(state, event) -> (state, command)`, with a driver that carries out
   the commands.
   - `step` takes no capability: *compiler-enforced* (the trait signature);
     in kernel and core crates it cannot reach ambient IO either (TAU-A001,
     *lint*).
   - A recorded tape replays exactly (`machine::replay`): *tested*.
   - An invariant kept by every step holds in every reachable state
     (`Tauten.Machine.inv_of_step`, shipped as `proofs/Proofs/TautenMachine.lean`):
     *proved* once; your protocol's invariant is proved only when you add
     that claim.
3. **The world is reached only through capabilities named in the effect
   row.** Core code takes capabilities
   ([`tauten::cap`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten/src/cap.rs):
   `Clock`, `Env`, `Dir`, `ReadDir`, `#[derive(Cap)]` bundles) as
   parameters; only the edge creates them.
   - Ambient filesystem, environment, clock, network, stdio and randomness
     in kernel and core: TAU-A001, *lint*. `Authority::acquire` outside the
     edge: TAU-A002, *lint*.
   - `Authority` and `SystemClock` have private constructors, and a
     `#[derive(Cap)]` field must be a capability: *compiler-enforced*. Test
     fakes (`ManualClock`, `Env::from_pairs`) can be built anywhere; they
     reach nothing real.
   - `#[effects(..)]` lists exactly what the signature grants (TAU-F001):
     *lint*, decided by the proved
     [`effects::verdict`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/effects.rs);
     parameter types are classified by name, and an unclassified type only
     warns. With `effects = "required"` in a crate's
     `[package.metadata.tauten]`, a missing or wrong row is an error.
4. **A check travels as evidence or a binding.**
   - [`tauten::origin::Evidence`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten/src/origin.rs)
     (`#[evidence(subject = T)]`) borrows the state it checked: changing
     the state while it lives, forging it outside its module, or cloning it
     does not compile: *compiler-enforced* (until `redeem()`; redeem it in
     the call that acts).
   - A recorded result (a proof outcome, a generated file) is reused only
     while every input it was bound to is byte-identical: *proved*
     ([`binding::fresh`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/binding.rs)).
5. **A result is shown with the badge its controls admit.** `proved` only
   for a fresh Lean theorem over the extracted code with no assumption;
   otherwise `proved-under`, `model-proved`, `bounded`, `tested`,
   `reviewed`, `pending` or `not-established`: *proved*
   ([`badge::badge`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/badge.rs),
   [`control::admit`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/control.rs)).
   A timeout or a missing tool never counts as a pass. Say only what the
   badge says.
6. **Registries are never weakened.** A recorded claim may not lose a
   theorem or mutant or gain an assumption without an `[[amend]]` entry
   with a reason (TAU-R002), and the lint baseline may only shrink
   (TAU-R001): *proved*
   ([`registry::conserves`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/registry.rs),
   [`ratchet`](https://github.com/tauten-dev/tauten/blob/main/crates/tauten-check-kernel/src/ratchet.rs)),
   relative to the committed `proofs/claims.json` and baseline.

**Tiers.** Every crate declares `[package.metadata.tauten] tier`:
- `kernel`: pure decisions. `no_std`, no IO, no floats, no `HashMap`, no
  shared mutability, no lint exceptions at all.
- `core`: library logic. No direct filesystem, environment, process, clock,
  randomness, network, stdout/stderr or global state. These arrive as
  parameters (a capability, a `&mut impl Write`, a `Clock` trait).
- `edge`: `main`, startup, adapters. The only place that touches the world
  or creates capabilities. Keep it thin.

Dependencies point inward only: edge → core → kernel.

**Protocols are step machines.** Write a protocol (a publish, a lease, a
retry) as `tauten::machine::Machine`: a pure `step(state, event) ->
(state, command)` with an empty effect row, and let an edge or core driver
carry out the commands. The empty effect row is what makes a recorded tape
replay exactly (`tauten::machine::replay`) and what lets the protocol be
proved once by induction (`Tauten.Machine.inv_of_step` in the proofs).

**When a rule fires, fix the cause.**
- Do not add `#[allow]`. If an exception is truly correct, use
  `#[expect(lint, reason = "why this is safe here")]` on the smallest item,
  and say so in your report. Some rules (stubs, `unsafe` outside the edge,
  anything in a kernel crate) admit no exceptions.
- Do not move code to the edge tier, change a crate's tier, or edit
  `[lints]`/`clippy.toml` to make a finding go away.
- Replace `unwrap`/`expect` with `?` and a typed error enum.
- Replace `a + b` with `checked_add` (or `saturating_*` when that is the
  specified behaviour), `v[i]` with `.get(i)`, `as` with `try_from`.
- List every enum variant instead of `_ =>`.
- Never leave `todo!()`, `unimplemented!()`, `dbg!()` or "for now" comments.

**Tests.** Tests may panic, index, do arithmetic, use temp files and print.
They may not silently drop a `Result`, use stubs, or use `#[allow]`. Put
helpers used only by tests inside `#[cfg(test)]` modules. Never delete,
weaken or `#[ignore]` a test to make a task pass; report the conflict instead.

**Proved kernel code.** A kernel function with a proof must stay inside
what the pinned translation toolchain accepts (lean-4.35.0-aeneas-c5eb4a2-charon-8cae8fd).
`cargo tauten proofs extract` decides; these notes are advice:
- No trait objects (`dyn Trait`, `&dyn Fn`): use an enum or a generic.
- No names Lean reserves (`given`, `set`, `at`, `fun`, `have`, `show`, ...) for parameters, locals or fields; the extract stage builds the translation, so Lean reports any such name.
- Do not name a parameter like its own module (`fn width(span: Span)` in `mod span`): in Lean it shadows the module's types.
- No floating point: use integers or fixed point.
- No slices of structs that hold borrows: use owned data (`Vec<u8>`, indices).
- At most one loop per function when it returns early from inside a loop: split the loops into helpers.
- No `match` on `xs.get(i)` directly in a loop body: use `if let Some(x) = xs.get(i)` or a helper.
- No iterator chains (`iter().map(..).sum()`): they translate only as axioms, which the proof audit refuses. Write a `while` loop over `get`.
Known to translate:
- `while` loops over `get`, `matches!` / `match` on enums, owned `Vec<u8>`, `saturating_*`.
- Integer `as` casts translate (the profile still prefers `try_from` for narrowing).
- A derived `PartialEq` used with `==` translates.

### Cadence specifics

- **Crates and tiers.** `cadence-kernel` is `kernel` (`no_std`, no dependencies, the only crate Lean theorems are about).
  `cadence-core`, `cadence-trainer` and `cadence-engine` are `core`. `cadence-data`, `cadence-tui`, `cadence-runtime`,
  `cadence-driver`, `cadence` (app) and `xtask` are `edge`. Dependencies point inward; `xtask boundary` enforces the exact
  edges, including `core -> kernel` and `data -> kernel`.
- **What the kernel decides.** First-attempt slots and counters, pace and accuracy arithmetic, active time and the half-open phase
  window, track-counter validity, and which checkpoint is applied, replayed, refused or stale. Production calls these
  functions; never add a second copy of any of them, and never move scoring arithmetic out of the kernel to avoid a proof.
- **Commands.** `cargo +1.98.0 tauten check` (structure and Clippy) must pass before a task is reported done, in addition
  to the Cadence gates (`cargo clippy --workspace --all-targets --locked`, `cargo test --workspace --locked`). The proof
  gate is `cargo +1.98.0 tauten proofs` (needs Docker and the pinned proofs image); `cargo tauten proofs extract` checks that
  kernel edits still translate; `cargo tauten proofs try --mutants <name>` is the developer loop. After editing
  `crates/kernel`, run `cargo fmt`, then `sh proofs/refresh.sh`, commit `proofs/Extracted/`, and rerun the gate.
- **tauten runs on Rust 1.98.0.** The product stays on the pinned 1.97.1 (`rust-toolchain.toml`); only the tool uses 1.98.0.
- **Registries.** `proofs/claims.toml`, `proofs/claims.json` and the lint baseline only grow; weakening one needs an
  `[[amend]]` entry with a reason.
- **Test lint allowances** are exactly the four in `clippy.toml` (`unwrap`, `expect`, `panic`, indexing in tests); other
  exceptions are `#[expect(.., reason = "..")]` on the smallest item. Integration-test crates carry `#![cfg(test)]` so Clippy
  recognises their helpers as test code.
- **Honest limits.** A proved kernel function says nothing about the Rust code that calls it (adapters, SQL, the reducer);
  those stay tested. `docs/TAUTEN.md` lists every claim, its badge and what is trusted.
