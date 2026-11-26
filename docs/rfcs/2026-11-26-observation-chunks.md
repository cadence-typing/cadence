---
rfc: "2026-11-26-observation-chunks"
title: "Append-only observation chunks and a rebuildable learner state"
status: accepted
implementation: not-started
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/11
milestone: "0.1.0-rc.1"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Append-only observation chunks and a rebuildable learner state

## Summary and motivation

L-01 separates three things: an immutable observation ledger, a learner model derived from it, and a planner policy. D-02 lists "immutable normalized observation chunks" and "learner derived state with source watermark" among the tables. The shipped store has neither. `cadence_data::learning_rows::save` deletes and re-inserts every learning row of the session (`skill_observations`, `word_outcomes`, `prose_seen`, plan and focus) inside each checkpoint transaction. Consequences:

- recorded evidence is mutable by construction: any checkpoint can rewrite what an earlier one stored;
- "evidence counted once" depends on the engine always sending the complete cumulative snapshot (the `FeedbackLoop` model's `EvidenceCountedOnce` and its `append-checkpoints` mutant describe exactly this risk);
- a checkpoint's cost grows with the session, inside a `synchronous=FULL` transaction every 2 s, which is the root of the slow acknowledgements behind #51;
- the learner model cannot be recomputed under a new policy from anything finer than per-session totals.

Why an RFC: a persisted format and the checkpoint protocol change.

Outside this decision: storing raw keystrokes or typed text (D-02: not in the normal database), a general job registry (covered by job tickets in the [configuration RFC](2026-11-26-configuration-layout-and-settings.md)).

## Observable behaviour

Nothing changes on screen, in scores or in the plan for the same history. Observable differences: `cadence export` lists chunks per session; `cadence doctor --json` reports the chunk and derived watermarks and whether they agree; `cadence doctor --rebuild-derived` recomputes the derived tables from chunks and reports any difference instead of hiding it; an attempt to change stored evidence fails at the database.

## Proof obligations

### TLA+ first

`models/CheckpointSave.tla` gains the chunk watermark: a checkpoint carries chunks `(stored + 1 .. n)`; the worker appends them and the totals in one transaction. Invariants: `ChunksAppendOnly` (a stored chunk never changes), `EvidenceCountedOnce` (derived totals equal the fold of stored chunks, whatever was retried or replayed), `NoGap`. Mutant configurations: `rewrite-chunk`, `append-on-replay`, `skip-chunk`.

### Lean

| Rust entry point (kernel) | Theorem | Mutant | Production caller |
|---|---|---|---|
| `chunks::admit(stored_last, has_stored, incoming_seq, same_digest) -> Admit` with `Admit::{Append, Replay, Conflict, Gap}` | `Chunks.append_iff_next`: `Append` iff `incoming_seq` is the successor of the stored last (or the first); `Chunks.replay_iff_same`: an already stored sequence with the same digest is `Replay` and changes nothing; `Chunks.conflict_on_changed_payload` (VP-03); `Chunks.gap_refused`; total | `append-any-seq`, `replay-on-changed`, `gap-accepted` | `cadence_data::store::ProgressStore::apply_checkpoint` |
| `chunks::watermark_ok(derived, stored) -> bool` | `Chunks.derived_never_ahead`: true iff `derived <= stored` | `derived-ahead-ok` | store open and `doctor` |

Tested, not proved: the canonical chunk encoding and digest, SQL, triggers, the fold that derives tallies, migration.

## Design and boundaries

- **Chunk** = the evidence of one closed exercise: session, chunk sequence (dense from 1), track, exercise ordinal, per-skill tallies, word outcomes, passage, timing-quality flags; canonical binary encoding with a digest. Built by `cadence-engine` at `Learning::close`, bounded by the exercise.
- **Schema 7 (`cadence-data`)**: `observation_chunks(session_id, chunk_seq, track, payload, digest, PRIMARY KEY(session_id, chunk_seq))` with triggers that abort `UPDATE` and `DELETE`; `derived_state(session_id, source_chunk_seq)`. The existing learning tables become derived: each checkpoint appends new chunks and updates derived rows from them in the same transaction (upserts, no delete-and-reinsert).
- **Checkpoint** carries only chunks after the last acknowledged sequence plus the cumulative progress counters; a retry resends the same chunks and is admitted as `Replay`.
- **Migration 6 to 7**: adds the tables. Sessions recorded earlier have no chunks and are marked `legacy`: their derived rows remain the authority and are never synthesized into chunks (no invented observations).
- **Learner model** reads derived rows as today; `rebuild-derived` is the proof that they are a function of the chunks.

## Compatibility and reversibility

Existing profiles migrate without row changes; receipts keep their digests. Older binaries refuse schema 7. Reverting to rewrite-per-checkpoint would be a new schema; chunks would stay as inert data.

## Alternatives

- **Leave rows mutable and rely on tests.** Keeps L-01's central property unenforced.
- **Chunk per key.** D-02 forbids a transaction per key and it stores more than the model needs.
- **Synthesize chunks for old sessions.** Invents a granularity that was never recorded.

## Evidence and rollout

One issue (#11) in this order: model and mutants; kernel admission with claims; schema 7 and the store; engine chunk construction; `doctor`. Tests: lost acknowledgement then retry stores each chunk once; a changed payload under a stored sequence is refused and nothing is written; killing the process between any two statements of a checkpoint leaves chunks and derived rows consistent on reopen (with the crash campaign of #88); rebuild equals stored for a recorded multi-phase session; checkpoint time no longer grows with session length (measured in #15).

## Unresolved questions

None blocking. Owner may amend: whether `legacy` sessions should ever be re-encoded.

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
