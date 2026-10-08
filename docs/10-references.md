# Inspected references and evidence limits

## Owner input

- The conversation approved the quiet Ratatui design and requested a detailed implementation handover, Linux/WSL/Docker, strong Rust quality, typed/generational asynchronous UI updates, headless debugging, bounded e2e and formal verification.
- The owner explicitly set the practice objective to approximately 15 minutes/day in the latest message. This supersedes the first prototype's 12-minute example.
- `reference/shared-buildbot-ci-plan-2026-10-04.md` is the unchanged uploaded CI proposal. Its canonical owner path is `/home/tarek/workspace/SHARED-BUILDBOT-CI-PLAN.md`. Current services, disk capacity and release infrastructure were not inspected on the owner's workstation during this handover.

## Source inspection performed for this handover

The public repositories were cloned read-only into the task scratch area on 8 October 2026. No edits were made to them. Do not treat their build/deployment rules as overriding the owner's new Buildbot direction.

### Strop

Repository: https://github.com/stropdev/strop

Inspected commit: `b09e8e4e959ccec6e0ecc1039d0e43c6045e3391`.

| Path at that commit | Observed design used here |
|---|---|
| `AGENTS.md` | No await in input→render; typed errors; module discipline; Docker gate path |
| `crates/strop-engine/src/lib.rs` | Engine independent of physical terminal; composition root owns terminal and captures |
| `crates/strop-engine/src/editor/events.rs` | Typed app events, bounded transport, lossless semantic completions, coalescible wake hints |
| `crates/strop-engine/src/editor/trace/drive.rs` | Recording at shared action boundary before filtering; replay through the same handlers |
| `crates/strop-engine/src/editor/trace/services.rs` | Shared service completion handlers for live and headless delivery |
| `crates/strop-engine/src/editor/prepare.rs` | Explicit prepared presentation keyed by identity/revision/generation |
| `crates/strop-core/src/viewguard.rs` | Same-source Verus freshness decision called from the actual paint path |
| `crates/strop/src/headless.rs` and `headless/driver.rs` | Ratatui TestBackend frames, shared event channel, explicit jobs barrier |
| `plans/0006-e2e-harness.md` | Rust headless behavioural tier; small PTY tier; caveats about timing/VT scraping |
| `Dockerfile` | Separate pinned TLC, TLAPS and same-source Verus lanes |

These are observed patterns, not a claim Cadence shares or inherits Strop's proofs. Cadence needs its own smaller models and production mappings. No Strop source has been copied into the application. If implementation copies code, check the actual licence/attribution at the chosen revision.

Reference tool pins observed in Strop (not a Cadence toolchain lock): TLC 1.7.4 with SHA-256 `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`; Verus `0.2026.09.06.8dea4a2`; TLAPS 1.5.0 release tag `202210041448`. This handover downloaded and checksum-verified the TLC jar for the included finite design checks. It did not run Strop's full build/proof/test suite.

### Rootle

Repository: https://github.com/rootledev/rootle

Inspected commit: `0be7cbc9512081e231dddef20e09b6cc62be570f`.

Read `doc/development.md` and `crates/rootle/src/headless.rs`. Observed action-mediated sibling coordination, domain-tagged request generations, headless shared key/event/render paths, and explicit settle bounds. The public README describes headless and Python/PTY e2e. Cadence adopts the deterministic shared-handler principle and uses Rust for its main suite; no claim is made that all Rootle tests were run or migrated.

### Ratagrid

https://kahwee.github.io/ratagrid/ was inspected visually for the original concept. Inspiration is its restrained terminal palette, alignment, semantic colours and thin borders. Ratagrid is not required as a dependency; use a standard Ratatui table unless its actual interactions justify a grid dependency. The concept is illustrative and contains no measured user performance.

## Primary references informing contracts

- [XDG Base Directory Specification 0.8](https://specifications.freedesktop.org/basedir/0.8/): separation of config, durable data/state, cache and runtime; absolute-path rule.
- [Buildbot current ShellCommand reference](https://docs.buildbot.net/current/manual/configuration/steps/shell_command.html): worker command execution, argv-list interface, logs and timeout behaviour. “Current” is a moving documentation alias; resolve the installed release in the implementation lock.
- [Buildbot architecture](https://docs.buildbot.net/current/manual/introduction.html) and the owner's supplied plan: shared scheduler versus project gate responsibilities.
- [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/): legacy ambiguity, progressive enhancement, keypad identities and event reporting. Detect capability on the actual terminal; no blanket assertion of Windows Terminal support.
- [Dhakal et al., 2018, Observations on Typing from 136 Million Keystrokes](https://userinterfaces.aalto.fi/136Mkeystrokes/): large observational study of typing patterns. Correlation does not establish optimal training treatment. Its dataset's use restrictions must be checked before any import; the trainer does not require this dataset.
- [Yamaguchi and Logan, 2014, chunking in skilled typewriting](https://pubmed.ncbi.nlm.nih.gov/23875575/): supports examining whole-word/pattern behaviour.
- [Yamaguchi and Logan, 2016, memory chunking](https://pubmed.ncbi.nlm.nih.gov/27336783/): repetition/spacing effects depend on task conditions. The app's intervals and allocations are engineering hypotheses, not a clinical or universal learning prescription.
- [Verus research paper](https://arxiv.org/abs/2303.05491): SMT-based Rust verification and ghost/executable modes.
- [TLA+ tools source](https://github.com/tlaplus/tlaplus) and [TLAPS](https://github.com/tlaplus/tlapm): model checking and proof tools have different scopes. See the exact tool receipt for checks run here.

## What this package does not establish

No Cadence implementation exists in this package. No real user typing baseline was measured. No broad vocabulary/prose pack is included or licensed yet. No Cadence Rust build, Verus proof, TLAPS proof, runtime e2e, SQLite durability test, terminal-performance measurement, owner-machine inspection, Buildbot deployment or signed release was performed. All of those remain explicit work/acceptance items. The validation report distinguishes document/schema/fixture checks and bounded TLA model checks from future application qualification.
