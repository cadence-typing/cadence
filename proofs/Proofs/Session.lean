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

/-- A session is marked completed only when its last typing phase's deadline
passes or the plan reaches its review: ending early or exiting never
completes it. -/
theorem completed_only_after_last_phase (s : session.State) (e : session.Event) :
    session.step s e
      ⦃ r => ∀ n c, r = session.Step.Move n c session.Outcome.Completed →
        e = session.Event.LastDeadline ∨ e = session.Event.PlanDone ⦄ := by
  cases s <;> cases e <;> simp [session.step, WP.spec_ok]

/-- Typing starts or continues only by `Start`, `Resume` or `NextPhase`. -/
theorem running_only_by_start_resume_or_next (s : session.State) (e : session.Event) :
    session.step s e
      ⦃ r => ∀ c o, r = session.Step.Move session.State.Running c o →
        e = session.Event.Start ∨ e = session.Event.Resume ∨ e = session.Event.NextPhase ⦄ := by
  cases s <;> cases e <;> simp [session.step, WP.spec_ok]

/-- An invalid measurement is admitted in every state, so the engine's
`invalidate` never needs a refusal path. -/
theorem invalidate_admitted (s : session.State) :
    session.step s session.Event.Invalidate ⦃ r => r ≠ session.Step.Refused ⦄ := by
  cases s <;> simp [session.step, WP.spec_ok]

end Proofs.Session
