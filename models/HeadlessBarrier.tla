--------------------------- MODULE HeadlessBarrier ---------------------------
(***************************************************************************)
(* Headless barriers (VP-04, #71, RFC 2026-11-26-headless-driver-and-      *)
(* replay). Extends the idea of formal/CausalBarrier.tla (frozen) with     *)
(* child registration when a parent's result is consumed and a durable     *)
(* scope over the checkpoint watermark.                                    *)
(*                                                                         *)
(* Events are admitted in order. An event may dispatch a checkpoint job in *)
(* its own causal group. Consuming a job's result may register a child job *)
(* in the parent's group (the follow-up checkpoint). A barrier through an  *)
(* admitted event waits on its scope (groups <= through); a durable        *)
(* barrier also waits for an acknowledgement at or above the watermark it  *)
(* captured. Decisions follow cadence_kernel::barrier::{causal, durable}.  *)
(*                                                                         *)
(* Fault switches, TRUE in the positive configuration and FALSE in exactly *)
(* one mutant configuration:                                               *)
(*   ScopeGuard:    the barrier checks its scope before returning.         *)
(*   ChildAtomic:   a child is registered in the step that consumes its    *)
(*                  parent's result (no gap).                              *)
(*   WatermarkGuard: a durable barrier checks the acknowledged sequence.   *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS MaxEvents, MaxJobs, ScopeGuard, ChildAtomic, WatermarkGuard

VARIABLES
    latest,       \* the latest admitted event
    jobs,         \* outstanding jobs: [id |-> n, group |-> g, seq |-> s]
    nextJob,      \* the next job id (also the next checkpoint sequence)
    gap,          \* a consumed parent whose child is not registered yet (0: none)
    gapGroup,     \* that parent's group
    acked,        \* the highest acknowledged checkpoint sequence (0: none)
    barrier,      \* "none", "waiting" or "done"
    through,      \* the barrier's scope
    durable,      \* the barrier is durable
    watermark,    \* the checkpoint sequence a durable barrier captured
    lateChild,    \* a child joined a scope that was already reported done
    waitingOn     \* the job ids the barrier waits on

vars == <<latest, jobs, nextJob, gap, gapGroup, acked, barrier, through, durable,
          watermark, lateChild, waitingOn>>

InScope(j) == j.group <= through

TypeOK ==
    /\ latest \in 0..MaxEvents
    /\ nextJob \in 1..(MaxJobs + 1)
    /\ barrier \in {"none", "waiting", "done"}
    /\ durable \in BOOLEAN /\ lateChild \in BOOLEAN

Init ==
    /\ latest = 0 /\ jobs = {} /\ nextJob = 1 /\ gap = 0 /\ gapGroup = 0
    /\ acked = 0 /\ barrier = "none" /\ through = 0 /\ durable = FALSE
    /\ watermark = 0 /\ lateChild = FALSE /\ waitingOn = {}

\* An event is admitted; it may dispatch a checkpoint job in its group.
Event(dispatch) ==
    /\ latest < MaxEvents /\ gap = 0
    /\ latest' = latest + 1
    /\ IF dispatch /\ nextJob <= MaxJobs
       THEN /\ jobs' = jobs \cup {[id |-> nextJob, group |-> latest + 1, seq |-> nextJob]}
            /\ nextJob' = nextJob + 1
       ELSE UNCHANGED <<jobs, nextJob>>
    /\ UNCHANGED <<gap, gapGroup, acked, barrier, through, durable, watermark, lateChild, waitingOn>>

\* A job completes and its result is consumed (the acknowledgement): it may
\* register a child in its group.
Consume(j, child) ==
    /\ j \in jobs /\ gap = 0
    /\ acked' = IF j.seq > acked THEN j.seq ELSE acked
    /\ IF child /\ nextJob <= MaxJobs
       THEN IF ChildAtomic
            THEN /\ jobs' = (jobs \ {j}) \cup {[id |-> nextJob, group |-> j.group, seq |-> nextJob]}
                 /\ nextJob' = nextJob + 1
                 /\ lateChild' = (lateChild \/ (barrier = "done" /\ j.group <= through))
                 /\ UNCHANGED <<gap, gapGroup>>
            ELSE /\ jobs' = jobs \ {j}
                 /\ gap' = nextJob /\ gapGroup' = j.group
                 /\ UNCHANGED <<nextJob, lateChild>>
       ELSE /\ jobs' = jobs \ {j}
            /\ UNCHANGED <<nextJob, gap, gapGroup, lateChild>>
    /\ UNCHANGED <<latest, barrier, through, durable, watermark, waitingOn>>

\* The mutant's second step: the child registers after its parent drained.
RegisterChild ==
    /\ gap # 0
    /\ jobs' = jobs \cup {[id |-> nextJob, group |-> gapGroup, seq |-> nextJob]}
    /\ nextJob' = nextJob + 1
    /\ lateChild' = (lateChild \/ (barrier = "done" /\ gapGroup <= through))
    /\ gap' = 0 /\ gapGroup' = 0
    /\ UNCHANGED <<latest, acked, barrier, through, durable, watermark, waitingOn>>

\* A barrier through an admitted event starts; a durable one captures the
\* newest dispatched checkpoint as its watermark.
Start(t, d) ==
    /\ barrier = "none" /\ t <= latest
    /\ barrier' = "waiting" /\ through' = t /\ durable' = d
    /\ watermark' = IF d THEN nextJob - 1 ELSE 0
    /\ waitingOn' = {j.id : j \in {k \in jobs : k.group <= t}}
    /\ UNCHANGED <<latest, jobs, nextJob, gap, gapGroup, acked, lateChild>>

\* The barrier checks its scope (barrier::causal) and the watermark
\* (barrier::durable).
Check ==
    /\ barrier = "waiting"
    /\ LET scope == {j \in jobs : InScope(j)}
           drained == scope = {}
           covered == ~durable \/ acked >= watermark
       IN /\ (ScopeGuard => drained)
          /\ (WatermarkGuard => covered)
          /\ barrier' = "done"
          /\ waitingOn' = {j.id : j \in scope}
    /\ UNCHANGED <<latest, jobs, nextJob, gap, gapGroup, acked, through, durable, watermark, lateChild>>

Next ==
    \/ \E d \in BOOLEAN : Event(d)
    \/ \E j \in jobs, c \in BOOLEAN : Consume(j, c)
    \/ RegisterChild
    \/ \E t \in 0..MaxEvents, d \in BOOLEAN : Start(t, d)
    \/ Check

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* A done barrier has no outstanding job in its scope.
NoEarlyReturn == barrier = "done" => {j \in jobs : InScope(j)} = {} \/ lateChild

\* A child is registered before its parent is drained: no child ever joins
\* a scope already reported done.
ChildBeforeDrain == ~lateChild

\* A done durable barrier is covered by an acknowledgement at or above its
\* watermark.
DurableCoversWatermark == (barrier = "done" /\ durable) => acked >= watermark

\* The barrier only ever waits on work in its scope.
LaterWorkExcluded == \A j \in jobs : j.id \in waitingOn => j.group <= through
=============================================================================
