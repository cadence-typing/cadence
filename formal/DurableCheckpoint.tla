----------------------- MODULE DurableCheckpoint -----------------------
EXTENDS Naturals, FiniteSets
CONSTANTS MaxSeq, AckAfterCommit, Idempotent
VARIABLES issued, durable, saved, replies, credited
vars == <<issued, durable, saved, replies, credited>>
Init == /\ issued = 0 /\ durable = 0 /\ saved = 0
        /\ replies = {} /\ credited = 0
Issue == /\ issued < MaxSeq
         /\ issued' = issued + 1
         /\ replies' = IF AckAfterCommit THEN replies
                       ELSE replies \cup {issued + 1}
         /\ UNCHANGED <<durable, saved, credited>>
Commit(s) ==
  /\ s \in 1..issued
  /\ durable' = IF s > durable THEN s ELSE durable
  /\ credited' = IF Idempotent
                 THEN (IF s > durable THEN credited + (s - durable)
                       ELSE credited)
                 ELSE credited + s
  /\ replies' = replies \cup {s}
  /\ UNCHANGED <<issued, saved>>
ObserveAck(s) == /\ s \in replies
                 /\ saved' = IF s > saved THEN s ELSE saved
                 /\ replies' = replies \ {s}
                 /\ UNCHANGED <<issued, durable, credited>>
LoseReplies == /\ replies # {} /\ replies' = {}
               /\ UNCHANGED <<issued, durable, saved, credited>>
Recover == /\ saved' = durable
           /\ UNCHANGED <<issued, durable, replies, credited>>
Next == Issue \/ (\E s \in 1..issued : Commit(s))
        \/ (\E s \in replies : ObserveAck(s)) \/ LoseReplies \/ Recover
Spec == Init /\ [][Next]_vars
TypeOK == /\ issued \in 0..MaxSeq /\ durable \in 0..issued
          /\ saved \in 0..issued /\ replies \subseteq 1..issued
          /\ credited \in Nat
AckIsDurable == saved <= durable
NoDoubleCredit == credited = durable
=============================================================================
