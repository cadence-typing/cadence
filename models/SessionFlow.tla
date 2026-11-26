----------------------------- MODULE SessionFlow -----------------------------
(***************************************************************************)
(* The practice session flow (#31, RFC                                     *)
(* 2026-11-26-formal-gates-and-production-mapping).                        *)
(*                                                                         *)
(* State changes follow the proved table cadence_kernel::session::step;    *)
(* this model adds what the table does not see and the engine decides      *)
(* around it: the active clock and its half-open phase window              *)
(* (Engine::advance_clock, Micros::within_half_open = time::in_window),    *)
(* the interlude grace (Engine::interlude_settled), the guards on resume   *)
(* and end-early (an invalid measurement or a failed save) and the pause   *)
(* reason (Engine::pause_for). Time is discrete: one Tick is one unit of   *)
(* wall time, and active time grows only while the clock runs.             *)
(*                                                                         *)
(* Phases 1..Phases-1 are scored; phase Phases is the closing review      *)
(* (#49): typed and clocked like any phase, its deadline opens the results *)
(* (PlanDone). The session is completed at the last scored deadline.       *)
(*                                                                         *)
(* Abstracted away: content (a Type event stands for one scored key), the  *)
(* checkpoint protocol (models/CheckpointSave.tla), the exit path after    *)
(* the review, and resize (another pause reason).                          *)
(*                                                                         *)
(* Fault switches, each TRUE in the positive configuration and FALSE in    *)
(* exactly one mutant configuration:                                       *)
(*   PauseStopsClock:   a pause closes the open segment (Clock::Stop).     *)
(*   DeadlineExclusive: a key at the exact deadline is outside the phase.  *)
(*   GraceGuard:        the interlude ignores Enter/Esc during the grace.  *)
(*   EndEarlyAbandons:  ending early marks an unfinished plan abandoned.   *)
(*   CatchUpGuard:      a late acknowledgement resumes only the "saving"   *)
(*                      pause it caused (#51).                             *)
(*   ReviewCompletes:   the last scored deadline, not the review's,        *)
(*                      completes the session (#49).                       *)
(***************************************************************************)
EXTENDS Naturals

CONSTANTS Phases, Budget, Grace, MaxTime,
          PauseStopsClock, DeadlineExclusive, GraceGuard, EndEarlyAbandons, CatchUpGuard,
          ReviewCompletes

ASSUME Phases >= 2

\* The phase whose deadline completes the session.
CompletingPhase == IF ReviewCompletes THEN Phases - 1 ELSE Phases

States == {"Ready", "Running", "Paused", "PhaseComplete", "Review"}
Reasons == {"none", "requested", "focus", "paste", "invalid", "save", "saving"}

VARIABLES
    state,        \* the kernel session state
    phase,        \* current phase, 1..Phases
    active,       \* active time in the current phase
    started,      \* a content key opened this phase's clock (TimingSegments.started)
    clock,        \* a segment is open (TimingSegments::is_running)
    now,          \* wall time
    interlude,    \* when the phase-complete screen opened
    reason,       \* pause reason shown
    invalid,      \* Engine.invalid
    failed,       \* a save failed and awaits retry
    status,       \* "open", "completed" or "abandoned"
    \* history, for the invariants
    scoredOutside,  \* a key was scored at or past the phase deadline
    skipped,        \* an interlude was left before its grace
    endedEarly,     \* the session was ended before the last typing deadline
    stalled,        \* a checkpoint acknowledgement is overdue (#51)
    resumedOther,   \* a late acknowledgement resumed another reason's pause
    scoredDone      \* the last scored phase reached its deadline (#49)

vars == <<state, phase, active, started, clock, now, interlude, reason, invalid,
          failed, status, scoredOutside, skipped, endedEarly, stalled, resumedOther, scoredDone>>

TypeOK ==
    /\ state \in States
    /\ phase \in 1..Phases
    /\ active \in 0..MaxTime
    /\ started \in BOOLEAN /\ clock \in BOOLEAN
    /\ now \in 0..MaxTime /\ interlude \in 0..MaxTime
    /\ reason \in Reasons
    /\ invalid \in BOOLEAN /\ failed \in BOOLEAN
    /\ status \in {"open", "completed", "abandoned"}
    /\ scoredOutside \in BOOLEAN /\ skipped \in BOOLEAN /\ endedEarly \in BOOLEAN
    /\ stalled \in BOOLEAN /\ resumedOther \in BOOLEAN /\ scoredDone \in BOOLEAN

Init ==
    /\ state = "Ready" /\ phase = 1 /\ active = 0 /\ started = FALSE
    /\ clock = FALSE /\ now = 0 /\ interlude = 0 /\ reason = "none"
    /\ invalid = FALSE /\ failed = FALSE /\ status = "open"
    /\ scoredOutside = FALSE /\ skipped = FALSE /\ endedEarly = FALSE
    /\ stalled = FALSE /\ resumedOther = FALSE /\ scoredDone = FALSE

History == <<scoredOutside, skipped, endedEarly, resumedOther, scoredDone>>

\* The deadline has passed for this phase (Engine::advance_clock asks
\* within_half_open, i.e. active < Budget; the mutant asks active <= Budget).
InWindow(a) == IF DeadlineExclusive THEN a < Budget ELSE a <= Budget

\* Enter on the start screen: Start (table) opens the first phase; the clock
\* waits for the first content key.
Start ==
    /\ state = "Ready"
    /\ state' = "Running"
    /\ UNCHANGED <<phase, active, started, clock, now, interlude, reason, invalid,
                   failed, status, History, stalled>>

\* One content key, scored only inside the phase window
\* (Engine::practice_key after advance_clock).
Type ==
    /\ state = "Running" /\ ~invalid
    /\ InWindow(active)
    /\ started' = TRUE
    /\ clock' = TRUE
    /\ scoredOutside' = (scoredOutside \/ active >= Budget)
    /\ UNCHANGED <<state, phase, active, now, interlude, reason, invalid, failed,
                   status, skipped, endedEarly, stalled, resumedOther, scoredDone>>

\* Wall time passes; active time grows only while a segment is open
\* (TimingSegments::active_through = time::segment_active).
Tick ==
    /\ now < MaxTime
    /\ now' = now + 1
    /\ active' = IF clock /\ active < MaxTime THEN active + 1 ELSE active
    /\ UNCHANGED <<state, phase, started, clock, interlude, reason, invalid,
                   failed, status, History, stalled>>

\* The deadline: Deadline or LastDeadline (table), the open segment ends at
\* the boundary (TimingSegments::end_phase), the interlude opens. The
\* closing review's deadline opens the results instead (PlanDone).
Deadline ==
    /\ state = "Running"
    /\ ~InWindow(active)
    /\ state' = IF phase = Phases THEN "Review" ELSE "PhaseComplete"
    /\ clock' = FALSE
    /\ started' = FALSE
    /\ interlude' = now
    /\ status' = IF phase >= CompletingPhase THEN "completed" ELSE status
    /\ scoredDone' = (scoredDone \/ phase >= Phases - 1)
    /\ UNCHANGED <<phase, active, now, reason, invalid, failed, scoredOutside, skipped,
                   endedEarly, resumedOther, stalled>>

\* Esc, focus loss, paste (Engine::pause_for) and a failed save: Pause or
\* SaveFailed (table); the pause keeps its reason.
Pause(r) ==
    /\ state = "Running"
    /\ state' = "Paused"
    /\ clock' = IF PauseStopsClock THEN FALSE ELSE clock
    /\ reason' = r
    /\ failed' = (failed \/ r = "save")
    /\ UNCHANGED <<phase, active, started, now, interlude, invalid, status, History, stalled>>

\* Engine::resume: refused while invalid or while a save has failed.
Resume ==
    /\ state = "Paused" /\ ~invalid /\ ~failed /\ reason # "saving"
    /\ state' = "Running"
    /\ clock' = started
    /\ reason' = "none"
    /\ UNCHANGED <<phase, active, started, now, interlude, invalid, failed, status, History, stalled>>

\* `r` after a failed save (Engine::retry_save, then an acknowledgement).
Retried ==
    /\ failed
    /\ failed' = FALSE
    /\ UNCHANGED <<state, phase, active, started, clock, now, interlude, reason,
                   invalid, status, History, stalled>>

\* Engine::invalidate: Invalidate (table) stops practice for good.
Invalidate ==
    /\ ~invalid /\ state # "Review"
    /\ invalid' = TRUE
    /\ state' = "Paused"
    /\ clock' = FALSE
    /\ reason' = "invalid"
    /\ UNCHANGED <<phase, active, started, now, interlude, failed, status, History, stalled>>

\* Enter/Esc on the interlude (Engine::advance_phase after interlude_settled).
NextPhase ==
    /\ state = "PhaseComplete" /\ ~invalid
    /\ GraceGuard => now - interlude >= Grace
    /\ skipped' = (skipped \/ now - interlude < Grace)
    /\ IF phase < Phases
       THEN /\ state' = "Running" /\ phase' = phase + 1 /\ active' = 0
            /\ UNCHANGED status
       ELSE /\ state' = "Review" /\ status' = "completed"
            /\ UNCHANGED <<phase, active>>
    /\ UNCHANGED <<started, clock, now, interlude, reason, invalid, failed,
                   scoredOutside, endedEarly, stalled, resumedOther, scoredDone>>

\* `e` in the pause controls or Ctrl-C (Engine::end_early): EndEarly (table),
\* refused while invalid or after a failed save.
EndEarly ==
    /\ state \in {"Running", "Paused", "PhaseComplete"} /\ ~invalid /\ ~failed
    /\ state' = "Review"
    /\ clock' = FALSE
    /\ reason' = "none"
    /\ status' = IF status = "completed" THEN status
                 ELSE IF EndEarlyAbandons THEN "abandoned" ELSE "completed"
    /\ endedEarly' = (endedEarly \/ ~scoredDone)
    /\ UNCHANGED <<phase, active, started, now, interlude, invalid, failed,
                   scoredOutside, skipped, stalled, resumedOther, scoredDone>>

\* Engine::watchdog while running: the outstanding checkpoint passed the
\* 2 s bound; SaveStalled pauses with the reason "saving".
Stall ==
    /\ state = "Running" /\ ~stalled
    /\ stalled' = TRUE
    /\ state' = "Paused"
    /\ clock' = IF PauseStopsClock THEN FALSE ELSE clock
    /\ reason' = "saving"
    /\ UNCHANGED <<phase, active, started, now, interlude, invalid, failed, status, History>>

\* Focus loss, a paste or Esc during the stall replaces its reason.
Repause(r) ==
    /\ state = "Paused" /\ reason = "saving"
    /\ reason' = r
    /\ UNCHANGED <<state, phase, active, started, clock, now, interlude, invalid, failed,
                   status, History, stalled>>

\* The stalled checkpoint's success acknowledgement (Engine::save_result):
\* SaveCaughtUp resumes the "saving" pause and nothing else.
CatchUp ==
    /\ stalled /\ state = "Paused" /\ ~invalid /\ ~failed
    /\ CatchUpGuard => reason = "saving"
    /\ stalled' = FALSE
    /\ state' = "Running"
    /\ clock' = started
    /\ resumedOther' = (resumedOther \/ reason # "saving")
    /\ reason' = "none"
    /\ UNCHANGED <<phase, active, started, now, interlude, invalid, failed, status,
                   scoredOutside, skipped, endedEarly, scoredDone>>

\* The acknowledgement arrives when there is no "saving" pause to end.
CatchUpQuiet ==
    /\ stalled /\ ~(state = "Paused" /\ reason = "saving")
    /\ stalled' = FALSE
    /\ UNCHANGED <<state, phase, active, started, clock, now, interlude, reason, invalid,
                   failed, status, History>>

Next ==
    \/ Stall \/ CatchUp \/ CatchUpQuiet
    \/ \E r \in {"requested", "focus", "paste"} : Repause(r)
    \/ Start \/ Type \/ Tick \/ Deadline \/ Resume \/ Retried \/ Invalidate
    \/ NextPhase \/ EndEarly
    \/ \E r \in {"requested", "focus", "paste", "save"} : Pause(r)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
\* Active time accumulates only while practice is running.
ClockStoppedUnlessRunning == clock => state = "Running"

\* No key is scored at or after its phase's deadline.
NoScoreOutsidePhase == ~scoredOutside

\* No key leaves the phase-complete screen within the grace.
InterludeGrace == ~skipped

\* Ending before the last scored (non-review) deadline never yields a
\* completed session.
EndEarlyIsUnfinished == endedEarly => status # "completed"

\* Once the last scored phase reached its deadline the session is completed,
\* and nothing in or after the closing review takes that back (#49).
ReviewNeverUncompletes == scoredDone => status = "completed"

\* A paused session always says why.
PauseKeepsReason == state = "Paused" => reason # "none"

\* A late acknowledgement never ends a pause the learner (or the window)
\* caused for another reason.
CatchUpKeepsOtherPauses == ~resumedOther
=============================================================================
