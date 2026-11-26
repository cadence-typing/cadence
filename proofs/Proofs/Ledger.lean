import Extracted.CadenceKernel
import Proofs.First
import Proofs.Counters

/-!
# One attempt-ledger step (`cadence_kernel::ledger`)

`cadence-core`'s `AttemptLedger::apply` asks `ledger.insert` whether a
content key is admitted under the buffer bound, whether it lands inside the
target and what the counters become; it writes the first outcome with
`ledger.settle_at` and updates the word's flags with `ledger.flags_after_*`.
`AttemptLedger::is_complete` is `ledger.complete` and a word's cleanliness is
`ledger.clean`.

Trusted, not proved: that the adapter passes the buffer length as the
position, `same` as "equals the target unit there", the number of matching
positions to `complete`, and the word index of the position or tail (tested:
`crates/core/tests/ledger_properties.rs`, the scoring fixtures). Backspace and
delete-word take no slot vector, so they cannot change a first outcome; their
counter step is `counters.deletion_step` (proved in `Counters`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Ledger

theorem set_opt_none {α} (l : List α) (i : Nat) : l.set_opt i none = l := by
  induction l generalizing i with
  | nil => simp
  | cons x tl ih => cases i <;> simp [List.set_opt, ih]

theorem set_opt_some {α} (l : List α) (i : Nat) (x : α) : l.set_opt i (some x) = l.set i x := by
  induction l generalizing i with
  | nil => simp
  | cons y tl ih => cases i <;> simp [List.set_opt, ih]

/-- First outcomes are write-once over the whole slot vector: the length is
kept, no slot other than `position` changes, and a slot that is already
settled does not change at all, whatever was typed. -/
theorem first_write_once (slots : Slice first.Slot) (p : Usize) (m : Bool) :
    ledger.settle_at slots p m
      ⦃ s' => s'.val.length = slots.val.length ∧
        (∀ (j : Nat), j ≠ p.val → s'.val[j]? = slots.val[j]?) ∧
        (∀ (j : Nat), slots.val[j]? ≠ some first.Slot.Open → s'.val[j]? = slots.val[j]?) ⦄ := by
  unfold ledger.settle_at
  simp only [core.slice.Slice.get_mut, core.slice.index.Usize.get_mut, bind_ok]
  cases h : slots[p]? with
  | none =>
    simp [WP.spec_ok, Slice.set_opt_val_eq, set_opt_none]
  | some slot =>
    simp
    apply WP.spec_bind (Proofs.First.settle_spec slot m)
    intro r hr
    have hp : slots.val[p.val]? = some slot := by simpa using h
    simp only [WP.spec_ok, Slice.set_opt_val_eq, set_opt_some, List.length_set]
    refine ⟨trivial, fun j hj => List.getElem?_set_ne (Ne.symm hj), ?_⟩
    intro j hj
    by_cases hjp : j = p.val
    · subst hjp
      rw [hp] at hj ⊢
      have hlt : p.val < slots.val.length := by
        rcases (List.getElem?_eq_some_iff.mp hp) with ⟨hl, _⟩; exact hl
      rw [List.getElem?_set_self hlt]
      cases slot <;> simp_all
    · exact List.getElem?_set_ne (Ne.symm hjp)

/-- A content key is admitted only below the bound `target + allowance`, so
the buffer never exceeds it; it lands inside the target exactly when the
buffer is shorter than the target, and only a key inside the target can
match. Total. -/
theorem insert_spec (c : counters.Counters) (b t a : Usize) (same : Bool) :
    ledger.insert c b t a same
      ⦃ r => match r with
        | .Rejected => t.val + a.val ≤ b.val ∨ Usize.max < t.val + a.val
        | .Overflow => b.val < t.val + a.val
        | .Counted _ m inT => b.val + 1 ≤ t.val + a.val ∧ (inT = true ↔ b.val < t.val) ∧
            (m = true → inT = true) ⦄ := by
  unfold ledger.insert
  have hs := Usize.checked_add_bv_spec t a
  cases e : Usize.checked_add t a with
  | none =>
    rw [e] at hs
    simp only [lift, bind_ok, WP.spec_ok]
    simp at hs
    omega
  | some bound =>
    rw [e] at hs
    simp at hs
    simp only [lift, bind_ok]
    split
    · rename_i hge
      simp only [WP.spec_ok]
      left
      have : bound.val ≤ b.val := by scalar_tac
      omega
    · rename_i hlt
      have hb : b.val < bound.val := by scalar_tac
      apply WP.spec_bind (Proofs.Counters.content_step_spec c (decide (b < t)) same)
      intro s _
      cases s with
      | Overflow => simp only [WP.spec_ok]; omega
      | Counted c' =>
        simp only
        apply WP.spec_bind (Proofs.Counters.matched_spec (decide (b < t)) same)
        intro r hr
        subst hr
        simp only [WP.spec_ok]
        refine ⟨by omega, ?_, ?_⟩
        · simp [UScalar.lt_equiv]
        · intro hm; simp_all

/-- A key at or past the bound is refused: nothing is inserted or counted. -/
theorem over_bound_rejects (c : counters.Counters) (b t a : Usize) (same : Bool)
    (h : t.val + a.val ≤ b.val) :
    ledger.insert c b t a same ⦃ r => r = ledger.Insert.Rejected ⦄ := by
  apply WP.spec_mono (insert_spec c b t a same)
  intro r hr
  cases r with
  | Rejected => rfl
  | Overflow => simp only at hr; omega
  | Counted _ _ _ => simp only at hr; omega

/-- Every word flag is sticky: no step clears a flag that is set, and an
unmatched key always marks an error. -/
theorem word_flag_sticky (f : ledger.WordFlags) (m : Bool) :
    ledger.flags_after_insert f m ⦃ g =>
      (f.any_error = true → g.any_error = true) ∧ (m = false → g.any_error = true) ∧
      g.any_deletion = f.any_deletion ∧ g.interrupted = f.interrupted ∧ g.pasted = f.pasted ⦄ ∧
    ledger.flags_after_deletion f ⦃ g =>
      g.any_error = f.any_error ∧ g.any_deletion = true ∧
      g.interrupted = f.interrupted ∧ g.pasted = f.pasted ⦄ ∧
    ledger.flags_after_interrupt f ⦃ g =>
      g.any_error = f.any_error ∧ g.any_deletion = f.any_deletion ∧
      g.interrupted = true ∧ g.pasted = f.pasted ⦄ := by
  cases f with
  | mk e d i p =>
    cases e <;> cases d <;> cases i <;> cases p <;> cases m <;>
      simp [ledger.flags_after_insert, ledger.flags_after_deletion,
        ledger.flags_after_interrupt, WP.spec_ok]

/-- A word is clean exactly when no flag is set. Total. -/
theorem clean_spec (f : ledger.WordFlags) :
    ledger.clean f ⦃ r => (r = true ↔ f.any_error = false ∧ f.any_deletion = false ∧
      f.interrupted = false ∧ f.pasted = false) ⦄ := by
  unfold ledger.clean
  cases f with
  | mk e d i p => cases e <;> cases d <;> cases i <;> cases p <;> simp [WP.spec_ok]

/-- The buffer is complete exactly when it has the target's length and every
position matches; a matching prefix with extra input is not complete. Total. -/
theorem complete_iff_exact (b t v : Usize) :
    ledger.complete b t v ⦃ r => (r = true ↔ b.val = t.val ∧ v.val = t.val) ⦄ := by
  unfold ledger.complete
  split <;> simp_all [WP.spec_ok, UScalar.eq_equiv]

end Proofs.Ledger
