import Extracted.CadenceKernel

/-!
# Attempt counters (`cadence_kernel::counters`)

`cadence-core`'s `AttemptLedger::apply` calls `counters.content_step` for
every content insertion and `counters.deletion_step` for every deletion.
The theorems say what each counter becomes, that an insertion past the
target is never counted correct, that a mistyped unit still counts as an
attempt (the denominator of first-pass and attempt accuracy), and that the
invariant `correct + overflow ≤ content` (the one `SessionProgress::validate`
requires of persisted counters) is preserved and the step cannot overflow
below the `u64` limit.

Trusted, not proved: that the ledger passes the real "target has a unit at
this position" and "typed unit equals it" facts (tested).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Counters

/-- The invariant persisted counters must satisfy. -/
def Inv (c : counters.Counters) : Prop :=
  c.correct.val + c.overflow.val ≤ c.content.val

theorem matched_spec (ep same : Bool) :
    counters.matched ep same ⦃ r => r = (ep && same) ⦄ := by
  unfold counters.matched
  cases ep <;> simp

/-- An insertion past the target (no expected unit) never matches. -/
theorem overflow_never_matches (same : Bool) :
    counters.matched false same ⦃ r => r = false ⦄ := by
  apply WP.spec_mono (matched_spec false same)
  intro r hr
  simpa using hr

/-- The body of `content_step` for given increments, as extracted. -/
def blk (c : counters.Counters) (ac ao : U64) : Result counters.Step := do
  let o ← lift (U64.checked_add c.content 1#u64)
  match o with
  | none => ok counters.Step.Overflow
  | some content =>
    let o1 ← lift (U64.checked_add c.correct ac)
    match o1 with
    | none => ok counters.Step.Overflow
    | some correct =>
      let o2 ← lift (U64.checked_add c.overflow ao)
      match o2 with
      | none => ok counters.Step.Overflow
      | some overflow =>
        ok (counters.Step.Counted { c with content, correct, overflow })

theorem blk_spec (c : counters.Counters) (ac ao : U64) :
    blk c ac ao
      ⦃ r => match r with
        | .Counted c' => c'.content.val = c.content.val + 1 ∧
            c'.correct.val = c.correct.val + ac.val ∧
            c'.overflow.val = c.overflow.val + ao.val ∧ c'.deletions = c.deletions
        | .Overflow => U64.max < c.content.val + 1 ∨ U64.max < c.correct.val + ac.val ∨
            U64.max < c.overflow.val + ao.val ⦄ := by
  unfold blk
  have h1 := U64.checked_add_bv_spec c.content 1#u64
  cases e1 : U64.checked_add c.content 1#u64 with
  | none =>
    rw [e1] at h1
    simp only [lift, bind_tc_ok, bind_ok, e1, WP.spec_ok]
    simp at h1
    omega
  | some content =>
    rw [e1] at h1
    have h2 := U64.checked_add_bv_spec c.correct ac
    cases e2 : U64.checked_add c.correct ac with
    | none =>
      rw [e2] at h2
      simp only [lift, bind_tc_ok, bind_ok, e1, e2, WP.spec_ok]
      simp at h2
      omega
    | some correct =>
      rw [e2] at h2
      have h3 := U64.checked_add_bv_spec c.overflow ao
      cases e3 : U64.checked_add c.overflow ao with
      | none =>
        rw [e3] at h3
        simp only [lift, bind_tc_ok, bind_ok, e1, e2, e3, WP.spec_ok]
        simp at h3
        omega
      | some overflow =>
        rw [e3] at h3
        simp only [lift, bind_tc_ok, bind_ok, e1, e2, e3, WP.spec_ok]
        simp at h1 h2 h3
        exact ⟨h1.2.1, h2.2.1, h3.2.1, trivial⟩

theorem content_step_eq (c : counters.Counters) (ep same : Bool) :
    counters.content_step c ep same =
      blk c (if ep && same then 1#u64 else 0#u64) (if ep then 0#u64 else 1#u64) := by
  unfold counters.content_step counters.matched blk
  cases ep <;> cases same <;> simp <;> rfl

/-- What an insertion does to each counter, and when it refuses: the
content counter grows by exactly one, the correct counter by one only for a
matching insertion at an existing position, the overflow counter by one only
past the target; deletions are untouched. A refusal means a counter was at
the `u64` limit. Holds for every input; never fails. -/
theorem content_step_spec (c : counters.Counters) (ep same : Bool) :
    counters.content_step c ep same
      ⦃ r => match r with
        | .Counted c' => c'.content.val = c.content.val + 1 ∧
            c'.correct.val = c.correct.val + (if ep && same then 1 else 0) ∧
            c'.overflow.val = c.overflow.val + (if ep then 0 else 1) ∧
            c'.deletions = c.deletions
        | .Overflow => U64.max < c.content.val + 1 ∨
            U64.max < c.correct.val + (if ep && same then 1 else 0) ∨
            U64.max < c.overflow.val + (if ep then 0 else 1) ⦄ := by
  rw [content_step_eq]
  apply WP.spec_mono (blk_spec c _ _)
  intro r hr
  cases r with
  | Counted c' =>
    cases ep <;> cases same <;> simpa using hr
  | Overflow =>
    cases ep <;> cases same <;> simpa using hr

/-- An insertion past the target is never counted correct and counts as an
overflow attempt. -/
theorem overflow_insertion_never_correct (c : counters.Counters) (same : Bool) :
    counters.content_step c false same
      ⦃ r => ∀ c', r = counters.Step.Counted c' →
        c'.correct = c.correct ∧ c'.overflow.val = c.overflow.val + 1 ∧
        c'.content.val = c.content.val + 1 ⦄ := by
  apply WP.spec_mono (content_step_spec c false same)
  intro r hr c' hc'
  subst hc'
  simp at hr
  exact ⟨UScalar.eq_of_val_eq (by omega), by omega, by omega⟩

/-- A mistyped unit at an existing position is counted as an attempt (the
accuracy denominator grows) and not as a correct one: it can never raise a
displayed score. -/
theorem mistype_counts_attempt_not_correct (c : counters.Counters) :
    counters.content_step c true false
      ⦃ r => ∀ c', r = counters.Step.Counted c' →
        c'.correct = c.correct ∧ c'.content.val = c.content.val + 1 ∧
        c'.overflow = c.overflow ⦄ := by
  apply WP.spec_mono (content_step_spec c true false)
  intro r hr c' hc'
  subst hc'
  simp at hr
  exact ⟨UScalar.eq_of_val_eq (by omega), by omega, UScalar.eq_of_val_eq (by omega)⟩

/-- The persisted-counter invariant `correct + overflow ≤ content` is kept
by every insertion. -/
theorem content_step_keeps_inv (c : counters.Counters) (h : Inv c) (ep same : Bool) :
    counters.content_step c ep same
      ⦃ r => ∀ c', r = counters.Step.Counted c' → Inv c' ⦄ := by
  apply WP.spec_mono (content_step_spec c ep same)
  intro r hr c' hc'
  subst hc'
  simp only [Inv] at h ⊢
  obtain ⟨h1, h2, h3, _⟩ := hr
  cases ep <;> cases same <;> simp at h2 h3 <;> omega

/-- Below the `u64` limit an insertion is always counted: a refusal needs
the content counter at the limit. -/
theorem content_step_counts_below_limit (c : counters.Counters) (h : Inv c)
    (hm : c.content.val < U64.max) (ep same : Bool) :
    counters.content_step c ep same
      ⦃ r => ∃ c', r = counters.Step.Counted c' ⦄ := by
  apply WP.spec_mono (content_step_spec c ep same)
  intro r hr
  cases r with
  | Counted c' => exact ⟨c', rfl⟩
  | Overflow =>
    simp only [Inv] at h
    exfalso
    cases ep <;> cases same <;> simp at hr <;> omega

/-- Deletion: the counter grows by exactly the removed count and refuses
exactly when that sum does not fit `u64`; nothing else changes. -/
theorem deletion_step_spec (c : counters.Counters) (removed : U64) :
    counters.deletion_step c removed
      ⦃ r => match r with
        | .Counted c' => c'.deletions.val = c.deletions.val + removed.val ∧
            c'.content = c.content ∧ c'.correct = c.correct ∧ c'.overflow = c.overflow
        | .Overflow => U64.max < c.deletions.val + removed.val ⦄ := by
  unfold counters.deletion_step
  have h := U64.checked_add_bv_spec c.deletions removed
  cases e : U64.checked_add c.deletions removed with
  | none =>
    rw [e] at h
    simp only [lift, bind_tc_ok, bind_ok, e, WP.spec_ok]
    simpa using h
  | some d =>
    rw [e] at h
    simp only [lift, bind_tc_ok, bind_ok, e, WP.spec_ok]
    simp at h
    exact ⟨h.2.1, trivial, trivial, trivial⟩

/-- Deletions never decrease the counters and refuse exactly on overflow. -/
theorem deletion_step_monotone (c : counters.Counters) (removed : U64) :
    counters.deletion_step c removed
      ⦃ r => ∀ c', r = counters.Step.Counted c' → c.deletions.val ≤ c'.deletions.val ⦄ := by
  apply WP.spec_mono (deletion_step_spec c removed)
  intro r hr c' hc'
  subst hc'
  simp only at hr
  omega

end Proofs.Counters
