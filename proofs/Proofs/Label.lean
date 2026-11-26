import Extracted.CadenceKernel
import Proofs.Time

/-!
# Item labels and list cycling (`cadence_kernel::label`, `cadence_kernel::cycle`, #39)

`engine::presentation::exercise_context` labels each slot with `label.of`
("due review", "again: word"); `trainer::plan_content` asks it whether a prose
passage is a repeat; `engine::practice::finish_exercise` advances through a
phase's prepared list with `cycle.advance` and keeps its `wrapped` flag.

Proved: an item is `Fresh` exactly when it was not served before, is not a
reinforcement or a due review, its list has not wrapped and its passage is
outside the no-repeat window; it is `Again` exactly when it is a
reinforcement; the list wraps exactly at its last index, and the next index
is always inside the list. Total.

Trusted, not proved: that the callers pass the right facts (tested: the
caption tests in `tui/tests/render_path.rs` and `engine/tests`, and
`the_list_wraps_exactly_after_its_last_exercise`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Label

/-- `Fresh` exactly when nothing makes the item a repetition. Total. -/
theorem fresh_iff_unseen (f : label.Facts) :
    label.of f ⦃ l => (l = .Fresh ↔ f.served_before.val = 0 ∧ f.reinforcement = false ∧
      f.due_review = false ∧ f.wrapped = false ∧ f.recent_passage = false) ⦄ := by
  unfold label.of
  split_ifs <;> simp_all [WP.spec_ok] <;> scalar_tac

/-- `Again` exactly for a reinforcement. Total. -/
theorem again_is_reinforcement (f : label.Facts) :
    label.of f ⦃ l => (l = .Again ↔ f.reinforcement = true) ⦄ := by
  unfold label.of
  split_ifs <;> simp_all [WP.spec_ok]

/-- The list wraps exactly at its last index, and the next index is inside
a non-empty list. Total. -/
theorem wrap_exact (index count : Usize) (h : index.val < count.val) :
    cycle.advance index count ⦃ a => (a.wrapped = true ↔ index.val + 1 = count.val) ∧
      a.next.val < count.val ⦄ := by
  unfold cycle.advance
  have hs := Proofs.Time.usize_succ index (by scalar_tac)
  simp only [lift, bind_ok]
  split
  · rename_i hge
    simp only [WP.spec_ok, true_iff]
    have : count.val ≤ index.val + 1 := by scalar_tac
    refine ⟨by omega, by simp; omega⟩
  · rename_i hlt
    simp only [WP.spec_ok, Bool.false_eq_true, false_iff]
    have : index.val + 1 < count.val := by scalar_tac
    refine ⟨by omega, by rw [hs]; omega⟩

/-- `advance` never fails. -/
theorem advance_total (index count : Usize) : cycle.advance index count ⦃ _ => True ⦄ := by
  unfold cycle.advance
  simp only [lift, bind_ok]
  split <;> simp [WP.spec_ok]

end Proofs.Label
