# Cadence — implementation status ledger

Per docs/09-execution.md: each row carries status (`pending`,
`in_progress`, `implemented`, `verified`, `blocked`), source symbols,
tests, proof claims and receipts. "Implemented" is not "verified".
Gate-level truth lives in `cargo xtask gates` (B-05 inventory); this
ledger maps work packets to that inventory.

Last updated: 2026-10-08 (P0, P1 verified; P2 in progress: trainer
algorithm core + corpus gate done; plan builder/engine wiring pending).

## Work packets

| Packet | Status | Notes |
|---|---|---|
| P0 baseline | verified | Workspace, typed core, boundary+manifest checks, Docker composition, decisions 0001–0004. Evidence: host `cargo xtask quality` green; offline Docker `quality` green; pending gates fail honestly. |
| P1 core + minimal UI | implemented | `cadence-core` ledger/metrics/session, engine reducer, shared paint, live slice. All fixture/property/render tests green; real-PTY journey smoke-verified (drill → type → pause/resume → ctrl-c restore). Verus kernel wiring pending → P7. |
| P2 corpus + planner | in_progress | Policy parsing, exact allocation, lanes/caps, bounded scan, review queue, pace hysteresis all tested (32 trainer tests). Licensed 25k-word pack + corpus gate green. Pending: deterministic 900s plan builder, numeric generators, engine/Today wiring. |
| P3 async + durable progress | pending | |
| P4 headless + replay | pending | |
| P7 assurance inventory | pending | |
| P8 Buildbot + install | partially blocked | Local gates runnable; `buildbot-recovery` blocked on shared controller (decision 0003). |

## P0 acceptance detail

| Requirement | Status | Evidence |
|---|---|---|
| No duplicate checkout | verified | Only `~/cadence/cadence-handover` (packet) existed; implementation at `~/workspace/cadence` |
| Name provisional + support profile | implemented | Binary `cadence`; profile `linux-x86_64-v1` (decisions 0001/0003) |
| Dependency compatibility actually checked | verified | Full workspace build + tests + clippy with pinned versions (decision 0001) |
| Corpus decision reproducible | implemented | Decision 0002; acquisition tooling lands with P2 |
| Workspace + types + ports | implemented | `crates/core`: ids, tick, units, ports, error taxonomy |
| Architecture dependency checks | verified | `cargo xtask boundary` (A-01 edges + decision 0004), part of `quality` |
| Handover contracts pinned | verified | `cargo xtask verify-contracts` recomputes `MANIFEST.sha256` (48 files) |
| Clean Docker build of composition | verified | `docker compose build prepare`; `quality` service passes with `network_mode: none` |
| CI capacity/permissions | inspected | Decision 0003; no shared infrastructure touched |
| `quality` works; others visibly pending | verified | `cargo xtask gates`; `./ci gate headless` exits nonzero naming P4 |

## P1 acceptance detail

| Requirement (P1 exit) | Status | Evidence |
|---|---|---|
| `cat` correction fixture exact ratios | verified | `crates/core/tests/scoring_fixtures.rs`: all 8 hand-calculated cases + numpad + timing case pass against the real ledger/metrics |
| Partial exercise, overflow, pause cases | verified | Same suite (`incomplete`, `overflow-then-correction`, timing pauses) |
| Corrections cannot change history | verified | `crates/core/tests/ledger_properties.rs` proptest: write-once, monotone counters, buffer bound, no-erase |
| Rendering cannot mutate the session | verified | `cadence-tui` paint takes `&PreparedView` (pure); engine owns all state |
| No key lost on ordinary input | verified | Engine ordered admission (sequence check refuses gaps without mutation) |
| Pure headless and live share one handler | verified | `crates/tui/tests/render_path.rs` drives the same `Engine::reduce` + `paint` through `TestBackend`; live loop (`crates/app`) uses the same pair |
| First working UI journey | verified | PTY smoke 2026-10-08: Today → Enter drill → type → Esc pause (visible) → resume → Backspace → Ctrl-C; alternate screen/raw mode restored; exit 0 |
| VK-01/VK-02 proofs begun in production source | partial | Kernels exist with documented contracts + property tests; the Verus toolchain lane is not yet invoked — tracked as P7 obligation, honestly not claimed |
| Viewport totality | verified | `paint_is_total_for_every_nonzero_viewport` at 1×1 … 240×80 incl. one-below-minimum |

## Gate inventory state

Authoritative list: `cargo xtask gates` (also `./ci gates`). As of this
update: `quality` implemented; all other gates pending their packets;
`buildbot-recovery` blocked (shared controller absent).

## Verification receipts

- P0: host `cargo run -p xtask -- quality` → 0; Docker `prepare` → 0;
  Docker `quality` (network disabled) → 0; honest-failure probe of
  `./ci gate headless` → exit 1 naming P4.
- P1: `cargo test --workspace` green (43 tests across core/tui incl.
  fixtures + proptests); clippy `-D warnings` clean; PTY journey logs
  (exit 0, screen text present, alt-screen enter/leave bytes verified).

## Known gaps carried forward

- cargo-deny licence/dependency audit not yet wired (`quality`
  extension, next packet).
- Verus lane not invoked; VK-01/VK-02 kernels await same-source proof
  wiring (P1 began, P7 completes).
- TLAPS not installed; TLC needs the pinned Docker image (no host Java)
  — `model`/`prove` images pinned at P7.
- Engine `MeasurementInvalid` outcomes pause scoring but have no visible
  diagnostic surface yet (P3).
