import Extracted.CadenceKernel
import Proofs.Time

/-!
# First-attempt outcomes (`cadence_kernel::first`)

`cadence-core`'s `AttemptLedger` stores one `first.Slot` per expected
position, updates it only through `first.settle`, and reads
`first_correct()` / `first_attempted()` from `first.count_first`.

The product's core honesty rule (AGENTS.md rule 1): a corrected mistake
still counts as a first-attempt error. `settle_write_once` says a settled
slot is never changed by any later insertion, correct or not, so retyping a
position correctly after Backspace cannot turn `Incorrect` into `Correct`.

Trusted, not proved: that the ledger calls `settle` for every content
insertion and never writes a slot any other way (tested by
`first_outcomes_are_write_once` and `correction_cannot_erase_first_errors`
in `crates/core/tests/ledger_properties.rs`), and that Backspace does not
touch the slot list.
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.First

/-- The slot a content insertion leaves: an open slot takes the outcome, a
settled one is unchanged, whatever `matched` says. -/
theorem settle_spec (s : first.Slot) (m : Bool) :
    first.settle s m
      ⦃ r => r = match s with
        | .Open => (if m then first.Slot.Correct else first.Slot.Incorrect)
        | .Correct => first.Slot.Correct
        | .Incorrect => first.Slot.Incorrect ⦄ := by
  unfold first.settle
  cases s <;> cases m <;> simp

/-- Write-once: a settled slot is returned unchanged for every later
insertion. -/
theorem settle_write_once (s : first.Slot) (h : s ≠ first.Slot.Open) (m : Bool) :
    first.settle s m ⦃ r => r = s ⦄ := by
  apply WP.spec_mono (settle_spec s m)
  intro r hr
  subst hr
  cases s <;> simp_all

/-- A corrected mistake still counts as a first-attempt error: after a
mismatching first insertion the slot is `Incorrect`, and no later insertion,
including a correct retype, changes it. -/
theorem mistake_stays_an_error (later : Bool) :
    first.settle first.Slot.Open false
      ⦃ s => s = first.Slot.Incorrect ∧
        first.settle s later ⦃ r => r = first.Slot.Incorrect ⦄ ⦄ := by
  unfold first.settle
  simp

/-- A correct first insertion is `Correct`, and stays so. -/
theorem first_match_is_kept (later : Bool) :
    first.settle first.Slot.Open true
      ⦃ s => s = first.Slot.Correct ∧
        first.settle s later ⦃ r => r = first.Slot.Correct ⦄ ⦄ := by
  unfold first.settle
  simp

/-- Whether a slot is a correct first outcome. -/
def isC : first.Slot → Bool
  | .Correct => true
  | _ => false

/-- Whether a slot has a first outcome at all. -/
def isA : first.Slot → Bool
  | .Open => false
  | _ => true

/-- Number of first-correct slots. -/
def cntC (l : List first.Slot) : Nat := l.countP isC

/-- Number of first-attempted slots. -/
def cntA (l : List first.Slot) : Nat := l.countP isA

theorem cntC_cons (x : first.Slot) (l : List first.Slot) :
    cntC (x :: l) = cntC l + (if isC x then 1 else 0) := by
  simp [cntC, List.countP_cons]

theorem cntA_cons (x : first.Slot) (l : List first.Slot) :
    cntA (x :: l) = cntA l + (if isA x then 1 else 0) := by
  simp [cntA, List.countP_cons]

theorem cnt_le (l : List first.Slot) : cntC l ≤ cntA l ∧ cntA l ≤ l.length := by
  constructor
  · apply List.countP_mono_left
    intro x _ hx
    cases x <;> simp_all [isC, isA]
  · exact List.countP_le_length

theorem count_first_loop_spec (slots : Slice first.Slot) (i correct attempted : Usize)
    (hi : i.val ≤ slots.val.length)
    (hc : correct.val + (slots.val.length - i.val) ≤ Usize.max)
    (ha : attempted.val + (slots.val.length - i.val) ≤ Usize.max) :
    first.count_first_loop_loop slots i correct attempted
      ⦃ r => r.1.val = correct.val + cntC (slots.val.drop i.val) ∧
             r.2.val = attempted.val + cntA (slots.val.drop i.val) ⦄ := by
  unfold first.count_first_loop_loop
  have hlen : (Slice.len slots).val = slots.val.length := Slice.len_val slots
  apply loop.spec_decr_nat
    (fun s : Usize × Usize × Usize => slots.val.length - s.1.val)
    (fun s : Usize × Usize × Usize => s.1.val ≤ slots.val.length ∧
      s.2.1.val + (slots.val.length - s.1.val) ≤ Usize.max ∧
      s.2.2.val + (slots.val.length - s.1.val) ≤ Usize.max ∧
      s.2.1.val + cntC (slots.val.drop s.1.val) = correct.val + cntC (slots.val.drop i.val) ∧
      s.2.2.val + cntA (slots.val.drop s.1.val) = attempted.val + cntA (slots.val.drop i.val))
  · rintro ⟨j, c, a⟩ ⟨hj, hcj, haj, hic, hia⟩
    simp only at hj hcj haj hic hia
    unfold first.count_first_loop_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < slots.val.length := by scalar_tac
      have hget : slots.val[j.val]? = some slots.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
      have hdrop : slots.val.drop j.val = slots.val[j.val] :: slots.val.drop (j.val + 1) :=
        List.drop_eq_getElem_cons hlt'
      have hs := Proofs.Time.usize_succ j (by scalar_tac)
      rw [hdrop, cntC_cons] at hic
      rw [hdrop, cntA_cons] at hia
      have hc1 := Proofs.Time.usize_succ c (by scalar_tac)
      have ha1 := Proofs.Time.usize_succ a (by scalar_tac)
      cases hx : slots.val[j.val] with
      | Open =>
        rw [hx] at hic hia
        simp only [isC, isA] at hic hia
        simp only [lift, bind_tc_ok, bind_ok, hs, hc1, ha1]
        show ok (ControlFlow.cont (core.num.Usize.saturating_add j 1#usize, c, a)) ⦃ _ ⦄
        simp only [WP.spec_ok, hs, hc1, ha1]
        simp at hic hia
        refine ⟨⟨by scalar_tac, by scalar_tac, by scalar_tac, by omega, by omega⟩, by scalar_tac⟩
      | Correct =>
        rw [hx] at hic hia
        simp only [isC, isA] at hic hia
        simp only [lift, bind_tc_ok, bind_ok, hs, hc1, ha1]
        show ok (ControlFlow.cont (core.num.Usize.saturating_add j 1#usize, core.num.Usize.saturating_add c 1#usize, core.num.Usize.saturating_add a 1#usize)) ⦃ _ ⦄
        simp only [WP.spec_ok, hs, hc1, ha1]
        simp at hic hia
        refine ⟨⟨by scalar_tac, by scalar_tac, by scalar_tac, by omega, by omega⟩, by scalar_tac⟩
      | Incorrect =>
        rw [hx] at hic hia
        simp only [isC, isA] at hic hia
        simp only [lift, bind_tc_ok, bind_ok, hs, hc1, ha1]
        show ok (ControlFlow.cont (core.num.Usize.saturating_add j 1#usize, c, core.num.Usize.saturating_add a 1#usize)) ⦃ _ ⦄
        simp only [WP.spec_ok, hs, hc1, ha1]
        simp at hic hia
        refine ⟨⟨by scalar_tac, by scalar_tac, by scalar_tac, by omega, by omega⟩, by scalar_tac⟩
    · rename_i hge
      have hd : slots.val.drop j.val = [] := List.drop_eq_nil_of_le (by scalar_tac)
      simp only [WP.spec_ok]
      rw [hd] at hic hia
      simp [cntC, cntA] at hic hia ⊢
      omega
  · exact ⟨hi, hc, ha, rfl, rfl⟩

/-- `count_first` counts exactly the correct and the attempted slots, never
fails, and `first_correct ≤ first_attempted ≤ length`: the numerator of
first-pass accuracy never exceeds its denominator. -/
theorem count_first_spec (slots : Slice first.Slot) :
    first.count_first slots
      ⦃ r => r.correct.val = cntC slots.val ∧ r.attempted.val = cntA slots.val ∧
             r.correct.val ≤ r.attempted.val ∧ r.attempted.val ≤ slots.val.length ⦄ := by
  unfold first.count_first first.count_first_loop
  have hlen : slots.val.length ≤ Usize.max := by
    have := Slice.len_val slots
    scalar_tac
  have := count_first_loop_spec slots 0#usize 0#usize 0#usize (by simp) (by simp)
    (by simp)
  apply WP.spec_bind this
  rintro ⟨pc, pa⟩ ⟨hp1, hp2⟩
  show ok ({ correct := pc, attempted := pa } : first.FirstCounts) ⦃ _ ⦄
  simp only [WP.spec_ok]
  have h := cnt_le slots.val
  simp only [Nat.zero_add, List.drop_zero] at hp1 hp2
  refine ⟨?_, ?_, ?_, ?_⟩ <;> simp_all <;> omega

end Proofs.First
