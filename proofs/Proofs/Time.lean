import Extracted.CadenceKernel

/-!
# Active time (`cadence_kernel::time`)

`cadence-core`'s `TimingSegments::active_through` (and so `pause`,
`end_phase` and the engine's phase clock) calls `time.segment_active`:
active time is the completed segments plus the open segment's elapsed time,
exactly. The engine decides whether active time is still inside the phase
with `Micros::within_half_open`, which calls `time.in_window`. Only explicit
pauses close a segment; ordinary hesitation stays in active time (AGENTS.md
rule 1).

Trusted, not proved: that `TimingSegments` closes a segment on every
explicit pause and nothing else, and passes the open segment's start (the
engine's pause bookkeeping; tested, including the pinned timing fixture).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Time

/-- `saturating_add i 1` is `i + 1` below the maximum (shared with `First`). -/
theorem usize_succ (i : Usize) (h : i.val < Usize.max) :
    (core.num.Usize.saturating_add i 1#usize).val = i.val + 1 := by
  have h1 : (UScalar.saturating_add i 1#usize).val = min (UScalar.max .Usize) (i.val + (1#usize).val) := by
    show (BitVec.ofNat _ (min (UScalar.max .Usize) (i.val + (1#usize).val))).toNat = _
    rw [BitVec.toNat_ofNat]
    apply Nat.mod_eq_of_lt
    have := Nat.two_pow_pos (UScalarTy.Usize.numBits)
    rw [UScalar.max_def]
    omega
  simp only [UScalar.max_USize_eq] at h1
  simp only [core.num.Usize.saturating_add, h1]
  simp; omega

/-- Every case of `segment_active`, in unbounded arithmetic. With no open
segment the result is the completed total. With a segment open since `s`:
`Time t` with `t = completed + (now - s)` exactly, `Backward` exactly when
`now < s`, and `Overflow` exactly when the exact sum exceeds `u64`. It never
fails. -/
theorem segment_active_spec (completed : U64) (open_start : Option U64) (now : U64) :
    time.segment_active completed open_start now
      ⦃ r => match open_start with
        | none => r = time.Through.Time completed
        | some s => match r with
          | .Time t => s.val ≤ now.val ∧ t.val = completed.val + (now.val - s.val)
          | .Backward => now.val < s.val
          | .Overflow => s.val ≤ now.val ∧ U64.max < completed.val + (now.val - s.val) ⦄ := by
  unfold time.segment_active
  cases open_start with
  | none => simp [WP.spec_ok]
  | some s =>
    simp only
    have hs := U64.checked_sub_bv_spec now s
    cases hso : U64.checked_sub now s with
    | none =>
      rw [hso] at hs
      simp only at hs
      simp only [lift, bind_tc_ok, bind_ok, hso, WP.spec_ok]
      omega
    | some o =>
      rw [hso] at hs
      simp only at hs
      have ha := U64.checked_add_bv_spec completed o
      cases hao : U64.checked_add completed o with
      | none =>
        rw [hao] at ha
        simp only at ha
        simp only [lift, bind_tc_ok, bind_ok, hso, hao, WP.spec_ok]
        omega
      | some t =>
        rw [hao] at ha
        simp only at ha
        simp only [lift, bind_tc_ok, bind_ok, hso, hao, WP.spec_ok]
        omega

/-- With no open segment (paused, between phases, before the first key)
active time is exactly the completed total, whatever the clock reads. -/
theorem segment_active_closed (completed now : U64) :
    time.segment_active completed none now
      ⦃ r => r = time.Through.Time completed ⦄ := by
  apply WP.spec_mono (segment_active_spec completed none now)
  intro r hr
  exact hr

/-- Active time never runs backwards: for the same segments, a later clock
reading never gives less active time. -/
theorem segment_active_monotone (completed : U64) (open_start : Option U64) (n1 n2 : U64)
    (h : n1.val ≤ n2.val) :
    time.segment_active completed open_start n1 ⦃ r1 =>
      time.segment_active completed open_start n2 ⦃ r2 =>
        ∀ t1 t2, r1 = time.Through.Time t1 → r2 = time.Through.Time t2 → t1.val ≤ t2.val ⦄ ⦄ := by
  apply WP.spec_mono (segment_active_spec completed open_start n1)
  intro r1 hr1
  apply WP.spec_mono (segment_active_spec completed open_start n2)
  intro r2 hr2 t1 t2 e1 e2
  subst e1 e2
  cases open_start with
  | none =>
    simp only [time.Through.Time.injEq] at hr1 hr2
    subst hr1 hr2
    omega
  | some s =>
    simp only at hr1 hr2
    omega

/-- The half-open phase window: a tick is inside exactly when
`start ≤ tick < deadline`; a key at the deadline is outside (A-05). -/
theorem in_window_spec (tick start deadline : U64) :
    time.in_window tick start deadline
      ⦃ r => (r = true ↔ start.val ≤ tick.val ∧ tick.val < deadline.val) ⦄ := by
  unfold time.in_window
  split <;> simp_all [UScalar.le_equiv, UScalar.lt_equiv]

end Proofs.Time
