------------------------- MODULE AsyncAdmission -------------------------
EXTENDS Naturals, Sequences, FiniteSets
CONSTANTS MaxEpoch, MaxRequest, MaxAccepted, CheckEpoch, CheckPending
VARIABLES epoch, request, issued, pending, accepted
vars == <<epoch, request, issued, pending, accepted>>
Tickets == [epoch : 0..MaxEpoch, request : 0..MaxRequest]
Current == [epoch |-> epoch, request |-> request]
Init == /\ epoch = 0 /\ request = 0
        /\ issued = {} /\ pending = {} /\ accepted = <<>>
Start == /\ Current \notin issued
         /\ issued' = issued \cup {Current}
         /\ pending' = pending \cup {Current}
         /\ UNCHANGED <<epoch, request, accepted>>
Refresh == /\ request < MaxRequest
           /\ request' = request + 1
           /\ UNCHANGED <<epoch, issued, pending, accepted>>
Restart == /\ epoch < MaxEpoch
           /\ epoch' = epoch + 1 /\ request' = 0
           /\ UNCHANGED <<issued, pending, accepted>>
Deliver(t) ==
  /\ t \in issued
  /\ Len(accepted) < MaxAccepted
  /\ LET admit == (t.request = request)
                  /\ (IF CheckEpoch THEN t.epoch = epoch ELSE TRUE)
                  /\ (IF CheckPending THEN t \in pending ELSE TRUE)
     IN accepted' = IF admit
          THEN Append(accepted, [ticket |-> t, atEpoch |-> epoch,
                                 atRequest |-> request])
          ELSE accepted
  /\ pending' = pending \ {t}
  /\ UNCHANGED <<epoch, request, issued>>
Next == Start \/ Refresh \/ Restart \/ (\E t \in issued : Deliver(t))
Spec == Init /\ [][Next]_vars
TypeOK == /\ epoch \in 0..MaxEpoch /\ request \in 0..MaxRequest
          /\ issued \subseteq Tickets /\ pending \subseteq issued
          /\ accepted \in Seq([ticket : Tickets, atEpoch : 0..MaxEpoch,
                                atRequest : 0..MaxRequest])
          /\ Len(accepted) <= MaxAccepted
AcceptedFresh == \A i \in 1..Len(accepted) :
  /\ accepted[i].ticket.epoch = accepted[i].atEpoch
  /\ accepted[i].ticket.request = accepted[i].atRequest
AtMostOnce == \A i,j \in 1..Len(accepted) :
  accepted[i].ticket = accepted[j].ticket => i = j
=============================================================================
