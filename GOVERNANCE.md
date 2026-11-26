# Governance

The owner ([@tknawara](https://github.com/tknawara)) decides Cadence's scope, accepts or declines work, sets release milestones, and decides what ships. Contributors and agents may report problems, draft RFCs, review proposals, and implement accepted work. **Only the owner can accept or reject an RFC.** Merging a draft, putting an issue in a milestone, or passing a gate does not accept a design. Since 2026-11-26 the owner accepts RFCs written by the project's agents by default: the agent marks the RFC `accepted` with a decision-log line saying so, and the owner reviews and amends afterwards ([AGENTS.md](AGENTS.md)). RFCs from other contributors still wait for the owner.

[AGENTS.md](AGENTS.md) holds the engineering rules. No issue, RFC or milestone overrides it; changing those rules is itself an RFC.

## Triage

A [GitHub issue](https://github.com/cadence-typing/cadence/issues) tracks a problem, the agreed outcome, and the work left to do. Open one before substantial work. For exceptions and private security reports, see [Contributing](CONTRIBUTING.md) and [Security](SECURITY.md).

| Label | How to use it |
| --- | --- |
| `needs-triage` | New issue awaiting an owner decision. Remove it on acceptance or decline. |
| `accepted` | The owner wants this outcome. If it also needs an RFC, implementation waits until the owner accepts the design. |
| `declined` | The owner decided not to pursue it. Record why, then close as *not planned*. |
| `blocked` | Work cannot proceed. Link the dependency or state what would unblock it. |
| `needs-owner` | A specific question only the owner can settle. Remove it when the answer is recorded. |
| `needs-rfc` | A design decision is required before implementation. |
| `rfc` | Tracks a specific RFC. Link the file from the issue and the issue from the RFC. Replaces `needs-rfc` once the RFC exists. |
| `needs-proof` | A kernel decision needs a Lean theorem over the Aeneas translation of the shipped Rust. Remove it when the claim is recorded `proved`, not when a test passes. |

Add one type label: `bug`, `enhancement` or `documentation`. Add an area when it helps route work:

| Area | Scope |
| --- | --- |
| `area/kernel` | `no_std` decisions and their Lean proofs (`proofs/`) |
| `area/training` | Planner, learner model, corpus and content (`crates/trainer`, `corpus/`) |
| `area/engine` | Reducer, sessions, scoring and presentation snapshots (`crates/core`, `crates/engine`) |
| `area/storage` | SQLite profile, migrations, backup and export (`crates/data`, `crates/runtime` writer) |
| `area/tui` | Rendering, layout, themes and terminal handling (`crates/tui`, `crates/runtime`) |
| `area/headless` | NDJSON driver, replay and capture (`crates/driver`) |
| `area/release` | Packaging, `install.sh`, CI and release qualification |
| `area/site` | [cadence-typing.github.io](https://cadence-typing.github.io) |

A blocker or area label is not a triage decision.

## Scheduling and closure

The owner assigns accepted work to a [milestone](https://github.com/cadence-typing/cadence/milestones): the next preview, a later release, or `Later` when no release is chosen. An issue without a milestone has no release commitment. Do not invent releases or due dates. When a release is tagged, move unfinished issues to the agreed milestone or `Later` with a note on each, then close the release milestone.

Use `Refs #N` for partial work and `Closes #N` only when the agreed outcome lands on `main`. Accepting an RFC is a decision, not evidence of delivery. Design decisions, implementation state and supersession links live in the [RFC registry](docs/rfcs/README.md). Release-facing changes go in [CHANGELOG.md](CHANGELOG.md). Requirement-level evidence lives in [STATUS.md](STATUS.md).

## Claims

Cadence states what it measures, proves, tests and assumes. Every public claim, whether in the README, the website, release notes or the changelog, carries the strongest badge its evidence supports and no stronger: `proved`, `model-checked`, `tested`, `reviewed` or `pending`. A missing tool, a timeout or a skipped gate never counts as a pass. Software proofs do not certify a learning gain; whether practice helps is an empirical question.
