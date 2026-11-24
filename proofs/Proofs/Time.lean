import Extracted.CadenceKernel

/-!
# Active time (`cadence_kernel::time`)

`cadence-core`'s `metrics::active_time` calls `time.active_time` for the
exact identity `active = elapsed - Σ pauses`, and
`MonoTick::within_half_open` calls `time.in_window`. Only explicit pauses
leave active time; ordinary hesitation stays in it (AGENTS.md rule 1).

The loop of `pause_total` is proved by an invariant that conserves
`total + Σ (remaining pauses)` in unbounded arithmetic.

Trusted, not proved: that the caller passes every explicit pause once and
nothing else (the engine's pause bookkeeping; tested).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Time

/-- The exact (unbounded) sum of a list of pauses. -/
def pauseSum (l : List U64) : Nat := (l.map (fun p => p.val)).sum

theorem pauseSum_cons (p : U64) (l : List U64) : pauseSum (p :: l) = p.val + pauseSum l := by
  simp [pauseSum]

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

theorem pause_total_loop_spec (ps : Slice U64) (i : Usize) (total : U64)
    (hi : i.val ≤ ps.val.length) :
    time.pause_total_loop_loop ps i total
      ⦃ r => match r with
        | .Sum t => t.val = total.val + pauseSum (ps.val.drop i.val)
        | .Overflow => U64.max < total.val + pauseSum (ps.val.drop i.val) ⦄ := by
  unfold time.pause_total_loop_loop
  have hlen : (Slice.len ps).val = ps.val.length := Slice.len_val ps
  apply loop.spec_decr_nat
    (fun s : Usize × U64 => ps.val.length - s.1.val)
    (fun s : Usize × U64 => s.1.val ≤ ps.val.length ∧
      s.2.val + pauseSum (ps.val.drop s.1.val) = total.val + pauseSum (ps.val.drop i.val))
  · rintro ⟨j, t⟩ ⟨hj, hinv⟩
    simp only at hj hinv
    unfold time.pause_total_loop_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < ps.val.length := by scalar_tac
      have hget : ps.val[j.val]? = some ps.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
      have hdrop : ps.val.drop j.val = ps.val[j.val] :: ps.val.drop (j.val + 1) :=
        List.drop_eq_getElem_cons hlt'
      have hs := usize_succ j (by scalar_tac)
      have hc := U64.checked_add_bv_spec t ps.val[j.val]
      cases hco : U64.checked_add t ps.val[j.val] with
      | none =>
        rw [hco] at hc
        simp only [lift, bind_tc_ok, bind_ok, WP.spec_ok]
        rw [hdrop, pauseSum_cons] at hinv
        simp only [hdrop, pauseSum_cons] at *
        omega
      | some nx =>
        rw [hco] at hc
        simp only [lift, bind_tc_ok, bind_ok, WP.spec_ok, hs]
        refine ⟨⟨by scalar_tac, ?_⟩, by scalar_tac⟩
        rw [hdrop, pauseSum_cons] at hinv
        omega
    · rename_i hge
      have hd : ps.val.drop j.val = [] := List.drop_eq_nil_of_le (by scalar_tac)
      simp only [WP.spec_ok]
      rw [hd] at hinv
      simp [pauseSum] at hinv ⊢
      omega
  · exact ⟨hi, rfl⟩

/-- `pause_total` is the exact sum when it fits `u64`, and `Overflow`
exactly when the exact sum does not. It never fails. -/
theorem pause_total_spec (ps : Slice U64) :
    time.pause_total ps
      ⦃ r => match r with
        | .Sum t => t.val = pauseSum ps.val
        | .Overflow => U64.max < pauseSum ps.val ⦄ := by
  unfold time.pause_total time.pause_total_loop
  have := pause_total_loop_spec ps 0#usize 0#u64 (by simp)
  simpa using this

/-- Every case of `active_time`, with the pause total in unbounded
arithmetic: the identity `active + Σ pauses = elapsed`, the overflow case
and the pauses-exceed case. -/
theorem active_time_spec (elapsed : U64) (ps : Slice U64) :
    time.active_time elapsed ps
      ⦃ r => match r with
        | .Time a => a.val + pauseSum ps.val = elapsed.val
        | .PauseOverflow => U64.max < pauseSum ps.val
        | .PausesExceed t => t.val = pauseSum ps.val ∧ elapsed.val < pauseSum ps.val ⦄ := by
  unfold time.active_time
  apply WP.spec_bind (pause_total_spec ps)
  intro t ht
  cases t with
  | Overflow => simpa using ht
  | Sum total =>
    simp only at ht
    have hc := U64.checked_sub_bv_spec elapsed total
    cases hco : U64.checked_sub elapsed total with
    | none =>
      rw [hco] at hc
      simp only at hc
      simp only [lift, bind_tc_ok, bind_ok, hco, WP.spec_ok]
      omega
    | some a =>
      rw [hco] at hc
      simp only at hc
      simp only [lift, bind_tc_ok, bind_ok, hco, WP.spec_ok]
      omega

/-- Active time never exceeds elapsed time: pausing can only remove time. -/
theorem active_time_le_elapsed (elapsed : U64) (ps : Slice U64) :
    time.active_time elapsed ps
      ⦃ r => ∀ a, r = time.Active.Time a → a.val ≤ elapsed.val ⦄ := by
  apply WP.spec_mono (active_time_spec elapsed ps)
  intro r hr a ha
  subst ha
  simp only at hr
  omega

/-- With no pauses, active time is the elapsed time: nothing is excluded
for hesitation. -/
theorem active_time_no_pauses (elapsed : U64) (ps : Slice U64) (h : ps.val = []) :
    time.active_time elapsed ps ⦃ r => r = time.Active.Time elapsed ⦄ := by
  apply WP.spec_mono (active_time_spec elapsed ps)
  intro r hr
  cases r with
  | Time a =>
    simp only [h, pauseSum, List.map_nil, List.sum_nil, Nat.add_zero] at hr
    have : a = elapsed := UScalar.eq_of_val_eq hr
    simp [this]
  | PauseOverflow => simp [h, pauseSum] at hr
  | PausesExceed t => simp [h, pauseSum] at hr

/-- The half-open phase window: a tick is inside exactly when
`start ≤ tick < deadline`; a key at the deadline is outside (A-05). -/
theorem in_window_spec (tick start deadline : U64) :
    time.in_window tick start deadline
      ⦃ r => (r = true ↔ start.val ≤ tick.val ∧ tick.val < deadline.val) ⦄ := by
  unfold time.in_window
  split <;> simp_all [UScalar.le_equiv, UScalar.lt_equiv]

end Proofs.Time
