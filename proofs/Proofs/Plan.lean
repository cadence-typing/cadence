import Extracted.CadenceKernel
import Proofs.Time
import Proofs.Ledger

/-!
# Phase allocation (`cadence_kernel::plan`, VK-03, #34)

`cadence_core::plan::allocate` (called by `trainer::plan::build_session_plan`
and `SessionPolicy::default_allocation`) owns the vectors and the typed
errors and asks `plan.allocate` for the seconds of every phase.

Proved: on `Done` the seconds sum exactly to the budget and every phase keeps
its minimum; `Infeasible` is returned exactly when the minimums sum (fits and)
exceeds the budget; each leftover second goes to the phase with the largest
remainder among those not yet served, the lower index first among equal
remainders.

Trusted, not proved: that a feasible input never ends in `Overflow` (the
leftover is below the phase count, so `pick` always finds a phase; tested
exhaustively on a small space against the sort-based reference rule in
`crates/core/src/plan.rs`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Plan

/-- The sum of a list of `u64` values, in `Nat`. -/
def sumV (l : List U64) : Nat := (l.map (·.val)).sum

theorem sumV_append (a b : List U64) : sumV (a ++ b) = sumV a + sumV b := by
  simp [sumV]

theorem sumV_take_succ (l : List U64) (i : Nat) (h : i < l.length) :
    sumV (l.take (i + 1)) = sumV (l.take i) + l[i].val := by
  rw [List.take_add_one, sumV_append, List.getElem?_eq_getElem h]
  simp [sumV]

theorem sumV_take_le (l : List U64) (n : Nat) : sumV (l.take n) ≤ sumV l := by
  have h := sumV_append (l.take n) (l.drop n)
  rw [List.take_append_drop] at h
  omega

theorem sumV_take_all (l : List U64) (n : Nat) (h : l.length ≤ n) :
    sumV (l.take n) = sumV l := by
  rw [List.take_of_length_le h]

theorem sumV_set (l : List U64) (i : Nat) (v : U64) (h : i < l.length) :
    sumV (l.set i v) + l[i].val = sumV l + v.val := by
  induction l generalizing i with
  | nil => simp at h
  | cons x tl ih =>
    cases i with
    | zero => simp [sumV]; omega
    | succ j =>
      simp at h
      have := ih j h
      simp [sumV] at this ⊢
      omega

theorem take_set_self (l : List U64) (i : Nat) (v : U64) (h : i < l.length) :
    (l.set i v).take (i + 1) = l.take i ++ [v] := by
  induction l generalizing i with
  | nil => simp at h
  | cons x tl ih =>
    cases i with
    | zero => simp
    | succ j =>
      simp at h
      simp [List.take_succ_cons, ih j h]

theorem u64_val_lt_max (k : U64) (l : U64) (h : k.val < l.val) : k.val < U64.max := by
  scalar_tac

/-! ## The checked sum -/

theorem total_loop_spec (vs : Slice U64) (i : Usize) (s : U64)
    (hi : i.val ≤ vs.val.length) (hs : s.val = sumV (vs.val.take i.val)) :
    plan.total_loop vs i s ⦃ r => match r with
      | .Fits t => t.val = sumV vs.val
      | .Overflow => U64.max < sumV vs.val ⦄ := by
  unfold plan.total_loop
  have hlen : (Slice.len vs).val = vs.val.length := Slice.len_val vs
  apply loop.spec_decr_nat
    (fun st : Usize × U64 => vs.val.length - st.1.val)
    (fun st : Usize × U64 => st.1.val ≤ vs.val.length ∧
      st.2.val = sumV (vs.val.take st.1.val))
  · rintro ⟨j, t⟩ ⟨hj, ht⟩
    simp only at hj ht
    unfold plan.total_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < vs.val.length := by scalar_tac
      have hget : vs.val[j.val]? = some vs.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
      have hc := U64.checked_add_bv_spec t vs.val[j.val]
      have hsucc := sumV_take_succ vs.val j.val hlt'
      have hle := sumV_take_le vs.val (j.val + 1)
      cases e : U64.checked_add t vs.val[j.val] with
      | none =>
        rw [e] at hc
        simp only [lift, bind_ok, WP.spec_ok]
        simp at hc
        omega
      | some nx =>
        rw [e] at hc
        simp at hc
        have hs1 := Proofs.Time.usize_succ j (by scalar_tac)
        simp only [lift, bind_ok, WP.spec_ok, hs1]
        refine ⟨⟨by omega, by omega⟩, by omega⟩
    · rename_i hge
      simp only [WP.spec_ok]
      have : vs.val.length ≤ j.val := by scalar_tac
      rw [sumV_take_all _ _ this] at ht
      exact ht
  · exact ⟨hi, hs⟩

/-- `total` is the exact sum when it fits, `Overflow` exactly when it does
not. Total. -/
theorem total_spec (vs : Slice U64) :
    plan.total vs ⦃ r => match r with
      | .Fits t => t.val = sumV vs.val
      | .Overflow => U64.max < sumV vs.val ⦄ := by
  unfold plan.total
  exact total_loop_spec vs 0#usize 0#u64 (by simp) (by simp [sumV])

/-! ## The next phase for a leftover second -/

/-- Position `j` comes after `after` in the order largest remainder first,
lower index first among equals (and is a position at all). -/
def cand (rs : List U64) (after : Option Usize) (j : Nat) : Prop :=
  ∃ h : j < rs.length, match after with
    | none => True
    | some a => ∃ ha : a.val < rs.length,
        rs[j].val < rs[a.val].val ∨ (rs[j].val = rs[a.val].val ∧ a.val < j)

theorem after_pick_spec (rs : Slice U64) (after : Option Usize) (j : Usize) (r : U64)
    (hj : j.val < rs.val.length) (hr : rs.val[j.val] = r) :
    plan.after_pick rs after j r ⦃ b => (b = true ↔ cand rs.val after j.val) ⦄ := by
  unfold plan.after_pick
  cases after with
  | none => simp [WP.spec_ok, cand, hj]
  | some a =>
    simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
      core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq]
    by_cases ha : a.val < rs.val.length
    · have hget : rs.val[a.val]? = some rs.val[a.val] := List.getElem?_eq_getElem ha
      simp only [hget, bind_ok]
      subst hr
      unfold cand
      split_ifs with h1 h2
      · simp only [WP.spec_ok, true_iff]
        exact ⟨hj, ha, Or.inl (by scalar_tac)⟩
      · simp only [WP.spec_ok]
        constructor
        · intro hd
          exact ⟨hj, ha, Or.inr ⟨by rw [h2], by simp at hd; scalar_tac⟩⟩
        · rintro ⟨_, _, h | ⟨_, h⟩⟩
          · exact absurd h (by scalar_tac)
          · simp; scalar_tac
      · simp only [WP.spec_ok, Bool.false_eq_true, false_iff]
        rintro ⟨_, _, h | ⟨h, _⟩⟩
        · exact h1 (by scalar_tac)
        · exact h2 (by scalar_tac)
    · have hget : rs.val[a.val]? = none := List.getElem?_eq_none (by omega)
      simp only [hget, bind_ok, WP.spec_ok, cand]
      simp [ha]

theorem beats_spec (best : Option Usize) (br r : U64) :
    plan.beats best br r ⦃ b => (b = true ↔ (best = none ∨ br.val < r.val)) ⦄ := by
  unfold plan.beats
  cases best <;> simp [WP.spec_ok]

/-- One scan step of `pick`: position `i` replaces the best so far exactly
when it is a candidate and the first one or strictly larger. -/
theorem pick_step (rs : Slice U64) (after : Option Usize) (i : Usize) (ri : U64)
    (b : Option Usize) (r : U64) (hi : i.val < rs.val.length) (hri : rs.val[i.val] = ri) :
    (do
      let c ← plan.after_pick rs after i ri
      let take ← if c then plan.beats b r ri else ok false
      if take then ok (some i, ri) else ok (b, r))
      ⦃ p => ((cand rs.val after i.val ∧ (b = none ∨ r.val < ri.val)) → p.1 = some i ∧ p.2 = ri) ∧
             (¬ (cand rs.val after i.val ∧ (b = none ∨ r.val < ri.val)) → p.1 = b ∧ p.2 = r) ⦄ := by
  apply WP.spec_bind (after_pick_spec rs after i ri hi hri)
  intro c hc
  cases c with
  | true =>
    simp only [↓reduceIte]
    apply WP.spec_bind (beats_spec b r ri)
    intro t ht
    cases t with
    | true =>
      simp only [↓reduceIte]
      rw [WP.spec_ok]
      exact ⟨fun _ => ⟨rfl, rfl⟩, fun h => absurd ⟨hc.mp rfl, ht.mp rfl⟩ h⟩
    | false =>
      simp only [Bool.false_eq_true, ↓reduceIte]
      rw [WP.spec_ok]
      exact ⟨fun h => absurd (ht.mpr h.2) (by simp), fun _ => ⟨rfl, rfl⟩⟩
  | false =>
    simp only [Bool.false_eq_true, ↓reduceIte, bind_ok]
    rw [WP.spec_ok]
    exact ⟨fun h => absurd (hc.mpr h.1) (by simp), fun _ => ⟨rfl, rfl⟩⟩

/-- The pick invariant after scanning positions below `j`. -/
def pickInv (rs : List U64) (after : Option Usize) (j : Nat)
    (best : Option Usize) (br : U64) : Prop :=
  (best = none → ∀ k, k < j → ¬ cand rs after k) ∧
  (∀ b, best = some b →
    b.val < j ∧ cand rs after b.val ∧ ∃ hb : b.val < rs.length, rs[b.val] = br ∧
    ∀ k (hk : k < rs.length), k < j → cand rs after k →
      rs[k].val ≤ br.val ∧ (rs[k].val = br.val → b.val ≤ k))

/-- The pick result: the first phase in remainder order after `after`. -/
def pickPost (rs : List U64) (after : Option Usize) (r : Option Usize) : Prop :=
  match r with
  | none => ∀ k, ¬ cand rs after k
  | some b => cand rs after b.val ∧ ∃ hb : b.val < rs.length,
      ∀ k (hk : k < rs.length), cand rs after k →
        rs[k].val ≤ rs[b.val].val ∧ (rs[k].val = rs[b.val].val → b.val ≤ k)

theorem pick_loop_spec (rs : Slice U64) (after : Option Usize) (j : Usize)
    (best : Option Usize) (br : U64)
    (hj : j.val ≤ rs.val.length) (hinv : pickInv rs.val after j.val best br) :
    plan.pick_loop rs after j best br ⦃ r => pickPost rs.val after r ⦄ := by
  unfold plan.pick_loop
  have hlen : (Slice.len rs).val = rs.val.length := Slice.len_val rs
  apply loop.spec_decr_nat
    (fun st : Usize × Option Usize × U64 => rs.val.length - st.1.val)
    (fun st : Usize × Option Usize × U64 => st.1.val ≤ rs.val.length ∧
      pickInv rs.val after st.1.val st.2.1 st.2.2)
  · rintro ⟨i, b, r⟩ ⟨hi, hb1, hb2⟩
    simp only at hi hb1 hb2
    unfold plan.pick_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : i.val < rs.val.length := by scalar_tac
      have hget : rs.val[i.val]? = some rs.val[i.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok,
        bind_assoc_eq]
      have hs1 := Proofs.Time.usize_succ i (by scalar_tac)
      apply WP.spec_bind (pick_step rs after i rs.val[i.val] b r hlt' rfl)
      rintro ⟨b', r'⟩ hp
      simp only [lift, bind_ok]
      show ok (ControlFlow.cont (core.num.Usize.saturating_add i 1#usize, b', r')) ⦃ _ ⦄
      simp only [WP.spec_ok, hs1]
      by_cases htake : cand rs.val after i.val ∧ (b = none ∨ r.val < rs.val[i.val].val)
      · obtain ⟨hb', hr'⟩ := hp.1 htake
        simp only at hb' hr'
        rw [hb', hr']
        simp only [pickInv]
        obtain ⟨hcand, hbetter⟩ := htake
        refine ⟨⟨by omega, ?_, ?_⟩, by omega⟩
        · intro h; cases h
        · intro b0 hb0
          cases hb0
          refine ⟨by omega, hcand, hlt', rfl, ?_⟩
          intro k hk hkj hck
          by_cases hki : k = i.val
          · subst hki; exact ⟨le_refl _, fun _ => le_refl _⟩
          · cases b with
            | none => exact absurd hck (hb1 rfl k (by omega))
            | some b1 =>
              obtain ⟨_, _, _, _, hall⟩ := hb2 b1 rfl
              have h1 := hall k hk (by omega) hck
              rcases hbetter with hn | hlt2
              · cases hn
              · refine ⟨by omega, fun _ => by omega⟩
      · obtain ⟨hb', hr'⟩ := hp.2 htake
        simp only at hb' hr'
        rw [hb', hr']
        simp only [pickInv]
        refine ⟨⟨by omega, ?_, ?_⟩, by omega⟩
        · intro hn k hk
          by_cases hki : k = i.val
          · subst hki
            intro hck
            exact htake ⟨hck, Or.inl hn⟩
          · exact hb1 hn k (by omega)
        · intro b0 hb0
          obtain ⟨hb0j, hcb, hbl, hbr, hall⟩ := hb2 b0 hb0
          refine ⟨by omega, hcb, hbl, hbr, ?_⟩
          intro k hk hkj hck
          by_cases hki : k = i.val
          · subst hki
            have hnot : ¬ r.val < rs.val[i.val].val := fun h => htake ⟨hck, Or.inr h⟩
            subst hbr
            refine ⟨by omega, fun _ => by omega⟩
          · exact hall k hk (by omega) hck
    · rename_i hge
      have hge' : rs.val.length ≤ i.val := by scalar_tac
      simp only [WP.spec_ok]
      cases b with
      | none =>
        intro k hck
        exact hb1 rfl k (by obtain ⟨hk, _⟩ := hck; omega) hck
      | some b0 =>
        obtain ⟨_, hcb, hbl, hbr, hall⟩ := hb2 b0 rfl
        refine ⟨hcb, hbl, ?_⟩
        intro k hk hck
        rw [hbr]
        exact hall k hk (by omega) hck
  · exact ⟨hj, hinv⟩

/-- `pick` returns the first phase in the order largest remainder first,
lower index first among equals, among those after `after`; `none` only when
there is none. Total. -/
theorem pick_spec (rs : Slice U64) (after : Option Usize) :
    plan.pick rs after ⦃ r => pickPost rs.val after r ⦄ := by
  unfold plan.pick
  apply pick_loop_spec rs after 0#usize none 0#u64 (by simp)
  refine ⟨fun _ k hk => by simp at hk, fun b hb => by cases hb⟩

/-- Among equal remainders the extra second goes to the lower index: when
`pick` serves `i`, no position `k < i` with the same remainder is still
waiting, and no waiting position has a larger remainder. -/
theorem allocate_tie_to_lower_index (rs : Slice U64) (after : Option Usize) :
    plan.pick rs after ⦃ r => ∀ i, r = some i →
      ∀ k (hk : k < rs.val.length) (hi : i.val < rs.val.length), cand rs.val after k →
        rs.val[k].val = rs.val[i.val].val → i.val ≤ k ⦄ := by
  apply WP.spec_mono (pick_spec rs after)
  intro r hr i hri k hk hi hck heq
  subst hri
  obtain ⟨_, _, hall⟩ := hr
  exact (hall k hk hck).2 heq

/-! ## Shares: minimum plus weighted floor -/

/-- What `shares` promises on success: the seconds keep their length, every
phase has at least its minimum, and they sum to the minimums plus the
returned floor total. -/
def sharesPost (mins sec : List U64) (r : plan.Sum) : Prop :=
  match r with
  | .Overflow => True
  | .Fits a => sec.length = mins.length ∧
      (∀ k (hk : k < sec.length) (hm : k < mins.length), mins[k].val ≤ sec[k].val) ∧
      sumV sec = sumV mins + a.val

/-- The shares loop invariant after positions below `i`. -/
def sharesInv (mins sec : List U64) (i : Nat) (a : U64) : Prop :=
  i ≤ mins.length ∧ sec.length = mins.length ∧
  (∀ k (hk : k < sec.length) (hm : k < mins.length), k < i → mins[k].val ≤ sec[k].val) ∧
  sumV (sec.take i) = sumV (mins.take i) + a.val

theorem shares_loop_spec (remaining divisor : U64) (mins ws sec rems : Slice U64)
    (i : Usize) (a : U64) (hinv : sharesInv mins.val sec.val i.val a) :
    plan.shares_loop remaining divisor mins ws sec rems i a
      ⦃ r => sharesPost mins.val r.2.1.val r.1 ⦄ := by
  unfold plan.shares_loop
  have hlen : (Slice.len mins).val = mins.val.length := Slice.len_val mins
  apply loop.spec_decr_nat
    (fun st : Slice U64 × Slice U64 × Usize × U64 => mins.val.length - st.2.2.1.val)
    (fun st : Slice U64 × Slice U64 × Usize × U64 => sharesInv mins.val st.1.val st.2.2.1.val st.2.2.2)
  · rintro ⟨s1, r1, j, al⟩ ⟨hj, hsl, hpt, hsum⟩
    simp only at hj hsl hpt hsum
    unfold plan.shares_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < mins.val.length := by scalar_tac
      have hget : mins.val[j.val]? = some mins.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
      cases hw : ws.val[j.val]? with
      | none => simp only [hw, WP.spec_ok, sharesPost]
      | some w =>
        simp only [hw, bind_ok]
        have h1 := U64.checked_mul_bv_spec remaining w
        cases e1 : U64.checked_mul remaining w with
        | none => simp only [lift, bind_ok, WP.spec_ok, sharesPost]
        | some sc =>
          have h2 := U64.checked_div_bv_spec sc divisor
          simp only [lift, bind_ok]
          cases e2 : U64.checked_div sc divisor with
          | none => simp only [lift, bind_ok, WP.spec_ok, sharesPost]
          | some f =>
            have h3 := U64.checked_rem_bv_spec sc divisor
            cases e3 : U64.checked_rem sc divisor with
            | none => simp only [lift, bind_ok, WP.spec_ok, sharesPost]
            | some rm =>
              simp only [lift, bind_ok]
              split
              · simp only [WP.spec_ok, sharesPost]
              · rename_i sh e4
                have h4 := U64.checked_add_bv_spec mins.val[j.val] f
                rw [e4] at h4
                split
                · simp only [WP.spec_ok, sharesPost]
                · rename_i al' e5
                  have h5 := U64.checked_add_bv_spec al f
                  rw [e5] at h5
                  simp at h4 h5
                  simp only [core.slice.Slice.get_mut, core.slice.index.Usize.get_mut, bind_ok]
                  have hjs : j.val < s1.val.length := by omega
                  have hs : s1[j]? = some s1.val[j.val] := by
                    rw [Slice.getElem?_Usize_eq]; exact List.getElem?_eq_getElem hjs
                  simp only [hs, e4]
                  simp
                  split
                  · simp only [WP.spec_ok, sharesPost]
                  · have hs1 := Proofs.Time.usize_succ j (by scalar_tac)
                    simp only [WP.spec_ok, hs1, Slice.set_opt_val_eq,
                      Proofs.Ledger.set_opt_some, List.length_set, sharesInv]
                    refine ⟨⟨by omega, hsl, ?_, ?_⟩, by omega⟩
                    · intro k hk hm hkj
                      by_cases hkj' : k = j.val
                      · subst hkj'
                        simp only [List.getElem_set_self]
                        omega
                      · rw [List.getElem_set_ne (Ne.symm hkj')]
                        exact hpt k (by simpa using hk) hm (by omega)
                    · rw [take_set_self _ _ _ hjs, sumV_append, sumV_take_succ _ _ hlt']
                      simp [sumV] at hsum ⊢
                      omega
    · rename_i hge
      have hge' : mins.val.length ≤ j.val := by scalar_tac
      simp only [WP.spec_ok, sharesPost]
      refine ⟨hsl, fun k hk hm => hpt k hk hm (by omega), ?_⟩
      rw [sumV_take_all _ _ (by omega), sumV_take_all _ _ (by omega)] at hsum
      exact hsum
  · exact hinv

/-- On success `shares` keeps the length, gives every phase at least its
minimum, and the seconds sum to the minimums plus the returned floors.
Total. -/
theorem shares_spec (remaining divisor : U64) (mins ws sec rems : Slice U64)
    (hl : sec.val.length = mins.val.length) :
    plan.shares remaining divisor mins ws sec rems
      ⦃ r => sharesPost mins.val r.2.1.val r.1 ⦄ := by
  unfold plan.shares
  apply shares_loop_spec
  refine ⟨by simp, hl, fun k _ _ h => by simp at h, by simp [sumV]⟩

/-! ## Handing out the leftover seconds -/

/-- What `hand_out` promises on success: the length is kept, no phase loses
a second, and exactly `leftover` seconds were added. -/
def handPost (sec : List U64) (leftover : U64) (r : Bool × Slice U64) : Prop :=
  r.1 = true → r.2.val.length = sec.length ∧
    (∀ k (hk : k < sec.length) (hk' : k < r.2.val.length), sec[k].val ≤ r.2.val[k].val) ∧
    sumV r.2.val = sumV sec + leftover.val

theorem hand_out_loop_spec (rems sec : Slice U64) (leftover : U64) (k : U64)
    (prev : Option Usize) (s0 : List U64)
    (hl : sec.val.length = s0.length)
    (hpt : ∀ i (hi : i < s0.length) (hi' : i < sec.val.length), s0[i].val ≤ sec.val[i].val)
    (hs : sumV sec.val = sumV s0 + k.val) (hk : k.val ≤ leftover.val) :
    plan.hand_out_loop rems sec leftover k prev ⦃ r => handPost s0 leftover r ⦄ := by
  unfold plan.hand_out_loop
  apply loop.spec_decr_nat
    (fun st : Slice U64 × U64 × Option Usize => leftover.val - st.2.1.val)
    (fun st : Slice U64 × U64 × Option Usize =>
      st.1.val.length = s0.length ∧
      (∀ i (hi : i < s0.length) (hi' : i < st.1.val.length), s0[i].val ≤ st.1.val[i].val) ∧
      sumV st.1.val = sumV s0 + st.2.1.val ∧ st.2.1.val ≤ leftover.val)
  · rintro ⟨s1, k1, p1⟩ ⟨hl1, hpt1, hs1, hk1⟩
    simp only at hl1 hpt1 hs1 hk1
    unfold plan.hand_out_loop.body
    simp only []
    split
    · rename_i hlt
      apply WP.spec_bind (pick_spec rems p1)
      intro o _
      cases o with
      | none => simp only [WP.spec_ok, handPost]; intro h; cases h
      | some n =>
        simp only [core.slice.Slice.get_mut, core.slice.index.SliceIndexUsizeSlice,
          core.slice.index.Usize.get_mut, bind_ok]
        cases hsn : s1[n]? with
        | none =>
          simp only [WP.spec_ok, handPost]
          simp
        | some slot =>
          simp
          have hc := U64.checked_add_bv_spec slot 1#u64
          cases e1 : U64.checked_add slot 1#u64 with
          | none => simp [lift, bind_ok, WP.spec_ok, handPost]
          | some v =>
            rw [e1] at hc
            simp at hc
            have hc2 := U64.checked_add_bv_spec k1 1#u64
            cases e2 : U64.checked_add k1 1#u64 with
            | none => simp [lift, bind_ok, WP.spec_ok, handPost]
            | some k2 =>
              rw [e2] at hc2
              simp at hc2
              simp only [lift, bind_ok, WP.spec_ok, Slice.set_opt_val_eq,
                Proofs.Ledger.set_opt_some, List.length_set]
              have hn : n.val < s1.val.length := by
                rw [Slice.getElem?_Usize_eq] at hsn
                exact (List.getElem?_eq_some_iff.mp hsn).1
              have hslot : s1.val[n.val] = slot := by
                rw [Slice.getElem?_Usize_eq] at hsn
                exact (List.getElem?_eq_some_iff.mp hsn).2
              have hset := sumV_set s1.val n.val v hn
              have hlt' : k1.val < leftover.val := by scalar_tac
              refine ⟨hl1, ?_, ?_, by omega, by omega⟩
              · intro i hi hi'
                by_cases hin : i = n.val
                · subst hin
                  simp only [List.getElem_set_self]
                  have := hpt1 n.val hi (by omega)
                  rw [hslot] at this
                  omega
                · rw [List.getElem_set_ne (Ne.symm hin)]
                  exact hpt1 i hi (by simpa using hi')
              · rw [hslot] at hset
                omega
    · rename_i hge
      simp only [WP.spec_ok, handPost]
      intro _
      refine ⟨hl1, fun i hi hi' => hpt1 i hi hi', ?_⟩
      have : k1.val = leftover.val := by scalar_tac
      omega
  · exact ⟨hl, hpt, hs, hk⟩

/-- On success `hand_out` adds exactly `leftover` seconds and takes none
away. Total. -/
theorem hand_out_spec (rems sec : Slice U64) (leftover : U64) :
    plan.hand_out rems sec leftover ⦃ r => handPost sec.val leftover r ⦄ := by
  unfold plan.hand_out
  exact hand_out_loop_spec rems sec leftover 0#u64 none sec.val rfl
    (fun _ _ _ => le_refl _) (by simp) (by simp)

/-! ## The allocation -/

/-- What `allocate` promises: on `Done` the seconds have the phases' length,
keep every minimum and sum exactly to the budget; `Infeasible` is returned
exactly when the shapes agree and the minimums sum (fits and) exceeds the
budget. -/
def allocPost (budget : U64) (mins ws sec rems : List U64)
    (r : plan.Allocation × Slice U64 × Slice U64) : Prop :=
  (r.1 = .Done → r.2.1.val.length = mins.length ∧
    (∀ k (hk : k < mins.length) (hk' : k < r.2.1.val.length), mins[k].val ≤ r.2.1.val[k].val) ∧
    sumV r.2.1.val = budget.val) ∧
  (r.1 = .Infeasible ↔ (mins.length ≠ 0 ∧ ws.length = mins.length ∧
    sec.length = mins.length ∧ rems.length = mins.length ∧
    sumV mins ≤ U64.max ∧ budget.val < sumV mins))

/-- The part of `allocate` after the remaining seconds `r` and the divisor are
known: it never reports `Infeasible`, and on `Done` the seconds keep the
minimums and sum to the minimums plus `r`. -/
theorem allocate_tail (r divisor : U64) (mins ws sec rems : Slice U64)
    (hl : sec.val.length = mins.val.length) :
    (do
      let (s2, seconds1, remainders1) ← plan.shares r divisor mins ws sec rems
      match s2 with
      | plan.Sum.Fits a =>
        match U64.checked_sub r a with
        | none => ok (plan.Allocation.Overflow, seconds1, remainders1)
        | some l =>
          let (b, seconds2) ← plan.hand_out remainders1 seconds1 l
          if b then ok (plan.Allocation.Done, seconds2, remainders1)
          else ok (plan.Allocation.Overflow, seconds2, remainders1)
      | plan.Sum.Overflow => ok (plan.Allocation.Overflow, seconds1, remainders1))
      ⦃ x => x.1 ≠ .Infeasible ∧ (x.1 = .Done → x.2.1.val.length = mins.val.length ∧
        (∀ k (hk : k < mins.val.length) (hk' : k < x.2.1.val.length),
          mins.val[k].val ≤ x.2.1.val[k].val) ∧
        sumV x.2.1.val = sumV mins.val + r.val) ⦄ := by
  apply WP.spec_bind (shares_spec r divisor mins ws sec rems hl)
  rintro ⟨s2, sec1, rem1⟩ hsh
  cases s2 with
  | Overflow => simp [WP.spec_ok]
  | Fits a =>
    simp only [sharesPost] at hsh
    obtain ⟨hl1, hp1, hs1⟩ := hsh
    try simp only [lift, bind_ok]
    simp
    split
    · simp [WP.spec_ok]
    · rename_i l e
      have hc := U64.checked_sub_bv_spec r a
      rw [e] at hc
      simp at hc
      apply WP.spec_bind (hand_out_spec rem1 sec1 l)
      rintro ⟨b, sec2⟩ hh
      cases b with
      | false =>
        show (ok (plan.Allocation.Overflow, sec2, rem1)) ⦃ _ ⦄
        simp [WP.spec_ok]
      | true =>
        obtain ⟨hl2, hp2, hs2⟩ := hh rfl
        simp only at hl2 hp2 hs2
        show (ok (plan.Allocation.Done, sec2, rem1)) ⦃ _ ⦄
        simp only [WP.spec_ok]
        refine ⟨by simp, fun _ => ⟨by omega, ?_, by omega⟩⟩
        intro k hk hk'
        have h1 := hp1 k (by omega) hk
        have h2 := hp2 k (by omega) hk'
        omega

theorem allocate_spec (budget : U64) (mins ws sec rems : Slice U64) :
    plan.allocate budget mins ws sec rems
      ⦃ r => allocPost budget mins.val ws.val sec.val rems.val r ⦄ := by
  unfold plan.allocate
  have hm := Slice.len_val mins
  have hw := Slice.len_val ws
  have hs := Slice.len_val sec
  have hr := Slice.len_val rems
  dsimp only
  split
  · rename_i h0
    simp only [WP.spec_ok, allocPost]
    refine ⟨fun h => (by cases h), fun h => (by cases h), fun h => absurd ?_ h.1⟩
    scalar_tac
  · rename_i h0
    split
    · rename_i h1
      simp only [WP.spec_ok, allocPost]
      refine ⟨fun h => (by cases h), fun h => (by cases h), fun h => absurd h.2.1 (by simp at h1; scalar_tac)⟩
    · rename_i h1
      split
      · rename_i h2
        simp only [WP.spec_ok, allocPost]
        refine ⟨fun h => (by cases h), fun h => (by cases h), fun h => absurd h.2.2.1 (by simp at h2; scalar_tac)⟩
      · rename_i h2
        split
        · rename_i h3
          simp only [WP.spec_ok, allocPost]
          refine ⟨fun h => (by cases h), fun h => (by cases h), fun h => absurd h.2.2.2.1 (by simp at h3; scalar_tac)⟩
        · rename_i h3
          simp at h1 h2 h3
          have hwl : ws.val.length = mins.val.length := by scalar_tac
          have hsl : sec.val.length = mins.val.length := by scalar_tac
          have hrl : rems.val.length = mins.val.length := by scalar_tac
          have hne : mins.val.length ≠ 0 := by scalar_tac
          apply WP.spec_bind (total_spec mins)
          intro t ht
          cases t with
          | Overflow =>
            simp only [WP.spec_ok, allocPost]
            refine ⟨fun h => (by cases h), fun h => (by cases h), fun h => absurd h.2.2.2.2.1 (by omega)⟩
          | Fits tm =>
            simp only at ht
            simp only [lift, bind_ok]
            split
            · rename_i e
              have hc := U64.checked_sub_bv_spec budget tm
              rw [e] at hc
              simp only [WP.spec_ok, allocPost]
              refine ⟨fun h => (by cases h), fun _ =>
                ⟨hne, hwl, hsl, hrl, by scalar_tac, by omega⟩, fun _ => trivial⟩
            · rename_i rm e
              have hc := U64.checked_sub_bv_spec budget tm
              rw [e] at hc
              simp at hc
              apply WP.spec_bind (total_spec ws)
              intro tw htw
              have notInf : ¬ (mins.val.length ≠ 0 ∧ ws.val.length = mins.val.length ∧
                  sec.val.length = mins.val.length ∧ rems.val.length = mins.val.length ∧
                  sumV mins.val ≤ U64.max ∧ budget.val < sumV mins.val) := fun h => by omega
              cases tw with
              | Overflow =>
                simp only [WP.spec_ok, allocPost]
                exact ⟨fun h => (by cases h), fun h => (by cases h), fun h => absurd h notInf⟩
              | Fits wt =>
                simp only
                split
                · rename_i hz0
                  split
                  · simp only [WP.spec_ok, allocPost]
                    exact ⟨fun h => (by cases h), fun h => (by cases h), fun h => absurd h notInf⟩
                  · simp only [hz0, ↓reduceIte, bind_ok]
                    apply WP.spec_mono (allocate_tail rm 1#u64 mins ws sec rems hsl)
                    rintro x ⟨hx1, hx2⟩
                    refine ⟨fun hd => ?_, ⟨fun h => absurd h hx1, fun h => absurd h notInf⟩⟩
                    obtain ⟨a, b, c⟩ := hx2 hd
                    exact ⟨a, fun k hk hk' => b k hk hk', by omega⟩
                · rename_i hz
                  simp only [hz, ↓reduceIte, bind_ok]
                  apply WP.spec_mono (allocate_tail rm wt mins ws sec rems hsl)
                  rintro x ⟨hx1, hx2⟩
                  refine ⟨fun hd => ?_, ⟨fun h => absurd h hx1, fun h => absurd h notInf⟩⟩
                  obtain ⟨a, b, c⟩ := hx2 hd
                  exact ⟨a, fun k hk hk' => b k hk hk', by omega⟩

/-- On `Done` the seconds sum exactly to the budget. -/
theorem allocate_sum_exact (budget : U64) (mins ws sec rems : Slice U64) :
    plan.allocate budget mins ws sec rems
      ⦃ r => r.1 = .Done → sumV r.2.1.val = budget.val ⦄ := by
  apply WP.spec_mono (allocate_spec budget mins ws sec rems)
  intro r hr hd
  exact (hr.1 hd).2.2

/-- On `Done` every phase gets at least its minimum. -/
theorem allocate_keeps_minimums (budget : U64) (mins ws sec rems : Slice U64) :
    plan.allocate budget mins ws sec rems
      ⦃ r => r.1 = .Done → r.2.1.val.length = mins.val.length ∧
        ∀ k (hk : k < mins.val.length) (hk' : k < r.2.1.val.length),
          mins.val[k].val ≤ r.2.1.val[k].val ⦄ := by
  apply WP.spec_mono (allocate_spec budget mins ws sec rems)
  intro r hr hd
  exact ⟨(hr.1 hd).1, (hr.1 hd).2.1⟩

/-- `Infeasible` exactly when the minimums sum above the budget (for
well-shaped input whose minimums sum fits). -/
theorem allocate_infeasible_exact (budget : U64) (mins ws sec rems : Slice U64) :
    plan.allocate budget mins ws sec rems
      ⦃ r => (r.1 = .Infeasible ↔ (mins.val.length ≠ 0 ∧ ws.val.length = mins.val.length ∧
        sec.val.length = mins.val.length ∧ rems.val.length = mins.val.length ∧
        sumV mins.val ≤ U64.max ∧ budget.val < sumV mins.val)) ⦄ := by
  apply WP.spec_mono (allocate_spec budget mins ws sec rems)
  intro r hr
  exact hr.2

end Proofs.Plan
