---------------------------- MODULE RequestCache ----------------------------
(***************************************************************************)
(* Headless request admission (VP-05, #67, RFC                             *)
(* 2026-11-26-headless-driver-and-replay).                                 *)
(*                                                                         *)
(* An ordered request stream over a bounded response cache with FIFO       *)
(* eviction. Admission follows cadence_kernel::request::admit: an id       *)
(* greater than every applied id is new and applies; an old id replays    *)
(* (same digest), conflicts (other digest) or has expired (evicted). Only  *)
(* a new id mutates. Eviction follows request::evict.                      *)
(*                                                                         *)
(* Fault switches, TRUE in the positive configuration and FALSE in exactly *)
(* one mutant configuration:                                               *)
(*   ReplayGuard:   a replay returns the cached response, applying nothing. *)
(*   ConflictGuard: a conflict applies nothing.                            *)
(*   ExpiryGuard:   an evicted id is expired, never re-executed.           *)
(***************************************************************************)
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS Ids, Digests, Capacity, ReplayGuard, ConflictGuard, ExpiryGuard

VARIABLES
    lastNew,          \* the newest applied id (0: none)
    cache,            \* cached responses, oldest first: <<id, digest>>
    applied,          \* times each id changed state
    conflictMutated,  \* a conflicting repeat changed state
    expiredMutated    \* an expired repeat changed state

vars == <<lastNew, cache, applied, conflictMutated, expiredMutated>>

Cached(i) == {k \in 1..Len(cache) : cache[k][1] = i}

TypeOK ==
    /\ lastNew \in {0} \cup Ids
    /\ applied \in [Ids -> Nat]
    /\ conflictMutated \in BOOLEAN /\ expiredMutated \in BOOLEAN

Init ==
    /\ lastNew = 0 /\ cache = << >>
    /\ applied = [i \in Ids |-> 0]
    /\ conflictMutated = FALSE /\ expiredMutated = FALSE

\* request::evict: drop the oldest response when the cache is full.
Insert(i, d) ==
    IF Len(cache) >= Capacity
    THEN Append(Tail(cache), <<i, d>>)
    ELSE Append(cache, <<i, d>>)

Apply(i) == applied' = [applied EXCEPT ![i] = @ + 1]

Request(i, d) ==
    /\ applied[i] < 3      \* bounds the mutants' state space
    /\ IF i > lastNew
       THEN /\ Apply(i) /\ lastNew' = i /\ cache' = Insert(i, d)
            /\ UNCHANGED <<conflictMutated, expiredMutated>>
       ELSE IF \E k \in Cached(i) : cache[k][2] = d
       THEN \* Replay
            /\ IF ReplayGuard THEN UNCHANGED applied ELSE Apply(i)
            /\ UNCHANGED <<lastNew, cache, conflictMutated, expiredMutated>>
       ELSE IF Cached(i) # {}
       THEN \* Conflict
            /\ IF ConflictGuard
               THEN UNCHANGED <<applied, conflictMutated>>
               ELSE Apply(i) /\ conflictMutated' = TRUE
            /\ UNCHANGED <<lastNew, cache, expiredMutated>>
       ELSE \* Expired
            /\ IF ExpiryGuard
               THEN UNCHANGED <<applied, expiredMutated>>
               ELSE Apply(i) /\ expiredMutated' = TRUE
            /\ UNCHANGED <<lastNew, cache, conflictMutated>>

Next == \E i \in Ids, d \in Digests : Request(i, d)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* A request id changes state at most once.
AtMostOnceApply == \A i \in Ids : applied[i] <= 1

\* A conflicting repeat never changes state.
ConflictNeverMutates == ~conflictMutated

\* An expired (evicted) repeat never changes state.
ExpiredNeverMutates == ~expiredMutated

\* The cache never holds more than its capacity.
CacheBounded == Len(cache) <= Capacity
=============================================================================
