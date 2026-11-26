import Extracted.CadenceKernel
import Proofs.Ratio

/-!
# Track counter validity (`cadence_kernel::progress`)

`cadence-core`'s `SessionProgress::validate` calls `progress.track_valid`
for every track before a checkpoint can be built or a stored one decoded.
`track_valid_spec` states exactly which counters it accepts. The
consequence theorem connects it to the accuracy arithmetic: a valid track's
first-pass accuracy is a ratio `first_correct / first_attempted` that is at
most 100%, so `ratio.basis_points` never reports it over-full.

Trusted, not proved: the conversion of the Rust `TrackProgress` into
`TrackCounters` (a field-by-field copy, tested).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false
set_option maxHeartbeats 1000000

namespace Proofs.Progress

theorem cmax_val : (progress.COUNTER_MAXIMUM).val = 9223372036854775807 := by
  simp [progress.COUNTER_MAXIMUM]

/-- Every counter is within the persisted range. -/
def InRange (t : progress.TrackCounters) : Prop :=
  t.first_correct.val ≤ 9223372036854775807 ∧
  t.first_attempted.val ≤ 9223372036854775807 ∧
  t.correct_attempts.val ≤ 9223372036854775807 ∧
  t.content_attempts.val ≤ 9223372036854775807 ∧
  t.deletions.val ≤ 9223372036854775807 ∧
  t.overflow_attempts.val ≤ 9223372036854775807 ∧
  t.correct_units.val ≤ 9223372036854775807 ∧
  t.correct_digits.val ≤ 9223372036854775807 ∧
  t.active_micros.val ≤ 9223372036854775807

/-- The counters are ordered. -/
def Ordered (t : progress.TrackCounters) : Prop :=
  t.first_correct.val ≤ t.first_attempted.val ∧
  t.first_attempted.val ≤ t.content_attempts.val ∧
  t.correct_attempts.val ≤ t.content_attempts.val ∧
  t.first_correct.val ≤ t.correct_attempts.val ∧
  t.overflow_attempts.val ≤ t.content_attempts.val ∧
  t.correct_units.val ≤ t.correct_attempts.val ∧
  t.correct_digits.val ≤ t.correct_units.val

theorem in_range_spec (t : progress.TrackCounters) :
    progress.in_range t ⦃ r => (r = true ↔ InRange t) ⦄ := by
  unfold progress.in_range
  simp only [InRange, UScalar.le_equiv, cmax_val]
  split_ifs <;> simp_all

theorem ordered_spec (t : progress.TrackCounters) :
    progress.ordered t ⦃ r => (r = true ↔ Ordered t) ⦄ := by
  unfold progress.ordered
  simp only [Ordered, UScalar.le_equiv]
  split_ifs <;> simp_all

theorem split_fits_spec (t : progress.TrackCounters) :
    progress.split_fits t
      ⦃ r => (r = true ↔ t.correct_attempts.val + t.overflow_attempts.val ≤ t.content_attempts.val) ⦄ := by
  unfold progress.split_fits
  have h := U64.checked_add_bv_spec t.correct_attempts t.overflow_attempts
  cases e : U64.checked_add t.correct_attempts t.overflow_attempts with
  | none =>
    rw [e] at h
    simp only [lift, bind_tc_ok, bind_ok, e, WP.spec_ok]
    have hc : t.content_attempts.val ≤ U64.max := by scalar_tac
    simp only [Bool.false_eq_true, false_iff]
    omega
  | some sum =>
    rw [e] at h
    simp only [lift, bind_tc_ok, bind_ok, e, WP.spec_ok]
    simp only [UScalar.le_equiv]
    simp at h
    simp [h.2.1]

/-- `track_valid` accepts exactly the counters that are within range,
ordered, and whose correct and overflow insertions fit within all
insertions. -/
theorem track_valid_spec (t : progress.TrackCounters) :
    progress.track_valid t
      ⦃ r => (r = true ↔ InRange t ∧ Ordered t ∧
        t.correct_attempts.val + t.overflow_attempts.val ≤ t.content_attempts.val) ⦄ := by
  unfold progress.track_valid
  apply WP.spec_bind (in_range_spec t)
  intro b1 h1
  by_cases hb1 : b1 = true
  · subst hb1
    simp only [ite_true]
    apply WP.spec_bind (ordered_spec t)
    intro b2 h2
    by_cases hb2 : b2 = true
    · subst hb2
      simp only [ite_true]
      apply WP.spec_mono (split_fits_spec t)
      intro r hr
      constructor
      · intro hrt; exact ⟨h1.mp rfl, h2.mp rfl, hr.mp hrt⟩
      · intro h; exact hr.mpr h.2.2
    · simp only [hb2, Bool.false_eq_true, ite_false, WP.spec_ok]
      constructor
      · intro h; cases h
      · intro h; exact absurd (h2.mpr h.2.1) hb2
  · simp only [hb1, Bool.false_eq_true, ite_false, WP.spec_ok]
    constructor
    · intro h; cases h
    · intro h; exact absurd (h1.mpr h.1) hb1

/-- A valid track never has more first-correct than first-attempted
positions, so its first-pass accuracy is at most 100%. -/
theorem valid_accuracy_at_most_full (t : progress.TrackCounters) :
    progress.track_valid t
      ⦃ r => r = true → t.first_correct.val ≤ t.first_attempted.val ∧
        t.first_attempted.val ≤ t.content_attempts.val ⦄ := by
  apply WP.spec_mono (track_valid_spec t)
  intro r hr hv
  have h := (hr.mp hv).2.1
  exact ⟨h.1, h.2.1⟩

/-- A valid track's accuracy basis points are computed, never over-full and
at most `10000`, whenever the first-attempted count is nonzero and the
scaled numerator fits `u64`. -/
theorem valid_track_accuracy_in_range (t : progress.TrackCounters)
    (hd : 0 < t.first_attempted.val) (hm : t.first_correct.val * 10000 ≤ U64.max) :
    progress.track_valid t
      ⦃ r => r = true →
        ratio.basis_points t.first_correct t.first_attempted
          ⦃ b => ∃ v, b = ratio.BasisPoints.Value v ∧ v.val ≤ 10000 ⦄ ⦄ := by
  apply WP.spec_mono (track_valid_spec t)
  intro r hr hv
  have hle : t.first_correct.val ≤ t.first_attempted.val := (hr.mp hv).2.1.1
  apply WP.spec_mono (Proofs.Ratio.basis_points_spec t.first_correct t.first_attempted)
  intro b hb
  cases b with
  | ZeroDenominator => simp at hb; omega
  | OverFull => simp at hb; omega
  | Overflow => simp at hb; omega
  | Value v => exact ⟨v, rfl, by simp at hb; omega⟩


/-- Merging an exercise into a track total: every one of the nine counters
is the exact (unbounded) sum, or the merge is `Overflow` exactly when some
sum does not fit `u64`; nothing is dropped, saturated or merged partially.
Total. -/
theorem merge_exact (a b : progress.TrackCounters) :
    progress.merge a b
      ⦃ r => match r with
        | .Merged t =>
            t.first_correct.val = a.first_correct.val + b.first_correct.val ∧
            t.first_attempted.val = a.first_attempted.val + b.first_attempted.val ∧
            t.correct_attempts.val = a.correct_attempts.val + b.correct_attempts.val ∧
            t.content_attempts.val = a.content_attempts.val + b.content_attempts.val ∧
            t.deletions.val = a.deletions.val + b.deletions.val ∧
            t.overflow_attempts.val = a.overflow_attempts.val + b.overflow_attempts.val ∧
            t.correct_units.val = a.correct_units.val + b.correct_units.val ∧
            t.correct_digits.val = a.correct_digits.val + b.correct_digits.val ∧
            t.active_micros.val = a.active_micros.val + b.active_micros.val
        | .Overflow =>
            U64.max < a.first_correct.val + b.first_correct.val ∨
            U64.max < a.first_attempted.val + b.first_attempted.val ∨
            U64.max < a.correct_attempts.val + b.correct_attempts.val ∨
            U64.max < a.content_attempts.val + b.content_attempts.val ∨
            U64.max < a.deletions.val + b.deletions.val ∨
            U64.max < a.overflow_attempts.val + b.overflow_attempts.val ∨
            U64.max < a.correct_units.val + b.correct_units.val ∨
            U64.max < a.correct_digits.val + b.correct_digits.val ∨
            U64.max < a.active_micros.val + b.active_micros.val ⦄ := by
  unfold progress.merge
  have h0 := U64.checked_add_bv_spec a.first_correct b.first_correct
  cases e0 : U64.checked_add a.first_correct b.first_correct with
  | none =>
    rw [e0] at h0
    simp only [lift, bind_tc_ok, bind_ok, e0, WP.spec_ok]
    simp at h0
    omega
  | some x0 =>
    rw [e0] at h0
    have h1 := U64.checked_add_bv_spec a.first_attempted b.first_attempted
    cases e1 : U64.checked_add a.first_attempted b.first_attempted with
    | none =>
      rw [e1] at h1
      simp only [lift, bind_tc_ok, bind_ok, e0, e1, WP.spec_ok]
      simp at h1
      omega
    | some x1 =>
      rw [e1] at h1
      have h2 := U64.checked_add_bv_spec a.correct_attempts b.correct_attempts
      cases e2 : U64.checked_add a.correct_attempts b.correct_attempts with
      | none =>
        rw [e2] at h2
        simp only [lift, bind_tc_ok, bind_ok, e0, e1, e2, WP.spec_ok]
        simp at h2
        omega
      | some x2 =>
        rw [e2] at h2
        have h3 := U64.checked_add_bv_spec a.content_attempts b.content_attempts
        cases e3 : U64.checked_add a.content_attempts b.content_attempts with
        | none =>
          rw [e3] at h3
          simp only [lift, bind_tc_ok, bind_ok, e0, e1, e2, e3, WP.spec_ok]
          simp at h3
          omega
        | some x3 =>
          rw [e3] at h3
          have h4 := U64.checked_add_bv_spec a.deletions b.deletions
          cases e4 : U64.checked_add a.deletions b.deletions with
          | none =>
            rw [e4] at h4
            simp only [lift, bind_tc_ok, bind_ok, e0, e1, e2, e3, e4, WP.spec_ok]
            simp at h4
            omega
          | some x4 =>
            rw [e4] at h4
            have h5 := U64.checked_add_bv_spec a.overflow_attempts b.overflow_attempts
            cases e5 : U64.checked_add a.overflow_attempts b.overflow_attempts with
            | none =>
              rw [e5] at h5
              simp only [lift, bind_tc_ok, bind_ok, e0, e1, e2, e3, e4, e5, WP.spec_ok]
              simp at h5
              omega
            | some x5 =>
              rw [e5] at h5
              have h6 := U64.checked_add_bv_spec a.correct_units b.correct_units
              cases e6 : U64.checked_add a.correct_units b.correct_units with
              | none =>
                rw [e6] at h6
                simp only [lift, bind_tc_ok, bind_ok, e0, e1, e2, e3, e4, e5, e6, WP.spec_ok]
                simp at h6
                omega
              | some x6 =>
                rw [e6] at h6
                have h7 := U64.checked_add_bv_spec a.correct_digits b.correct_digits
                cases e7 : U64.checked_add a.correct_digits b.correct_digits with
                | none =>
                  rw [e7] at h7
                  simp only [lift, bind_tc_ok, bind_ok, e0, e1, e2, e3, e4, e5, e6, e7, WP.spec_ok]
                  simp at h7
                  omega
                | some x7 =>
                  rw [e7] at h7
                  have h8 := U64.checked_add_bv_spec a.active_micros b.active_micros
                  cases e8 : U64.checked_add a.active_micros b.active_micros with
                  | none =>
                    rw [e8] at h8
                    simp only [lift, bind_tc_ok, bind_ok, e0, e1, e2, e3, e4, e5, e6, e7, e8, WP.spec_ok]
                    simp at h8
                    omega
                  | some x8 =>
                    rw [e8] at h8
                    simp only [lift, bind_tc_ok, bind_ok, e0, e1, e2, e3, e4, e5, e6, e7, e8, WP.spec_ok]
                    simp at h0 h1 h2 h3 h4 h5 h6 h7 h8
                    omega

/-- Merging two valid tracks keeps the counters ordered and the correct and
overflow insertions within all insertions; with the merged values in the
persisted range the merged track is valid. -/
theorem merge_keeps_valid (a b : progress.TrackCounters)
    (ha : Ordered a ∧ a.correct_attempts.val + a.overflow_attempts.val ≤ a.content_attempts.val)
    (hb : Ordered b ∧ b.correct_attempts.val + b.overflow_attempts.val ≤ b.content_attempts.val) :
    progress.merge a b
      ⦃ r => ∀ t, r = progress.Merge.Merged t → InRange t →
        InRange t ∧ Ordered t ∧
          t.correct_attempts.val + t.overflow_attempts.val ≤ t.content_attempts.val ⦄ := by
  apply WP.spec_mono (merge_exact a b)
  intro r hr t ht hin
  subst ht
  simp only at hr
  obtain ⟨⟨a1, a2, a3, a4, a5, a6, a7⟩, a8⟩ := ha
  obtain ⟨⟨b1, b2, b3, b4, b5, b6, b7⟩, b8⟩ := hb
  refine ⟨hin, ?_, ?_⟩
  · unfold Ordered
    omega
  · omega

/-- An exercise is credited exactly when it is newer than every credited
one: never twice, never an older one. Total. -/
theorem credit_once_spec (last : Option U64) (closing : U64) :
    progress.credit_once last closing
      ⦃ r => (r = true ↔ ∀ l, last = some l → l.val < closing.val) ⦄ := by
  unfold progress.credit_once
  cases last with
  | none => simp [WP.spec_ok]
  | some l => simp [WP.spec_ok, UScalar.lt_equiv]

end Proofs.Progress
