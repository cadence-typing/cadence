---
rfc: "2026-11-26-protocol-proofs-as-kernel-machines"
title: "Unbounded protocol proofs as kernel machines in Lean"
status: accepted
implementation: complete
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-27
issue: https://github.com/cadence-typing/cadence/issues/12
milestone: "0.1.0-rc.1"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Unbounded protocol proofs as kernel machines in Lean

Decides the question #12 kept open after [Formal gates with production mappings](2026-11-26-formal-gates-and-production-mapping.md).

## Summary and motivation

The handover (V-01, V-03, P7) asks for general, unbounded safety proofs of the protocols with TLAPS, next to bounded TLC runs. Since then the project replaced Verus with Lean over the Aeneas translation of `crates/kernel`, and tauten ships `Tauten.Machine.inv_of_step`: an invariant kept by every step of a `Machine` holds in every reachable state. TLC models now run as a gate, but every protocol badge is `model-proved` (bounded) at most, and the gates `prove`, `verify` and `correspondence` in `xtask` are still `Pending`.

Decision: the unbounded proofs are Lean theorems over the shipped step functions, not TLAPS proofs over the TLA+ text. TLAPS is not adopted. TLA+ with TLC stays for interleavings between components (worker thread, SQLite, driver), where no single Rust function is the protocol.

Why this is better here: a TLAPS proof is about the model, and its link to the Rust is by review. A Lean invariant over the extracted `step` is about the code the binary runs. It also avoids a fourth proof toolchain.

Outside this decision: liveness beyond VL-01's bounded-rank statement; proofs about SQLite, the OS or the thread scheduler.

## Observable behaviour

No product behaviour changes. `docs/TAUTEN.md` gains an assurance table with one row per handover claim (VP-01 to VP-05, VK-01 to VK-06, VL-01, VR-01) listing the Lean claims, the TLC models, the runtime tests and the badge. `cargo xtask gate model|prove|verify|correspondence` run and can pass or fail, instead of reporting `Pending`.

## Proof obligations

| Claim | Kernel machine (step function) | Invariant proved for every reachable state | Mutant |
|---|---|---|---|
| VP-03 save protocol | `save::step(state, event) -> (state, command)`: new, moved from `engine::storage` (pending, failed, last acknowledged, sequence, finishing) | `Save.single_flight`: at most one checkpoint outstanding; `Save.ack_monotone`: the acknowledged sequence never decreases and never exceeds the dispatched one; `Save.failed_tail_named`; `Save.clean_exit_needs_final_ack`: the `Quit(success)` command is issued only after the final checkpoint's acknowledgement | `dispatch-while-pending`, `ack-rewinds`, `quit-before-ack` |
| Session flow | `session::step` (exists) | `Session.reachable_clock`: in every reachable state the clock runs only in `Running`; `Session.completed_is_stable`: once completed, no reachable step makes the outcome unfinished | `resume-from-review` |
| VP-05 request stream | `request::admit` plus a stream state step (from the headless RFC) | `Request.applied_once`: over any request sequence each id is applied at most once | `replay-reapplies` |
| VP-04 barriers | barrier scope state step | `Barrier.done_is_drained` for every reachable scope state, including child registration | `child-after-drain` |
| VP-01, VP-02 worker results | `ticket::admit` plus slot state step (from the configuration RFC) | `Ticket.accepted_fresh`, `Ticket.completion_retained` (a terminal completion is consumed or explicitly retired, never dropped) | `drop-full-slot` |
| VL-01 review fairness | `queue` FIFO service step over the review queue | `Queue.rank_decreases`: serving the head lowers every other item's rank by one and tail admission never raises it; `Queue.served_within_rank_plus_one` | `admit-at-head` |

Each machine implements `tauten::machine::Machine`; its invariant theorem uses `inv_of_step` and is registered in `proofs/claims.toml` with a production caller. The liveness half of VL-01 remains the temporal property `DueEventuallyServed` in `models/FeedbackLoop.tla` under stated weak fairness; its badge is `model-proved`, and the table says so.

Tested, not proved: that the engine, driver and worker drive these machines and carry out their commands (replay of recorded tapes with `tauten::machine::replay`, plus the runtime tests named in `models/claims.toml`).

## Design and boundaries

- `engine::storage` becomes a driver over `kernel::save::step`: it holds the `Checkpoint` payloads and timestamps; the machine holds sequence numbers and flags only (integers, no payloads).
- **Assurance manifest**: `assurance/claims.toml`, one entry per handover claim id with `lean`, `models`, `runtime_tests` and `status`. `cargo xtask assurance` checks that every id of the frozen `formal/claims.json` has an entry, that every named theorem is `proved` in `proofs/claims.json`, every model is `mapped` in `models/claims.json` and every test exists; it writes the table in `docs/TAUTEN.md`. A claim id with nothing behind it fails the gate.
- **Gates**: `model` = the TLC gate; `verify` = `cargo tauten proofs` recorded for the commit (kernel claims; replaces "same-source Verus"); `prove` = the machine invariants above are present and `proved`; `correspondence` = `xtask assurance` plus the named runtime tests.
- A decision record in `docs/decisions/` states the departure from V-01 (TLAPS and Verus) and links this RFC.

## Compatibility and reversibility

No persisted or wire format changes. Moving the save state machine is a refactor guarded by the existing save tests and the `CheckpointSave` runtime tests.

## Alternatives

- **TLAPS as specified.** Proves the model, needs a new pinned toolchain and proof-maintenance skill, and still leaves the model-to-code link to review.
- **Stay bounded.** Honest but weaker than the handover's exit for P7.

## Evidence and rollout

#89 save machine; #90 fairness lemma; #91 assurance manifest and the four gates. The machines for requests, barriers and tickets get their invariant theorems in #91 once their kernels exist (beta.2, beta.3). Finish line: `cargo xtask assurance` passes with no claim id unbound, and `docs/TAUTEN.md` shows `proved` for every safety row and `model-proved` for the liveness row.

## Unresolved questions

None blocking. Owner may amend: dropping TLAPS (this RFC's central choice).

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
- 2026-11-27: implemented (#89, #90, #91). Every invariant in the table is proved for every reachable state with `Tauten.Machine.rinv_of_step` over the extracted step (`Proofs/Save.lean`, `Proofs/Session.lean`, `Proofs/Machines.lean`, `Proofs/DueQueue.lean`) and recorded in `proofs/claims.json`; the assurance manifest binds them to the handover ids. Departures, each named: the save step is a new kernel function and `engine::storage` its driver, as planned. The request, ticket and barrier machines are Lean models that apply the extracted decision as their drivers do, against a world that may deliver any event: the barrier scope's counts (children included) are the world's, so child registration is the driver's tested counting, not a machine step. Mutants: `replay-reapplies` as planned; `done-with-outstanding` for the barrier (the RFC's `child-after-drain` is a property of the driver's counting, which the machine takes as given); `accept-any-request` for worker results (`drop-full-slot` remains the TLC mutant of `WorkerResults.tla`); `review-resumes`, already registered, is the RFC's `resume-from-review`, and `completed-reopens` kills `Session.completed_is_stable` directly; `admit-at-head` (#90). The Rust `tauten::machine::Machine` trait is not implemented: every `tauten` release needs rustc 1.98 and the workspace builds with 1.97.1, so the recorded save tape (`crates/engine/tests/golden/save-tape.txt`) replays through a local `replay` with the same semantics.
