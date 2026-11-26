import Extracted.CadenceKernel

/-!
# Comparable sessions (`cadence_kernel::comparison`, #77)

`cadence-engine`'s review (the like-for-like session baseline and the
start-screen recap) asks `comparison.same_basis` for each earlier session.

Trusted, not proved: that the engine and the store fill the basis correctly
(the layout revision with `unspecified` read as its adopter, the content
class, the provenance, the headline track set; tested).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Comparison

/-- Never across layouts. -/
theorem never_across_layout (a b : comparison.Basis) (h : a.layout ≠ b.layout) :
    comparison.same_basis a b ⦃ r => r = false ⦄ := by
  unfold comparison.same_basis
  simp [h, WP.spec_ok]

/-- Never with a synthetic session on either side. -/
theorem never_with_synthetic (a b : comparison.Basis)
    (h : a.synthetic = true ∨ b.synthetic = true) :
    comparison.same_basis a b ⦃ r => r = false ⦄ := by
  unfold comparison.same_basis
  rcases h with h | h <;> simp [h, WP.spec_ok] <;> (repeat' split) <;> simp_all [WP.spec_ok]

/-- Never across content classes. -/
theorem never_across_class (a b : comparison.Basis) (h : a.content_class ≠ b.content_class) :
    comparison.same_basis a b ⦃ r => r = false ⦄ := by
  unfold comparison.same_basis
  by_cases hl : a.layout = b.layout <;> simp [hl, h, WP.spec_ok]

/-- Comparable exactly when every condition holds. Total. -/
theorem same_basis_iff_all (a b : comparison.Basis) :
    comparison.same_basis a b ⦃ r => (r = true ↔
      a.layout = b.layout ∧ a.content_class = b.content_class ∧
      a.synthetic = false ∧ b.synthetic = false ∧ a.tracks = b.tracks) ⦄ := by
  unfold comparison.same_basis
  by_cases hl : a.layout = b.layout <;> by_cases hc : a.content_class = b.content_class <;>
    cases ha : a.synthetic <;> cases hb : b.synthetic <;> simp [hl, hc, ha, hb, WP.spec_ok]

end Proofs.Comparison
