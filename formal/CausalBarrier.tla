-------------------------- MODULE CausalBarrier -------------------------
EXTENDS Naturals, FiniteSets
CONSTANTS Depth, CountDeferred
VARIABLES pending, deferred, finished, returned
vars == <<pending, deferred, finished, returned>>
Jobs == 0..Depth
Init == /\ pending = {0} /\ deferred = {} /\ finished = {}
        /\ returned = FALSE
Process(i) ==
  /\ i \in pending
  /\ pending' = pending \ {i}
  /\ deferred' = IF i < Depth THEN deferred \cup {i + 1} ELSE deferred
  /\ finished' = finished \cup {i}
  /\ UNCHANGED returned
ProcessAny == \E i \in pending : Process(i)
Dispatch == /\ deferred # {}
            /\ pending' = pending \cup deferred /\ deferred' = {}
            /\ UNCHANGED <<finished, returned>>
Return == /\ ~returned /\ pending = {}
          /\ (IF CountDeferred THEN deferred = {} ELSE TRUE)
          /\ returned' = TRUE
          /\ UNCHANGED <<pending, deferred, finished>>
Next == ProcessAny \/ Dispatch \/ Return
Spec == Init /\ [][Next]_vars
        /\ WF_vars(ProcessAny) /\ WF_vars(Dispatch) /\ WF_vars(Return)
TypeOK == /\ pending \subseteq Jobs /\ deferred \subseteq Jobs
          /\ finished \subseteq Jobs /\ returned \in BOOLEAN
          /\ pending \cap deferred = {} /\ pending \cap finished = {}
          /\ deferred \cap finished = {}
NoEarlyReturn == returned => finished = Jobs
EventuallyReturned == <>returned
=============================================================================
