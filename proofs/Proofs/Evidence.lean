import Extracted.CadenceKernel

/-!
# Learner evidence attribution (`cadence_kernel::evidence`, #42, #41)

`engine::learning::Learning::first_attempt` reads the previous position's
first outcome from the ledger and the current timing-segment ordinal, and
asks `evidence.pair_first_correct` whether the transition into the key was
first-correct and `evidence.clean_interval` whether the gap before it is a
clean timing sample.

Proved: a transition is first-correct exactly when the previous key's first
attempt was correct and this key matched (L-02); the gap is `Timed g`
exactly when there is an anchor at the previous position, the key matched,
no pause separates them (same segment ordinal), and `0 < g ≤ ceiling` with
`g = active - anchor` (L-03); every other answer is given exactly under its
own condition, in order. Total.

Trusted, not proved: that the engine passes the ledger's first outcome of
the previous position and the open segment's ordinal (which rises at every
pause), and sets the anchor only on a matched key (tested in
`engine/tests/learning_evidence.rs`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Evidence

/-- A transition is first-correct exactly when both keys were. Total. -/
theorem pair_correct_iff_both (s : first.Slot) (m : Bool) :
    evidence.pair_first_correct s m ⦃ r => (r = true ↔ s = .Correct ∧ m = true) ⦄ := by
  unfold evidence.pair_first_correct
  cases s <;> cases m <;> simp [WP.spec_ok]

/-- The anchor is the previous position. -/
def adjacent (mk : evidence.Mark) (p : Usize) : Prop := mk.position.val + 1 = p.val

/-- The full interval decision, each answer under its exact condition. -/
def intervalPost (mk : evidence.Mark) (p : Usize) (m : Bool) (a seg ceil : U64)
    (r : evidence.Interval) : Prop :=
  match r with
  | .NoAnchor => mk.present = false
  | .NotAdjacent => mk.present = true ∧ ¬ adjacent mk p
  | .Error => mk.present = true ∧ adjacent mk p ∧ m = false
  | .CrossesPause => mk.present = true ∧ adjacent mk p ∧ m = true ∧ mk.segment ≠ seg
  | .Backward => mk.present = true ∧ adjacent mk p ∧ m = true ∧ mk.segment = seg ∧
      a.val < mk.active_us.val
  | .Unresolved => mk.present = true ∧ adjacent mk p ∧ m = true ∧ mk.segment = seg ∧
      a.val = mk.active_us.val
  | .AboveCeiling => mk.present = true ∧ adjacent mk p ∧ m = true ∧ mk.segment = seg ∧
      ceil.val < a.val - mk.active_us.val ∧ mk.active_us.val < a.val
  | .Timed g => mk.present = true ∧ adjacent mk p ∧ m = true ∧ mk.segment = seg ∧
      mk.active_us.val < a.val ∧ g.val = a.val - mk.active_us.val ∧ g.val ≤ ceil.val

/-- Each answer is given exactly under its condition. Total. -/
theorem interval_reason_exact (mk : evidence.Mark) (p : Usize) (m : Bool) (a seg ceil : U64) :
    evidence.clean_interval mk p m a seg ceil ⦃ r => intervalPost mk p m a seg ceil r ⦄ := by
  unfold evidence.clean_interval
  cases hp : mk.present with
  | false => simp [WP.spec_ok, intervalPost, hp]
  | true =>
    simp only [↓reduceIte, lift, bind_ok]
    have hc := Usize.checked_add_bv_spec mk.position 1#usize
    cases e : Usize.checked_add mk.position 1#usize with
    | none =>
      rw [e] at hc
      simp at hc
      simp only [e, bind_ok, Bool.false_eq_true, ↓reduceIte, WP.spec_ok, intervalPost, hp,
        adjacent, true_and]
      scalar_tac
    | some nx =>
      rw [e] at hc
      simp at hc
      simp only [e, bind_ok]
      by_cases hadj : nx = p
      · have had : adjacent mk p := by unfold adjacent; rw [← hadj]; omega
        simp only [hadj, decide_true, ↓reduceIte]
        cases m with
        | false => simp [WP.spec_ok, intervalPost, hp, had]
        | true =>
          simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte]
          by_cases hs : mk.segment = seg
          · simp only [hs, bne_self_eq_false, Bool.false_eq_true, ↓reduceIte]
            have hd := U64.checked_sub_bv_spec a mk.active_us
            cases e2 : U64.checked_sub a mk.active_us with
            | none =>
              rw [e2] at hd; simp at hd
              simp [lift, e2, WP.spec_ok, intervalPost, hp, had, hs, hd]
            | some g =>
              rw [e2] at hd
              obtain ⟨hle, hg, _⟩ := hd
              simp only [lift, e2, bind_ok]
              split
              · rename_i h0
                simp only [WP.spec_ok, intervalPost, hp, had, hs, true_and]
                scalar_tac
              · rename_i h0
                split
                · simp only [WP.spec_ok, intervalPost, hp, had, hs, true_and]
                  scalar_tac
                · simp only [WP.spec_ok, intervalPost, hp, had, hs, true_and]
                  refine ⟨by scalar_tac, hg, by scalar_tac⟩
          · have hne : (mk.segment != seg) = true := by simp [hs]
            simp [hne, WP.spec_ok, intervalPost, hp, had, hs]
      · have hnad : ¬ adjacent mk p := by
          unfold adjacent; intro h; apply hadj; scalar_tac
        simp [hadj, WP.spec_ok, intervalPost, hp, hnad]

/-- A clean timing sample exactly when the anchor is the previous position,
the key matched, no pause separates them and the gap is positive and within
the ceiling. -/
theorem interval_timed_iff (mk : evidence.Mark) (p : Usize) (m : Bool) (a seg ceil : U64) :
    evidence.clean_interval mk p m a seg ceil ⦃ r =>
      ((∃ g, r = .Timed g) ↔ mk.present = true ∧ adjacent mk p ∧ m = true ∧
        mk.segment = seg ∧ mk.active_us.val < a.val ∧ a.val - mk.active_us.val ≤ ceil.val) ⦄ := by
  apply WP.spec_mono (interval_reason_exact mk p m a seg ceil)
  intro r hr
  cases r
  case Timed g =>
    obtain ⟨h1, h2, h3, h4, h5, h6, h7⟩ := hr
    exact ⟨fun _ => ⟨h1, h2, h3, h4, h5, by omega⟩, fun _ => ⟨g, rfl⟩⟩
  all_goals
    refine ⟨fun ⟨g, hg⟩ => (by cases hg), fun ⟨h1, h2, h3, h4, h5, h6⟩ => ?_⟩
    simp only [intervalPost] at hr
  · simp_all
  · exact (hr.2 h2).elim
  · simp_all
  · exact (hr.2.2.2 h4).elim
  · omega
  · omega
  · omega

end Proofs.Evidence
