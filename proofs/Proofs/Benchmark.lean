import Extracted.CadenceKernel

/-!
# Benchmark probes (`cadence_kernel::benchmark`, #63)

`cadence-trainer`'s `plan::probe_due` calls `benchmark.due` with the number
of completed sessions with prose since the last probe, the policy interval
and whether an unseen probe passage is available. `benchmark.comparable`
decides whether a probe result may be compared with earlier ones (#65).

Trusted, not proved: that the callers count completed sessions and report
the probe's facts correctly (tested where they are computed).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Benchmark

/-- A probe is due exactly when at least `every` completed sessions passed
and a passage is available; when none is, the answer is `NoPassage`, never
a silent `NotYet`. Total. -/
theorem due_exact (c every : U64) (unseen : Bool) :
    benchmark.due c every unseen ⦃ r =>
      (r = .Probe ↔ every.val ≤ c.val ∧ unseen = true) ∧
      (r = .NotYet ↔ c.val < every.val) ∧
      (r = .NoPassage ↔ every.val ≤ c.val ∧ unseen = false) ⦄ := by
  unfold benchmark.due
  split
  · rename_i h
    have : c.val < every.val := by scalar_tac
    cases unseen <;> simp [WP.spec_ok, this]
  · rename_i h
    have : every.val ≤ c.val := by scalar_tac
    cases unseen <;> simp [WP.spec_ok, this]

/-- A probe that was paused, ended early, pasted into, or cut short is
never comparable. -/
theorem interrupted_never_comparable (f : benchmark.Facts)
    (h : f.paused = true ∨ f.ended_early = true ∨ f.pasted = true ∨ f.completed_window = false) :
    benchmark.comparable f ⦃ r => r = false ⦄ := by
  unfold benchmark.comparable
  rcases h with h | h | h | h <;> simp [h, WP.spec_ok] <;> (repeat' split) <;> simp_all [WP.spec_ok]

/-- A probe typed in a synthetic (headless) session is never comparable. -/
theorem synthetic_never_comparable (f : benchmark.Facts) (h : f.synthetic = true) :
    benchmark.comparable f ⦃ r => r = false ⦄ := by
  unfold benchmark.comparable
  simp [h, WP.spec_ok] <;> (repeat' split) <;> simp_all [WP.spec_ok]

/-- Comparable exactly when every condition holds. Total. -/
theorem comparable_iff_all (f : benchmark.Facts) :
    benchmark.comparable f ⦃ r => (r = true ↔
      f.completed_window = true ∧ f.paused = false ∧ f.ended_early = false ∧
      f.pasted = false ∧ f.synthetic = false ∧ f.same_layout = true ∧
      f.same_strata = true) ⦄ := by
  unfold benchmark.comparable
  cases hc : f.completed_window <;> cases hp : f.paused <;> cases he : f.ended_early <;>
    cases hx : f.pasted <;> cases hs : f.synthetic <;> cases hl : f.same_layout <;>
    cases ht : f.same_strata <;> simp [hc, hp, he, hx, hs, hl, ht, WP.spec_ok]

end Proofs.Benchmark
