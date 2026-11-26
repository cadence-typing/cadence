---
rfc: "2026-11-26-headless-driver-and-replay"
title: "Headless driver and replay: proved request admission and barriers"
status: accepted
implementation: complete
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/4
milestone: "0.1.0-beta.2"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Headless driver and replay: proved request admission and barriers

## Summary and motivation

The handover specifies the headless protocol (H-01 to H-04, `contracts/headless-*.schema.json`, `contracts/frame.schema.json`, `fixtures/headless-session.jsonl`). `crates/driver` is a 12-line placeholder. Evidence for behaviour today is reducer tests plus hand-driven PTY runs, which is slow and cannot be replayed. This RFC does not redesign the protocol; the handover text is the contract. It decides the three things the handover left open and that are costly to get wrong:

1. which parts are kernel decisions with Lean claims (request admission, barrier completion);
2. which TLA+ models are living and mapped (`CausalBarrier` in `formal/` is frozen and a design probe);
3. the order of work, models and proofs first.

Outside this decision: native-service mode with the real clock beyond a smoke test, a socket or daemon (D-01 says stdio only), and the journeys that need features from later milestones (first run, config, benchmark).

## Observable behaviour

No change to live practice, scores or timing. New:

```sh
cadence doctor --json
cadence headless --stdio  --profile-dir /abs/profile --clock virtual --services fixture --fixture-root /abs/fixtures
cadence headless --script journey.jsonl --ephemeral-profile --clock virtual --services fixture --fixture-root /abs/fixtures
cadence replay recording.jsonl --read-only --capture-dir /abs/captures
```

`headless` refuses to start without `--profile-dir` or `--ephemeral-profile`. Sessions it records are stored with `provenance = synthetic` (column added by schema 5) and never enter comparisons or benchmarks. Each stdout line is one protocol record; diagnostics go to stderr. Errors use the stable codes of the schema (`StaleState`, `RequestIdConflict`, `RequestExpired`, ...) and mutate nothing.

## Proof obligations

### TLA+ first (living models in `models/`, registered `design-probe` until the driver lands, then `mapped`)

| Model | Invariants | Mutant configurations |
|---|---|---|
| `models/RequestCache.tla` (VP-05): ordered stream, bounded cache with eviction, repeats and conflicting repeats | `AtMostOnceApply`: a request id changes state at most once; `ConflictNeverMutates`; `ExpiredNeverMutates`; `CacheBounded` | `replay-reapplies`, `conflict-applies`, `evicted-reexecutes` |
| `models/HeadlessBarrier.tla` (VP-04): extends the idea of `CausalBarrier` with child registration at result consumption and a durable scope over the checkpoint watermark | `NoEarlyReturn`: a completed causal barrier has no outstanding job, pending dispatch or unconsumed result in its scope; `ChildBeforeDrain`: a child is registered before its parent is drained; `DurableCoversWatermark`: a completed durable barrier implies acknowledged sequence >= captured watermark; `LaterWorkExcluded` | `early-return`, `child-after-drain`, `ack-below-watermark` |

### Lean (new kernel modules)

| Rust entry point | Theorem | Mutant | Production caller |
|---|---|---|---|
| `request::admit(last_new_id, id, lookup) -> Admit` with `lookup` in `{Miss, SameDigest, OtherDigest}` and `Admit::{New, Replay, Conflict, Expired}` | `Request.new_iff_greater`: `New` iff `id > last_new_id`; `Request.replay_iff_same`; `Request.conflict_iff_other`; `Request.only_new_mutates`: every other result carries "no mutation"; total | `equal-id-is-new`, `conflict-replays`, `miss-reexecutes` | `cadence_driver::stream::admit` |
| `request::revision_ok(expected, present, current) -> bool` | `Request.stale_refused`: false iff `present` and `expected != current` | `stale-accepted` | same |
| `request::evict(len, capacity) -> bool` | `Request.cache_bounded`: after insertion with eviction, length <= capacity | `capacity-off-by-one` | `cadence_driver::cache` |
| `barrier::causal(through_seq, latest_seq, outstanding, pending_dispatch, unconsumed) -> Barrier` with `Barrier::{Done, Waiting, OutOfScope}` | `Barrier.no_early_return`: `Done` iff the three counts are zero and `through_seq <= latest_seq`; `Barrier.future_refused`: `through_seq > latest_seq` is `OutOfScope` | `done-with-unconsumed`, `future-accepted` | `cadence_driver::barrier` |
| `barrier::durable(causal, acked_seq, has_ack, watermark_seq) -> Barrier` | `Barrier.durable_covers_watermark`: `Done` implies `causal` is `Done` and `has_ack` and `acked_seq >= watermark_seq` | `ack-below-watermark` | same |

Tested, not proved: JSON parsing and schema validation, decimal-string conversion, the canonical request digest, frame invariants (`columns x rows` cells, wide-cell pairs, in-bounds cursor), virtual-clock stepping, trace equality.

## Design and boundaries

- **`cadence-driver` (`edge`).** Modules `envelope` (strict parse, limits, codes), `stream` (ordered admission and cache, calling `request::*`), `clock` (virtual clock capability), `fixture` (registered immutable scenarios, held completions for `deliver`), `barrier`, `capture`, `trace`, `replay`. It calls the same `Engine::reduce` and the same `cadence-tui` paint functions as live mode; `TestBackend` replaces only the terminal adapter (rule 5). `xtask boundary` allows `driver -> engine, tui, data, core, kernel`.
- **`cadence-engine` (`core`).** Exposes a state revision and an event sequence that rise on every admitted event, and gives every effect a causal group inherited by follow-ups. A checkpoint dispatched after an acknowledgement is consumed is the real parent-to-child case.
- **`cadence-data`.** Writes `provenance = synthetic` for headless sessions.
- **Trace format.** NDJSON: a header (build, seed, clock, services, fixture identities, capability configuration) and then every request and response; replay re-executes the requests and compares state revision, effects and cells, reporting the first divergence. A missing or modified line fails at that line.
- **Gate.** `cargo xtask gate headless` runs the journeys through the built binary; its state moves from `Pending("P4")`.

## Compatibility and reversibility

Additive. The protocol is versioned (`v: 1`); a change to it after release is a new RFC. No profile format change beyond the schema 5 column.

## Alternatives

- **Keep PTY runs as the only end-to-end evidence.** Not replayable, slow, and the handover's P6 journeys cannot be written.
- **Admission and barriers as tested driver code.** They are exactly the decisions VP-04 and VP-05 name; the kernel cost is small.
- **Edit `formal/CausalBarrier.tla`.** The file is manifest-pinned; living models go in `models/` as decided in [Formal gates](2026-11-26-formal-gates-and-production-mapping.md).

## Evidence and rollout

#67 request model and kernel; #68 envelope and cache; #69 driver loop and `doctor`; #70 capture; #71 barrier model, kernel and `deliver`; #72 trace and replay; #73 journeys and the gate. Finish line: `fixtures/headless-session.jsonl` runs against the built binary; journeys 2, 3, 4, 5, 7, 8, 10 and 11 of H-05 pass in `./ci gate headless`; both models are `mapped` with runtime tests.

## Unresolved questions

None blocking. Owner may amend: `--ephemeral-profile` location (default: a fresh directory under `$XDG_RUNTIME_DIR` or the system temporary directory, printed in `hello`).

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
