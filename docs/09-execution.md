# Execution work packets and acceptance gates

## Sequence and ownership

One integration owner maintains the dependency graph, architecture contracts, gate inventory and status ledger. Work may be split by crate only after the interfaces are agreed; this handover does not require multi-agent execution. Preserve the owner's priority: practice quality first, code quality throughout, then configuration/themes and delivery polish. Follow P0–P8 in order; work on tests/proofs alongside the relevant implementation rather than treating them as a final documentation task.

Every packet ends with a coherent compiling change and evidence. Keep short decision records for material changes. The next agent should make routine engineering decisions within these contracts and record them, rather than repeatedly asking the owner to reconfirm approved scope. Only a genuine scope/authority conflict or unavailable required environment blocks the relevant step.

## P0 — Freeze the implementation baseline

- Read this package and the current shared Buildbot plan/infrastructure state. Check for a pre-existing Cadence checkout before creating another project.
- Record application name as provisional and initial support profile `linux-x86_64-v1`.
- Resolve actual build/proof dependency compatibility: Rust, Ratatui, crossterm, rusqlite, Verus/vstd/solver, TLC, TLAPS, test crates and shared Buildbot adapter. Pin versions, checksums and image digests. Do not copy source-reference pins without an actual build/verify check.
- Establish corpus sources/licences, normalization and a reproducible acquisition recipe. Archive required notices; define the initial frequency bands and prose benchmark source.
- Create the workspace, error/ID/time types, ports, status ledger and gate inventory. Include architecture dependency checks.
- Verify CI-owned output/cache capacity and permission paths without touching unrelated data. No public infrastructure provisioning is required to write/build locally.

**Exit:** clean Docker build of an empty composition, locks committed, corpus sourcing decision reproducible, all gates classified, no undefined ownership or false toolchain claims. `quality` works; other gates visibly pending.

## P1 — Accurate core and minimal shared UI

- Implement checked types, session state machine, immutable target, first-outcome ledger, exact metrics and timing exclusions.
- Implement the single reducer and initial view preparation. Wire keyboard input and Ratatui TestBackend/live render through the same engine.
- First working UI: Today/start, a single practice exercise, error/cursor feedback, pause/resume, result, normal quit.
- Add hand-calculated scoring fixtures and state/property tests. Begin VK-01/VK-02 proofs in the production source.

**Exit:** `cat` correction fixture, partial exercise, overflow and pause cases give exact documented ratios; rendering cannot mutate the session; no key lost on ordinary input; pure headless and live paths share one handler. Demonstrate the slice, labelled experimental.

## P2 — Corpus and useful 15-minute trainer

- Implement licensed pack loader/indexes and at least the required 20,000 vetted word forms, natural prose, numeric generators and layout validation.
- Implement per-track observation summaries, the bounded candidate scan, 6/2/2 lanes/caps/fallbacks, review due queue, soft pace hysteresis and exact phase allocation.
- All algorithm coefficients/limits come from validated named policy data/types. Add VK-03 through VK-06 as functions land.
- Implement the complete default plan and progress explanations, first-run baseline, first-pass versus attempt accuracy, separate numeric histories.
- Add benchmark comparability/contamination rules and synthetic-learner pathology simulations. Record policy version; do not promise a specific WPM gain.

**Exit:** a deterministic seed/model/corpus produces a replayable 900-second plan; disabled tracks and shorter durations obey the allocator; no candidate loop can hang; unknown/sparse skills remain uncertain; tiny/infeasible corpus fails explicitly; default content breadth/count/licence/capacity gate passes.

## P3 — Bounded async updates and durable progress

- Add result-slot worker lifecycle, domain request generations, bounded input/effect/result transport and fairness per turn.
- Move SQL, corpus imports/indexing, model rebuilds and heavy progress queries off the owner thread.
- Implement XDG/config/database paths, single writer, migrations, cumulative checkpoints, ack/retry/recovery and durable watermark.
- Add save-error UI, controlled quit, backup/export, interrupted-session recovery and writer lock handling.
- Extend VP-01/VP-02/VP-03 models/proofs and actual-code correspondence schedules.

**Exit:** delayed old success and delayed old failure are harmless; queue storms do not drop terminal results; ordinary input remains responsive during save/import; ack loss/retry/reopen applies a session once; last accepted checkpoint survives process kill; bad disk/schema/lock states remain visible and non-destructive.

## P4 — Headless protocol, captures and replay

- Implement the v1 schema/envelope with strict validation, ordered request admission/cache, fixture/native service modes and explicit profile isolation.
- Implement all operations, discoverable actions, causal/durable barriers, actual shared Ratatui cell capture and trace replay.
- Add VP-04/VP-05, descendant-work tests, rejected-result trace coverage and divergence reporting.
- Ship a documented agent debugging recipe: reproduce from trace or fixture, inspect snapshot/cells, drive actual keys, capture first divergence, fix shared production code, add focused regression.

**Exit:** the example driver transcript runs against the built binary; wrong version/ID/payload/revision cannot mutate; barriers include follow-ups and persistence; replay has identical state/effects/cells and refuses missing or modified input; headless cannot use real HOME accidentally.

## P5 — Full product surface and configuration

- Complete Today/Train/Numpad/Progress, settings/help, terminal-size fallback and input ownership.
- Ship three semantic-role themes, optional keypad guidance, validated config/reload and correct binding hints.
- Add Rust/shell practice and explicit local import with bounded normalization preview. Never execute imported snippets.
- Ensure scoring-affecting settings apply at the declared boundary, layout changes partition skills, theme changes preserve them.

**Exit:** all requested modes work offline; no placeholder progress; settings/errors understandable; typical and minimum viewport captures accepted; no hard font/graphics/mouse dependency. Approved visual direction retained.

## P6 — Focused e2e, native integration and performance

- Complete the approximately 12 headless journeys, targeted render boundary cases and 3 native PTY smoke tests.
- Finish native database process-crash/migration/lock/storage campaigns.
- Run deterministic parser/ID/time/plan property campaigns and preserve regressions. No arbitrary sleeps.
- Measure actual latency/memory at stated corpus/viewport/load. Correct causes of stalls; do not weaken fidelity to meet a speed figure.

**Exit:** all declared behaviour contracts covered at an appropriate level; native PTY lifecycle works; test suite is bounded and hermetic; resource limits measured; no unresolved lost-input/double-save/stale-result/replay failures.

## P7 — Complete the assurance inventory

- Complete the full TLA+/TLC inventory and negative mutants, extend the included probes to the production design.
- Complete general TLAPS protocol safety and the fairness ranking claim with declared assumptions.
- Complete same-source Verus obligations and caller precondition establishment. Capture expected/actual theorem inventory and fail on omitted modules/zero proofs.
- Map each protocol claim to reducer/runtime/store paths, and run adverse schedules through those paths. Verify production cannot compile with enabled test mutants.
- Review assumptions and unsupported claims. Do not claim every third-party library, OS or human-learning effect is proven.

**Exit:** each required VP/VK/VL/VR row has the exact evidence it requests; positive proofs and named negative calibration succeed; source/tool/schema identities match candidate; no placeholders/admits/unchecked trusted-body additions. Packet TLC runs alone do not satisfy this exit.

## P8 — Buildbot integration and installed-owner journey

- Add the Cadence adapter to the shared operator flow; run one heavyweight gate at a time initially.
- Freeze an actual release candidate after version/locks/policies are final. Run the complete inventory and preserve attempt receipts.
- Demonstrate duplicate nomination, observer detachment, worker/controller restart and resumption without losing completed valid evidence.
- Produce/sign the immutable bundle through the shared owner mechanism; install the actual bundle in fresh private Linux/WSL homes and Docker.
- Perform the real WSL terminal check, including actual keypad protocol detection and fallback. Record the terminal/version; don't infer support from a fixture.
- Verify signature/digest/target rejection and rollback-safe executable promotion; keep data-schema limitations explicit.
- Provide one command to install/use and one command to inspect/resume qualification. No public registry task is necessary.

**Exit:** complete exact inventory qualifies; the installed bytes are the qualified bytes; the owner can practise offline and retain progress; Buildbot recovery and profile restore are demonstrated. Missing shared controller/signing/terminal evidence stays explicitly blocked, never converted to a pass.

## Final acceptance walkthrough

1. Install into a fresh private profile, network disabled, ordinary terminal font.
2. Choose actual layout/numpad once; press Enter into the 15-minute daily plan.
3. Make and correct an error; verify first-pass history remains honest.
4. Complete word/row/pad/prose stages; distinguish unknown capability from verified physical keypad input.
5. Leave/reopen; recover completed progress and review scheduling once.
6. Generate a headless capture/trace in an isolated profile and replay it identically.
7. Apply an invalid config/theme/import; retain last valid state with useful diagnostics.
8. Interrupt a qualification, return later, resume it and install the resulting accepted bundle without GitHub Actions.

## Status template to maintain in the repository

Each row: requirement ID; status (`pending`, `in_progress`, `implemented`, `verified`, `blocked`); source symbols; tests; proof claim/evidence; candidate/receipt links; concrete remaining issue. “Implemented” is not “verified.” Final release aggregation checks the manifest, not a manually edited Markdown checkbox.
