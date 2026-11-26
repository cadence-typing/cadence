import Extracted.CadenceKernel

/-!
# Local-calendar days (`cadence_kernel::calendar`, #52)

`cadence::main` (the only place that reads the wall clock) asks
`calendar.local_day` for the session's day from the UTC time and the local
offset read once at session start; the review recap counts days with
`calendar.days_ago`.

Proved: a day number is the floor of the local time over 86,400 seconds
(`86400·d ≤ unix + offset < 86400·(d+1)`); for a fixed offset a later time
never gives an earlier day; an offset beyond 18 hours either way is refused;
a day count is never negative and is 0 for a day not before today. Total.

Trusted, not proved: reading the offset from the system (edge code,
`chrono`), and that every consumer of "day" receives this value.
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Calendar

theorem day_seconds : calendar.DAY_SECONDS.val = 86400 := by
  unfold calendar.DAY_SECONDS; rfl

theorem max_offset : calendar.MAX_OFFSET_SECONDS.val = 64800 := by
  unfold calendar.MAX_OFFSET_SECONDS; rfl

/-- The full decision. Total. -/
theorem local_day_spec (u : I64) (o : I32) :
    calendar.local_day u o ⦃ r => match r with
      | .Number d => o.val.natAbs ≤ 64800 ∧ 0 ≤ u.val + o.val ∧
          86400 * d.val ≤ u.val + o.val ∧ u.val + o.val < 86400 * (d.val + 1)
      | .OffsetOutOfRange => 64800 < o.val.natAbs
      | .Overflow => o.val.natAbs ≤ 64800 ⦄ := by
  unfold calendar.local_day
  have hm := max_offset
  have hd := day_seconds
  split
  · rename_i h; simp [WP.spec_ok]; scalar_tac
  · rename_i h
    have hhigh : o.val ≤ 64800 := by scalar_tac
    step as ⟨i, hi⟩
    split
    · rename_i hl
      simp only [WP.spec_ok]
      have : o.val < -64800 := by rw [hm] at hi; scalar_tac
      omega
    · rename_i hl
      have hlow : -64800 ≤ o.val := by rw [hm] at hi; scalar_tac
      have hf := core.convert.num.FromI64I32.from_val_eq o
      simp only [lift, bind_ok]
      have hc := I64.checked_add_bv_spec u (core.convert.num.FromI64I32.from o)
      rw [show I64.checked_add u (core.convert.num.FromI64I32.from o) =
        core.num.checked_add_IScalar u (core.convert.num.FromI64I32.from o) from rfl]
      cases e : core.num.checked_add_IScalar u (core.convert.num.FromI64I32.from o) with
      | none =>
        simp only [WP.spec_ok]
        omega
      | some t =>
        rw [e] at hc
        obtain ⟨_, _, ht, _⟩ := hc
        rw [hf] at ht
        simp only []
        split
        · simp only [WP.spec_ok]; omega
        · rename_i hpos
          have ht0 : 0 ≤ t.val := by scalar_tac
          have hq := I64.checked_div_bv_spec t calendar.DAY_SECONDS
          cases e2 : core.num.checked_div_IScalar t calendar.DAY_SECONDS with
          | none =>
            rw [e2] at hq
            try simp only [lift, bind_ok]
            rw [show I64.checked_div t calendar.DAY_SECONDS = core.num.checked_div_IScalar t calendar.DAY_SECONDS from rfl, e2]
            simp only [WP.spec_ok]
            omega
          | some q =>
            rw [e2] at hq
            obtain ⟨_, _, hqv, _⟩ := hq
            try simp only [lift, bind_ok]
            rw [show I64.checked_div t calendar.DAY_SECONDS = core.num.checked_div_IScalar t calendar.DAY_SECONDS from rfl, e2]
            simp only [WP.spec_ok]
            rw [hd, Int.tdiv_eq_ediv_of_nonneg ht0] at hqv
            rw [hqv, ← ht]
            refine ⟨by omega, by omega, ?_, ?_⟩ <;> omega

/-- For a fixed offset, a later time never gives an earlier day. -/
theorem local_day_monotone (u1 u2 : I64) (o : I32) (h : u1.val ≤ u2.val) :
    calendar.local_day u1 o ⦃ r1 => calendar.local_day u2 o ⦃ r2 =>
      ∀ d1 d2, r1 = .Number d1 → r2 = .Number d2 → d1.val ≤ d2.val ⦄ ⦄ := by
  apply WP.spec_mono (local_day_spec u1 o)
  intro r1 h1
  apply WP.spec_mono (local_day_spec u2 o)
  intro r2 h2 d1 d2 e1 e2
  subst e1 e2
  obtain ⟨_, _, a1, b1⟩ := h1
  obtain ⟨_, _, a2, b2⟩ := h2
  omega

/-- An offset beyond 18 hours either way is refused. -/
theorem offset_bounded (u : I64) (o : I32) (h : 64800 < o.val.natAbs) :
    calendar.local_day u o ⦃ r => r = .OffsetOutOfRange ⦄ := by
  apply WP.spec_mono (local_day_spec u o)
  intro r hr
  cases r with
  | Number d => omega
  | OffsetOutOfRange => rfl
  | Overflow => omega

/-- A day count is never negative, and 0 for a day not before today. Total. -/
theorem days_ago_never_negative (today t : I64) :
    calendar.days_ago today t ⦃ r => 0 ≤ r.val ∧ (today.val ≤ t.val → r.val = 0) ⦄ := by
  unfold calendar.days_ago
  split
  · simp [WP.spec_ok]
  · rename_i h
    have hc := I64.checked_sub_bv_spec today t
    rw [show I64.checked_sub today t = core.num.checked_sub_IScalar today t from rfl]
    cases e : core.num.checked_sub_IScalar today t with
    | none =>
      simp only [lift, bind_ok, core.option.Option.unwrap_or, WP.spec_ok]
      have : (core.num.I64.MAX).val = 9223372036854775807 := by
        simp [core.num.I64.MAX, I64.rMax]
      constructor
      · rw [this]; omega
      · intro h2; exfalso; scalar_tac
    | some d =>
      rw [e] at hc
      obtain ⟨_, _, hd, _⟩ := hc
      simp only [lift, bind_ok, core.option.Option.unwrap_or, WP.spec_ok]
      constructor
      · have : t.val < today.val := by scalar_tac
        omega
      · intro h2; exfalso; scalar_tac

end Proofs.Calendar
