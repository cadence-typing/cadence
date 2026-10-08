# Architecture, asynchronous updates and protocols

## A-01 Workspace boundaries

| Crate | Owns | Must not own |
|---|---|---|
| `cadence-core` | Units, IDs, session state machine, first-attempt ledger, exact scoring, verified kernels | Ratatui, terminal, SQL, OS time, runtime |
| `cadence-trainer` | Learner summaries, quota planner, candidate ranking, review/pacing policy | Terminal, persistence implementation, file reads |
| `cadence-engine` | Sole app-state reducer, UI navigation state, effect requests, prepared immutable view model | Ratatui, worker execution, blocking I/O |
| `cadence-data` | SQLite writer/reader adapter, configuration, corpus loading/import/index adapters, backup/export | UI mutation or training policy decisions |
| `cadence-tui` | Pure layout/paint from view model; shared Ratatui draw and cell capture | Mutating learning/application state, filesystem, jobs |
| `cadence-runtime` | Bounded event transport, worker pool, effect execution, clocks, crossterm ingress, terminal lifecycle | Independent scoring/planner rules |
| `cadence-driver` | Versioned NDJSON commands, deterministic fixtures/clock, trace/replay, headless harness | Alternative engine or renderer |
| `cadence` | CLI composition and user-facing errors | Reusable domain implementation |
| `xtask` | Build/test/proof commands, candidate receipts, packaging and adapter tooling | Runtime application dependency |

Dependencies: core has no workspace dependency; trainer → core; engine → core/trainer; data → core/trainer and engine port DTOs if necessary; tui → engine/core; runtime → engine/data/core/trainer; driver → engine/runtime/tui/core; binary → runtime/driver/tui/data. No reverse edge from engine into runtime/tui/driver. Prefer port types in core/engine over a shared “utils” dumping ground. These are ownership boundaries; split modules inside them before inventing additional micro-crates.

The engine's public mutation API is conceptually `reduce(&mut self, envelope: EventEnvelope) -> Transition`. Transition contains bounded owned effects, dirty/presentation information and domain outcomes. Internal subsystem mutation remains private to the owner. Snapshot/view preparation is an explicit engine operation; Ratatui paint receives `&PreparedView` and immutable theme data. Paint cannot correct scroll state, initiate I/O, alter counters or advance time. Layout-dependent state changes are explicit `ViewportChanged`/preparation events before paint.

## A-02 UI bus and ownership

Terminal adapter → typed input envelope → sequenced event admission → engine reducer → prepared view → renderer. Async effects → bounded workers → immutable completion envelope → same event admission/reducer.

There is one owner thread for app state. Widgets return typed `UiIntent`s (such as `StartDailyPlan`, `Pause`, `SetTheme`) through command dispatch. Children never mutate parents/siblings. `Arc<Mutex<AppState>>` is forbidden. Share immutable corpus/model snapshots with workers; only the owner accepts/replaces them. A background database writer can be single-threaded with its own state; it never mutates the app state directly.

Keep input reduction synchronous and bounded. Preparation may copy cheap IDs/visible spans but not serialize the whole profile. Large trend calculation, corpus indexing, database work and plan candidate retrieval use workers. Rendering may skip obsolete frames, but logical input transitions and effects remain complete.

Async does not require putting a general Tokio executor around every object. A small bounded worker pool plus channels is sufficient for this offline workload. Use established channel primitives. If Tokio is selected, blocking SQL and file work stays on bounded blocking workers; the engine still has no `async fn` API.

## A-03 Typed identity and freshness

At minimum: `ProfileId`, `SessionId`, `ExerciseId`, `WordId`, `JobId`, `ViewId { slot, generation }`, `SessionEpoch`, `RequestRevision`, `LearnerRevision`, `CorpusVersion`, `PolicyVersion`, `LayoutRevision`, `EventSeq`, `CommitSeq`, `FrameRevision`, `MonoTick`, `UtcDay`, `TextUnitIndex`, `CellColumn`, `TerminalRows`, `BasisPoints`.

Use existing generational slot storage for ephemeral views/jobs, with typed wrappers. Persistent IDs are stable IDs; generations are not database row identities. Generation exhaustion retires a slot; no wraparound/reuse. An absent/deleted owner invalidates all of its pending view work. Cancelling a job is best effort; a late success or late failure still goes through admission.

An async result envelope records `job_id`, owner key, request kind/revision, and the dependencies appropriate to that result. The acceptance predicate is an explicit pure function for that result domain. Examples:

- Next-exercise candidate: session ID + session epoch + next-exercise request revision + requested learner revision + corpus/policy/layout versions.
- Progress query: profile ID + query revision + database snapshot watermark + filters; a view generation only governs whether it can paint that view.
- Save acknowledgement: profile/session + commit sequence + payload digest. It is independent of whether the original practice screen still exists.
- Theme preparation: theme request revision + view/display configuration revision, not the learner revision.

Do not compare every result against a global revision incremented by every keystroke: that can make useful background work permanently stale. Use immutable admitted planning bases. If the relevant learner basis changes before delivery, either supersede that request once or accept the snapshot under an explicitly frozen exercise plan; document the choice. V1 freezes a next-exercise request's learner basis until consumed, unless the session/policy/layout/corpus changes. In-session observation updates do not invalidate that already requested exercise; the following request uses the newer model.

Accept only a result with the matching pending `JobId` and complete key. Consuming removes that pending request, so duplicate completions cannot apply twice. Record rejected deliveries in a full debug trace for replay. An old failure must not replace the current success with an error banner.

## A-04 Bounded transport without lost completion facts

V1 suggested named limits: input FIFO 1,024 events / 256 KiB; at most 8 outstanding worker jobs; at most 4 MiB per completion payload; 32 MiB total retained completion payload; 64 input events per owner turn; 4 ms owner-turn budget. These are starting resource policies, not naked literals or proven optimal settings. A corpus-load result holds an immutable bounded handle/summary rather than sending 50,000 copied rows through the bus.

Worker lifecycle uses reserved result slots in a bounded `JobRegistry`. Reserve a slot before launching. Each slot holds one terminal outcome until the owner consumes it. Worker publication signals a coalescible wake hint; the outcome remains in the slot even if wakes merge. A wake is not the completion record. Cancellation requests do not release the slot before terminal acknowledgement/join. The result slot cannot be overwritten by another job or recycled under the old generation.

Input is ordered and semantic: never coalesce keys, paste, pause, stage-boundary actions, errors or save acknowledgements. Terminal producer backpressure is permitted off the owner thread, but backlog/latency thresholds must invalidate affected timing samples and visibly pause training if fidelity can no longer be guaranteed. Do not keep reporting reliable IKI while a producer is blocked behind a saturated queue. Bound input payloads before admission.

The owner never blocks sending effects to a full work queue. It keeps a bounded pending intent with an explicit `Busy/Preparing` presentation, or refuses a new user operation before claiming it started. Critical save intents have reserved/coalescible cumulative checkpoint storage so a planner task cannot starve persistence. Full checkpoints may supersede earlier unlaunched checkpoints only because they contain the full cumulative session state and monotonic sequence; transaction idempotency rules still apply. Never coalesce already-dispatched writes or their receipts.

Each turn services input plus ready results with a documented bounded round-robin policy, then prepares/renders when dirty or when a visible timer boundary requires it. Continuous input cannot starve save acks and completions; continuous results cannot starve input. Idle waits block on channel readiness. Timer/redraw hints may coalesce; explicit elapsed time is sampled and recorded at reduction, so a dropped tick cannot change the phase budget.

## A-05 Session lifecycle

States: `Ready`, `Running`, `Paused`, `PhaseComplete`, `Review`, `Finishing`, `Saved`, `SaveFailed`, `Aborted`.

Start freezes the plan and policy identity. First content input starts that phase's active clock. Pause records a clock boundary before changing state. Resume uses a fresh contiguous timing segment. Phase timeout consumes only events whose admitted timestamp is within the phase boundary; ties use the documented order: keys at the exact deadline are outside the scored slice. The input adapter must not let an earlier timestamp arrive after a later admitted one; reject backward time as measurement-invalid.

Finishing requests a final durable checkpoint and continues rendering a “saving” state. Only a matching final commit receipt enables `Saved`. If the user exits while a save is pending, try a bounded flush; on failure retain/recover the last acknowledged state and return an explicit non-success outcome. Never label an unacknowledged write saved. Data errors remain visible and retryable; do not discard the session just to exit cleanly.

## A-06 Persistence protocol

The engine issues cumulative session checkpoints with monotonic `CommitSeq` and content digest. A single database writer serializes commits. In one SQL transaction: validate session/profile identity and sequence, insert immutable new event chunks, update the cumulative session record and derived learner watermark, and store the commit receipt. Any duplicate `(SessionId, CommitSeq, digest)` returns the prior receipt. Same sequence with another digest is corruption/conflict. A stale sequence cannot regress current state; a later cumulative checkpoint may cover an earlier one only after validating its prefix digest/event continuity.

Commit acknowledgement is emitted after transaction success. A lost acknowledgement followed by a retry must not double-count errors, successes or review scheduling. On reopen, load the database receipt/watermark and expose recovered state without applying already committed deltas again. Updates are cumulative or explicit event-derived transitions, never blindly additive retry payloads.

No cross-process shared mutable session. Acquire an advisory writer lock before opening the writable profile; a second interactive writer fails clearly or opens an explicitly read-only progress view. A live lockfile's presence is not proof of ownership; use OS locks, not PID-file polling. Read-only queries use consistent snapshots.

## A-07 Trace and replay

A versioned trace header records build/scoring/policy/corpus/layout identities, start state or its fixture identity, clock origin, seed and capability evidence. Record external events before acceptance, including stale/failed responses, alongside reducer-consumption sequence and deterministic effect requests. Checkpoint observations carry state hashes; optional frame nodes carry canonical cell grids/cursor/style hashes. Time/randomness are inputs, not sampled inside pure functions.

Replay uses the same reducer and renderer with real effect execution disabled. It verifies effect requests and consumes recorded completions. Unexpected side effect, event gap, incompatible version, inconsistent digest, truncated trace or observation divergence is an error. Report the first diverging event and compact state/cell diff. Read-only replay must never write to the real profile or rerun imports.

Full replay traces containing typed/imported text are explicit opt-in debug exports. Default logs are bounded and omit raw text. A redacted trace is labelled non-replayable when required content is absent; never claim forensic replay from hashes alone. A synthetic fixture trace can always include its synthetic text.
