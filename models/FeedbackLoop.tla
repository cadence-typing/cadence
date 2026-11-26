----------------------------- MODULE FeedbackLoop -----------------------------
(***************************************************************************)
(* The cross-session feedback loop (#40, RFC                               *)
(* 2026-11-26-deliberate-practice-in-every-phase): observe first attempts, *)
(* checkpoint the session's evidence, end the session (finish or kill),    *)
(* replay the durable evidence through the review ladder, plan the next    *)
(* session with the due words in review slots, serve them.                 *)
(*                                                                         *)
(* Production: Learning::{open, close} (engine) observe and re-serve;      *)
(* queue::admit decides reinforcement; learning_rows::save replaces the    *)
(* session's rows with the cumulative snapshot; ProgressStore::bootstrap   *)
(* reads them back; trainer::learner::due_words replays them through       *)
(* review::ladder_step (one outcome per word per session: errors > 0 or    *)
(* clean) and plans the due words.                                         *)
(*                                                                         *)
(* Abstracted away: typing content (an observation is a word's first       *)
(* attempt, clean or missed), timing, focus selection, other lanes; every  *)
(* planned word has one slot, a due word's slot is a review slot, and one  *)
(* more review slot is free for reinforcement.                             *)
(*                                                                         *)
(* Fault switches, each TRUE in the positive configuration and FALSE in    *)
(* exactly one mutant configuration:                                       *)
(*   RecordRefusedMiss: a miss is recorded even when queue::admit refuses. *)
(*   DistinctDays:      a clean counts only when due, and promotes only on *)
(*                      a later day than the counted one.                  *)
(*   RetypeIsPractice:  a same-session retype is not a ladder outcome.     *)
(*   ReplaceSnapshot:   a checkpoint replaces the session's rows.          *)
(*   FreeSlotOnly:      reinforcement takes only the free review slot.     *)
(*   CheckDuplicate:    a queued word is not queued twice (queue::admit).  *)
(*   PlanDue:           the planner gives every due word a review slot.    *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS Words, Days, SessionsPerDay, QueueCap, RepsPerWord,
          RecordRefusedMiss, DistinctDays, RetypeIsPractice, ReplaceSnapshot,
          FreeSlotOnly, CheckDuplicate, PlanDue

Rungs == 0..2
Interval(r) == CASE r = 0 -> 1 [] r = 1 -> 3 [] r = 2 -> 7
MaxDay == Days + 8
Tally == [attempts : 0..4, errors : 0..4]
Sessions == 0..(Days * SessionsPerDay)
Zero == [attempts |-> 0, errors |-> 0]

VARIABLES
    day, sess, phase,  \* calendar and "idle" / "practice" / "over"
    ladder,            \* per word: rung, streak day (0 = none), due day (0 = none)
    slots,             \* words planned this session (one slot each)
    dueAtPlan,         \* words due when the session was planned
    free,              \* free review slots for reinforcement
    served,            \* words whose slot was typed
    live,              \* the session's evidence in memory
    stored,            \* the session's durable rows (last checkpoint)
    dirty,             \* evidence changed since the last checkpoint
    queue, reps,       \* reinforcement queue and per-word repetitions
    retyped,           \* words already retyped this session
    \* history, for the properties
    missed,            \* first-attempt misses observed per word this session
    steps,             \* ladder outcomes applied per word this session
    badPromotion,      \* a promotion without two clean days
    displaced,         \* due words whose review slot was taken
    killed,            \* the session ended without its final checkpoint
    sid                \* the session's number

vars == <<day, sess, phase, ladder, slots, dueAtPlan, free, served, live, stored,
          dirty, queue, reps, retyped, missed, steps, badPromotion, displaced, killed, sid>>

TypeOK ==
    /\ day \in 1..(Days + 1) /\ sess \in 0..SessionsPerDay
    /\ phase \in {"idle", "practice", "over"}
    /\ ladder \in [Words -> [rung : Rungs, streak : 0..MaxDay, due : 0..MaxDay]]
    /\ slots \subseteq Words /\ dueAtPlan \subseteq Words /\ served \subseteq Words
    /\ free \in 0..1
    /\ live \in [Words -> Tally] /\ stored \in [Words -> [attempts : 0..24, errors : 0..24]]
    /\ dirty \in BOOLEAN
    /\ queue \in Seq(Words) /\ reps \in [Words -> 0..RepsPerWord]
    /\ retyped \subseteq Words /\ missed \in [Words -> 0..4]
    /\ steps \in [Words -> 0..2] /\ badPromotion \in BOOLEAN
    /\ displaced \subseteq Words /\ killed \in BOOLEAN /\ sid \in Sessions

Init ==
    /\ day = 1 /\ sess = 0 /\ phase = "idle"
    /\ ladder = [w \in Words |-> [rung |-> 0, streak |-> 0, due |-> 0]]
    /\ slots = {} /\ dueAtPlan = {} /\ free = 0 /\ served = {}
    /\ live = [w \in Words |-> Zero] /\ stored = [w \in Words |-> Zero]
    /\ dirty = FALSE /\ queue = << >> /\ reps = [w \in Words |-> 0]
    /\ retyped = {} /\ missed = [w \in Words |-> 0] /\ steps = [w \in Words |-> 0]
    /\ badPromotion = FALSE /\ displaced = {} /\ killed = FALSE /\ sid = 0

IsDue(w) == ladder[w].due # 0 /\ ladder[w].due <= day
Queued(w) == \E i \in 1..Len(queue) : queue[i] = w

\* trainer::learner::due_words, then the planner: every due word gets a
\* review slot (PlanDue), every other word a regular slot.
Plan ==
    /\ phase = "idle"
    /\ LET due == {w \in Words : IsDue(w)} IN
        /\ dueAtPlan' = due
        /\ slots' = IF PlanDue THEN Words ELSE Words \ due
    /\ phase' = "practice"
    /\ free' = 1 /\ served' = {} /\ live' = [w \in Words |-> Zero]
    /\ stored' = [w \in Words |-> Zero] /\ dirty' = FALSE
    /\ queue' = << >> /\ reps' = [w \in Words |-> 0] /\ retyped' = {}
    /\ missed' = [w \in Words |-> 0] /\ steps' = [w \in Words |-> 0] /\ displaced' = {} /\ killed' = FALSE
    /\ sid' = sid + 1
    /\ UNCHANGED <<day, sess, ladder, badPromotion>>

\* Learning::close observes one completed word; a miss asks queue::admit
\* for a same-session repetition (the miss is recorded either way).
Observe(w, miss) ==
    /\ live' = [live EXCEPT ![w].attempts = @ + 1,
                            ![w].errors = @ + (IF miss THEN 1 ELSE 0)]
    /\ dirty' = TRUE
    /\ missed' = [missed EXCEPT ![w] = @ + (IF miss THEN 1 ELSE 0)]

Admit(w) == reps[w] < RepsPerWord /\ Len(queue) < QueueCap /\ (CheckDuplicate => ~Queued(w))

Enqueue(w, miss) ==
    IF miss /\ Admit(w)
    THEN /\ queue' = Append(queue, w) /\ reps' = [reps EXCEPT ![w] = @ + 1]
         /\ Observe(w, miss)
    ELSE /\ UNCHANGED <<queue, reps>>
         /\ IF miss /\ ~RecordRefusedMiss
            THEN /\ UNCHANGED live /\ dirty' = dirty /\ missed' = [missed EXCEPT ![w] = @ + 1]
            ELSE Observe(w, miss)

\* Type a planned word's slot.
Type(w, miss) ==
    /\ phase = "practice" /\ w \in slots \ served
    /\ served' = served \cup {w}
    /\ Enqueue(w, miss)
    /\ UNCHANGED <<day, sess, phase, ladder, slots, dueAtPlan, free, stored, retyped,
                   steps, badPromotion, displaced, killed, sid>>

\* Learning::open re-serves the queue head in a review slot: the free one,
\* never one holding a due word (FreeSlotOnly).
Reinforce(miss) ==
    /\ phase = "practice" /\ queue # << >>
    /\ LET w == Head(queue) IN
        /\ \/ /\ free > 0 /\ free' = free - 1 /\ UNCHANGED <<slots, displaced>>
           \/ /\ ~FreeSlotOnly
              /\ \E d \in (slots \cap dueAtPlan) \ served :
                    /\ slots' = slots \ {d} /\ displaced' = displaced \cup {d}
              /\ UNCHANGED free
        /\ queue' = Tail(queue)
        /\ live' = [live EXCEPT ![w].attempts = @ + 1,
                                ![w].errors = @ + (IF miss THEN 1 ELSE 0)]
        /\ missed' = [missed EXCEPT ![w] = @ + (IF miss THEN 1 ELSE 0)]
        /\ dirty' = TRUE
    /\ UNCHANGED <<day, sess, phase, ladder, dueAtPlan, served, stored, reps, retyped,
                   steps, badPromotion, killed, sid>>

\* The review ladder (cadence_kernel::review::ladder_step) for one session
\* outcome on `d`: an error resets; a clean before the due day is practice;
\* a due clean on a day after the counted one promotes.
Step(l, err, d) ==
    IF err THEN [rung |-> 0, streak |-> 0, due |-> d + 1]
    ELSE IF DistinctDays /\ (l.due = 0 \/ d < l.due) THEN l
    ELSE IF l.streak # 0 /\ (IF DistinctDays THEN d > l.streak ELSE d >= l.streak)
         THEN LET r == IF l.rung < 2 THEN l.rung + 1 ELSE l.rung
              IN [rung |-> r, streak |-> 0, due |-> d + Interval(r)]
         ELSE [rung |-> l.rung, streak |-> d, due |-> d + Interval(l.rung)]

Promoted(l, err, d) == Step(l, err, d).rung > l.rung

\* A same-session retype of a served word (the closing review). Learning::close
\* observes it like any word (a miss may queue it again); it is practice:
\* its outcome only adds to the session's evidence (RetypeIsPractice).
Retype(w, miss) ==
    /\ phase = "practice" /\ w \in served \ retyped
    /\ retyped' = retyped \cup {w}
    /\ Enqueue(w, miss)
    /\ IF RetypeIsPractice
       THEN UNCHANGED <<ladder, steps, badPromotion>>
       ELSE /\ ladder' = [ladder EXCEPT ![w] = Step(@, miss, day)]
            /\ steps' = [steps EXCEPT ![w] = @ + 1]
            /\ badPromotion' = (badPromotion \/
                 (Promoted(ladder[w], miss, day) /\ ladder[w].streak >= day))
    /\ UNCHANGED <<day, sess, phase, slots, dueAtPlan, free, served, stored,
                   displaced, killed, sid>>

\* The 2 s checkpoint: learning_rows::save writes the cumulative snapshot.
Checkpoint ==
    /\ phase = "practice" /\ dirty
    /\ stored' = IF ReplaceSnapshot THEN live
                 ELSE [w \in Words |-> [attempts |-> stored[w].attempts + live[w].attempts,
                                        errors |-> stored[w].errors + live[w].errors]]
    /\ dirty' = FALSE
    /\ UNCHANGED <<day, sess, phase, ladder, slots, dueAtPlan, free, served, live, queue,
                   reps, retyped, missed, steps, badPromotion, displaced, killed, sid>>

\* The next session's due_words replays the durable rows: one outcome per
\* word that was typed, errors > 0 or clean.
Replay(rows) ==
    /\ ladder' = [w \in Words |-> IF rows[w].attempts > 0
                                  THEN Step(ladder[w], rows[w].errors > 0, day) ELSE ladder[w]]
    /\ steps' = [w \in Words |-> steps[w] + (IF rows[w].attempts > 0 THEN 1 ELSE 0)]
    /\ badPromotion' = (badPromotion \/ \E w \in Words :
         rows[w].attempts > 0 /\ Promoted(ladder[w], rows[w].errors > 0, day)
         /\ ladder[w].streak >= day)

NextSession ==
    /\ IF sess + 1 = SessionsPerDay THEN day' = day + 1 /\ sess' = 0
                                    ELSE day' = day /\ sess' = sess + 1
    /\ phase' = IF sess + 1 = SessionsPerDay /\ day = Days THEN "over" ELSE "idle"

\* The plan is done: the final checkpoint, then the session ends.
Finish ==
    /\ phase = "practice" /\ slots \subseteq served
    /\ stored' = IF ReplaceSnapshot THEN live
                 ELSE [w \in Words |-> [attempts |-> stored[w].attempts + live[w].attempts,
                                        errors |-> stored[w].errors + live[w].errors]]
    /\ Replay(stored')
    /\ NextSession
    /\ dirty' = FALSE
    /\ UNCHANGED <<slots, dueAtPlan, free, served, live, queue, reps, retyped, missed,
                   displaced, killed, sid>>

\* The process dies: only the last checkpoint is durable.
Kill ==
    /\ phase = "practice"
    /\ killed' = TRUE
    /\ Replay(stored)
    /\ NextSession
    /\ UNCHANGED <<slots, dueAtPlan, free, served, live, stored, dirty, queue, reps,
                   retyped, missed, displaced, sid>>

\* The calendar ran out: the model stops (a stuttering step, not deadlock).
Over == phase = "over" /\ UNCHANGED vars

Next ==
    \/ Plan \/ Checkpoint \/ Finish \/ Kill \/ Over
    \/ \E w \in Words, m \in BOOLEAN : Type(w, m) \/ Retype(w, m)
    \/ \E m \in BOOLEAN : Reinforce(m)

Spec == Init /\ [][Next]_vars /\ WF_vars(Next)

-----------------------------------------------------------------------------
\* Every first-attempt miss is in the session's evidence, whether the queue
\* admitted it or not.
NoLostMiss == \A w \in Words : live[w].errors = missed[w]

\* A word is promoted only after clean completions on two distinct days.
NoFalsePromotion == ~badPromotion

\* At most one ladder outcome per word per session, so a same-session
\* retype (reinforcement, closing review) can never promote.
SameSessionNeverPromotes == \A w \in Words : steps[w] <= 1

\* The durable rows never count more than the session observed.
EvidenceCountedOnce == \A w \in Words : stored[w].attempts <= live[w].attempts

\* Reinforcement never takes a due word's review slot.
DueNotDisplaced == displaced = {}

\* The queue stays within capacity and holds each word at most once.
QueueBounded ==
    /\ Len(queue) <= QueueCap
    /\ \A i, j \in 1..Len(queue) : i # j => queue[i] # queue[j]

\* A word due when the session was planned is served in it, unless the
\* session is killed.
DueEventuallyServed ==
    \A w \in Words, k \in Sessions :
        (phase = "practice" /\ sid = k /\ w \in dueAtPlan) ~> (sid = k /\ (w \in served \/ killed))
=============================================================================
