---------------------------- MODULE WorkerResults ----------------------------
(***************************************************************************)
(* Worker results (VP-01, VP-02, #75, RFC 2026-11-26-configuration-layout- *)
(* and-settings). Living successor of the frozen formal/AsyncAdmission.tla.*)
(*                                                                         *)
(* One job kind: requests carry the current epoch and a fresh request id;  *)
(* results (success or failure) can arrive late, twice, or after a restart *)
(* raised the epoch. A result is accepted only through                     *)
(* cadence_kernel::ticket::admit (current epoch, the pending request); an  *)
(* accepted result is retained in the kind's single result slot until it  *)
(* is consumed.                                                            *)
(*                                                                         *)
(* Fault switches, TRUE in the positive configuration and FALSE in exactly *)
(* one mutant configuration:                                               *)
(*   EpochGuard:   a result from another epoch is refused.                 *)
(*   PendingGuard: a result whose request is not pending is refused.       *)
(*   RetainGuard:  an accepted result is kept even when the slot is full   *)
(*                 (it replaces the unconsumed one, which is superseded).  *)
(***************************************************************************)
EXTENDS Naturals, FiniteSets

CONSTANTS MaxEpoch, MaxRequests, EpochGuard, PendingGuard, RetainGuard

VARIABLES
    epoch,         \* the current epoch (a restart raises it)
    pending,       \* the pending request id (0: none)
    nextReq,       \* the next request id
    inflight,      \* results that may still arrive: [epoch, req, ok]
    applied,       \* times each <<epoch, req>> changed state
    slot,          \* the retained result (<<>>: empty)
    staleAccepted, \* a result from another epoch was accepted
    lost           \* an accepted result was dropped

vars == <<epoch, pending, nextReq, inflight, applied, slot, staleAccepted, lost>>

Keys == (0..MaxEpoch) \X (1..MaxRequests)

TypeOK ==
    /\ epoch \in 0..MaxEpoch /\ pending \in 0..MaxRequests
    /\ nextReq \in 1..(MaxRequests + 1)
    /\ applied \in [Keys -> Nat]
    /\ staleAccepted \in BOOLEAN /\ lost \in BOOLEAN

Init ==
    /\ epoch = 0 /\ pending = 0 /\ nextReq = 1 /\ inflight = {}
    /\ applied = [k \in Keys |-> 0] /\ slot = <<>>
    /\ staleAccepted = FALSE /\ lost = FALSE

\* A request: the kind waits for a fresh id in the current epoch.
Request ==
    /\ nextReq <= MaxRequests
    /\ pending' = nextReq /\ nextReq' = nextReq + 1
    /\ inflight' = inflight \cup {[epoch |-> epoch, req |-> nextReq, ok |-> TRUE],
                                  [epoch |-> epoch, req |-> nextReq, ok |-> FALSE]}
    /\ UNCHANGED <<epoch, applied, slot, staleAccepted, lost>>

\* The screen or session restarts: a new epoch, nothing pending, and
\* request ids start again (reuse): only the epoch tells results apart.
Restart ==
    /\ epoch < MaxEpoch
    /\ epoch' = epoch + 1 /\ pending' = 0 /\ nextReq' = 1
    /\ UNCHANGED <<inflight, applied, slot, staleAccepted, lost>>

\* A result arrives (it stays in flight: it may arrive again).
Deliver(r) ==
    /\ r \in inflight
    /\ applied[<<r.epoch, r.req>>] < 3
    /\ LET fresh == r.epoch = epoch
           waited == pending = r.req
       IN /\ (EpochGuard => fresh) /\ (PendingGuard => waited)
          /\ applied' = [applied EXCEPT ![<<r.epoch, r.req>>] = @ + 1]
          /\ staleAccepted' = (staleAccepted \/ ~fresh)
          /\ pending' = 0
          /\ IF slot # <<>> /\ ~RetainGuard
             THEN /\ lost' = TRUE /\ UNCHANGED slot
             ELSE /\ slot' = <<r.epoch, r.req, r.ok>> /\ UNCHANGED lost
    /\ UNCHANGED <<epoch, nextReq, inflight>>

\* The reducer consumes the retained result.
Consume ==
    /\ slot # <<>>
    /\ slot' = <<>>
    /\ UNCHANGED <<epoch, pending, nextReq, inflight, applied, staleAccepted, lost>>

Next == Request \/ Restart \/ Consume \/ \E r \in inflight : Deliver(r)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* Only a result of the current epoch, for the pending request, is accepted.
AcceptedFresh == ~staleAccepted

\* A request changes state at most once.
AtMostOnce == \A k \in Keys : applied[k] <= 1

\* An accepted result is retained until consumed, never dropped.
CompletionRetained == ~lost
=============================================================================
