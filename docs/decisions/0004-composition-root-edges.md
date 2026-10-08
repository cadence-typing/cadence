# Decision 0004 — composition-root dependency edges (P1)

Date: 2026-10-08 · Status: accepted

## Context

docs/03-architecture.md A-01 lists workspace dependencies as
"binary -> runtime/driver/tui/data", omitting `cadence-engine` and
`cadence-core`. The same table's ownership column says the binary must
not own "reusable domain implementation", and the prose requires "no
reverse edge from engine into runtime/tui/driver".

The P1 live owner loop (ingress → reduce → prepare → paint) is the
composition root's job: only the binary may legally combine the engine
(reduce/prepare), the runtime (terminal lifecycle/ingress) and the TUI
(paint), because runtime must not depend on tui and tui must not depend
on runtime.

## Decision

- The `cadence` binary may depend on every library crate (it is the
  composition root); the ownership prohibitions from A-01 apply
  unchanged.
- The `xtask` boundary table treats the binary edge list as
  "all library crates" while keeping every other crate's edges exactly
  as A-01 specifies.
- The handover document itself is not edited (it is pinned by
  `MANIFEST.sha256`); this record is the binding interpretation.

## Verification

`cargo xtask boundary` (part of the `quality` gate) enforces the table,
including the composed edge; `cargo xtask verify-contracts` continues to
pin the unmodified handover text.
