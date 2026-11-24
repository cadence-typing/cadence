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

end Proofs.Progress
