# 0009: Lean in place of Verus and TLAPS

Status: accepted (2026-11-27), issue #91. Departs from `docs/07-assurance.md`
V-01, which stays hash-pinned as handed over (`MANIFEST.sha256`); this record
and `docs/TAUTEN.md` state what is used instead.

## Context

V-01 asks for TLA+/TLC for interleavings, TLAPS for inductive protocol
safety and Verus for the executable Rust kernels. It also says why: prove
the functions the application actually calls, not a hand-written copy.

Since alpha.1 the kernels have been proved with `tauten`'s Lean gate
instead: `crates/kernel` (`no_std`, no dependencies) is translated by
Charon and Aeneas to Lean, and the theorems are about that translation of
the production source. The protocols that TLAPS would have covered are
written as kernel step machines (`save::step`, `session::step`, and the
request, ticket and barrier decisions placed in their drivers' machines),
and their invariants are proved for every reachable state by induction
over steps (`Tauten.Machine.rinv_of_step`), #89 and #91.

## Decision

- Lean (via Charon/Aeneas and `cargo tauten proofs`) replaces Verus for the
  executable kernels and TLAPS for unbounded protocol safety.
- TLA+/TLC stays for interleavings with the worker, the store and the
  driver, and for liveness under stated fairness (`models/`).
- The handover's claim ids are bound to the evidence in
  `assurance/claims.toml`, checked by `cargo xtask assurance` and the
  `prove`, `verify` and `correspondence` gates.

## Consequences

- The V-01 property holds: the proofs are about the production source, with
  no Lean reimplementation of the Rust. The trusted base is different from
  Verus's: Charon, Aeneas and its library, Lean and mathlib, and rustc's
  MIR (`docs/TAUTEN.md`, *Trust boundary*), instead of Verus and an SMT
  solver.
- TLAPS-style proofs of the TLA+ models themselves are not done. The
  models remain bounded checks; the unbounded statements are about the
  kernel machines, and that the models and the machines agree is review
  and the named runtime tests, not proof.
- `formal/claims.json` stays frozen; its ids keep their handover meaning
  and the manifest says which theorems, models and tests stand for each.
