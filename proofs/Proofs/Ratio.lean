import Extracted.CadenceKernel

/-!
# Pace and accuracy arithmetic (`cadence_kernel::ratio`)

`cadence-core`'s `metrics::correct_wpm` and `metrics::digits_per_minute`
call `ratio.wpm` and `ratio.digits_per_minute`; `BasisPoints::from_ratio_truncating`
calls `ratio.basis_points`. The theorems give the exact fixture identities
(`fixtures/scoring.json`: 3 units in 3 s is `180000000 / 15000000`), show
the `u128` products never overflow for `u64` inputs, show that pace never
improves by adding idle time or by removing typed units, and that a
truncated accuracy of a valid ratio lies in `[0, 10_000]` basis points.

Trusted, not proved: that callers pass the right counters (tested against
`fixtures/scoring.json`) and the f64 formatting at the UI edge (not part of
the kernel).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Ratio

theorem u64_max : U64.max = 18446744073709551615 := by
  simp [U64.max, UScalar.max, UScalarTy.numBits, U64.numBits]

theorem u128_max : U128.max = 340282366920938463463374607431768211455 := by
  simp [U128.max, UScalar.max, UScalarTy.numBits, U128.numBits]

theorem mpm_val : (ratio.MICROSECONDS_PER_MINUTE).val = 60000000 := by
  simp [ratio.MICROSECONDS_PER_MINUTE]

theorem wpu_val : (ratio.WPM_UNITS_PER_WORD).val = 5 := by
  simp [ratio.WPM_UNITS_PER_WORD]

theorem bpf_val : (ratio.BASIS_POINTS_FULL).val = 10000 := by
  simp [ratio.BASIS_POINTS_FULL]

/-- `fraction` keeps the pair exactly and refuses only a zero denominator. -/
theorem fraction_spec (num den : U64) :
    ratio.fraction num den
      ⦃ r => match r with
        | none => den.val = 0
        | some f => den.val ≠ 0 ∧ f.num.val = num.val ∧ f.den.val = den.val ⦄ := by
  unfold ratio.fraction
  by_cases h : den = 0#u64
  · simp [h]
  · have hv : den.val ≠ 0 := by
      intro h0; apply h; exact UScalar.eq_of_val_eq (by simpa using h0)
    simp only [h, ite_false, lift, bind_tc_ok, bind_ok, WP.spec_ok]
    refine ⟨hv, ?_, ?_⟩ <;> simp [core.convert.num.FromU128U64.from_val_eq]

/-- Correct pace, exactly: `Empty` iff no units or no active time; otherwise
the unreduced pair `units * 60e6 / (5 * active)`. The `u128` products never
overflow for `u64` inputs. -/
theorem wpm_spec (units active_us : U64) :
    ratio.wpm units active_us
      ⦃ r => match r with
        | .Empty => units.val = 0 ∨ active_us.val = 0
        | .Rate f => 0 < units.val ∧ 0 < active_us.val ∧
            f.num.val = units.val * 60000000 ∧ f.den.val = 5 * active_us.val
        | .Overflow => False ⦄ := by
  unfold ratio.wpm
  by_cases hu : units = 0#u64
  · simp [hu]
  · by_cases ha : active_us = 0#u64
    · simp [hu, ha]
    · have hu' : 0 < units.val := by
        rcases Nat.eq_zero_or_pos units.val with h | h
        · exact absurd (UScalar.eq_of_val_eq (by simpa using h)) hu
        · exact h
      have ha' : 0 < active_us.val := by
        rcases Nat.eq_zero_or_pos active_us.val with h | h
        · exact absurd (UScalar.eq_of_val_eq (by simpa using h)) ha
        · exact h
      simp only [hu, ha, ite_false, lift, bind_tc_ok, bind_ok]
      have f1 := core.convert.num.FromU128U64.from_val_eq units
      have f2 := core.convert.num.FromU128U64.from_val_eq active_us
      have m1 := U128.checked_mul_bv_spec (core.convert.num.FromU128U64.from units)
        ratio.MICROSECONDS_PER_MINUTE
      have hb1 : units.val ≤ U64.max := by scalar_tac
      have hb2 : active_us.val ≤ U64.max := by scalar_tac
      cases e1 : U128.checked_mul (core.convert.num.FromU128U64.from units)
          ratio.MICROSECONDS_PER_MINUTE with
      | none =>
        rw [e1] at m1
        simp only [mpm_val, f1] at m1
        simp only [u128_max, u64_max] at m1 hb1
        omega
      | some num =>
        rw [e1] at m1
        have m2 := U128.checked_mul_bv_spec (core.convert.num.FromU128U64.from active_us)
          ratio.WPM_UNITS_PER_WORD
        cases e2 : U128.checked_mul (core.convert.num.FromU128U64.from active_us)
            ratio.WPM_UNITS_PER_WORD with
        | none =>
          rw [e2] at m2
          simp only [wpu_val, f2] at m2
          simp only [u128_max, u64_max] at m2 hb2
          omega
        | some den =>
          rw [e2] at m2
          simp only [mpm_val, f1] at m1
          simp only [wpu_val, f2] at m2
          simp only [e1, e2, WP.spec_ok]
          exact ⟨hu', ha', m1.2.1, by have := m2.2.1; omega⟩

/-- Numpad pace: `Empty` iff no digits or no active time; otherwise
`digits * 60e6 / active`, never overflowing. -/
theorem digits_per_minute_spec (digits active_us : U64) :
    ratio.digits_per_minute digits active_us
      ⦃ r => match r with
        | .Empty => digits.val = 0 ∨ active_us.val = 0
        | .Rate f => 0 < digits.val ∧ 0 < active_us.val ∧
            f.num.val = digits.val * 60000000 ∧ f.den.val = active_us.val
        | .Overflow => False ⦄ := by
  unfold ratio.digits_per_minute
  by_cases hu : digits = 0#u64
  · simp [hu]
  · by_cases ha : active_us = 0#u64
    · simp [hu, ha]
    · have hu' : 0 < digits.val := by
        rcases Nat.eq_zero_or_pos digits.val with h | h
        · exact absurd (UScalar.eq_of_val_eq (by simpa using h)) hu
        · exact h
      have ha' : 0 < active_us.val := by
        rcases Nat.eq_zero_or_pos active_us.val with h | h
        · exact absurd (UScalar.eq_of_val_eq (by simpa using h)) ha
        · exact h
      simp only [hu, ha, ite_false, lift, bind_tc_ok, bind_ok]
      have f1 := core.convert.num.FromU128U64.from_val_eq digits
      have f2 := core.convert.num.FromU128U64.from_val_eq active_us
      have m1 := U128.checked_mul_bv_spec (core.convert.num.FromU128U64.from digits)
        ratio.MICROSECONDS_PER_MINUTE
      have hb1 : digits.val ≤ U64.max := by scalar_tac
      cases e1 : U128.checked_mul (core.convert.num.FromU128U64.from digits)
          ratio.MICROSECONDS_PER_MINUTE with
      | none =>
        rw [e1] at m1
        simp only [mpm_val, f1] at m1
        simp only [u128_max, u64_max] at m1 hb1
        omega
      | some num =>
        rw [e1] at m1
        simp only [mpm_val, f1] at m1
        simp only [e1, WP.spec_ok]
        exact ⟨hu', ha', m1.2.1, f2⟩

/-- More typed units in the same active time never lowers the pace: equal
denominators, numerator at least as large. -/
theorem wpm_monotone_in_units (u1 u2 a : U64) (h : u1.val ≤ u2.val)
    (h1 : 0 < u1.val) (ha : 0 < a.val) :
    ratio.wpm u1 a ⦃ r1 => ratio.wpm u2 a ⦃ r2 =>
      ∃ f1 f2, r1 = ratio.Pace.Rate f1 ∧ r2 = ratio.Pace.Rate f2 ∧
        f1.num.val ≤ f2.num.val ∧ f1.den.val = f2.den.val ⦄ ⦄ := by
  apply WP.spec_mono (wpm_spec u1 a)
  intro r1 hr1
  apply WP.spec_mono (wpm_spec u2 a)
  intro r2 hr2
  cases r1 with
  | Empty => simp at hr1; omega
  | Overflow => exact absurd hr1 id
  | Rate f1 =>
    cases r2 with
    | Empty => simp at hr2; omega
    | Overflow => exact absurd hr2 id
    | Rate f2 =>
      refine ⟨f1, f2, rfl, rfl, ?_, ?_⟩
      · simp only at hr1 hr2; omega
      · simp only at hr1 hr2; omega

/-- Idle time never raises the pace: the same units over more active time
have the same numerator and a denominator at least as large (so the ratio is
at most as large). -/
theorem wpm_idle_time_never_helps (u a1 a2 : U64) (h : a1.val ≤ a2.val)
    (hu : 0 < u.val) (ha : 0 < a1.val) :
    ratio.wpm u a1 ⦃ r1 => ratio.wpm u a2 ⦃ r2 =>
      ∃ f1 f2, r1 = ratio.Pace.Rate f1 ∧ r2 = ratio.Pace.Rate f2 ∧
        f1.num.val = f2.num.val ∧ f1.den.val ≤ f2.den.val ⦄ ⦄ := by
  apply WP.spec_mono (wpm_spec u a1)
  intro r1 hr1
  apply WP.spec_mono (wpm_spec u a2)
  intro r2 hr2
  cases r1 with
  | Empty => simp at hr1; omega
  | Overflow => exact absurd hr1 id
  | Rate f1 =>
    cases r2 with
    | Empty => simp at hr2; omega
    | Overflow => exact absurd hr2 id
    | Rate f2 =>
      refine ⟨f1, f2, rfl, rfl, ?_, ?_⟩
      · simp only at hr1 hr2; omega
      · simp only at hr1 hr2; omega

/-- The truncated basis points of a ratio, for every input: refusals are
exactly a zero denominator, a ratio over 100%, or a scaling overflow; a value
is `floor(num * 10000 / den)` and never exceeds 100% (`10000`). -/
theorem basis_points_spec (num den : U64) :
    ratio.basis_points num den
      ⦃ r => match r with
        | .ZeroDenominator => den.val = 0
        | .OverFull => den.val ≠ 0 ∧ den.val < num.val
        | .Overflow => den.val ≠ 0 ∧ num.val ≤ den.val ∧ U64.max < num.val * 10000
        | .Value v => den.val ≠ 0 ∧ num.val ≤ den.val ∧
            v.val = num.val * 10000 / den.val ∧ v.val ≤ 10000 ⦄ := by
  unfold ratio.basis_points
  by_cases hd : den = 0#u64
  · simp [hd]
  · have hd' : den.val ≠ 0 := by
      intro h0; apply hd; exact UScalar.eq_of_val_eq (by simpa using h0)
    simp only [hd, ite_false]
    by_cases hn : num > den
    · have : den.val < num.val := by simpa [UScalar.lt_equiv] using hn
      simp [hn, hd', this]
    · have hn' : num.val ≤ den.val := by
        have : ¬ den.val < num.val := by simpa [UScalar.lt_equiv] using hn
        omega
      simp only [hn, ite_false, lift, bind_tc_ok, bind_ok]
      have m := U64.checked_mul_bv_spec num ratio.BASIS_POINTS_FULL
      cases e : U64.checked_mul num ratio.BASIS_POINTS_FULL with
      | none =>
        rw [e] at m
        simp only [bpf_val] at m
        simp only [e, WP.spec_ok]
        exact ⟨hd', hn', m⟩
      | some scaled =>
        rw [e] at m
        simp only [bpf_val] at m
        have dv := U64.checked_div_bv_spec scaled den
        cases e2 : U64.checked_div scaled den with
        | none =>
          rw [e2] at dv
          exact absurd dv hd'
        | some v =>
          rw [e2] at dv
          simp only [e, e2, WP.spec_ok]
          refine ⟨hd', hn', ?_, ?_⟩
          · rw [dv.2.1, m.2.1]
          · rw [dv.2.1, m.2.1]
            exact (Nat.div_le_iff_le_mul_add_pred (Nat.pos_of_ne_zero hd')).mpr (by nlinarith)

/-- Accuracy within `[0%, 100%]`: for a ratio of a count to a count that is
at least as large, the basis points are a value of at most `10000`, never
`OverFull`, and fail only on the scaling overflow (numerators above
`u64::MAX / 10000`). -/
theorem accuracy_in_range (num den : U64) (h : num.val ≤ den.val) :
    ratio.basis_points num den
      ⦃ r => (∀ v, r = ratio.BasisPoints.Value v → v.val ≤ 10000) ∧
             r ≠ ratio.BasisPoints.OverFull ⦄ := by
  apply WP.spec_mono (basis_points_spec num den)
  intro r hr
  cases r with
  | ZeroDenominator => simp
  | OverFull => simp at hr; omega
  | Overflow => simp
  | Value v => simp at hr ⊢; omega

/-- Below the scaling limit the accuracy is computed: a nonzero denominator
and `num ≤ den ≤ ...` give a value. -/
theorem accuracy_computed (num den : U64) (h : num.val ≤ den.val) (hd : 0 < den.val)
    (hm : num.val * 10000 ≤ U64.max) :
    ratio.basis_points num den ⦃ r => ∃ v, r = ratio.BasisPoints.Value v ⦄ := by
  apply WP.spec_mono (basis_points_spec num den)
  intro r hr
  cases r with
  | ZeroDenominator => simp at hr; omega
  | OverFull => simp at hr; omega
  | Overflow => simp at hr; omega
  | Value v => exact ⟨v, rfl⟩

end Proofs.Ratio
