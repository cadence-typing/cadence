import Extracted.CadenceKernel

/-!
# Block quotas and repetition caps (`cadence_kernel::lanes`, VK-05, #36)

`cadence_trainer::lanes::QuotaTracker` keeps the per-block counts in its maps
and asks `lanes.admissible` (`word_admissible`), `lanes.record` (`record`,
the only writer of the counts) and `lanes.complete` (`quotas_exactly_met`).

Proved: a recorded slot never overruns its lane quota, the per-block word
cap, the consecutive cap or the focus-owned cap; a word the admission rules
refuse is never recorded; a block is complete exactly when every lane met
its quota.

Trusted, not proved: that the tracker passes its stored counts and the
policy caps as the facts and stores the returned counts (tested in
`crates/trainer/src/lanes.rs`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Lanes

theorem u32_succ (x : U32) (h : x.val < U32.max) :
    (core.num.U32.saturating_add x 1#u32).val = x.val + 1 := by
  have h1 : (UScalar.saturating_add x 1#u32).val = min (UScalar.max .U32) (x.val + (1#u32).val) := by
    show (BitVec.ofNat _ (min (UScalar.max .U32) (x.val + (1#u32).val))).toNat = _
    rw [BitVec.toNat_ofNat]
    apply Nat.mod_eq_of_lt
    have := Nat.two_pow_pos (UScalarTy.U32.numBits)
    rw [UScalar.max_def]
    omega
  simp only [core.num.U32.saturating_add, h1]
  simp only [UScalar.max_U32_eq]
  rw [U32.max_eq] at h
  simp
  omega

/-- The word-level admission rules, exactly. Total. -/
theorem admissible_spec (f : lanes.Facts) :
    lanes.admissible f ⦃ a => match a with
      | .Admit => f.occurrences.val < f.word_cap.val ∧
          (f.same_word = true → f.run.val < f.run_cap.val) ∧
          (f.focused = true → f.owned.val < f.focus_cap.val)
      | .WordCap => f.word_cap.val ≤ f.occurrences.val
      | .Consecutive => f.same_word = true ∧ f.run_cap.val ≤ f.run.val
      | .FocusCap => f.focused = true ∧ f.focus_cap.val ≤ f.owned.val
      | .Quota => False ⦄ := by
  unfold lanes.admissible
  split_ifs <;> simp_all [WP.spec_ok] <;> scalar_tac

/-- What a recorded slot's counts are. -/
def recordedOk (f : lanes.Facts) (served occ run owned : U32) : Prop :=
  served.val = f.served.val + 1 ∧ served.val ≤ f.quota.val ∧
  occ.val = f.occurrences.val + 1 ∧ occ.val ≤ f.word_cap.val ∧
  (f.same_word = true → run.val = f.run.val + 1 ∧ run.val ≤ f.run_cap.val) ∧
  (f.same_word = false → run.val = 1) ∧
  (f.focused = true → owned.val = f.owned.val + 1 ∧ owned.val ≤ f.focus_cap.val) ∧
  (f.focused = false → owned = f.owned)

theorem record_spec (f : lanes.Facts) :
    lanes.record f ⦃ r => match r with
      | .Recorded s o ru ow => recordedOk f s o ru ow
      | .Refused a => (a = .Quota ↔ f.quota.val ≤ f.served.val) ∧ a ≠ .Admit ⦄ := by
  unfold lanes.record
  split
  · simp_all [WP.spec_ok]
  · rename_i hq
    apply WP.spec_bind (admissible_spec f)
    intro a ha
    cases a with
    | Admit =>
      obtain ⟨ho, hr, hf⟩ := ha
      have hs := u32_succ f.served (by scalar_tac)
      have hoc := u32_succ f.occurrences (by scalar_tac)
      simp only [lift, bind_ok]
      cases hsw : f.same_word <;> cases hfo : f.focused <;>
        simp_all [WP.spec_ok, recordedOk] <;>
        (try have := u32_succ f.run (by scalar_tac)) <;>
        (try have := u32_succ f.owned (by scalar_tac)) <;>
        simp_all <;> scalar_tac
    | Quota => simp at ha
    | WordCap => simp [WP.spec_ok]; scalar_tac
    | Consecutive => simp [WP.spec_ok]; scalar_tac
    | FocusCap => simp [WP.spec_ok]; scalar_tac

/-- A recorded slot never overruns its lane quota. -/
theorem never_overrun (f : lanes.Facts) :
    lanes.record f ⦃ r => ∀ s o ru ow, r = .Recorded s o ru ow → s.val ≤ f.quota.val ⦄ := by
  apply WP.spec_mono (record_spec f)
  intro r hr s o ru ow he
  subst he
  exact hr.2.1

/-- A recorded slot never takes a word past its per-block cap. -/
theorem word_cap (f : lanes.Facts) :
    lanes.record f ⦃ r => ∀ s o ru ow, r = .Recorded s o ru ow → o.val ≤ f.word_cap.val ⦄ := by
  apply WP.spec_mono (record_spec f)
  intro r hr s o ru ow he
  subst he
  exact hr.2.2.2.1

/-- A focused target slot never takes the focus past its owned-slot cap. -/
theorem focus_cap (f : lanes.Facts) :
    lanes.record f ⦃ r => ∀ s o ru ow, r = .Recorded s o ru ow →
      f.focused = true → ow.val ≤ f.focus_cap.val ⦄ := by
  apply WP.spec_mono (record_spec f)
  intro r hr s o ru ow he hf
  subst he
  exact (hr.2.2.2.2.2.2.1 hf).2

/-- A word repeated in consecutive slots never exceeds the consecutive
cap; a different word starts a run of one. -/
theorem consecutive_cap (f : lanes.Facts) :
    lanes.record f ⦃ r => ∀ s o ru ow, r = .Recorded s o ru ow →
      (f.same_word = true → ru.val ≤ f.run_cap.val) ∧ (f.same_word = false → ru.val = 1) ⦄ := by
  apply WP.spec_mono (record_spec f)
  intro r hr s o ru ow he
  subst he
  exact ⟨fun h => (hr.2.2.2.2.1 h).2, hr.2.2.2.2.2.1⟩

/-- A word the admission rules refuse is never recorded. -/
theorem record_refuses_inadmissible (f : lanes.Facts) :
    lanes.admissible f ⦃ a => a ≠ .Admit →
      lanes.record f ⦃ r => ∃ a', r = .Refused a' ⦄ ⦄ := by
  apply WP.spec_mono (admissible_spec f)
  intro a ha hna
  apply WP.spec_mono (record_spec f)
  intro r hr
  cases r with
  | Refused a' => exact ⟨a', rfl⟩
  | Recorded s o ru ow =>
    exfalso
    obtain ⟨_, _, _, ho, hrun, hrun1, hfoc, _⟩ := hr
    cases a with
    | Admit => exact hna rfl
    | Quota => exact ha
    | WordCap => omega
    | Consecutive => have := hrun ha.1; omega
    | FocusCap => have := hfoc ha.1; omega

/-- A block is complete exactly when every lane met its quota. Total. -/
theorem complete_block_exact (t tq r rq b bq : U32) :
    lanes.complete t tq r rq b bq ⦃ x => (x = true ↔ t = tq ∧ r = rq ∧ b = bq) ⦄ := by
  unfold lanes.complete
  split_ifs <;> simp_all [WP.spec_ok]

/-- A target word may recur exactly when it was not served before, or the
current exercise is at least the minimum gap after its last one (#58).
Total. -/
theorem repeat_gap_spec (served : Bool) (last current gap : U32) :
    lanes.gap_admissible served last current gap ⦃ r =>
      (r = true ↔ served = false ∨ last.val + gap.val ≤ current.val) ⦄ := by
  unfold lanes.gap_admissible
  cases served with
  | false => simp [WP.spec_ok]
  | true =>
    simp only [Bool.not_true, Bool.false_eq_true, ↓reduceIte, lift, bind_ok]
    have h := U32.checked_add_bv_spec last gap
    cases e : U32.checked_add last gap with
    | none =>
      rw [e] at h
      simp only [e, WP.spec_ok]
      simp at h ⊢
      have : current.val ≤ U32.max := by scalar_tac
      omega
    | some t =>
      rw [e] at h
      obtain ⟨_, ht, _⟩ := h
      simp only [e, WP.spec_ok]
      simp only [decide_eq_true_eq, false_or, Bool.true_eq_false]
      constructor <;> intro hh <;> scalar_tac

end Proofs.Lanes
