import Extracted.CadenceKernel
import Proofs.Time
import Proofs.Plan

/-!
# Bounded candidate selection (`cadence_kernel::select`, VK-04, #35)

`cadence_trainer::candidate::scan_select` (called by
`trainer::plan_words::WordSelector::select` for every word slot) converts the
typed candidates and asks `select.scan` for the best one; `utility` is the
weighted mean the scan ranks by.

Proved: the utility is `sum(w*t)/sum(w)` over the capped terms (0 when every
weight is 0), never above 10,000, and monotone in every term; the scan
returns `TooLarge` exactly when the vector exceeds the bound, otherwise an
eligible candidate whose `(utility, tie_break, id)` no eligible candidate
exceeds (the first among equal keys), and `Empty` exactly when none is
eligible.

Trusted, not proved: that the adapter passes `BasisPoints` values (so the
cap never changes a term), the policy bound, and maps the index back to the
candidate's `WordId` (tested in `crates/trainer/src/candidate.rs`, including a
sweep against the previous implementation).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Select

/-- A term capped to the basis-point scale. -/
def capN (v : Nat) : Nat := min v 10000

@[step]
theorem cap_spec (v : U64) : select.cap v ⦃ r => r.val = capN v.val ∧ r.val ≤ 10000 ⦄ := by
  unfold select.cap capN
  split <;> simp_all [WP.spec_ok, select.BASIS] <;> scalar_tac

@[step]
theorem mul_spec (a b : U64) (ha : a.val ≤ 10000) (hb : b.val ≤ 10000) :
    select.mul a b ⦃ r => r.val = a.val * b.val ∧ r.val ≤ 100000000 ⦄ := by
  unfold select.mul
  have hp : a.val * b.val ≤ 100000000 := Nat.mul_le_mul ha hb |>.trans (by norm_num)
  have h := U64.checked_mul_bv_spec a b
  cases e : U64.checked_mul a b with
  | none => rw [e] at h; simp at h; scalar_tac
  | some p =>
    rw [e] at h
    simp [lift, core.option.Option.unwrap_or, WP.spec_ok]
    omega

@[step]
theorem add_spec (a b : U64) (ha : a.val ≤ 1000000000) (hb : b.val ≤ 1000000000) :
    select.add a b ⦃ r => r.val = a.val + b.val ⦄ := by
  unfold select.add
  have h := U64.checked_add_bv_spec a b
  cases e : U64.checked_add a b with
  | none => rw [e] at h; simp at h; scalar_tac
  | some p =>
    rw [e] at h
    simp [lift, core.option.Option.unwrap_or, WP.spec_ok]
    omega

/-- The weighted numerator over capped terms. -/
def num (i w : select.Terms) : Nat :=
  capN w.error.val * capN i.error.val + capN w.hesitation.val * capN i.hesitation.val +
  capN w.relevance.val * capN i.relevance.val + capN w.uncertainty.val * capN i.uncertainty.val

/-- The weight total over capped weights. -/
def den (w : select.Terms) : Nat :=
  capN w.error.val + capN w.hesitation.val + capN w.relevance.val + capN w.uncertainty.val

theorem capN_le (v : Nat) : capN v ≤ 10000 := Nat.min_le_right _ _

theorem num_le (i w : select.Terms) : num i w ≤ 10000 * den w := by
  unfold num den
  have h1 := Nat.mul_le_mul_left (capN w.error.val) (capN_le i.error.val)
  have h2 := Nat.mul_le_mul_left (capN w.hesitation.val) (capN_le i.hesitation.val)
  have h3 := Nat.mul_le_mul_left (capN w.relevance.val) (capN_le i.relevance.val)
  have h4 := Nat.mul_le_mul_left (capN w.uncertainty.val) (capN_le i.uncertainty.val)
  nlinarith

/-- The utility is the weighted mean of the capped terms (Nat division, so 0
when every weight is 0). Total. -/
theorem utility_spec (i w : select.Terms) :
    select.utility i w ⦃ r => r.val = num i w / den w ⦄ := by
  unfold select.utility
  step* <;> try scalar_tac
  cases o <;> simp_all [core.option.Option.unwrap_or, num, den, WP.spec_ok]

/-- The utility is never above 10,000 basis points and is exactly the
weighted mean. Total. -/
theorem utility_in_range (i w : select.Terms) :
    select.utility i w ⦃ r => r.val ≤ 10000 ∧ r.val = num i w / den w ⦄ := by
  apply WP.spec_mono (utility_spec i w)
  intro r hr
  refine ⟨?_, hr⟩
  rw [hr]
  rcases Nat.eq_zero_or_pos (den w) with h0 | hpos
  · simp [h0]
  · exact (Nat.div_le_iff_le_mul_add_pred hpos).2 (by have := num_le i w; nlinarith)

theorem capN_mono {a b : Nat} (h : a ≤ b) : capN a ≤ capN b := min_le_min_right _ h

/-- Raising any term never lowers the utility. -/
theorem utility_monotone (a b w : select.Terms)
    (he : a.error.val ≤ b.error.val) (hh : a.hesitation.val ≤ b.hesitation.val)
    (hr : a.relevance.val ≤ b.relevance.val) (hu : a.uncertainty.val ≤ b.uncertainty.val) :
    select.utility a w ⦃ ra => select.utility b w ⦃ rb => ra.val ≤ rb.val ⦄ ⦄ := by
  apply WP.spec_mono (utility_spec a w)
  intro ra hra
  apply WP.spec_mono (utility_spec b w)
  intro rb hrb
  rw [hra, hrb]
  apply Nat.div_le_div_right
  unfold num
  have := Nat.mul_le_mul_left (capN w.error.val) (capN_mono he)
  have := Nat.mul_le_mul_left (capN w.hesitation.val) (capN_mono hh)
  have := Nat.mul_le_mul_left (capN w.relevance.val) (capN_mono hr)
  have := Nat.mul_le_mul_left (capN w.uncertainty.val) (capN_mono hu)
  omega

/-! ## The scan -/

/-- Strict lexicographic order on `(utility, tie_break, id)`. -/
def gt (u t id bu bt bid : Nat) : Prop :=
  u > bu ∨ (u = bu ∧ (t > bt ∨ (t = bt ∧ id > bid)))

/-- The utility of a candidate, in `Nat`. -/
def util (w : select.Terms) (c : select.Candidate) : Nat := num c.inputs w / den w

/-- Candidate `k` has a greater key than candidate `b`. -/
def better (w : select.Terms) (cs : List select.Candidate) (k b : Nat)
    (hk : k < cs.length) (hb : b < cs.length) : Prop :=
  gt (util w cs[k]) cs[k].tie_break.val cs[k].id.val
     (util w cs[b]) cs[b].tie_break.val cs[b].id.val

theorem greater_spec (u t id bu bt bid : U64) :
    select.greater u t id bu bt bid
      ⦃ r => (r = true ↔ gt u.val t.val id.val bu.val bt.val bid.val) ⦄ := by
  unfold select.greater gt
  split_ifs <;> simp_all [WP.spec_ok] <;> scalar_tac

theorem beats_spec (best : Option Usize) (u t id bu bt bid : U64) :
    select.beats best u t id bu bt bid
      ⦃ r => (r = true ↔ (best = none ∨ gt u.val t.val id.val bu.val bt.val bid.val)) ⦄ := by
  unfold select.beats
  cases best with
  | none => simp [WP.spec_ok]
  | some _ =>
    apply WP.spec_mono (greater_spec u t id bu bt bid)
    intro r hr
    simp [hr]

/-- One scan step: candidate `c` at `i` replaces the best so far exactly when
it is eligible and the first eligible one or strictly greater. -/
theorem scan_step (w : select.Terms) (c : select.Candidate) (i : Usize)
    (best : Option Usize) (bu bt bid : U64) :
    (do
      let u ← select.utility c.inputs w
      let take ← if c.eligible then select.beats best u c.tie_break c.id bu bt bid else ok false
      if take then ok (some i, u, c.tie_break, c.id) else ok (best, bu, bt, bid))
      ⦃ p =>
        ((c.eligible = true ∧ (best = none ∨
            gt (util w c) c.tie_break.val c.id.val bu.val bt.val bid.val)) →
          p.1 = some i ∧ p.2.1.val = util w c ∧ p.2.2.1 = c.tie_break ∧ p.2.2.2 = c.id) ∧
        (¬ (c.eligible = true ∧ (best = none ∨
            gt (util w c) c.tie_break.val c.id.val bu.val bt.val bid.val)) →
          p = (best, bu, bt, bid)) ⦄ := by
  apply WP.spec_bind (utility_spec c.inputs w)
  intro u hu
  cases he : c.eligible with
  | true =>
    simp only [↓reduceIte]
    apply WP.spec_bind (beats_spec best u c.tie_break c.id bu bt bid)
    intro t ht
    rw [hu] at ht
    cases t with
    | true =>
      simp only [↓reduceIte]
      rw [WP.spec_ok]
      refine ⟨fun _ => ⟨rfl, hu, rfl, rfl⟩, fun h => absurd ⟨by first | trivial | rfl, ?_⟩ h⟩
      exact ht.mp rfl
    | false =>
      simp only [Bool.false_eq_true, ↓reduceIte]
      rw [WP.spec_ok]
      refine ⟨fun h => absurd (ht.mpr h.2) (by simp), fun _ => rfl⟩
  | false =>
    simp only [Bool.false_eq_true, ↓reduceIte, bind_ok]
    rw [WP.spec_ok]
    refine ⟨fun h => absurd h.1 (by simp), fun _ => rfl⟩

/-- The scan invariant after positions below `i`. -/
def scanInv (w : select.Terms) (cs : List select.Candidate) (i : Nat)
    (best : Option Usize) (bu bt bid : U64) : Prop :=
  (best = none → ∀ k (hk : k < cs.length), k < i → cs[k].eligible = false) ∧
  (∀ b, best = some b → ∃ hb : b.val < cs.length, b.val < i ∧ cs[b.val].eligible = true ∧
    bu.val = util w cs[b.val] ∧ bt = cs[b.val].tie_break ∧ bid = cs[b.val].id ∧
    ∀ k (hk : k < cs.length), k < i → cs[k].eligible = true → ¬ better w cs k b.val hk hb)

/-- What the scan loop returns. -/
def scanPost (w : select.Terms) (cs : List select.Candidate) (r : Option Usize) : Prop :=
  match r with
  | none => ∀ k (hk : k < cs.length), cs[k].eligible = false
  | some b => ∃ hb : b.val < cs.length, cs[b.val].eligible = true ∧
      ∀ k (hk : k < cs.length), cs[k].eligible = true → ¬ better w cs k b.val hk hb

theorem scan_loop_spec (cs : Slice select.Candidate) (w : select.Terms) (i : Usize)
    (best : Option Usize) (bu bt bid : U64) (hi : i.val ≤ cs.val.length)
    (hinv : scanInv w cs.val i.val best bu bt bid) :
    select.scan_loop cs w i best bu bt bid ⦃ r => scanPost w cs.val r ⦄ := by
  unfold select.scan_loop
  have hlen : (Slice.len cs).val = cs.val.length := Slice.len_val cs
  apply loop.spec_decr_nat
    (fun st : Usize × Option Usize × U64 × U64 × U64 => cs.val.length - st.1.val)
    (fun st : Usize × Option Usize × U64 × U64 × U64 => st.1.val ≤ cs.val.length ∧
      scanInv w cs.val st.1.val st.2.1 st.2.2.1 st.2.2.2.1 st.2.2.2.2)
  · rintro ⟨j, b, u0, t0, d0⟩ ⟨hj, hb1, hb2⟩
    simp only at hj hb1 hb2
    unfold select.scan_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < cs.val.length := by scalar_tac
      have hget : cs.val[j.val]? = some cs.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
      have hs1 := Proofs.Time.usize_succ j (by scalar_tac)
      apply WP.spec_bind (scan_step w cs.val[j.val] j b u0 t0 d0)
      rintro ⟨b', u', t', d'⟩ hp
      simp only [lift, bind_ok]
      show ok (ControlFlow.cont (core.num.Usize.saturating_add j 1#usize, b', u', t', d')) ⦃ _ ⦄
      simp only [WP.spec_ok, hs1, scanInv]
      by_cases htake : cs.val[j.val].eligible = true ∧ (b = none ∨
          gt (util w cs.val[j.val]) cs.val[j.val].tie_break.val cs.val[j.val].id.val
            u0.val t0.val d0.val)
      · obtain ⟨hb', hu', ht', hd'⟩ := hp.1 htake
        simp only at hb' hu' ht' hd'
        subst hb' ht' hd'
        obtain ⟨hel, hbetter⟩ := htake
        refine ⟨⟨by omega, ?_, ?_⟩, by omega⟩
        · intro h; cases h
        · intro b0 hb0
          cases hb0
          refine ⟨hlt', by omega, hel, hu', rfl, rfl, ?_⟩
          intro k hk hkj hek
          by_cases hki : k = j.val
          · subst hki; unfold better gt; omega
          · cases b with
            | none => exact absurd hek (by rw [hb1 rfl k hk (by omega)]; simp)
            | some b1 =>
              obtain ⟨hb1l, _, _, hbu, hbt, hbd, hall⟩ := hb2 b1 rfl
              have h1 := hall k hk (by omega) hek
              rcases hbetter with hn | hg
              · cases hn
              · simp only [better, gt] at h1 hg ⊢
                subst hbt hbd
                rw [← hbu] at h1
                omega
      · have hp2 := hp.2 htake
        simp only [Prod.mk.injEq] at hp2
        obtain ⟨rfl, rfl, rfl, rfl⟩ := hp2
        refine ⟨⟨by omega, ?_, ?_⟩, by omega⟩
        · intro hn k hk hkj
          by_cases hki : k = j.val
          · subst hki
            cases hk' : cs.val[j.val].eligible
            · rfl
            · exact absurd ⟨hk', Or.inl hn⟩ htake
          · exact hb1 hn k hk (by omega)
        · intro b0 hb0
          obtain ⟨hbl, hbj, hel, hbu, hbt, hbd, hall⟩ := hb2 b0 hb0
          refine ⟨hbl, by omega, hel, hbu, hbt, hbd, ?_⟩
          intro k hk hkj hek
          by_cases hki : k = j.val
          · subst hki
            intro hg
            apply htake
            refine ⟨hek, Or.inr ?_⟩
            unfold better gt at hg
            unfold gt
            subst hbt hbd
            rw [hbu]
            omega
          · exact hall k hk (by omega) hek
    · rename_i hge
      have hge' : cs.val.length ≤ j.val := by scalar_tac
      simp only [WP.spec_ok, scanPost]
      cases b with
      | none => intro k hk; exact hb1 rfl k hk (by omega)
      | some b0 =>
        obtain ⟨hbl, _, hel, _, _, _, hall⟩ := hb2 b0 rfl
        exact ⟨hbl, hel, fun k hk hek => hall k hk (by omega) hek⟩
  · exact ⟨hi, hinv⟩

/-- The full scan decision. Total. -/
theorem scan_spec (cs : Slice select.Candidate) (w : select.Terms) (bound : Usize) :
    select.scan cs w bound ⦃ r => match r with
      | .TooLarge => bound.val < cs.val.length
      | .Empty => cs.val.length ≤ bound.val ∧
          ∀ k (hk : k < cs.val.length), cs.val[k].eligible = false
      | .Best b => cs.val.length ≤ bound.val ∧ ∃ hb : b.val < cs.val.length,
          cs.val[b.val].eligible = true ∧
          ∀ k (hk : k < cs.val.length), cs.val[k].eligible = true → ¬ better w cs.val k b.val hk hb ⦄ := by
  unfold select.scan
  have hlen : (Slice.len cs).val = cs.val.length := Slice.len_val cs
  dsimp only
  split
  · rename_i h
    simp only [WP.spec_ok]
    scalar_tac
  · rename_i h
    have hle : cs.val.length ≤ bound.val := by scalar_tac
    apply WP.spec_bind (scan_loop_spec cs w 0#usize none 0#u64 0#u64 0#u64 (by simp)
      ⟨fun _ k _ hk => by simp at hk, fun b hb => by cases hb⟩)
    intro r hr
    cases r with
    | none => simp only [WP.spec_ok]; exact ⟨hle, hr⟩
    | some b =>
      simp only [WP.spec_ok]
      obtain ⟨hb, hel, hall⟩ := hr
      exact ⟨hle, hb, hel, hall⟩

/-- A returned index is an eligible candidate. -/
theorem scan_only_eligible (cs : Slice select.Candidate) (w : select.Terms) (bound : Usize) :
    select.scan cs w bound ⦃ r => ∀ b, r = .Best b →
      ∃ hb : b.val < cs.val.length, cs.val[b.val].eligible = true ⦄ := by
  apply WP.spec_mono (scan_spec cs w bound)
  intro r hr b hb
  subst hb
  obtain ⟨_, hl, hel, _⟩ := hr
  exact ⟨hl, hel⟩

/-- No eligible candidate has a greater `(utility, tie_break, id)` than the
returned one. -/
theorem scan_optimal (cs : Slice select.Candidate) (w : select.Terms) (bound : Usize) :
    select.scan cs w bound ⦃ r => ∀ b, r = .Best b →
      ∃ hb : b.val < cs.val.length, ∀ k (hk : k < cs.val.length),
        cs.val[k].eligible = true → ¬ better w cs.val k b.val hk hb ⦄ := by
  apply WP.spec_mono (scan_spec cs w bound)
  intro r hr b hb
  subst hb
  obtain ⟨_, hl, _, hall⟩ := hr
  exact ⟨hl, hall⟩

/-- `Empty` exactly when the vector is within the bound and nothing is
eligible. -/
theorem scan_none_iff_no_eligible (cs : Slice select.Candidate) (w : select.Terms)
    (bound : Usize) :
    select.scan cs w bound ⦃ r => (r = .Empty ↔ (cs.val.length ≤ bound.val ∧
      ∀ k (hk : k < cs.val.length), cs.val[k].eligible = false)) ⦄ := by
  apply WP.spec_mono (scan_spec cs w bound)
  intro r hr
  cases r with
  | TooLarge => simp only at hr; simp; omega
  | Empty => simp only at hr; simp [hr]
  | Best b =>
    obtain ⟨_, hl, hel, _⟩ := hr
    simp only [reduceCtorEq, false_iff, not_and]
    intro _ hall
    have := hall b hl
    simp_all

/-- More than `bound` candidates is `TooLarge`, and never a partial scan. -/
theorem scan_bounded (cs : Slice select.Candidate) (w : select.Terms) (bound : Usize) :
    select.scan cs w bound ⦃ r => (r = .TooLarge ↔ bound.val < cs.val.length) ⦄ := by
  apply WP.spec_mono (scan_spec cs w bound)
  intro r hr
  cases r with
  | TooLarge => simp [hr]
  | Empty => simp only [reduceCtorEq, false_iff]; omega
  | Best b => simp only [reduceCtorEq, false_iff]; omega

end Proofs.Select
