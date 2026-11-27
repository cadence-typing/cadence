import Extracted.CadenceKernel
import Proofs.TautenMachine

/-!
# The save protocol as a kernel machine (`cadence_kernel::save::step`, #89)

`cadence_engine::storage` is the driver: it holds the checkpoint payloads
and timestamps, measures the watchdogs and carries out the commands. Every
flag and sequence of the single-flight protocol is the machine's, so what
is proved here holds in every reachable state (`Tauten.Machine.rinv_of_step`),
not only to `models/CheckpointSave.tla`'s bound.

Trusted, not proved: that the driver sends exactly the events named in
`save.rs` at the points they describe (an acknowledgement carries the
store's sequence; `Flush` is reached after every reduce), and that it
dispatches the payload for the sequence a `Dispatch` names (tested in
`engine/tests/plan_session.rs` and `save_machine.rs`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel
open Tauten.Machine

namespace Proofs.Save

/-- The inductive invariant: nothing in flight while failed, and the
acknowledged sequence never ahead of the dispatched one, strictly behind it
while a checkpoint is in flight or failed. -/
def Inv (s : save.State) : Prop :=
  (s.pending = true → s.failed = false) ∧
  s.last_ack.val ≤ s.sequence.val ∧
  (s.pending = true → s.last_ack.val < s.sequence.val) ∧
  (s.failed = true → s.last_ack.val < s.sequence.val)

/-- The machine as extracted. -/
def machine : RMachine save.State save.Event save.Command := ⟨save.step⟩

/-- Its initial state. -/
def init (s : save.State) : Prop := save.initial = ok s

theorem init_inv (s : save.State) (h : init s) : Inv s := by
  unfold init save.initial at h
  simp only [ok.injEq] at h
  subst h
  simp [Inv]

/-- What one step guarantees from any state satisfying the invariant: the
invariant again, no acknowledgement or sequence going backwards, a dispatch
only with nothing in flight or failed and for the next sequence, and a
successful quit only once the final checkpoint is acknowledged. Total. -/
theorem step_spec (s : save.State) (e : save.Event) (h : Inv s) :
    save.step s e ⦃ r =>
      Inv r.1 ∧ s.last_ack.val ≤ r.1.last_ack.val ∧ s.sequence.val ≤ r.1.sequence.val ∧
      (∀ n, r.2 = save.Command.Dispatch n →
        s.pending = false ∧ s.failed = false ∧ n.val = s.sequence.val + 1 ∧
        r.1.pending = true) ∧
      (r.2 = save.Command.Quit true →
        r.1.last_ack = r.1.sequence ∧ r.1.pending = false ∧ r.1.failed = false ∧
        r.1.finishing = true) ⦄ := by
  obtain ⟨sq, la, p, f, d, fin, st⟩ := s
  obtain ⟨h1, h2, h3, h4⟩ := h
  simp only at h1 h2 h3 h4
  cases e with
  | Flush =>
    simp only [save.step, save.flush]
    cases d <;> cases p <;> cases f <;>
      simp_all [WP.spec_ok, Inv, lift, bind_ok]
    cases ho : U64.checked_add sq 1#u64 with
    | none => simp [WP.spec_ok, h2]
    | some n =>
      have := U64.checked_add_bv_spec sq 1#u64
      rw [ho] at this
      have hn : n.val = sq.val + 1 := by
        have h' := this.2.1
        scalar_tac
      simp only [WP.spec_ok, reduceCtorEq, false_implies, and_true, save.Command.Dispatch.injEq,
        forall_eq', implies_true, true_and, and_self, hn]
      omega
  | Acked q =>
    simp only [save.step, save.acked]
    cases p
    · simp_all [WP.spec_ok, Inv]
    · by_cases hq : q = sq
      · subst hq
        have hlt := h3 rfl
        cases fin <;> cases d <;> cases st <;>
          simp_all [WP.spec_ok, Inv]
      · have : (q != sq) = true := by simpa using hq
        simp_all [WP.spec_ok, Inv]
  | AckFailed q =>
    simp only [save.step, save.ack_failed]
    cases p
    · simp_all [WP.spec_ok, Inv]
    · by_cases hq : q = sq
      · subst hq
        have hlt := h3 rfl
        simp_all [WP.spec_ok, Inv]
      · have : (q != sq) = true := by simpa using hq
        simp_all [WP.spec_ok, Inv]
  | Due | Retry | ExitRequested | FinalTimeout | Stalled =>
    simp only [save.step, save.exit_requested]
    cases p <;> cases f <;> cases fin <;> cases st <;>
      simp_all [WP.spec_ok, Inv]

/-- The invariant holds in every reachable state. -/
theorem reachable_inv (s : save.State) (h : RReachable machine init s) : Inv s :=
  rinv_of_step machine init Inv init_inv
    (fun s e hs => WP.spec_mono (step_spec s e hs) (fun _ hr => hr.1)) s h

/-- Single flight: in every reachable state a checkpoint is never in flight
while one has failed, and a step dispatches only with nothing in flight or
failed, and for the next sequence. -/
theorem single_flight (s : save.State) (h : RReachable machine init s) (e : save.Event) :
    (s.pending = true → s.failed = false) ∧
    save.step s e ⦃ r => ∀ n, r.2 = save.Command.Dispatch n →
      s.pending = false ∧ s.failed = false ∧ n.val = s.sequence.val + 1 ∧
      r.1.pending = true ⦄ :=
  ⟨(reachable_inv s h).1,
    WP.spec_mono (step_spec s e (reachable_inv s h)) (fun _ hr => hr.2.2.2.1)⟩

/-- Acknowledgements are monotone: no step lowers the acknowledged sequence,
and in every reachable state it is at most the dispatched one. -/
theorem ack_monotone (s : save.State) (h : RReachable machine init s) (e : save.Event) :
    s.last_ack.val ≤ s.sequence.val ∧
    save.step s e ⦃ r => s.last_ack.val ≤ r.1.last_ack.val ⦄ :=
  ⟨(reachable_inv s h).2.1,
    WP.spec_mono (step_spec s e (reachable_inv s h)) (fun _ hr => hr.2.1)⟩

/-- A failed tail is named: in every reachable failed state the failed
sequence is beyond the last acknowledged one. -/
theorem failed_tail_named (s : save.State) (h : RReachable machine init s)
    (hf : s.failed = true) : s.last_ack.val < s.sequence.val :=
  (reachable_inv s h).2.2.2 hf

/-- A clean exit needs the final acknowledgement: a successful quit leaves
the newest dispatched checkpoint acknowledged, nothing in flight and
nothing failed. -/
theorem clean_exit_needs_final_ack (s : save.State) (h : RReachable machine init s)
    (e : save.Event) :
    save.step s e ⦃ r => r.2 = save.Command.Quit true →
      r.1.last_ack = r.1.sequence ∧ r.1.pending = false ∧ r.1.failed = false ∧
      r.1.finishing = true ⦄ :=
  WP.spec_mono (step_spec s e (reachable_inv s h)) (fun _ hr => hr.2.2.2.2)

end Proofs.Save
