import Extracted.CadenceKernel

/-!
# Re-serving slots (`cadence_kernel::slots`, #43)

`engine::learning::Learning::open` offers each queued missed word every slot
of the exercise (review, then broad, then target) and places it in the first
one `slots.substitute` accepts.

Proved: a due review is never replaced, a target slot is never replaced, and
a word already in the exercise is never placed again. Total.

Trusted, not proved: that `open` passes each slot's lane and due flag and
whether the word is present, and replaces each slot at most once (tested in
`crates/engine/tests/learning_evidence.rs`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Slots

theorem substitute_spec (l : slots.SlotLane) (due present : Bool) :
    slots.substitute l due present ⦃ r => (r = true ↔
      (l = .Review ∨ l = .Broad) ∧ due = false ∧ present = false) ⦄ := by
  unfold slots.substitute
  cases l <;> cases due <;> cases present <;> simp [WP.spec_ok]

/-- A due review is never replaced. -/
theorem never_displaces_due (l : slots.SlotLane) (present : Bool) :
    slots.substitute l true present ⦃ r => r = false ⦄ := by
  unfold slots.substitute
  cases l <;> cases present <;> simp_all [WP.spec_ok]

/-- A target slot is never replaced. -/
theorem never_displaces_target (due present : Bool) :
    slots.substitute .Target due present ⦃ r => r = false ⦄ := by
  unfold slots.substitute
  simp [WP.spec_ok]

/-- A word already in the exercise is never placed again. -/
theorem no_duplicate_in_exercise (l : slots.SlotLane) (due : Bool) :
    slots.substitute l due true ⦃ r => r = false ⦄ := by
  unfold slots.substitute
  cases l <;> cases due <;> simp [WP.spec_ok]

end Proofs.Slots
