# Cadence RFCs

Start with a [GitHub issue](https://github.com/cadence-typing/cadence/issues). Issues track the problem, the work and its closure. [Milestones](https://github.com/cadence-typing/cadence/milestones) group accepted work by release, or under `Later` when no release is assigned. An RFC records a design and why it was chosen. Neither an issue nor an RFC is a release receipt.

## When to write an RFC

Write one before changing any of these:

- what counts as an error, how time is measured, or how scores are computed;
- the learner model or the session plan policy;
- a persisted format (SQLite schema, export, backup) or the headless protocol;
- a CLI contract users or scripts depend on;
- a kernel decision or the premises of its proof;
- a dependency, platform or release policy that would be costly to undo.

An ordinary bug fix that restores the existing contract needs only an issue and a regression test. See [Contributing](../../CONTRIBUTING.md) and [Governance](../../GOVERNANCE.md).

## Propose and decide

1. Find or open the issue. The owner triages it; `needs-rfc` means the design must be reviewed before implementation.
2. Copy [0000-template.md](0000-template.md) to `YYYY-MM-DD-kebab-title.md`. Use the drafting date, not a serial number, and keep the filename if the title changes. Link the issue both ways, label it `rfc`, and add a row to the registry below.
3. For a scoring, timing or kernel change, write the **proof obligations before the design**: which shipped Rust decision changes, the Lean theorem over its Aeneas translation, the mutant that must break it, and what is only tested or assumed. An RFC cannot waive [AGENTS.md](../../AGENTS.md).
4. Only the owner accepts or rejects a proposal; merging a draft is not acceptance. RFCs written by the project's agents are accepted by default (owner, 2026-11-26): the agent sets `accepted` with a decision-log line, implements, and the owner amends afterwards. Record the decision and its rationale in the RFC's decision log. Keep rejected and superseded records, and link replacements in both directions.
5. Implementation PRs use `Refs #N`, or `Closes #N` for the final delivered scope. Update the RFC's `implementation` field and its registry row in the same PR.

## Frontmatter

| Field | Meaning |
|---|---|
| `rfc`, `title` | Filename stem and heading text. |
| `status` | `draft`, `accepted`, `rejected` or `superseded`. A replacement must be accepted before it supersedes a decision. |
| `implementation` | `not-started`, `in-progress`, `partial`, `complete`, `removed` or `n/a`, for the agreed scope. |
| `authors`, `created`, `updated` | Authors, drafting date and last material revision (`YYYY-MM-DD`). |
| `issue`, `milestone` | Tracking issue URL and its milestone title, or `null`. |
| `supersedes`, `superseded_by`, `blocked_on` | Lists of RFC stems; for blockers, issue URLs or specific owner questions. A section-level amendment is described in the text, not marked as a whole-record replacement. |
| `legacy_id` | Backfilled records only: the original `plan/NNNN` citation. Never allocate a new one. |

Frontmatter is authoritative for RFC status and implementation. GitHub is authoritative for issue state and scheduling.

## History

Before 2026-11-24, designs were written as numbered plans in `plans/`. They were moved here unchanged apart from added frontmatter and a backfill note; citations like `plan 0003` resolve through the **Legacy** column. The original implementation contract, the handover packet, is frozen in [docs/handover/](../handover/README.md) and `docs/01-*` … `docs/10-*`, pinned by `MANIFEST.sha256`. Binding interpretations of that contract are the decision records in [docs/decisions/](../decisions/).

## Registry

Creation order. New proposals use `—` in the Legacy column.

| Legacy | RFC | Decision | Implementation | Tracking issue |
|---|---|---|---|---|
| plan/0001 | [Centered visual focus and large practice text](2026-10-19-visual-focus.md) | accepted (typography amended by the next row) | complete | [#21](https://github.com/cadence-typing/cadence/issues/21) |
| plan/0002 | [Smooth native 2× terminal text](2026-10-19-smooth-native-text.md) | accepted | complete | [#22](https://github.com/cadence-typing/cadence/issues/22) |
| plan/0003 | [Learn from mistakes; vary natural prose](2026-11-13-learning-from-mistakes.md) | accepted | complete | [#23](https://github.com/cadence-typing/cadence/issues/23) |
| plan/0004 | [Calm flow and results that answer "how did I do"](2026-11-14-calm-flow-and-honest-results.md) | accepted | complete | [#24](https://github.com/cadence-typing/cadence/issues/24) |
| — | [Public distribution: install script, website and project process](2026-11-24-public-distribution.md) | accepted | complete | [#1](https://github.com/cadence-typing/cadence/issues/1) |
| — | [Adopt tauten and prove the scoring kernel in Lean](2026-11-24-tauten-and-lean-kernel.md) | accepted | complete | [#2](https://github.com/cadence-typing/cadence/issues/2) |
| — | [Formal gates with production mappings](2026-11-26-formal-gates-and-production-mapping.md) | accepted | complete | [#25](https://github.com/cadence-typing/cadence/issues/25) |
| — | [Practice decisions in the kernel](2026-11-26-practice-decisions-in-the-kernel.md) | accepted | complete | [#26](https://github.com/cadence-typing/cadence/issues/26) |
| — | [Deliberate practice in every phase](2026-11-26-deliberate-practice-in-every-phase.md) | accepted | complete | [#27](https://github.com/cadence-typing/cadence/issues/27) |
| — | [Release bundles built from a tag in public CI](2026-11-26-release-bundles-from-a-tag.md) | accepted | complete | [#3](https://github.com/cadence-typing/cadence/issues/3) |
| — | [Evidence attribution and repeat-gap decisions in the kernel](2026-11-26-evidence-and-repeat-gap-decisions.md) | accepted | complete | [#54](https://github.com/cadence-typing/cadence/issues/54) |
| — | [A slow save pauses practice instead of invalidating the session](2026-11-26-slow-save-pauses-practice.md) | accepted | complete | [#51](https://github.com/cadence-typing/cadence/issues/51) |
| — | [Review days are local-calendar days with a recorded offset](2026-11-26-local-calendar-days.md) | accepted | complete | [#52](https://github.com/cadence-typing/cadence/issues/52) |
| — | [Pace targets and benchmark probes](2026-11-26-pace-targets-and-benchmark-probes.md) | accepted | complete | [#6](https://github.com/cadence-typing/cadence/issues/6) |
| — | [Headless driver and replay: proved request admission and barriers](2026-11-26-headless-driver-and-replay.md) | accepted | complete | [#4](https://github.com/cadence-typing/cadence/issues/4) |
| — | [Configuration, keyboard layout and the settings surface](2026-11-26-configuration-layout-and-settings.md) | accepted | complete | [#9](https://github.com/cadence-typing/cadence/issues/9) |
| — | [Local text imports and Rust/shell practice, kept apart from scores](2026-11-26-local-imports-and-code-practice.md) | accepted | complete | [#10](https://github.com/cadence-typing/cadence/issues/10) |
| — | [Append-only observation chunks and a rebuildable learner state](2026-11-26-observation-chunks.md) | accepted | complete | [#11](https://github.com/cadence-typing/cadence/issues/11) |
| — | [Unbounded protocol proofs as kernel machines in Lean](2026-11-26-protocol-proofs-as-kernel-machines.md) | accepted | complete | [#12](https://github.com/cadence-typing/cadence/issues/12) |
