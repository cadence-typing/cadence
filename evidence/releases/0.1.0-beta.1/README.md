# 0.1.0-beta.1 release evidence (#66)

Captured on 2026-11-26 on temporary profiles, no network.

- `scripted-eight-sessions.txt`: eight real 5-minute daily plans (policy v4,
  the shipped corpus) played through the engine into one temporary profile
  with virtual time (`crates/app/src/scripted_sessions.rs`, run with
  `CADENCE_EVIDENCE_OUT`). Sessions 1-6 are typed cleanly at 300 correct
  units a minute, session 7 with a slip every 8 keys. The words goal starts
  from the baseline (session 2), rises from session 4, falls after the sloppy
  session (session 8), and session 8 - after seven completed sessions with
  prose - carries the one transfer probe.
- Schema migration: a copy of the schema-4 profile from the alpha.7 PTY run
  (two full sessions, 1,128 receipts) opened by the beta.1 build
  (`cadence export`): version 4 -> 5, 2 sessions and 1,128 receipts intact,
  every session `live`, no pace or probe rows invented; the export reads
  schema 5.
- `corpus-gate.txt`: `cargo xtask gate corpus` on the release content.
- Proof gate: 57 of 57 claims proved, 142 theorems, 127 mutants killed
  (`docs/TAUTEN.md`, `proofs/claims.json`); the TLC gate passes in
  `./ci quality`.
