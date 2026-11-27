---------------------------- MODULE CheckpointSave ----------------------------
(***************************************************************************)
(* The shipped checkpoint save protocol (#30, RFC                          *)
(* 2026-11-26-formal-gates-and-production-mapping).                        *)
(*                                                                         *)
(* The engine (crates/engine/src/storage.rs) builds cumulative checkpoints *)
(* with increasing sequences and keeps at most one outstanding. The writer *)
(* thread (cadence_runtime::writer::SaveWorker) applies each one in one    *)
(* SQLite transaction (cadence_data::store::ProgressStore::apply_checkpoint)*)
(* whose credit decision is the proved cadence_kernel::checkpoint::admit,  *)
(* and replies. A failed save pauses practice until `r` retries with a new *)
(* sequence. Finishing (Ctrl-C, `e`, end of plan) dispatches a final-status*)
(* checkpoint; a clean exit waits for its acknowledgement, a second exit   *)
(* request after a failure or the 10 s final-flush watchdog exits          *)
(* unsuccessfully.                                                         *)
(*                                                                         *)
(* While practice runs, an acknowledgement overdue past the 2 s bound       *)
(* stalls practice (#51): the clock stops and nothing is scored until the  *)
(* stalled checkpoint is answered; success resumes, failure takes the      *)
(* save-failed path.                                                       *)
(*                                                                         *)
(* Abstracted away: the checkpoint contents (a sequence stands for its     *)
(* cumulative snapshot), the 2 s cadence (any time a checkpoint may become *)
(* due or overdue) and process crash (the frozen                           *)
(* formal/DurableCheckpoint.tla covers reopen).                            *)
(*                                                                         *)
(* Fault switches, each TRUE in the positive configuration and FALSE in    *)
(* exactly one mutant configuration:                                       *)
(*   SingleFlightGuard: dispatch waits while a checkpoint is outstanding.  *)
(*   AckAfterCommit:    the worker replies success only after commit.      *)
(*   Idempotent:        a replayed sequence is not credited again (admit). *)
(*   QuitWaitsForAck:   a clean exit needs the final acknowledgement.      *)
(*   OverdueStopsScore: no content is scored while a save is overdue.      *)
(*   StallKeepsPending: a stall keeps waiting for the stalled checkpoint.  *)
(*                                                                         *)
(* Observation chunks (#11): closing an exercise makes one chunk; a        *)
(* checkpoint carries the chunks after the last acknowledged chunk         *)
(* watermark, and the worker applies them in the same transaction through  *)
(* the proved cadence_kernel::chunks::admit: the next one is appended, a   *)
(* stored one with the same payload is a replay, a changed payload or a    *)
(* gap refuses the whole transaction. A reply can be lost after commit, so *)
(* a retry resends stored chunks. Fault switches:                          *)
(*   ReplayIsNoop: a replayed chunk is not folded into the totals again.   *)
(*   AppendOnly:   a stored chunk with a changed payload is refused.       *)
(*   RefuseGap:    a chunk after a missing one is refused.                 *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS MaxSeq, SingleFlightGuard, AckAfterCommit, Idempotent, QuitWaitsForAck,
          OverdueStopsScore, StallKeepsPending,
          MaxChunks, ReplayIsNoop, AppendOnly, RefuseGap

Seqs == 1..MaxSeq
None == 0
Chunks == 1..MaxChunks
\* A stored chunk's payload: 0 absent, 1 as first written, 2 rewritten.
Absent == 0
Written == 1
Rewritten == 2

VARIABLES
    \* engine (Storage and the Engine fields it reads)
    seq,          \* last allocated sequence (Storage.sequence)
    pending,      \* outstanding sequence or None (Storage.pending)
    failed,       \* a save failed and awaits retry (Storage.failed)
    lastAck,      \* last acknowledged sequence or None (Storage.last_ack)
    due,          \* Engine.checkpoint_due
    finishing,    \* Engine.finishing
    finals,       \* sequences dispatched while finishing (final status)
    exited,       \* "no", "ok" or "failed" (Transition.quit / exit_failed)
    answered,     \* sequences whose reply the engine has consumed
    \* writer thread and store
    queue,        \* submitted, not yet taken by the worker
    replies,      \* set of [s, ok] replies not yet consumed
    durable,      \* latest committed sequence (sessions row) or None
    credits,      \* times each sequence's snapshot was credited
    early,        \* sequences acknowledged before commit (AckAfterCommit = FALSE)
    overdue,      \* practice is stalled on an overdue acknowledgement (#51)
    scoredOverdue, \* history: content was scored while overdue
    \* observation chunks (#11)
    closed,       \* chunks the engine has closed (Learning::close)
    ackChunk,     \* the chunk watermark of the last acknowledged checkpoint
    carryFrom,    \* per sequence: the first chunk it carries
    carryTo,      \* per sequence: the last chunk it carries
    stored,       \* observation_chunks: the stored watermark
    payload,      \* per chunk: Absent, Written or Rewritten
    derived       \* the derived totals: chunks folded in (derived_state)

chunkVars == <<closed, ackChunk, carryFrom, carryTo, stored, payload, derived>>

vars == <<seq, pending, failed, lastAck, due, finishing, finals, exited,
          answered, queue, replies, durable, credits, early, overdue, scoredOverdue,
          closed, ackChunk, carryFrom, carryTo, stored, payload, derived>>

TypeOK ==
    /\ seq \in 0..MaxSeq
    /\ pending \in {None} \cup Seqs
    /\ failed \in BOOLEAN
    /\ lastAck \in {None} \cup Seqs
    /\ due \in BOOLEAN
    /\ finishing \in BOOLEAN
    /\ finals \subseteq Seqs
    /\ exited \in {"no", "ok", "failed"}
    /\ answered \subseteq Seqs
    /\ queue \in Seq(Seqs)
    /\ replies \subseteq [s : Seqs, ok : BOOLEAN]
    /\ durable \in {None} \cup Seqs
    /\ credits \in [Seqs -> 0..2]
    /\ early \subseteq Seqs
    /\ overdue \in BOOLEAN /\ scoredOverdue \in BOOLEAN
    /\ closed \in 0..MaxChunks /\ ackChunk \in 0..MaxChunks
    /\ carryFrom \in [Seqs -> 1..(MaxChunks + 1)] /\ carryTo \in [Seqs -> 0..MaxChunks]
    /\ stored \in 0..MaxChunks
    /\ payload \in [Chunks -> {Absent, Written, Rewritten}]
    /\ derived \in 0..(2 * MaxChunks * MaxSeq)

Init ==
    /\ seq = 0
    /\ pending = None
    /\ failed = FALSE
    /\ lastAck = None
    /\ due = FALSE
    /\ finishing = FALSE
    /\ finals = {}
    /\ exited = "no"
    /\ answered = {}
    /\ queue = << >>
    /\ replies = {}
    /\ durable = None
    /\ credits = [s \in Seqs |-> 0]
    /\ early = {}
    /\ overdue = FALSE /\ scoredOverdue = FALSE
    /\ closed = 0 /\ ackChunk = 0
    /\ carryFrom = [s \in Seqs |-> 1] /\ carryTo = [s \in Seqs |-> 0]
    /\ stored = 0
    /\ payload = [i \in Chunks |-> Absent]
    /\ derived = 0

Running == exited = "no"

\* Engine::watchdog (2 s cadence while running), exercise completion and
\* the end of a phase set checkpoint_due.
BecomeDue ==
    /\ Running /\ ~finishing /\ ~due
    /\ due' = TRUE
    /\ UNCHANGED chunkVars
    /\ UNCHANGED <<seq, pending, failed, lastAck, finishing, finals, exited,
                   answered, queue, replies, durable, credits, early, overdue, scoredOverdue>>

\* Engine::dispatch_checkpoint: single-flight, never while a failure waits.
Dispatch ==
    /\ Running /\ due /\ ~failed /\ seq < MaxSeq
    /\ SingleFlightGuard => pending = None
    /\ seq' = seq + 1
    /\ pending' = seq + 1
    /\ due' = FALSE
    /\ finals' = IF finishing THEN finals \cup {seq + 1} ELSE finals
    /\ queue' = Append(queue, seq + 1)
    \* the chunks after the acknowledged watermark (Storage carries them)
    /\ carryFrom' = [carryFrom EXCEPT ![seq + 1] = ackChunk + 1]
    /\ carryTo' = [carryTo EXCEPT ![seq + 1] = closed]
    /\ UNCHANGED <<closed, ackChunk, stored, payload, derived>>
    /\ UNCHANGED <<failed, lastAck, finishing, exited, answered, replies,
                   durable, credits, early, overdue, scoredOverdue>>

\* chunks::admit over one chunk: the next is appended, a stored one is a
\* replay, a later one is a gap. Applies from..to in order; a refused gap
\* refuses the whole transaction (ok = FALSE, nothing changes).
RECURSIVE ApplyChunks(_, _, _)
ApplyChunks(i, to, st) ==
    IF i > to THEN st
    ELSE IF ~st.ok THEN st
    ELSE IF i <= st.stored
         THEN \* a replay: the same payload changes nothing
              ApplyChunks(i + 1, to,
                IF ReplayIsNoop THEN st ELSE [st EXCEPT !.derived = @ + 1])
    ELSE IF i = st.stored + 1
         THEN ApplyChunks(i + 1, to,
                [st EXCEPT !.stored = i, !.payload[i] = Written, !.derived = @ + 1])
    ELSE IF RefuseGap THEN [st EXCEPT !.ok = FALSE]
    ELSE ApplyChunks(i + 1, to,
           [st EXCEPT !.stored = i, !.payload[i] = Written, !.derived = @ + 1])

ChunkState == [ok |-> TRUE, stored |-> stored, payload |-> payload, derived |-> derived]

\* The chunk part of a checkpoint's transaction: TRUE when it may commit.
ChunksCommit(s) ==
    LET st == ApplyChunks(carryFrom[s], carryTo[s], ChunkState) IN
    /\ st.ok
    /\ stored' = st.stored /\ payload' = st.payload /\ derived' = st.derived

\* ProgressStore::apply_checkpoint with checkpoint::admit: a newer sequence
\* is applied and credited once; an equal one is a read-only replay.
Commit(s) ==
    /\ durable' = IF durable = None \/ s > durable THEN s ELSE durable
    /\ credits' = IF durable = None \/ s > durable \/ ~Idempotent
                  THEN [credits EXCEPT ![s] = @ + 1] ELSE credits

\* SaveWorker's writer loop: take a request, commit and acknowledge it.
WorkerCommit ==
    /\ queue # << >>
    /\ LET s == Head(queue) IN
        /\ queue' = Tail(queue)
        /\ Commit(s)
        /\ ChunksCommit(s)
        /\ replies' = IF s \in early THEN replies ELSE replies \cup {[s |-> s, ok |-> TRUE]}
        /\ early' = early \ {s}
    /\ UNCHANGED <<closed, ackChunk, carryFrom, carryTo>>
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered, overdue, scoredOverdue>>

\* The transaction commits but its reply reports failure (an I/O error
\* after commit): the engine retries and resends the chunks, now stored.
WorkerCommitLostReply ==
    /\ queue # << >>
    /\ Head(queue) \notin early
    /\ LET s == Head(queue) IN
        /\ queue' = Tail(queue)
        /\ Commit(s)
        /\ ChunksCommit(s)
        /\ replies' = replies \cup {[s |-> s, ok |-> FALSE]}
    /\ UNCHANGED <<closed, ackChunk, carryFrom, carryTo>>
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered, early, overdue, scoredOverdue>>

\* A request whose chunks were tampered with or corrupted reaches the
\* store: a stored chunk with a changed payload, or a chunk after a gap.
\* The store must refuse it whole (AppendOnly, RefuseGap).
Forged ==
    \/ /\ stored > 0 /\ ~AppendOnly
       /\ \E i \in 1..stored : payload' = [payload EXCEPT ![i] = Rewritten]
       /\ UNCHANGED <<stored, derived>>
    \/ /\ stored + 2 <= MaxChunks /\ ~RefuseGap
       /\ stored' = stored + 2
       /\ payload' = [payload EXCEPT ![stored + 2] = Written]
       /\ derived' = derived + 1
    /\ UNCHANGED <<closed, ackChunk, carryFrom, carryTo>>
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals, exited,
                   answered, queue, replies, durable, credits, early, overdue, scoredOverdue>>

\* Learning::close: an exercise closes and its chunk is made; a checkpoint
\* becomes due.
Close ==
    /\ Running /\ ~finishing /\ closed < MaxChunks
    /\ closed' = closed + 1
    /\ due' = TRUE
    /\ UNCHANGED <<ackChunk, carryFrom, carryTo, stored, payload, derived>>
    /\ UNCHANGED <<seq, pending, failed, lastAck, finishing, finals, exited,
                   answered, queue, replies, durable, credits, early, overdue, scoredOverdue>>

\* The transaction fails (disk full, I/O error): nothing is committed.
WorkerFail ==
    /\ queue # << >>
    /\ Head(queue) \notin early
    /\ LET s == Head(queue) IN
        /\ queue' = Tail(queue)
        /\ replies' = replies \cup {[s |-> s, ok |-> FALSE]}
    /\ UNCHANGED chunkVars
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered, durable, credits, early, overdue, scoredOverdue>>

\* A committed request processed again (the same checkpoint retried after a
\* lost reply): admit must make it a replay.
WorkerReplay ==
    /\ durable # None
    /\ \E s \in 1..durable :
        /\ credits[s] > 0
        /\ credits[s] < 2
        /\ Commit(s)
        /\ ChunksCommit(s)
    /\ UNCHANGED <<closed, ackChunk, carryFrom, carryTo>>
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered, queue, replies, early, overdue, scoredOverdue>>

\* Mutant only: reply success before the transaction commits.
WorkerAckEarly ==
    /\ ~AckAfterCommit
    /\ queue # << >>
    /\ Head(queue) \notin early
    /\ early' = early \cup {Head(queue)}
    /\ replies' = replies \cup {[s |-> Head(queue), ok |-> TRUE]}
    /\ UNCHANGED chunkVars
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered, queue, durable, credits, overdue, scoredOverdue>>

\* Engine::save_result: only the outstanding key's reply counts; a success
\* moves the watermark and, when finishing with nothing due and the
\* checkpoint carries the final status, quits successfully.
Receive ==
    /\ \E r \in replies :
        /\ replies' = replies \ {r}
        /\ answered' = answered \cup {r.s}
        /\ IF Running /\ pending = r.s
           THEN /\ pending' = None
                \* the answer ends a stall: success resumes, failure pauses
                \* on the save-failed path
                /\ overdue' = FALSE
                /\ IF r.ok
                   THEN /\ lastAck' = r.s
                        /\ ackChunk' = IF carryTo[r.s] > ackChunk THEN carryTo[r.s] ELSE ackChunk
                        /\ failed' = FALSE
                        /\ exited' = IF finishing /\ ~due /\ r.s \in finals
                                     THEN "ok" ELSE exited
                   ELSE /\ failed' = TRUE
                        /\ UNCHANGED <<lastAck, exited, ackChunk>>
           ELSE UNCHANGED <<pending, lastAck, failed, exited, overdue, ackChunk>>
    /\ UNCHANGED <<closed, carryFrom, carryTo, stored, payload, derived>>
    /\ UNCHANGED <<seq, due, finishing, finals, queue, durable, credits, early, scoredOverdue>>

\* Engine::retry_save (`r`): the next dispatch rebuilds the cumulative
\* snapshot under a fresh sequence.
Retry ==
    /\ Running /\ failed
    /\ failed' = FALSE
    /\ due' = TRUE
    /\ UNCHANGED chunkVars
    /\ UNCHANGED <<seq, pending, lastAck, finishing, finals, exited, answered,
                   queue, replies, durable, credits, early, overdue, scoredOverdue>>

\* Engine::request_exit: the first request finishes and makes the final
\* checkpoint due; after a failure a second request exits unsuccessfully.
RequestExit ==
    /\ Running
    /\ \/ /\ ~finishing
          /\ finishing' = TRUE
          /\ due' = TRUE
          /\ UNCHANGED exited
       \/ /\ finishing /\ failed
          /\ exited' = "failed"
          /\ UNCHANGED <<finishing, due>>
       \/ /\ ~QuitWaitsForAck /\ finishing /\ ~failed
          /\ exited' = "ok"
          /\ UNCHANGED <<finishing, due>>
    /\ UNCHANGED chunkVars
    /\ UNCHANGED <<seq, pending, failed, lastAck, finals, answered, queue,
                   replies, durable, credits, early, overdue, scoredOverdue>>

\* Engine::watchdog: the final flush gives up after 10 s, unsuccessfully.
FinalTimeout ==
    /\ Running /\ finishing /\ pending # None
    /\ exited' = "failed"
    /\ UNCHANGED chunkVars
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   answered, queue, replies, durable, credits, early, overdue, scoredOverdue>>

\* Engine::watchdog while practice runs: the outstanding checkpoint passed
\* the 2 s bound: practice pauses ("Saving is slow"), SaveStalled.
Stall ==
    /\ Running /\ ~finishing /\ ~overdue /\ pending # None
    /\ overdue' = TRUE
    /\ pending' = IF StallKeepsPending THEN pending ELSE None
    /\ UNCHANGED chunkVars
    /\ UNCHANGED <<seq, failed, lastAck, due, finishing, finals, exited, answered,
                   queue, replies, durable, credits, early, scoredOverdue>>

\* A scored content key (Engine::practice_key while Running).
Score ==
    /\ Running /\ ~finishing /\ ~failed
    /\ OverdueStopsScore => ~overdue
    /\ scoredOverdue' = (scoredOverdue \/ overdue)
    /\ UNCHANGED chunkVars
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals, exited,
                   answered, queue, replies, durable, credits, early, overdue>>

Next ==
    \/ Stall \/ Score \/ BecomeDue \/ Dispatch \/ WorkerCommit \/ WorkerFail \/ WorkerReplay
    \/ WorkerAckEarly \/ Receive \/ Retry \/ RequestExit \/ FinalTimeout
    \/ WorkerCommitLostReply \/ Forged \/ Close

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* An acknowledged sequence is committed: the watermark never runs ahead.
AckIsDurable == lastAck # None => durable # None /\ lastAck <= durable

\* No cumulative snapshot is credited twice.
NoDoubleCredit == \A s \in Seqs : credits[s] <= 1

\* At most one dispatched checkpoint the engine has not yet heard back on.
SingleFlight == Cardinality({s \in 1..seq : s \notin answered}) <= 1

\* A successful exit means a final-status checkpoint was acknowledged and
\* is durable.
CleanExitIsDurable ==
    exited = "ok" => /\ lastAck \in finals
                     /\ durable # None
                     /\ lastAck <= durable

\* While a save has failed, what the engine shows as durable is exactly the
\* last acknowledged sequence, and no outstanding save hides behind it.
FailedTailIsNamed ==
    failed => /\ pending = None
              /\ (lastAck # None => lastAck <= durable)

\* No content is scored while an acknowledgement is overdue.
NoScoreWhileOverdue == ~scoredOverdue

\* A stall loses nothing: it keeps waiting for the stalled checkpoint, so
\* its answer still moves the watermark (or takes the save-failed path).
StallLosesNothing == overdue => pending # None

\* A stored chunk never changes.
ChunksAppendOnly == \A i \in Chunks : i <= stored => payload[i] = Written

\* The derived totals are the fold of the stored chunks, each counted once,
\* whatever was retried or replayed.
EvidenceCountedOnce == derived = stored

\* Stored chunks are exactly 1..stored: no gap.
NoGap == \A i \in Chunks : (payload[i] # Absent) <=> (i <= stored)
=============================================================================
