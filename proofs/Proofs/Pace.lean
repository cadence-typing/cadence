import Extracted.CadenceKernel
import Proofs.Time

/-!
# Pace hysteresis (`cadence_kernel::pace`, #61)

`cadence-trainer`'s plan builder calls `pace.decide` per typing track with
the track's last suggested pace and its recent blocks (one block = one
track's slice of one session), in session order.

Two quantities stay apart: the desired first-pass accuracy is the bar a
block must meet; the suggested pace (correct units per minute) is what
moves. The theorems: no baseline without a target; a raise is bounded by
`ceil(target * raise_bp / 10000)` and needs enough qualifying blocks from
enough sessions with no drop in attempt accuracy (stated over an
independent fold `tallyN`); a lowering is bounded likewise and never goes
below 1; a latest block strictly below the slow-down band lowers; between
the bands the pace stays; the accuracy comparison is exact.

Trusted, not proved: that the caller passes the blocks in session order
(distinct sessions are counted as changes of session between consecutive
qualifying blocks) and the right counters (tested in `trainer::plan`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Pace

theorem u64_max : U64.max = 18446744073709551615 := by
  simp [U64.max, UScalar.max, UScalarTy.numBits, U64.numBits]

theorem u128_max : U128.max = 340282366920938463463374607431768211455 := by
  simp [U128.max, UScalar.max, UScalarTy.numBits, U128.numBits]

theorem full_bp : pace.FULL_BP.val = 10000 := by simp [pace.FULL_BP]

/-- A product of two widened `u64` values never overflows `u128`. -/
theorem cmul (a b : U64) :
    ∃ x : U128, U128.checked_mul (core.convert.num.FromU128U64.from a)
        (core.convert.num.FromU128U64.from b) = some x ∧ x.val = a.val * b.val := by
  have m := U128.checked_mul_bv_spec (core.convert.num.FromU128U64.from a)
    (core.convert.num.FromU128U64.from b)
  have fa := core.convert.num.FromU128U64.from_val_eq a
  have fb := core.convert.num.FromU128U64.from_val_eq b
  have ha : a.val ≤ U64.max := by scalar_tac
  have hb : b.val ≤ U64.max := by scalar_tac
  cases e : U128.checked_mul (core.convert.num.FromU128U64.from a)
      (core.convert.num.FromU128U64.from b) with
  | none =>
    rw [e] at m
    simp only [fa, fb, u128_max] at m
    simp only [u64_max] at ha hb
    have : a.val * b.val ≤ 18446744073709551615 * 18446744073709551615 :=
      Nat.mul_le_mul ha hb
    omega
  | some x =>
    rw [e] at m
    simp only [fa, fb] at m
    exact ⟨x, rfl, m.2.1⟩

/-- Exact: strictly below `band / 10000` by cross-multiplication. -/
def belowN (fc fa band : Nat) : Bool := fa ≠ 0 && decide (fc * 10000 < band * fa)

/-- `block_accuracy_below` is exactly `belowN`: no rounding. Total. -/
theorem below_exact (fc fa band : U64) :
    pace.block_accuracy_below fc fa band ⦃ r => r = belowN fc.val fa.val band.val ⦄ := by
  unfold pace.block_accuracy_below belowN
  by_cases h : fa = 0#u64
  · have : fa.val = 0 := by simp [h]
    simp [h, this]
  · have hv : fa.val ≠ 0 := by
      intro h0; apply h; exact UScalar.eq_of_val_eq (by simpa using h0)
    obtain ⟨l, hl, hlv⟩ := cmul fc pace.FULL_BP
    obtain ⟨r, hr, hrv⟩ := cmul band fa
    simp only [h, ite_false, lift, bind_tc_ok, bind_ok, hl, hr, WP.spec_ok]
    rw [full_bp] at hlv
    simp [hv, hlv, hrv]

/-- Exact: `c / n` strictly below `pc / pn` by cross-multiplication. -/
def worseN (c n pc pn : Nat) : Bool := decide (c * pn < pc * n)

/-- `attempt_worse` is exactly `worseN`. Total. -/
theorem worse_exact (c n pc pn : U64) :
    pace.attempt_worse c n pc pn ⦃ r => r = worseN c.val n.val pc.val pn.val ⦄ := by
  unfold pace.attempt_worse worseN
  obtain ⟨l, hl, hlv⟩ := cmul c pn
  obtain ⟨r, hr, hrv⟩ := cmul pc n
  simp only [lift, bind_tc_ok, bind_ok, hl, hr, WP.spec_ok]
  simp [hlv, hrv]

/-- A block qualifies: enough positions, accuracy at or above the bar. -/
def qualN (d m : Nat) (b : pace.Block) : Bool :=
  decide (m ≤ b.first_attempted.val) && !belowN b.first_correct.val b.first_attempted.val d

theorem qualifies_spec (b : pace.Block) (d m : U64) :
    pace.qualifies b d m ⦃ r => r = qualN d.val m.val b ⦄ := by
  unfold pace.qualifies qualN
  split
  · rename_i h
    apply WP.spec_bind (below_exact b.first_correct b.first_attempted d)
    intro x hx
    simp only [WP.spec_ok, hx]
    have : m.val ≤ b.first_attempted.val := by scalar_tac
    simp [this]
  · rename_i h
    have : ¬ m.val ≤ b.first_attempted.val := by scalar_tac
    simp [WP.spec_ok, this]

theorem below_eq (fc fa band : U64) :
    pace.block_accuracy_below fc fa band = ok (belowN fc.val fa.val band.val) := by
  unfold pace.block_accuracy_below belowN
  by_cases h : fa = 0#u64
  · have : fa.val = 0 := by simp [h]
    simp [h, this]
  · have hv : fa.val ≠ 0 := by
      intro h0; apply h; exact UScalar.eq_of_val_eq (by simpa using h0)
    obtain ⟨l, hl, hlv⟩ := cmul fc pace.FULL_BP
    obtain ⟨r, hr, hrv⟩ := cmul band fa
    simp only [h, ite_false, lift, bind_tc_ok, bind_ok, hl, hr]
    rw [full_bp] at hlv
    simp [hv, hlv, hrv]

theorem worse_eq (c n pc pn : U64) :
    pace.attempt_worse c n pc pn = ok (worseN c.val n.val pc.val pn.val) := by
  unfold pace.attempt_worse worseN
  obtain ⟨l, hl, hlv⟩ := cmul c pn
  obtain ⟨r, hr, hrv⟩ := cmul pc n
  simp only [lift, bind_tc_ok, bind_ok, hl, hr]
  simp [hlv, hrv]

theorem qualifies_eq (b : pace.Block) (d m : U64) :
    pace.qualifies b d m = ok (qualN d.val m.val b) := by
  unfold pace.qualifies qualN
  split
  · rename_i h
    have : m.val ≤ b.first_attempted.val := by scalar_tac
    simp [below_eq, this]
  · rename_i h
    have : ¬ m.val ≤ b.first_attempted.val := by scalar_tac
    simp [this]

theorem sat1 (x : U64) :
    (core.num.U64.saturating_add x 1#u64).val = min (x.val + 1) U64.max := by
  have h1 : (UScalar.saturating_add x 1#u64).val = min (UScalar.max .U64) (x.val + (1#u64).val) := by
    show (BitVec.ofNat _ (min (UScalar.max .U64) (x.val + (1#u64).val))).toNat = _
    rw [BitVec.toNat_ofNat]
    apply Nat.mod_eq_of_lt
    have := Nat.two_pow_pos (UScalarTy.U64.numBits)
    rw [UScalar.max_def]
    omega
  simp only [core.num.U64.saturating_add, h1]
  simp [U64.max, Nat.min_comm]

/-- The tally as plain numbers. -/
structure TallyN where
  q : Nat
  s : Nat
  hasLast : Bool
  last : Nat
  deg : Bool
  pc : Nat
  pn : Nat
deriving DecidableEq

def ofT (t : pace.Tally) : TallyN :=
  ⟨t.qualifying.val, t.sessions.val, t.has_last, t.last_session.val, t.degraded,
    t.previous_correct.val, t.previous_total.val⟩

def initN : TallyN := ⟨0, 0, false, 0, false, 0, 0⟩

/-- One block, independently: a qualifying block counts, and counts a
session when it differs from the previous qualifying block's; attempt
accuracy strictly below the previous block with insertions degrades. -/
def stepN (d m : Nat) (t : TallyN) (b : pace.Block) : TallyN :=
  let qu := qualN d m b
  let content := decide (b.content_attempts.val ≠ 0)
  { q := if qu then min (t.q + 1) U64.max else t.q
    s := if qu && (!t.hasLast || decide (t.last ≠ b.session.val))
         then min (t.s + 1) U64.max else t.s
    hasLast := qu || t.hasLast
    last := if qu then b.session.val else t.last
    deg := t.deg || (decide (t.pn ≠ 0) && content &&
      worseN b.correct_attempts.val b.content_attempts.val t.pc t.pn)
    pc := if content then b.correct_attempts.val else t.pc
    pn := if content then b.content_attempts.val else t.pn }

def tallyN (d m : Nat) (bs : List pace.Block) : TallyN := bs.foldl (stepN d m) initN

theorem absorb_spec (t : pace.Tally) (b : pace.Block) (d m : U64) :
    pace.absorb t b d m ⦃ r => ofT r = stepN d.val m.val (ofT t) b ⦄ := by
  unfold pace.absorb
  rw [qualifies_eq]
  simp only [bind_ok]
  have hne : (t.last_session != b.session) = decide (t.last_session.val ≠ b.session.val) := by
    by_cases h : t.last_session = b.session
    · rw [h]; simp
    · have : t.last_session.val ≠ b.session.val := fun hv => h (UScalar.eq_of_val_eq hv)
      simp [h, this]
  cases hq : qualN d.val m.val b <;> cases hl : t.has_last <;> cases hd : t.degraded <;>
    by_cases hs : t.last_session.val = b.session.val <;>
    by_cases hp : t.previous_total.val = 0 <;> by_cases hc : b.content_attempts.val = 0 <;>
    simp [hp, hc, hl, hd, hs, hne, worse_eq, ofT, stepN, hq, sat1, WP.spec_ok]

theorem tally_loop_spec (bs : Slice pace.Block) (d m : U64) (acc : pace.Tally) (i : Usize)
    (hi : i.val ≤ bs.val.length) (hinv : ofT acc = tallyN d.val m.val (bs.val.take i.val)) :
    pace.tally_loop bs d m acc i ⦃ r => ofT r = tallyN d.val m.val bs.val ⦄ := by
  unfold pace.tally_loop
  have hlen : (Slice.len bs).val = bs.val.length := Slice.len_val bs
  apply loop.spec_decr_nat
    (fun st : pace.Tally × Usize => bs.val.length - st.2.val)
    (fun st : pace.Tally × Usize => st.2.val ≤ bs.val.length ∧
      ofT st.1 = tallyN d.val m.val (bs.val.take st.2.val))
  · rintro ⟨a, j⟩ ⟨hj, ha⟩
    simp only at hj ha
    unfold pace.tally_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < bs.val.length := by scalar_tac
      have hget : bs.val[j.val]? = some bs.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
      have hs1 := Proofs.Time.usize_succ j (by scalar_tac)
      apply WP.spec_bind (absorb_spec a bs.val[j.val] d m)
      intro a' ha'
      simp only [lift, bind_ok, WP.spec_ok, hs1]
      refine ⟨⟨by omega, ?_⟩, by omega⟩
      rw [ha', ha, tallyN, tallyN, List.take_succ_eq_append_getElem hlt', List.foldl_append]
      simp
    · rename_i hge
      have hge' : bs.val.length ≤ j.val := by scalar_tac
      simp only [WP.spec_ok]
      rw [ha, List.take_of_length_le hge']
  · exact ⟨hi, hinv⟩

/-- The tally is the independent fold over the blocks. Total. -/
theorem tally_spec (bs : Slice pace.Block) (d m : U64) :
    pace.tally bs d m ⦃ r => ofT r = tallyN d.val m.val bs.val ⦄ := by
  unfold pace.tally
  apply tally_loop_spec bs d m pace.EMPTY 0#usize (by simp)
  simp [ofT, tallyN, initN, pace.EMPTY]

/-- The qualifying count is the number of qualifying blocks (saturating
at the `u64` bound): the position floor and the accuracy bar are what
qualify a block. -/
theorem q_count (d m : Nat) (bs : List pace.Block) (t : TallyN) (ht : t.q ≤ U64.max) :
    (bs.foldl (stepN d m) t).q = min (t.q + bs.countP (qualN d m)) U64.max := by
  induction bs generalizing t with
  | nil => simp [Nat.min_eq_left ht]
  | cons b rest ih =>
    have hs : (stepN d m t b).q ≤ U64.max := by
      simp only [stepN]; split <;> omega
    rw [List.foldl_cons, ih _ hs, List.countP_cons]
    simp only [stepN]
    split <;> (try simp_all) <;> omega

/-- `ceil(t * bp / 10000)`. -/
def ceilShare (t bp : Nat) : Nat := (t * bp + 9999) / 10000

theorem round_up : pace.ROUND_UP.val = 9999 := by simp [pace.ROUND_UP]

theorem share_spec (t bp : U64) :
    pace.share t bp ⦃ o =>
      (t.val * bp.val + 9999 ≤ U64.max → ∃ x, o = some x ∧ x.val = ceilShare t.val bp.val) ∧
      ∀ x, o = some x → x.val = ceilShare t.val bp.val ⦄ := by
  unfold pace.share ceilShare
  have m := U64.checked_mul_bv_spec t bp
  simp only [lift, bind_tc_ok, bind_ok]
  cases e : U64.checked_mul t bp with
  | none =>
    rw [e] at m
    simp only [WP.spec_ok]
    refine ⟨fun h => absurd m (by omega), fun x hx => by cases hx⟩
  | some p =>
    rw [e] at m
    have a := U64.checked_add_bv_spec p pace.ROUND_UP
    simp only
    cases e2 : U64.checked_add p pace.ROUND_UP with
    | none =>
      rw [e2] at a
      simp only [WP.spec_ok]
      rw [round_up] at a
      refine ⟨fun h => absurd a (by omega), fun x hx => by cases hx⟩
    | some r =>
      rw [e2] at a
      have dv := U64.checked_div_bv_spec r pace.FULL_BP
      simp only
      cases e3 : U64.checked_div r pace.FULL_BP with
      | none =>
        rw [e3] at dv
        rw [full_bp] at dv
        exact absurd dv (by omega)
      | some v =>
        rw [e3] at dv
        rw [WP.spec_ok]
        have hv : v.val = (t.val * bp.val + 9999) / 10000 := by
          rw [dv.2.1, a.2.1, m.2.1, full_bp, round_up]
        exact ⟨fun _ => ⟨v, rfl, hv⟩, fun x hx => by cases hx; exact hv⟩

theorem ne_zero_val {x : U64} (h : ¬ x = 0#u64) : x.val ≠ 0 :=
  fun hv => h (UScalar.eq_of_val_eq (by simpa using hv))

/-- A raise is strictly up and at most `ceil(t * bp / 10000)`; `raise`
returns only `Raise` or `Steady`. -/
theorem raise_spec (t bp : U64) :
    pace.raise t bp ⦃ d => (d = .Steady ∨ ∃ n, d = .Raise n) ∧
      ∀ n, d = .Raise n → t.val < n.val ∧ n.val ≤ t.val + ceilShare t.val bp.val ⦄ := by
  unfold pace.raise
  apply WP.spec_bind (share_spec t bp)
  rintro o ⟨_, ho⟩
  cases o with
  | none => simp [WP.spec_ok]
  | some delta =>
    have hd := ho delta rfl
    simp only
    by_cases h0 : delta = 0#u64
    · simp [h0, WP.spec_ok]
    · have hv := ne_zero_val h0
      simp only [h0, ite_false, lift, bind_tc_ok, bind_ok]
      have a := U64.checked_add_bv_spec t delta
      cases e : U64.checked_add t delta with
      | none => simp [WP.spec_ok]
      | some n =>
        rw [e] at a
        simp only [WP.spec_ok]
        refine ⟨Or.inr ⟨n, rfl⟩, fun n' hn => ?_⟩
        cases hn
        have := a.2.1
        omega

/-- A lowering is strictly down, at most `ceil(t * bp / 10000)` and never
below 1; it happens whenever the share fits, is nonzero and `t >= 2`. -/
theorem lower_spec (t bp : U64) :
    pace.lower t bp ⦃ d => (d = .Steady ∨ ∃ n, d = .Lower n) ∧
      (∀ n, d = .Lower n → t.val - ceilShare t.val bp.val ≤ n.val ∧ n.val < t.val ∧ 1 ≤ n.val) ∧
      (2 ≤ t.val → 1 ≤ bp.val → t.val * bp.val + 9999 ≤ U64.max → ∃ n, d = .Lower n) ⦄ := by
  unfold pace.lower
  apply WP.spec_bind (share_spec t bp)
  rintro o ⟨hfit, ho⟩
  have hpos : 1 ≤ bp.val → 1 ≤ t.val → 1 ≤ ceilShare t.val bp.val := by
    intro h1 h2; unfold ceilShare
    have : 1 ≤ t.val * bp.val := Nat.one_le_iff_ne_zero.mpr (Nat.mul_ne_zero (by omega) (by omega))
    omega
  cases o with
  | none =>
    simp only [WP.spec_ok]
    refine ⟨by simp, by simp, fun h2 h1 hf => ?_⟩
    obtain ⟨x, hx, _⟩ := hfit hf
    cases hx
  | some delta =>
    have hd := ho delta rfl
    simp only
    by_cases h0 : delta = 0#u64
    · simp only [h0, ite_true, WP.spec_ok]
      refine ⟨by simp, by simp, fun h2 h1 hf => ?_⟩
      have := hpos h1 (by omega)
      rw [h0] at hd
      simp at hd
      omega
    · have hv := ne_zero_val h0
      simp only [h0, ite_false]
      by_cases hlt : delta < t
      · have hlt' : delta.val < t.val := by scalar_tac
        simp only [hlt, ite_true, lift, bind_tc_ok, bind_ok]
        have sb := U64.checked_sub_bv_spec t delta
        cases e : U64.checked_sub t delta with
        | none =>
          rw [e] at sb
          exact absurd sb (by omega)
        | some l =>
          rw [e] at sb
          simp only [WP.spec_ok]
          have hl := sb.2.1
          refine ⟨Or.inr ⟨l, rfl⟩, fun n hn => ?_, fun _ _ _ => ⟨l, rfl⟩⟩
          cases hn
          omega
      · have hge : t.val ≤ delta.val := by scalar_tac
        simp only [hlt, ite_false]
        by_cases h1 : t > 1#u64
        · have h1' : 1 < t.val := by scalar_tac
          simp only [h1, ite_true, WP.spec_ok]
          refine ⟨Or.inr ⟨_, rfl⟩, fun n hn => ?_, fun _ _ _ => ⟨_, rfl⟩⟩
          cases hn
          refine ⟨by simp; omega, by simp; omega, by simp⟩
        · have h1' : t.val ≤ 1 := by scalar_tac
          simp only [h1, ite_false, WP.spec_ok]
          exact ⟨by simp, by simp, fun h2 => absurd h2 (by omega)⟩

/-- The latest block, exactly. -/
def latestN (band : Nat) (bs : List pace.Block) : Bool :=
  match bs.getLast? with
  | none => false
  | some b => belowN b.first_correct.val b.first_attempted.val band

theorem latest_spec (bs : Slice pace.Block) (band : U64) :
    pace.latest_below bs band ⦃ r => r = latestN band.val bs.val ⦄ := by
  unfold pace.latest_below latestN
  have hlen : (Slice.len bs).val = bs.val.length := Slice.len_val bs
  have sb := Usize.checked_sub_bv_spec (Slice.len bs) 1#usize
  simp only [lift, bind_tc_ok, bind_ok]
  cases e : Usize.checked_sub (Slice.len bs) 1#usize with
  | none =>
    rw [e] at sb
    have h1 : (Slice.len bs).val < 1 := by simpa using sb
    have h0 : bs.val.length = 0 := by omega
    have : bs.val = [] := List.eq_nil_of_length_eq_zero h0
    simp [this, WP.spec_ok]
  | some last =>
    rw [e] at sb
    have hl : last.val + 1 = bs.val.length := by
      have h1 := sb.1
      have h2 := sb.2.1
      have h3 : (1#usize : Usize).val = 1 := by simp
      omega
    have hlt : last.val < bs.val.length := by omega
    have hget : bs.val[last.val]? = some bs.val[last.val] := List.getElem?_eq_getElem hlt
    simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
      core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
    rw [below_eq, WP.spec_ok, List.getLast?_eq_getElem?]
    have : bs.val.length - 1 = last.val := by omega
    rw [this, hget]

/-- No suggested pace, no adjustment. -/
theorem no_baseline_without_target (d : U64) (bs : Slice pace.Block) (pol : pace.Policy) :
    pace.decide none d bs pol ⦃ r => r = .NoBaseline ⦄ ∧
    pace.decide (some 0#u64) d bs pol ⦃ r => r = .NoBaseline ⦄ := by
  unfold pace.decide
  simp [WP.spec_ok]

/-- The raise conditions over the independent fold. -/
def raiseN (d : Nat) (pol : pace.Policy) (bs : List pace.Block) : Prop :=
  (tallyN d pol.minimum_positions.val bs).deg = false ∧
  pol.qualifying_blocks.val ≤ (tallyN d pol.minimum_positions.val bs).q ∧
  pol.minimum_sessions.val ≤ (tallyN d pol.minimum_positions.val bs).s

/-- What `decide` does with a nonzero target, case by case. Total. -/
theorem decide_cases (t d : U64) (bs : Slice pace.Block) (pol : pace.Policy) (ht : t.val ≠ 0) :
    pace.decide (some t) d bs pol ⦃ r =>
      (latestN pol.slow_down_below_bp.val bs.val = true →
        (r = .Steady ∨ ∃ n, r = .Lower n) ∧
        (∀ n, r = .Lower n →
          t.val - ceilShare t.val pol.lower_bp.val ≤ n.val ∧ n.val < t.val ∧ 1 ≤ n.val) ∧
        (2 ≤ t.val → 1 ≤ pol.lower_bp.val → t.val * pol.lower_bp.val + 9999 ≤ U64.max →
          ∃ n, r = .Lower n)) ∧
      (latestN pol.slow_down_below_bp.val bs.val = false →
        (raiseN d.val pol bs.val →
          (r = .Steady ∨ ∃ n, r = .Raise n) ∧
          ∀ n, r = .Raise n → t.val < n.val ∧ n.val ≤ t.val + ceilShare t.val pol.raise_bp.val) ∧
        (¬ raiseN d.val pol bs.val → r = .Steady)) ⦄ := by
  unfold pace.decide
  have h0 : ¬ t = 0#u64 := fun h => ht (by simp [h])
  simp only [h0, ite_false]
  apply WP.spec_bind (latest_spec bs pol.slow_down_below_bp)
  intro b hb
  cases b with
  | true =>
    simp only [ite_true]
    apply WP.spec_mono (lower_spec t pol.lower_bp)
    intro r hr
    refine ⟨fun _ => hr, fun h => ?_⟩
    rw [← hb] at h; cases h
  | false =>
    simp only [Bool.false_eq_true, ite_false]
    apply WP.spec_bind (tally_spec bs d pol.minimum_positions)
    intro T hT
    have hq : T.qualifying.val = (tallyN d.val pol.minimum_positions.val bs.val).q := by
      rw [← hT]; rfl
    have hs : T.sessions.val = (tallyN d.val pol.minimum_positions.val bs.val).s := by
      rw [← hT]; rfl
    have hdg : T.degraded = (tallyN d.val pol.minimum_positions.val bs.val).deg := by
      rw [← hT]; rfl
    have notTrue : ∀ {P : Prop}, false = true → P := fun h => by cases h
    cases hdeg : T.degraded with
    | true =>
      simp only [ite_true, WP.spec_ok]
      refine ⟨fun h => notTrue (hb ▸ h), fun _ => ⟨fun hr => ?_, fun _ => by simp⟩⟩
      exfalso
      have := hr.1
      rw [← hdg, hdeg] at this
      cases this
    | false =>
      simp only [Bool.false_eq_true, ite_false]
      by_cases hqb : pol.qualifying_blocks ≤ T.qualifying
      · have hqb' : pol.qualifying_blocks.val ≤ T.qualifying.val := by scalar_tac
        simp only [hqb, ite_true]
        by_cases hsb : pol.minimum_sessions ≤ T.sessions
        · have hsb' : pol.minimum_sessions.val ≤ T.sessions.val := by scalar_tac
          simp only [hsb, ite_true]
          apply WP.spec_mono (raise_spec t pol.raise_bp)
          intro r hr
          refine ⟨fun h => notTrue (hb ▸ h), fun _ => ⟨fun _ => hr, fun hn => ?_⟩⟩
          exfalso; apply hn
          exact ⟨by rw [← hdg, hdeg], by rw [← hq]; exact hqb', by rw [← hs]; exact hsb'⟩
        · have hsb' : ¬ pol.minimum_sessions.val ≤ T.sessions.val := by scalar_tac
          simp only [hsb, ite_false, WP.spec_ok]
          refine ⟨fun h => notTrue (hb ▸ h), fun _ => ⟨fun hr => ?_, fun _ => by simp⟩⟩
          exfalso
          exact hsb' (by rw [hs]; exact hr.2.2)
      · have hqb' : ¬ pol.qualifying_blocks.val ≤ T.qualifying.val := by scalar_tac
        simp only [hqb, ite_false, WP.spec_ok]
        refine ⟨fun h => notTrue (hb ▸ h), fun _ => ⟨fun hr => ?_, fun _ => by simp⟩⟩
        exfalso
        exact hqb' (by rw [hq]; exact hr.2.1)

theorem zero_target (d : U64) (bs : Slice pace.Block) (pol : pace.Policy) :
    pace.decide (some 0#u64) d bs pol = ok .NoBaseline := by
  unfold pace.decide; simp

theorem target_zero_val {t : U64} (h : t.val = 0) : t = 0#u64 :=
  UScalar.eq_of_val_eq (by simpa using h)

/-- `decide` returns for every input. -/
theorem decide_total (target : Option U64) (d : U64) (bs : Slice pace.Block) (pol : pace.Policy) :
    pace.decide target d bs pol ⦃ _ => True ⦄ := by
  cases target with
  | none => unfold pace.decide; simp [WP.spec_ok]
  | some t =>
    by_cases ht : t.val = 0
    · rw [target_zero_val ht, zero_target]; simp [WP.spec_ok]
    · exact WP.spec_mono (decide_cases t d bs pol ht) (fun _ _ => trivial)

/-- A raise is strictly up and bounded by `ceil(t * raise_bp / 10000)`. -/
theorem raise_bounded (t d : U64) (bs : Slice pace.Block) (pol : pace.Policy) :
    pace.decide (some t) d bs pol ⦃ r => ∀ n, r = .Raise n →
      t.val < n.val ∧ n.val ≤ t.val + ceilShare t.val pol.raise_bp.val ⦄ := by
  by_cases ht : t.val = 0
  · rw [target_zero_val ht, zero_target]; simp [WP.spec_ok]
  · apply WP.spec_mono (decide_cases t d bs pol ht)
    intro r ⟨hlow, hhigh⟩ n hn
    cases hl : latestN pol.slow_down_below_bp.val bs.val
    · by_cases hr : raiseN d.val pol bs.val
      · exact ((hhigh hl).1 hr).2 n hn
      · rw [(hhigh hl).2 hr] at hn; cases hn
    · rcases (hlow hl).1 with h | ⟨m, h⟩ <;> rw [h] at hn <;> cases hn

/-- A lowering is strictly down, bounded by `ceil(t * lower_bp / 10000)`,
and never below 1. -/
theorem lower_bounded (t d : U64) (bs : Slice pace.Block) (pol : pace.Policy) :
    pace.decide (some t) d bs pol ⦃ r => ∀ n, r = .Lower n →
      t.val - ceilShare t.val pol.lower_bp.val ≤ n.val ∧ n.val < t.val ∧ 1 ≤ n.val ⦄ := by
  by_cases ht : t.val = 0
  · rw [target_zero_val ht, zero_target]; simp [WP.spec_ok]
  · apply WP.spec_mono (decide_cases t d bs pol ht)
    intro r ⟨hlow, hhigh⟩ n hn
    cases hl : latestN pol.slow_down_below_bp.val bs.val
    · by_cases hr : raiseN d.val pol bs.val
      · rcases ((hhigh hl).1 hr).1 with h | ⟨m, h⟩ <;> rw [h] at hn <;> cases hn
      · rw [(hhigh hl).2 hr] at hn; cases hn
    · exact (hlow hl).2.1 n hn

/-- A raise needs evidence: the latest block is not below the slow-down
band, no block's attempt accuracy dropped, at least `qualifying_blocks`
blocks qualify (each with at least the position floor and at or above the
desired accuracy) and they come from at least `minimum_sessions` sessions. -/
theorem raise_needs_evidence (t d : U64) (bs : Slice pace.Block) (pol : pace.Policy) :
    pace.decide (some t) d bs pol ⦃ r => ∀ n, r = .Raise n →
      latestN pol.slow_down_below_bp.val bs.val = false ∧
      (tallyN d.val pol.minimum_positions.val bs.val).deg = false ∧
      pol.qualifying_blocks.val ≤ bs.val.countP (qualN d.val pol.minimum_positions.val) ∧
      pol.minimum_sessions.val ≤ (tallyN d.val pol.minimum_positions.val bs.val).s ⦄ := by
  by_cases ht : t.val = 0
  · rw [target_zero_val ht, zero_target]; simp [WP.spec_ok]
  · apply WP.spec_mono (decide_cases t d bs pol ht)
    intro r ⟨hlow, hhigh⟩ n hn
    cases hl : latestN pol.slow_down_below_bp.val bs.val
    · by_cases hr : raiseN d.val pol bs.val
      · obtain ⟨hd, hq, hs⟩ := hr
        have hc := q_count d.val pol.minimum_positions.val bs.val initN (by simp [initN])
        rw [← tallyN] at hc
        simp only [initN, Nat.zero_add] at hc
        rw [hc] at hq
        exact ⟨rfl, hd, by omega, hs⟩
      · rw [(hhigh hl).2 hr] at hn; cases hn
    · rcases (hlow hl).1 with h | ⟨m, h⟩ <;> rw [h] at hn <;> cases hn

/-- A latest block strictly below the slow-down band lowers the pace
(when it can fall: a pace of at least 2, a nonzero share that fits). -/
theorem low_block_lowers (t d : U64) (bs : Slice pace.Block) (pol : pace.Policy)
    (ht : 2 ≤ t.val) (hbp : 1 ≤ pol.lower_bp.val)
    (hfit : t.val * pol.lower_bp.val + 9999 ≤ U64.max)
    (hlow : latestN pol.slow_down_below_bp.val bs.val = true) :
    pace.decide (some t) d bs pol ⦃ r => ∃ n, r = .Lower n ⦄ := by
  apply WP.spec_mono (decide_cases t d bs pol (by omega))
  intro r ⟨h, _⟩
  exact (h hlow).2.2 ht hbp hfit

/-- Between the bands the pace stays: the latest block is not below the
slow-down band and the raise evidence is missing. -/
theorem between_bands_steady (t d : U64) (bs : Slice pace.Block) (pol : pace.Policy)
    (hlow : latestN pol.slow_down_below_bp.val bs.val = false)
    (hno : ¬ raiseN d.val pol bs.val) :
    pace.decide (some t) d bs pol ⦃ r => r = .Steady ∨ r = .NoBaseline ⦄ := by
  by_cases ht : t.val = 0
  · rw [target_zero_val ht, zero_target]; simp [WP.spec_ok]
  · apply WP.spec_mono (decide_cases t d bs pol ht)
    intro r ⟨_, h⟩
    exact Or.inl ((h hlow).2 hno)

end Proofs.Pace
