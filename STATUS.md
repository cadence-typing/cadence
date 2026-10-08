# Cadence — implementation status ledger

Per docs/09-execution.md: each row carries status (`pending`,
`in_progress`, `implemented`, `verified`, `blocked`), source symbols,
tests, proof claims and receipts. "Implemented" is not "verified".
Gate-level truth lives in `cargo xtask gates` (B-05 inventory); this
ledger maps work packets to that inventory.

Last updated: 2026-10-08 (P0 complete, P1 next).

## Work packets

| Packet | Status | Notes |
|---|---|---|
| P0 baseline | verified | Workspace, typed core, boundary+manifest checks, Docker composition, decision records 0001–0003. Evidence: host `cargo xtask quality` green; offline Docker `quality` service green; pending gates fail honestly. |
| P1 core + minimal UI | pending | Scoring ledger, reducer, shared render path, fixture tests. |
| P2 corpus + planner | pending | Sources/licences decided in `docs/decisions/0002-corpus-sourcing.md`; acquisition recipe reproducible. |
| P3 async + durable progress | pending | |
| P4 headless + replay | pending | |
| P5 full surface + config | pending | |
| P6 e2e/PTY/perf | pending | |
| P7 assurance inventory | pending | |
| P8 Buildbot + install | partially blocked | Local gates runnable; `buildbot-recovery` blocked on shared controller (decision 0003). |

## P0 acceptance detail

| Requirement | Status | Evidence |
|---|---|---|
| No duplicate checkout | verified | Only `~/cadence/cadence-handover` (packet) existed; implementation lives at `~/workspace/cadence` |
| Name provisional + support profile | implemented | Binary `cadence`; profile `linux-x86_64-v1` declared in decision 0001/0003 posture |
| Dependency compatibility actually checked | verified | Full workspace build + tests + clippy with pinned versions (decision 0001) |
| Corpus decision reproducible | implemented | Decision 0002; acquisition tooling lands with P2 |
| Workspace + types + ports | implemented | `crates/core`: ids, tick, units, ports, error taxonomy; ownership docs in every crate root |
| Architecture dependency checks | verified | `cargo xtask boundary` (A-01 edge table), part of `quality` |
| Handover contracts pinned | verified | `cargo xtask verify-contracts` recomputes `MANIFEST.sha256` (48 files) |
| Clean Docker build of composition | verified | `docker compose build prepare`; `quality` service passes with `network_mode: none` |
| CI capacity/permissions | inspected | Decision 0003; no shared infrastructure touched |
| `quality` works; others visibly pending | verified | `cargo xtask gates` output; `./ci gate headless` exits nonzero naming P4 |

## Gate inventory state

Authoritative list: `cargo xtask gates` (also `./ci gates`). As of this
update: `quality` implemented; all other gates pending their packets;
`buildbot-recovery` blocked (shared controller absent).

## Verification receipts (P0)

- Host: `cargo run -p xtask -- quality` → exit 0 (fmt, clippy
  `-D warnings`, 12 unit tests, boundary, manifest).
- Docker: `docker compose -f docker/docker-compose.yml run --rm
  prepare` → exit 0; `run --rm quality` (network disabled) → exit 0.
- Honest failure probe: `./ci gate headless` → exit 1 with
  `gate headless is not implemented yet (work packet P4)`.

## Known gaps carried forward

- cargo-deny licence/dependency audit not yet wired (planned in
  `quality` extension, P1).
- TLAPS not installed; TLC requires the pinned Docker image (no host
  Java) — `model`/`prove` service images pinned at P7.
- Verus same-source kernel pattern unproven until the first P1 kernel.
