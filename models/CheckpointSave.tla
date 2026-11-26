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
(* Abstracted away: the checkpoint contents (a sequence stands for its     *)
(* cumulative snapshot), the 2 s cadence (any time a checkpoint may become *)
(* due), the running-phase watchdog invalidation (#51) and process crash   *)
(* (the frozen formal/DurableCheckpoint.tla covers reopen).                *)
(*                                                                         *)
(* Fault switches, each TRUE in the positive configuration and FALSE in    *)
(* exactly one mutant configuration:                                       *)
(*   SingleFlightGuard: dispatch waits while a checkpoint is outstanding.  *)
(*   AckAfterCommit:    the worker replies success only after commit.      *)
(*   Idempotent:        a replayed sequence is not credited again (admit). *)
(*   QuitWaitsForAck:   a clean exit needs the final acknowledgement.      *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS MaxSeq, SingleFlightGuard, AckAfterCommit, Idempotent, QuitWaitsForAck

Seqs == 1..MaxSeq
None == 0

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
    early         \* sequences acknowledged before commit (AckAfterCommit = FALSE)

vars == <<seq, pending, failed, lastAck, due, finishing, finals, exited,
          answered, queue, replies, durable, credits, early>>

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

Running == exited = "no"

\* Engine::watchdog (2 s cadence while running), exercise completion and
\* the end of a phase set checkpoint_due.
BecomeDue ==
    /\ Running /\ ~finishing /\ ~due
    /\ due' = TRUE
    /\ UNCHANGED <<seq, pending, failed, lastAck, finishing, finals, exited,
                   answered, queue, replies, durable, credits, early>>

\* Engine::dispatch_checkpoint: single-flight, never while a failure waits.
Dispatch ==
    /\ Running /\ due /\ ~failed /\ seq < MaxSeq
    /\ SingleFlightGuard => pending = None
    /\ seq' = seq + 1
    /\ pending' = seq + 1
    /\ due' = FALSE
    /\ finals' = IF finishing THEN finals \cup {seq + 1} ELSE finals
    /\ queue' = Append(queue, seq + 1)
    /\ UNCHANGED <<failed, lastAck, finishing, exited, answered, replies,
                   durable, credits, early>>

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
        /\ replies' = IF s \in early THEN replies ELSE replies \cup {[s |-> s, ok |-> TRUE]}
        /\ early' = early \ {s}
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered>>

\* The transaction fails (disk full, I/O error): nothing is committed.
WorkerFail ==
    /\ queue # << >>
    /\ Head(queue) \notin early
    /\ LET s == Head(queue) IN
        /\ queue' = Tail(queue)
        /\ replies' = replies \cup {[s |-> s, ok |-> FALSE]}
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered, durable, credits, early>>

\* A committed request processed again (the same checkpoint retried after a
\* lost reply): admit must make it a replay.
WorkerReplay ==
    /\ durable # None
    /\ \E s \in 1..durable :
        /\ credits[s] > 0
        /\ credits[s] < 2
        /\ Commit(s)
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered, queue, replies, early>>

\* Mutant only: reply success before the transaction commits.
WorkerAckEarly ==
    /\ ~AckAfterCommit
    /\ queue # << >>
    /\ Head(queue) \notin early
    /\ early' = early \cup {Head(queue)}
    /\ replies' = replies \cup {[s |-> Head(queue), ok |-> TRUE]}
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   exited, answered, queue, durable, credits>>

\* Engine::save_result: only the outstanding key's reply counts; a success
\* moves the watermark and, when finishing with nothing due and the
\* checkpoint carries the final status, quits successfully.
Receive ==
    /\ \E r \in replies :
        /\ replies' = replies \ {r}
        /\ answered' = answered \cup {r.s}
        /\ IF Running /\ pending = r.s
           THEN /\ pending' = None
                /\ IF r.ok
                   THEN /\ lastAck' = r.s
                        /\ failed' = FALSE
                        /\ exited' = IF finishing /\ ~due /\ r.s \in finals
                                     THEN "ok" ELSE exited
                   ELSE /\ failed' = TRUE
                        /\ UNCHANGED <<lastAck, exited>>
           ELSE UNCHANGED <<pending, lastAck, failed, exited>>
    /\ UNCHANGED <<seq, due, finishing, finals, queue, durable, credits, early>>

\* Engine::retry_save (`r`): the next dispatch rebuilds the cumulative
\* snapshot under a fresh sequence.
Retry ==
    /\ Running /\ failed
    /\ failed' = FALSE
    /\ due' = TRUE
    /\ UNCHANGED <<seq, pending, lastAck, finishing, finals, exited, answered,
                   queue, replies, durable, credits, early>>

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
    /\ UNCHANGED <<seq, pending, failed, lastAck, finals, answered, queue,
                   replies, durable, credits, early>>

\* Engine::watchdog: the final flush gives up after 10 s, unsuccessfully.
FinalTimeout ==
    /\ Running /\ finishing /\ pending # None
    /\ exited' = "failed"
    /\ UNCHANGED <<seq, pending, failed, lastAck, due, finishing, finals,
                   answered, queue, replies, durable, credits, early>>

Next ==
    \/ BecomeDue \/ Dispatch \/ WorkerCommit \/ WorkerFail \/ WorkerReplay
    \/ WorkerAckEarly \/ Receive \/ Retry \/ RequestExit \/ FinalTimeout

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
=============================================================================
