# Cadence — implementation handover

Revision 1 · 8 October 2026 · Owner: Tarek

**Build an offline, Linux-native personal typing trainer in Rust/Ratatui, with a guided 15-minute daily session.** Practice quality, reliable measurement, and maintainable code take precedence over decoration. The owner approved the visual direction and authorised this implementation handover. “Cadence” is a working product/binary name; package and domain availability have not been established. Internal crates need not be published.

This package is an implementation contract, not a working application or a release certificate. Commands below describe interfaces the implementation agent must deliver. The included TLA+ models are small, executable design checks; their actual execution status is recorded in `evidence/packet-validation.json`. Production proofs, implementation correspondence, runtime tests, and Buildbot qualification remain mandatory future work.

## Read in this order

1. [Agent rules](AGENTS.md).
2. [Product and daily practice](docs/01-product.md).
3. [Learning, scoring and planner](docs/02-learning.md).
4. [Architecture and event protocols](docs/03-architecture.md).
5. [UI and configuration](docs/04-ui.md).
6. [Persistence and corpus](docs/05-data.md).
7. [Headless interface and tests](docs/06-headless-tests.md).
8. [Formal assurance](docs/07-assurance.md).
9. [Buildbot, Docker and release](docs/08-build-release.md).
10. [Work packets and acceptance](docs/09-execution.md).
11. [Inspected references and evidence limits](docs/10-references.md).

`HANDOVER.html` combines the documents for convenient reading. Canonical editable contracts are the individual Markdown documents, configuration, schemas, fixtures, and formal specifications. The copied [shared CI plan](reference/shared-buildbot-ci-plan-2026-10-04.md) is the owner's supplied source, preserved unchanged. It governs shared infrastructure; do not copy unrelated Gripsack release matrices into this new application.

## Decisions already made

- Local operation, no login, no telemetry, no remote training service. No LLM/API dependency for daily practice.
- Linux x86_64 is the first qualified target, covering native Linux, WSL2 and interactive Docker. No macOS or native Windows executable. ARM64 is deferred for this new product; this does not withdraw any existing project's ARM/Windows commitments.
- A 15-minute plan, with separate main-keyboard, number-row and numpad learning histories.
- Accurate word/sequence practice, broad real vocabulary, prose transfer checks, optional Rust/shell and local imports.
- One state owner, typed commands/events, domain-specific generations, immutable presentation snapshots, bounded asynchronous work. No blocking I/O or `await` in the input/reducer/render path.
- Same engine and same Ratatui rendering path in the TUI, headless driver and replay.
- Rust-first behavioural tests; a deliberately small real-PTY suite. Python is acceptable for Buildbot configuration and packet tooling, not a second implementation of the trainer.
- TLA+/TLC plus general safety proofs in TLAPS for the small protocols; Verus on the shipping Rust decision/arithmetic kernels. Formal claims must state their assumptions and scope.
- Buildbot on owned WSL/Linux capacity, project-owned gate commands, immutable candidate-bound evidence, resumable qualification, owner installation before optional public distribution.

## First action for the implementing agent

Create a clean project checkout, copy these contracts, and execute P0 in the work-packet document. Inspect the owner's current shared Buildbot setup before provisioning anything. The supplied plan records a decision, not proof that services exist. No repository, CI service, signing identity, domain or registry package was created by this handover.

Implement a small working vertical slice early: type a sentence → preserve first-attempt errors → render it through both frontends → commit a session → reopen the same progress. Continue through all required work packets; do not label that slice the finished product.

## Package helpers

- `python3 tools/check_packet.py`: checks this packet's cross-references, structured contracts, fixture arithmetic, budget totals and manifest.
- `TLA2TOOLS_JAR=/absolute/path/tla2tools.jar python3 tools/run_models.py`: runs bounded models and requires named counterexamples from deliberately faulty variants.
- `contracts/headless-request.schema.json`: normative request envelope and operations for version 1.
- `fixtures/scoring.json`: independent expected results, including correction, pause and numeric cases.
- `fixtures/headless-session.jsonl`: proposed deterministic driver example; requires the future Cadence executable.
- `design/initial-concept.html`: original approved visual reference with illustrative data. It still shows the initial 12-minute proposal. The 15-minute contract in this packet supersedes that number and the browser prototype's simplified scoring/controls. It is not production code or a behavioural oracle.

## Definition of done

All required P0–P8 acceptance rows have source-bound passing evidence; required proof inventories are complete; the exact qualified bundle installs in a fresh private home and completes the daily/headless/native smoke journeys; Buildbot can recover an interrupted qualification without inventing success or redoing valid completed gates. The owner can run `cadence`, press Enter, practise, quit and resume progress without configuration or accounts.
