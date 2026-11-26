# 0.1.0-beta.2 release evidence (#74)

Captured on 2026-11-26, no network, on temporary profiles.

- `headless-gate.txt`: `cargo xtask gate headless`: the six journeys in
  `fixtures/journeys/` replayed by the release build.
- `tlc-gate.txt`: `tools/tlc_gate.py` (pinned TLC 1.7.4): 8 models, 38
  configurations, including `RequestCache.tla` and `HeadlessBarrier.tla`
  with every mutant failing on the invariant it names.
- Proof gate: 62 of 62 claims proved, 152 theorems, all 128 mutants killed,
  on a snapshot of `eb290fd` (`docs/TAUTEN.md`, `proofs/claims.json`).
- Golden frames: `crates/app/tests/golden/` (start, practice, numpad,
  results, save-error at 60x18 and 100x32; too-small at 40x12).
