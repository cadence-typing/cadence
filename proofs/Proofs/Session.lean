import Extracted.CadenceKernel

/-!
# The session lifecycle (`cadence_kernel::session`)

`cadence-engine`'s `Engine::transition` (`crates/engine/src/flow.rs`) is the
only writer of the session state: it asks `session.step` and carries out the
clock command and the status consequence; `Engine::invalidate` asks the same
table. The theorems are over every state and every event.

Trusted, not proved: that the engine reports the right event (a deadline as
`Deadline`/`LastDeadline`, Ctrl-C during practice as `EndEarly`, …) and
carries out the clock command (`TimingSegments::pause`/`resume`; tested), and
the guards the engine keeps outside the table (an invalid measurement or a
failed save blocks `Resume`; the interlude grace blocks `NextPhase`).
`models/SessionFlow.tla` checks those interleavings, bounded.
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Session

/-- The active clock runs only while practice is running: a step that leaves
`Running` stops the clock, and only a step into `Running` resumes it. Total:
`step` returns for every state and event. -/
theorem clock_runs_only_running (s : session.State) (e : session.Event) :
    session.step s e
      ⦃ r => match r with
        | .Move n c _ =>
            (c = session.Clock.Resume → n = session.State.Running) ∧
            (s = session.State.Running → n ≠ session.State.Running → c = session.Clock.Stop)
        | .Refused => True ⦄ := by
  cases s <;> cases e <;> simp [session.step, WP.spec_ok]

/-- Once the review shows (or the session is finishing or saved), no event
leads back to typing: the successors of these states are among them. So no
sequence of steps from the review reaches `Running`, `Paused` or
`PhaseComplete`. -/
theorem review_is_terminal_for_typing (s : session.State) (e : session.Event)
    (h : s = session.State.Review ∨ s = session.State.Finishing ∨ s = session.State.Saved) :
    session.step s e
      ⦃ r => ∀ n c o, r = session.Step.Move n c o →
        n = session.State.Review ∨ n = session.State.Finishing ∨ n = session.State.Saved ⦄ := by
  rcases h with h | h | h <;> subst h <;> cases e <;> simp [session.step, WP.spec_ok]

/-- A session is marked completed only when its last scored (non-review)
phase's deadline passes (`LastDeadline`; a closing review may follow as one
more typing phase, #49) or the plan reaches its end: ending early or exiting
never completes it. -/
theorem completed_only_after_last_phase (s : session.State) (e : session.Event) :
    session.step s e
      ⦃ r => ∀ n c, r = session.Step.Move n c session.Outcome.Completed →
        e = session.Event.LastDeadline ∨ e = session.Event.PlanDone ⦄ := by
  cases s <;> cases e <;> simp [session.step, WP.spec_ok]

/-- Typing starts or continues only by `Start`, `Resume`, `NextPhase` or a
stalled save catching up (#51). -/
theorem running_only_by_start_resume_or_next (s : session.State) (e : session.Event) :
    session.step s e
      ⦃ r => ∀ c o, r = session.Step.Move session.State.Running c o →
        e = session.Event.Start ∨ e = session.Event.Resume ∨ e = session.Event.NextPhase ∨
        e = session.Event.SaveCaughtUp ⦄ := by
  cases s <;> cases e <;> simp [session.step, WP.spec_ok]

/-- An invalid measurement is admitted in every state, so the engine's
`invalidate` never needs a refusal path. -/
theorem invalidate_admitted (s : session.State) :
    session.step s session.Event.Invalidate ⦃ r => r ≠ session.Step.Refused ⦄ := by
  cases s <;> simp [session.step, WP.spec_ok]

/-- A stalled save stops the clock: it is admitted only while running and
moves to `Paused` with `Clock::Stop` (#51). -/
theorem stall_stops_clock (s : session.State) :
    session.step s session.Event.SaveStalled
      ⦃ r => ∀ n c o, r = session.Step.Move n c o →
        s = session.State.Running ∧ n = session.State.Paused ∧ c = session.Clock.Stop ⦄ := by
  cases s <;> simp [session.step, WP.spec_ok]

/-- A stall changes nothing about the session's outcome. -/
theorem stall_changes_no_outcome (s : session.State) :
    session.step s session.Event.SaveStalled
      ⦃ r => ∀ n c o, r = session.Step.Move n c o → o = session.Outcome.Unchanged ⦄ := by
  cases s <;> simp [session.step, WP.spec_ok]

/-- A save catching up resumes typing only from a pause, never from the
review or anything after it. -/
theorem caught_up_only_from_paused (s : session.State) :
    session.step s session.Event.SaveCaughtUp
      ⦃ r => ∀ n c o, r = session.Step.Move n c o →
        s = session.State.Paused ∧ n = session.State.Running ∧ o = session.Outcome.Unchanged ⦄ := by
  cases s <;> simp [session.step, WP.spec_ok]

/-- The closing review keeps a completed session completed (#49): once the
last scored phase completed it, no outcome — ending early during the review,
exiting, another deadline — changes the status. Total. -/
theorem review_phase_keeps_completed (o : session.Outcome) :
    session.settle session.Status.Completed o ⦃ r => r = session.Status.Completed ⦄ := by
  cases o <;> simp [session.settle, WP.spec_ok]

/-- Only a `Completed` outcome completes; ending an open plan abandons it;
nothing reopens a session. -/
theorem settle_spec (st : session.Status) (o : session.Outcome) :
    session.settle st o
      ⦃ r => (r = session.Status.Completed → o = session.Outcome.Completed ∨
                st = session.Status.Completed) ∧
             (st = session.Status.Open → o = session.Outcome.Unfinished →
                r = session.Status.Abandoned) ∧
             (r = session.Status.Open → st = session.Status.Open) ⦄ := by
  cases st <;> cases o <;> simp [session.settle, WP.spec_ok]

/-- Overdue exactly when strictly past the bound. Total. -/
theorem overdue_spec (elapsed bound : U64) :
    session.overdue elapsed bound ⦃ r => (r = true ↔ bound.val < elapsed.val) ⦄ := by
  unfold session.overdue
  simp [WP.spec_ok]

end Proofs.Session
