import Extracted.CadenceKernel

/-!
# Same-session reinforcement admission (`cadence_kernel::queue`, VK-06, #55)

`cadence_engine::learning::Learning::enqueue` computes whether the missed
word is already queued, the queue length and capacity, and the word's
repetitions so far, and asks `queue.admit`.

Proved: an admitted word is not already queued, the queue is below capacity
(so it stays within capacity after the push), and the word is below its
per-session limit. Total.

Trusted, not proved: that `enqueue` pushes exactly once on `Admitted` and
counts the repetition (tested in `crates/engine/tests/learning_evidence.rs`,
one reducer test per refusal).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Queue

theorem admit_spec (q : Bool) (len cap : Usize) (served lim : U32) :
    queue.admit q len cap served lim ⦃ a => match a with
      | .Admitted => q = false ∧ len.val < cap.val ∧ served.val < lim.val
      | .Duplicate => q = true
      | .Full => cap.val ≤ len.val
      | .Exhausted => lim.val ≤ served.val ⦄ := by
  unfold queue.admit
  split_ifs <;> simp_all [WP.spec_ok] <;> scalar_tac

/-- An admitted word is not already queued. -/
theorem admit_no_duplicate (q : Bool) (len cap : Usize) (served lim : U32) :
    queue.admit q len cap served lim ⦃ a => a = .Admitted → q = false ⦄ := by
  apply WP.spec_mono (admit_spec q len cap served lim)
  intro a ha he
  subst he
  exact ha.1

/-- An admitted word leaves the queue within capacity. -/
theorem admit_bounded (q : Bool) (len cap : Usize) (served lim : U32) :
    queue.admit q len cap served lim ⦃ a => a = .Admitted → len.val + 1 ≤ cap.val ⦄ := by
  apply WP.spec_mono (admit_spec q len cap served lim)
  intro a ha he
  subst he
  have := ha.2.1
  omega

/-- No word is repeated more than its per-session limit. -/
theorem per_word_bound (q : Bool) (len cap : Usize) (served lim : U32) :
    queue.admit q len cap served lim ⦃ a => a = .Admitted → served.val + 1 ≤ lim.val ⦄ := by
  apply WP.spec_mono (admit_spec q len cap served lim)
  intro a ha he
  subst he
  have := ha.2.2
  omega

end Proofs.Queue
