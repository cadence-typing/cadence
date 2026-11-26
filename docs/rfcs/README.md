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
4. Only the owner accepts or rejects a proposal; merging a draft is not acceptance. Record the decision and its rationale in the RFC's decision log. Keep rejected and superseded records, and link replacements in both directions.
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
| plan/0001 | [Centered visual focus and large practice text](2026-10-19-visual-focus.md) | accepted (typography amended by the next row) | complete | — |
| plan/0002 | [Smooth native 2× terminal text](2026-10-19-smooth-native-text.md) | accepted | complete | — |
| plan/0003 | [Learn from mistakes; vary natural prose](2026-11-13-learning-from-mistakes.md) | accepted | complete | — |
| plan/0004 | [Calm flow and results that answer "how did I do"](2026-11-14-calm-flow-and-honest-results.md) | accepted | complete | — |
| — | [Public distribution: install script, website and project process](2026-11-24-public-distribution.md) | draft | in-progress | [#1](https://github.com/cadence-typing/cadence/issues/1) |
| — | [Adopt tauten and prove the scoring kernel in Lean](2026-11-24-tauten-and-lean-kernel.md) | draft | in-progress | [#2](https://github.com/cadence-typing/cadence/issues/2) |
| — | [Formal gates with production mappings](2026-11-26-formal-gates-and-production-mapping.md) | accepted | not-started | [#25](https://github.com/cadence-typing/cadence/issues/25) |
| — | [Practice decisions in the kernel](2026-11-26-practice-decisions-in-the-kernel.md) | accepted | not-started | [#26](https://github.com/cadence-typing/cadence/issues/26) |
| — | [Deliberate practice in every phase](2026-11-26-deliberate-practice-in-every-phase.md) | accepted | not-started | [#27](https://github.com/cadence-typing/cadence/issues/27) |
