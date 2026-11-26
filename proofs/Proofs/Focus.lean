import Extracted.CadenceKernel
import Proofs.Time
import Proofs.Plan

/-!
# Focus selection in integers (`cadence_kernel::focus`, #38, #56)

`cadence_trainer::learner::LearnerModel::from_history` weighs each session's
evidence with `focus.decay_weight`; `score` computes `posterior_bp`,
`uncertainty_bp` and `hesitation_bp`; `choose_focus` builds integer
candidates in skill-key order and loops `focus.pick` (which ranks by
`signal` among the candidates that `qualifies`).

Proved: the decay weight is `WEIGHT_ONE` at age 0, exactly
`WEIGHT_ONE / 2^k` after `k` half-lives, and never grows with age; the
posterior is at most 10,000, the prior mean without evidence, and never
lower with more errors; hesitation is 0 below its sample minimum; a sparse
or a quiet skill never qualifies; `pick` returns only a qualifying, unchosen
candidate, and none with a greater signal (ties to the lower index).

Trusted, not proved: that the trainer builds the weighted evidence from the
stored tallies (saturating sums, far below `u64::MAX` for the history
window) and applies the "explained key" filter (tested in
`learner_tests.rs`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Focus

theorem u64_succ (x : U64) (h : x.val < U64.max) :
    (core.num.U64.saturating_add x 1#u64).val = x.val + 1 := by
  have h1 : (UScalar.saturating_add x 1#u64).val = min (UScalar.max .U64) (x.val + (1#u64).val) := by
    show (BitVec.ofNat _ (min (UScalar.max .U64) (x.val + (1#u64).val))).toNat = _
    rw [BitVec.toNat_ofNat]
    apply Nat.mod_eq_of_lt
    have := Nat.two_pow_pos (UScalarTy.U64.numBits)
    rw [UScalar.max_def]
    omega
  simp only [core.num.U64.saturating_add, h1]
  simp only [UScalar.max_U64_eq]
  rw [U64.max_eq] at h
  simp
  omega

/-! ## Decay -/

/-- `WEIGHT_ONE` halved `k` times. -/
def H (k : Nat) : Nat := 65536 / 2 ^ k

/-- The decay weight in `Nat`. -/
def decayN (a h : Nat) : Nat := H (a / h) * (2 * h - a % h) / (2 * h)

theorem weight_one : focus.WEIGHT_ONE.val = 65536 := by
  unfold focus.WEIGHT_ONE; rfl

theorem halved_loop_spec (k base i : U64) (hi : i.val ≤ k.val) (hb : base.val = H i.val) :
    focus.halved_loop k base i ⦃ r => r.val = H k.val ⦄ := by
  unfold focus.halved_loop
  apply loop.spec_decr_nat
    (fun st : U64 × U64 => k.val - st.2.val)
    (fun st : U64 × U64 => st.2.val ≤ k.val ∧ st.1.val = H st.2.val)
  · rintro ⟨b, j⟩ ⟨hj, hbj⟩
    simp only at hj hbj
    unfold focus.halved_loop.body
    by_cases hlt : j.val < k.val
    · have hlt' : j < k := by scalar_tac
      simp only [hlt', ↓reduceIte]
      by_cases hpos : 0 < b.val
      · have hpos' : b > 0#u64 := by scalar_tac
        simp only [hpos', ↓reduceIte, lift, bind_ok]
        have hd := U64.checked_div_bv_spec b 2#u64
        have hs := u64_succ j (by scalar_tac)
        cases e : U64.checked_div b 2#u64 with
        | none => rw [e] at hd; simp at hd
        | some q =>
          rw [e] at hd
          simp at hd
          simp only [e, core.option.Option.unwrap_or, hs, WP.spec_ok]
          refine ⟨⟨by omega, ?_⟩, by omega⟩
          rw [hd.1, hbj]
          unfold H
          rw [Nat.div_div_eq_div_mul, ← Nat.pow_succ]
      · have hpos' : ¬ b > 0#u64 := by scalar_tac
        simp only [hpos', ↓reduceIte, WP.spec_ok]
        have h0 : b.val = 0 := Nat.eq_zero_of_not_pos hpos
        have hle : H k.val ≤ H j.val :=
          Nat.div_le_div_left (Nat.pow_le_pow_right (by norm_num) hj) (by positivity)
        omega
    · have hlt' : ¬ j < k := by scalar_tac
      simp only [hlt', ↓reduceIte, WP.spec_ok]
      have : j.val = k.val := by omega
      rw [hbj, this]
  · exact ⟨hi, hb⟩

theorem halved_spec (k : U64) : focus.halved k ⦃ r => r.val = H k.val ⦄ := by
  unfold focus.halved
  exact halved_loop_spec k focus.WEIGHT_ONE 0#u64 (by simp) (by simp [weight_one, H])

theorem H_le (k : Nat) : H k ≤ 65536 := Nat.div_le_self _ _

/-- The decay weight is `decayN`, when the half-life is positive and small
enough that `WEIGHT_ONE * 2h` fits. -/
theorem decay_weight_spec (a h : U64) (hpos : 0 < h.val) (hsmall : h.val ≤ 2 ^ 40) :
    focus.decay_weight a h ⦃ r => r.val = decayN a.val h.val ⦄ := by
  unfold focus.decay_weight
  simp only [lift, bind_ok]
  have d1 := U64.checked_div_bv_spec a h
  have d2 := U64.checked_rem_bv_spec a h
  have d3 := U64.checked_mul_bv_spec h 2#u64
  split
  · rename_i e; rw [e] at d1; simp at d1; omega
  rename_i k e1
  rw [e1] at d1
  obtain ⟨_, hk, _⟩ := d1
  split
  · rename_i e; rw [e] at d2; simp at d2; omega
  rename_i r e2
  rw [e2] at d2
  obtain ⟨_, hrv, _⟩ := d2
  split
  · rename_i e; rw [e] at d3; simp at d3; scalar_tac
  rename_i sp e3
  rw [e3] at d3
  obtain ⟨_, hsv, _⟩ := d3
  simp at hsv
  have hrl : r.val < h.val := by rw [hrv]; exact Nat.mod_lt _ hpos
  have d4 := U64.checked_sub_bv_spec sp r
  split
  · rename_i e; rw [e] at d4; simp at d4; omega
  rename_i l e4
  rw [e4] at d4
  obtain ⟨_, hlv, _⟩ := d4
  apply WP.spec_bind (halved_spec k)
  intro b hb
  have hbH := H_le k.val
  have d5 := U64.checked_mul_bv_spec b l
  split
  · rename_i e
    rw [e] at d5; simp at d5
    have : b.val * l.val ≤ 65536 * (2 * 2 ^ 40) := Nat.mul_le_mul (by omega) (by omega)
    scalar_tac
  rename_i p e5
  rw [e5] at d5
  obtain ⟨_, hpv, _⟩ := d5
  have d6 := U64.checked_div_bv_spec p sp
  cases e6 : U64.checked_div p sp with
  | none => rw [e6] at d6; simp at d6; omega
  | some q =>
    rw [e6] at d6
    obtain ⟨_, hqv, _⟩ := d6
    simp only [core.option.Option.unwrap_or, WP.spec_ok]
    unfold decayN
    rw [hqv, hpv, hb, hlv, hsv, hrv, hk]
    ring_nf

/-- One step older never weighs more. -/
theorem decayN_step (a h : Nat) (hpos : 0 < h) : decayN (a + 1) h ≤ decayN a h := by
  unfold decayN
  have hdm := Nat.div_add_mod a h
  have hr := Nat.mod_lt a hpos
  set q := a / h
  set r := a % h
  by_cases hlast : r + 1 < h
  · have e1 : (a + 1) / h = q := by
      have : a + 1 = (r + 1) + h * q := by omega
      rw [this, Nat.add_mul_div_left _ _ hpos, Nat.div_eq_of_lt hlast]; simp
    have e2 : (a + 1) % h = r + 1 := by
      have : a + 1 = (r + 1) + h * q := by omega
      rw [this, Nat.add_mul_mod_self_left, Nat.mod_eq_of_lt hlast]
    rw [e1, e2]
    apply Nat.div_le_div_right
    apply Nat.mul_le_mul_left
    omega
  · have hq : r + 1 = h := by omega
    have e1 : (a + 1) / h = q + 1 := by
      have : a + 1 = h * (q + 1) := by rw [Nat.mul_add]; omega
      rw [this, Nat.mul_div_cancel_left _ hpos]
    have e2 : (a + 1) % h = 0 := by
      have : a + 1 = h * (q + 1) := by rw [Nat.mul_add]; omega
      rw [this, Nat.mul_mod_right]
    rw [e1, e2]
    have hH : H (q + 1) = H q / 2 := by
      unfold H; rw [Nat.pow_succ, ← Nat.div_div_eq_div_mul]
    simp only [Nat.sub_zero]
    rw [Nat.mul_div_cancel _ (by omega), hH]
    have : H q * h / (2 * h) = H q / 2 := Nat.mul_div_mul_right _ _ hpos
    calc H q / 2 = H q * h / (2 * h) := this.symm
      _ ≤ H q * (2 * h - r) / (2 * h) := by
        apply Nat.div_le_div_right; apply Nat.mul_le_mul_left; omega

theorem decayN_antitone (a d h : Nat) (hpos : 0 < h) : decayN (a + d) h ≤ decayN a h := by
  induction d with
  | zero => simp
  | succ n ih => exact le_trans (decayN_step (a + n) h hpos) ih

/-- An undecayed session weighs `WEIGHT_ONE`. -/
theorem decay_one_at_zero (h : U64) (hpos : 0 < h.val) (hsmall : h.val ≤ 2 ^ 40) :
    focus.decay_weight 0#u64 h ⦃ r => r.val = 65536 ⦄ := by
  apply WP.spec_mono (decay_weight_spec 0#u64 h hpos hsmall)
  intro r hr
  rw [hr]
  unfold decayN H
  simp
  exact Nat.mul_div_cancel _ (by omega)

/-- After `k` whole half-lives the weight is exactly `WEIGHT_ONE / 2^k`. -/
theorem decay_halves (a h : U64) (k : Nat) (hpos : 0 < h.val) (hsmall : h.val ≤ 2 ^ 40)
    (ha : a.val = k * h.val) :
    focus.decay_weight a h ⦃ r => r.val = 65536 / 2 ^ k ⦄ := by
  apply WP.spec_mono (decay_weight_spec a h hpos hsmall)
  intro r hr
  rw [hr]
  unfold decayN H
  rw [ha, Nat.mul_div_cancel _ hpos, Nat.mul_mod_left]
  simp
  exact Nat.mul_div_cancel _ (by omega)

/-- An older session never weighs more. -/
theorem decay_antitone (a b h : U64) (hab : a.val ≤ b.val) (hpos : 0 < h.val)
    (hsmall : h.val ≤ 2 ^ 40) :
    focus.decay_weight a h ⦃ ra => focus.decay_weight b h ⦃ rb => rb.val ≤ ra.val ⦄ ⦄ := by
  apply WP.spec_mono (decay_weight_spec a h hpos hsmall)
  intro ra hra
  apply WP.spec_mono (decay_weight_spec b h hpos hsmall)
  intro rb hrb
  rw [hra, hrb]
  have := decayN_antitone a.val (b.val - a.val) h.val hpos
  rwa [Nat.add_sub_cancel' hab] at this

/-! ## Posterior, uncertainty and hesitation -/

theorem basis_val : focus.BASIS.val = 10000 := by
  unfold focus.BASIS; rfl

/-- The posterior mean in `Nat` (division by 0 is 0, as the code's). -/
def postN (e a al be : Nat) : Nat := (e + al * 65536) * 10000 / (a + (al + be) * 65536)

theorem posterior_spec (e a al be : U64) (hal : al.val ≤ 2 ^ 20) (hbe : be.val ≤ 2 ^ 20)
    (he : e.val ≤ 2 ^ 40) (ha : a.val ≤ 2 ^ 40) :
    focus.posterior_bp e a al be ⦃ r => r.val = postN e.val a.val al.val be.val ⦄ := by
  unfold focus.posterior_bp
  simp only [lift, bind_ok]
  have w := weight_one
  have bs := basis_val
  have d1 := U64.checked_mul_bv_spec al focus.WEIGHT_ONE
  split
  · rename_i h; rw [h] at d1; simp at d1; scalar_tac
  rename_i p e1
  rw [e1] at d1
  obtain ⟨_, hp, _⟩ := d1
  have d2 := U64.checked_add_bv_spec al be
  split
  · rename_i h; rw [h] at d2; simp at d2; scalar_tac
  rename_i t e2
  rw [e2] at d2
  obtain ⟨_, ht, _⟩ := d2
  have d3 := U64.checked_mul_bv_spec t focus.WEIGHT_ONE
  split
  · rename_i h; rw [h] at d3; simp at d3; scalar_tac
  rename_i p1 e3
  rw [e3] at d3
  obtain ⟨_, hp1, _⟩ := d3
  have d4 := U64.checked_add_bv_spec e p
  split
  · rename_i h; rw [h] at d4; simp at d4; scalar_tac
  rename_i n e4
  rw [e4] at d4
  obtain ⟨_, hn, _⟩ := d4
  have d5 := U64.checked_mul_bv_spec n focus.BASIS
  split
  · rename_i h; rw [h] at d5; simp at d5; scalar_tac
  rename_i m e5
  rw [e5] at d5
  obtain ⟨_, hm, _⟩ := d5
  have d6 := U64.checked_add_bv_spec a p1
  split
  · rename_i h; rw [h] at d6; simp at d6; scalar_tac
  rename_i d e6
  rw [e6] at d6
  obtain ⟨_, hd, _⟩ := d6
  have d7 := U64.checked_div_bv_spec m d
  cases e7 : U64.checked_div m d with
  | none =>
    rw [e7] at d7; simp at d7
    simp only [core.option.Option.unwrap_or, WP.spec_ok, postN]
    rw [← w, ← bs, ← hp, ← ht, ← hp1, ← hn, ← hm, ← hd, d7]
    simp
  | some q =>
    rw [e7] at d7
    obtain ⟨_, hq, _⟩ := d7
    simp only [core.option.Option.unwrap_or, WP.spec_ok, postN]
    rw [hq, hm, hd, hn, hp1, ht, hp, w, bs]

/-- At most 10,000 basis points when errors never exceed attempts. -/
theorem posterior_in_range (e a al be : U64) (hal : al.val ≤ 2 ^ 20) (hbe : be.val ≤ 2 ^ 20)
    (he : e.val ≤ a.val) (ha : a.val ≤ 2 ^ 40) :
    focus.posterior_bp e a al be ⦃ r => r.val ≤ 10000 ⦄ := by
  apply WP.spec_mono (posterior_spec e a al be hal hbe (by omega) ha)
  intro r hr
  rw [hr]
  unfold postN
  apply Nat.div_le_of_le_mul
  have : e.val + al.val * 65536 ≤ a.val + (al.val + be.val) * 65536 := by nlinarith
  nlinarith

/-- With no evidence the posterior is the prior mean. -/
theorem posterior_prior_at_zero (al be : U64) (hal : al.val ≤ 2 ^ 20) (hbe : be.val ≤ 2 ^ 20) :
    focus.posterior_bp 0#u64 0#u64 al be ⦃ r => r.val = al.val * 10000 / (al.val + be.val) ⦄ := by
  apply WP.spec_mono (posterior_spec 0#u64 0#u64 al be hal hbe (by simp) (by simp))
  intro r hr
  rw [hr]
  unfold postN
  have z : (0#u64 : U64).val = 0 := by simp
  rw [z, Nat.zero_add, Nat.zero_add]
  rw [show al.val * 65536 * 10000 = al.val * 10000 * 65536 by ring]
  exact Nat.mul_div_mul_right _ _ (by norm_num)

/-- More errors at equal attempts never lower the posterior. -/
theorem posterior_monotone_errors (e1 e2 a al be : U64) (h12 : e1.val ≤ e2.val)
    (hal : al.val ≤ 2 ^ 20) (hbe : be.val ≤ 2 ^ 20) (he : e2.val ≤ 2 ^ 40) (ha : a.val ≤ 2 ^ 40) :
    focus.posterior_bp e1 a al be ⦃ r1 => focus.posterior_bp e2 a al be ⦃ r2 => r1.val ≤ r2.val ⦄ ⦄ := by
  apply WP.spec_mono (posterior_spec e1 a al be hal hbe (by omega) ha)
  intro r1 hr1
  apply WP.spec_mono (posterior_spec e2 a al be hal hbe he ha)
  intro r2 hr2
  rw [hr1, hr2]
  unfold postN
  apply Nat.div_le_div_right
  apply Nat.mul_le_mul_right
  omega

/-- Below its sample minimum a skill's hesitation is 0. Total. -/
theorem hesitation_needs_samples (st sm gt gm mn : U64) (h : st.val < mn.val) :
    focus.hesitation_bp st sm gt gm mn ⦃ r => r.val = 0 ⦄ := by
  unfold focus.hesitation_bp
  have : st < mn := by scalar_tac
  simp [this, WP.spec_ok]

/-- Hesitation is always within 0..=10,000. Total. -/
theorem hesitation_in_range (st sm gt gm mn : U64) :
    focus.hesitation_bp st sm gt gm mn ⦃ r => r.val ≤ 10000 ⦄ := by
  unfold focus.hesitation_bp
  have bs := basis_val
  split
  · simp [WP.spec_ok]
  · simp only [lift, bind_ok]
    split
    · simp [WP.spec_ok]
    · split
      · simp [WP.spec_ok]
      · split
        · simp [WP.spec_ok, bs]
        · split
          · simp [WP.spec_ok]
          · try simp only [bind_ok, lift]
            split
            · simp [WP.spec_ok, bs]
            · rename_i hle
              simp only [WP.spec_ok]
              scalar_tac

/-! ## Qualifying and picking -/

theorem sat_add_val (x y : U64) :
    (core.num.U64.saturating_add x y).val = min (x.val + y.val) U64.max := by
  have h1 : (UScalar.saturating_add x y).val = min (UScalar.max .U64) (x.val + y.val) := by
    show (BitVec.ofNat _ (min (UScalar.max .U64) (x.val + y.val))).toNat = _
    rw [BitVec.toNat_ofNat]
    apply Nat.mod_eq_of_lt
    have := Nat.two_pow_pos (UScalarTy.U64.numBits)
    rw [UScalar.max_def]
    omega
  simp only [core.num.U64.saturating_add, h1, UScalar.max_U64_eq, U64.max_eq]
  omega

theorem weighted_spec (w v : U64) :
    focus.weighted w v ⦃ r => r.val = min w.val 10000 * min v.val 10000 ⦄ := by
  unfold focus.weighted
  have bs := basis_val
  have key : ∀ x : U64, ∃ x' : U64,
      (if x > focus.BASIS then ok focus.BASIS else ok x : Result U64) = ok x' ∧
      x'.val = min x.val 10000 := by
    intro x
    by_cases h : x > focus.BASIS
    · refine ⟨focus.BASIS, by simp [h], ?_⟩
      have : 10000 < x.val := by scalar_tac
      rw [bs]; omega
    · refine ⟨x, by simp [h], ?_⟩
      have : x.val ≤ 10000 := by scalar_tac
      omega
  obtain ⟨w', ew, hw'⟩ := key w
  obtain ⟨v', ev, hv'⟩ := key v
  rw [ew, ev]
  simp only [bind_ok, lift]
  have d := U64.checked_mul_bv_spec w' v'
  have hb : w'.val * v'.val ≤ 10000 * 10000 :=
    Nat.mul_le_mul (by rw [hw']; omega) (by rw [hv']; omega)
  cases e : U64.checked_mul w' v' with
  | none => rw [e] at d; simp at d; scalar_tac
  | some p =>
    rw [e] at d
    obtain ⟨_, hp, _⟩ := d
    simp only [core.option.Option.unwrap_or, WP.spec_ok]
    rw [hp, hw', hv']

theorem above_spec (v th : U64) :
    focus.above v th ⦃ r => r.val = if th.val ≤ v.val then v.val else 0 ⦄ := by
  unfold focus.above
  split <;> simp_all [WP.spec_ok] <;> scalar_tac

theorem sat_sub_val (x y : U64) : (core.num.U64.saturating_sub x y).val = x.val - y.val := by
  show (BitVec.ofNat _ (Max.max 0 (x.val - y.val))).toNat = _
  rw [BitVec.toNat_ofNat]
  simp only [Nat.zero_max]
  apply Nat.mod_eq_of_lt
  have := x.hBounds
  omega

/-- `value` when it reaches `threshold`, else 0. -/
def ab (v th : Nat) : Nat := if th ≤ v then v else 0

/-- The signal in `Nat`. -/
def sigOf (c : focus.Candidate) (t : focus.Thresholds) : Nat :=
  let ex := if 0 < c.errors.val then c.posterior.val - c.group_rate.val else 0
  min (min t.weight_error.val 10000 * min (ab ex t.excess.val) 10000 +
       min t.weight_hesitation.val 10000 * min (ab c.hesitation.val t.hesitation.val) 10000)
      U64.max

theorem signal_spec (c : focus.Candidate) (t : focus.Thresholds) :
    focus.signal c t ⦃ s => s.val = sigOf c t ⦄ := by
  unfold focus.signal
  have step : ∀ x : U64, x.val = (if 0 < c.errors.val then c.posterior.val - c.group_rate.val else 0) →
      (do
      let i ← focus.above x t.excess
      let e ← focus.weighted t.weight_error i
      let i1 ← focus.above c.hesitation t.hesitation
      let h ← focus.weighted t.weight_hesitation i1
      ok (core.num.U64.saturating_add e h) : Result U64) ⦃ s => s.val = sigOf c t ⦄ := by
    intro x hx
    apply WP.spec_bind (above_spec x t.excess); intro i hi
    apply WP.spec_bind (weighted_spec t.weight_error i); intro e he
    apply WP.spec_bind (above_spec c.hesitation t.hesitation); intro i1 hi1
    apply WP.spec_bind (weighted_spec t.weight_hesitation i1); intro h hh
    simp only [WP.spec_ok, sat_add_val, sigOf, ab]
    rw [he, hh, hi, hi1, hx]
  split
  · rename_i hpos
    simp only [bind_ok]
    apply step
    rw [sat_sub_val]
    have : 0 < c.errors.val := by scalar_tac
    simp [this]
  · rename_i hz
    simp only [bind_ok]
    apply step
    have : ¬ 0 < c.errors.val := by scalar_tac
    simp [this]

/-- A quiet skill's signal is 0: no errors and hesitation below its
threshold. -/
theorem signal_quiet (c : focus.Candidate) (t : focus.Thresholds)
    (he : c.errors.val = 0) (hh : c.hesitation.val < t.hesitation.val) :
    focus.signal c t ⦃ s => s.val = 0 ⦄ := by
  unfold focus.signal
  have hne : ¬ c.errors > 0#u64 := by scalar_tac
  simp only [hne, ↓reduceIte, bind_ok]
  apply WP.spec_bind (above_spec 0#u64 t.excess); intro i hi
  have hi0 : i.val = 0 := by rw [hi]; split <;> simp
  apply WP.spec_bind (weighted_spec t.weight_error i); intro e he'
  apply WP.spec_bind (above_spec c.hesitation t.hesitation); intro i1 hi1
  have hi10 : i1.val = 0 := by rw [hi1]; split <;> [omega; rfl]
  apply WP.spec_bind (weighted_spec t.weight_hesitation i1); intro h hh'
  simp only [WP.spec_ok, sat_add_val]
  rw [he', hh', hi0, hi10]
  simp

theorem qualifies_spec (c : focus.Candidate) (t : focus.Thresholds) :
    focus.qualifies c t ⦃ q => (q = true ↔ t.attempts.val ≤ c.attempts.val ∧ 0 < sigOf c t) ⦄ := by
  unfold focus.qualifies
  split
  · rename_i hge
    apply WP.spec_bind (signal_spec c t)
    intro s hs
    simp only [WP.spec_ok, decide_eq_true_eq]
    constructor
    · intro h; exact ⟨by scalar_tac, by rw [← hs]; scalar_tac⟩
    · intro ⟨_, h⟩; rw [← hs] at h; scalar_tac
  · rename_i hlt
    simp only [WP.spec_ok, Bool.false_eq_true, false_iff, not_and]
    intro h; exfalso; scalar_tac

/-- Sparse evidence never qualifies (it is uncertain, never a focus). -/
theorem sparse_never_qualifies (c : focus.Candidate) (t : focus.Thresholds)
    (h : c.attempts.val < t.attempts.val) :
    focus.qualifies c t ⦃ q => q = false ⦄ := by
  unfold focus.qualifies
  have : ¬ c.attempts >= t.attempts := by scalar_tac
  simp [this, WP.spec_ok]

/-- A quiet skill (no errors, hesitation below its threshold) never
qualifies. -/
theorem quiet_never_qualifies (c : focus.Candidate) (t : focus.Thresholds)
    (he : c.errors.val = 0) (hh : c.hesitation.val < t.hesitation.val) :
    focus.qualifies c t ⦃ q => q = false ⦄ := by
  unfold focus.qualifies
  split
  · apply WP.spec_bind (signal_quiet c t he hh)
    intro s hs
    simp only [WP.spec_ok, decide_eq_false_iff_not]
    scalar_tac
  · simp [WP.spec_ok]

def qualOf (c : focus.Candidate) (t : focus.Thresholds) : Prop :=
  t.attempts.val ≤ c.attempts.val ∧ 0 < sigOf c t

/-- Candidate `k` may be picked: present, not chosen, qualifying. -/
def elig (cs : List focus.Candidate) (ch : List Bool) (t : focus.Thresholds) (k : Nat) : Prop :=
  ∃ hk : k < cs.length, ∃ hc : k < ch.length, ch[k] = false ∧ qualOf cs[k] t

theorem taken_spec (ch : Slice Bool) (i : Usize) :
    focus.taken ch i ⦃ b => (b = false ↔ ∃ hc : i.val < ch.val.length, ch.val[i.val] = false) ⦄ := by
  unfold focus.taken
  simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
    core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, bind_ok]
  by_cases hi : i.val < ch.val.length
  · rw [List.getElem?_eq_getElem hi]
    simp [WP.spec_ok, hi]
  · rw [List.getElem?_eq_none (by omega)]
    simp [WP.spec_ok, hi]

theorem beats_spec (best : Option Usize) (bs s : U64) :
    focus.beats best bs s ⦃ r => (r = true ↔ best = none ∨ bs.val < s.val) ⦄ := by
  unfold focus.beats
  cases best <;> simp [WP.spec_ok]

theorem pick_step (cs : Slice focus.Candidate) (ch : Slice Bool) (t : focus.Thresholds)
    (i : Usize) (hi : i.val < cs.val.length) (best : Option Usize) (bs : U64) :
    (do
      let s ← focus.signal cs.val[i.val] t
      let b ← focus.taken ch i
      if b then ok (best, bs)
      else
        let b1 ← focus.qualifies cs.val[i.val] t
        if b1 then
          let b2 ← focus.beats best bs s
          if b2 then ok (some i, s) else ok (best, bs)
        else ok (best, bs))
      ⦃ p => ((elig cs.val ch.val t i.val ∧ (best = none ∨ bs.val < sigOf cs.val[i.val] t)) →
              p.1 = some i ∧ p.2.val = sigOf cs.val[i.val] t) ∧
             (¬ (elig cs.val ch.val t i.val ∧ (best = none ∨ bs.val < sigOf cs.val[i.val] t)) →
              p = (best, bs)) ⦄ := by
  apply WP.spec_bind (signal_spec cs.val[i.val] t)
  intro s hs
  apply WP.spec_bind (taken_spec ch i)
  intro b hb
  cases b with
  | true =>
    simp only [↓reduceIte]
    rw [WP.spec_ok]
    refine ⟨fun ⟨⟨_, hc, hf, _⟩, _⟩ => absurd (hb.mpr ⟨hc, hf⟩) (by simp), fun _ => rfl⟩
  | false =>
    obtain ⟨hc, hf⟩ := hb.mp rfl
    simp only [Bool.false_eq_true, ↓reduceIte]
    apply WP.spec_bind (qualifies_spec cs.val[i.val] t)
    intro q hq
    cases q with
    | false =>
      simp only [Bool.false_eq_true, ↓reduceIte]
      rw [WP.spec_ok]
      refine ⟨fun ⟨⟨_, _, _, hql⟩, _⟩ => absurd (hq.mpr hql) (by simp), fun _ => rfl⟩
    | true =>
      simp only [↓reduceIte]
      apply WP.spec_bind (beats_spec best bs s)
      intro r hr
      rw [hs] at hr
      cases r with
      | true =>
        simp only [↓reduceIte]
        rw [WP.spec_ok]
        refine ⟨fun _ => ⟨rfl, hs⟩, fun h => absurd ⟨⟨hi, hc, hf, hq.mp rfl⟩, hr.mp rfl⟩ h⟩
      | false =>
        simp only [Bool.false_eq_true, ↓reduceIte]
        rw [WP.spec_ok]
        refine ⟨fun ⟨_, h⟩ => absurd (hr.mpr h) (by simp), fun _ => rfl⟩

/-- The pick invariant after positions below `j`. -/
def pickInv (cs : List focus.Candidate) (ch : List Bool) (t : focus.Thresholds) (j : Nat)
    (best : Option Usize) (bs : U64) : Prop :=
  (best = none → ∀ k, k < j → ¬ elig cs ch t k) ∧
  (∀ b, best = some b → ∃ hb : b.val < cs.length, b.val < j ∧ elig cs ch t b.val ∧
    bs.val = sigOf cs[b.val] t ∧
    ∀ k (hk : k < cs.length), k < j → elig cs ch t k →
      sigOf cs[k] t ≤ bs.val ∧ (sigOf cs[k] t = bs.val → b.val ≤ k))

/-- What `pick` returns. -/
def pickPost (cs : List focus.Candidate) (ch : List Bool) (t : focus.Thresholds)
    (r : Option Usize) : Prop :=
  match r with
  | none => ∀ k, ¬ elig cs ch t k
  | some b => ∃ hb : b.val < cs.length, elig cs ch t b.val ∧
      ∀ k (hk : k < cs.length), elig cs ch t k →
        sigOf cs[k] t ≤ sigOf cs[b.val] t ∧ (sigOf cs[k] t = sigOf cs[b.val] t → b.val ≤ k)

theorem pick_loop_spec (cs : Slice focus.Candidate) (ch : Slice Bool) (t : focus.Thresholds)
    (i : Usize) (best : Option Usize) (bs : U64) (hi : i.val ≤ cs.val.length)
    (hinv : pickInv cs.val ch.val t i.val best bs) :
    focus.pick_loop cs ch t i best bs ⦃ r => pickPost cs.val ch.val t r ⦄ := by
  unfold focus.pick_loop
  have hlen : (Slice.len cs).val = cs.val.length := Slice.len_val cs
  apply loop.spec_decr_nat
    (fun st : Usize × Option Usize × U64 => cs.val.length - st.1.val)
    (fun st : Usize × Option Usize × U64 => st.1.val ≤ cs.val.length ∧
      pickInv cs.val ch.val t st.1.val st.2.1 st.2.2)
  · rintro ⟨j, b, sb⟩ ⟨hj, hb1, hb2⟩
    simp only at hj hb1 hb2
    unfold focus.pick_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < cs.val.length := by scalar_tac
      have hget : cs.val[j.val]? = some cs.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
      have hs1 := Proofs.Time.usize_succ j (by scalar_tac)
      apply WP.spec_bind (pick_step cs ch t j hlt' b sb)
      rintro ⟨b', s'⟩ hp
      simp only [lift, bind_ok]
      show ok (ControlFlow.cont (core.num.Usize.saturating_add j 1#usize, b', s')) ⦃ _ ⦄
      simp only [WP.spec_ok, hs1, pickInv]
      by_cases htake : elig cs.val ch.val t j.val ∧ (b = none ∨ sb.val < sigOf cs.val[j.val] t)
      · obtain ⟨hb', hs'⟩ := hp.1 htake
        simp only at hb' hs'
        rw [hb']
        obtain ⟨hel, hbetter⟩ := htake
        refine ⟨⟨by omega, ?_, ?_⟩, by omega⟩
        · intro h; cases h
        · intro b0 hb0
          cases hb0
          refine ⟨hlt', by omega, hel, hs', ?_⟩
          intro k hk hkj hek
          by_cases hki : k = j.val
          · subst hki; rw [hs']; exact ⟨le_refl _, fun _ => le_refl _⟩
          · cases b with
            | none => exact absurd hek (hb1 rfl k (by omega))
            | some b1 =>
              obtain ⟨_, _, _, hbs, hall⟩ := hb2 b1 rfl
              have h1 := hall k hk (by omega) hek
              rcases hbetter with hn | hlt2
              · cases hn
              · rw [hs']; refine ⟨by omega, fun _ => by omega⟩
      · have hp2 := hp.2 htake
        simp only [Prod.mk.injEq] at hp2
        obtain ⟨hb', hs'⟩ := hp2
        rw [hb', hs']
        refine ⟨⟨by omega, ?_, ?_⟩, by omega⟩
        · intro hn k hk hel
          by_cases hki : k = j.val
          · subst hki; exact htake ⟨hel, Or.inl hn⟩
          · exact hb1 hn k (by omega) hel
        · intro b0 hb0
          obtain ⟨hbl, hbj, hel, hbs, hall⟩ := hb2 b0 hb0
          refine ⟨hbl, by omega, hel, hbs, ?_⟩
          intro k hk hkj hek
          by_cases hki : k = j.val
          · subst hki
            have : ¬ sb.val < sigOf cs.val[j.val] t := fun h => htake ⟨hek, Or.inr h⟩
            refine ⟨by omega, fun _ => by omega⟩
          · exact hall k hk (by omega) hek
    · rename_i hge
      have hge' : cs.val.length ≤ j.val := by scalar_tac
      simp only [WP.spec_ok, pickPost]
      cases b with
      | none =>
        intro k hk
        exact hb1 rfl k (by obtain ⟨hk', _⟩ := hk; omega) hk
      | some b0 =>
        obtain ⟨hbl, _, hel, hbs, hall⟩ := hb2 b0 rfl
        refine ⟨hbl, hel, ?_⟩
        intro k hk hek
        rw [← hbs]
        exact hall k hk (by omega) hek
  · exact ⟨hi, hinv⟩

theorem pick_spec (cs : Slice focus.Candidate) (ch : Slice Bool) (t : focus.Thresholds) :
    focus.pick cs ch t ⦃ r => pickPost cs.val ch.val t r ⦄ := by
  unfold focus.pick
  apply pick_loop_spec cs ch t 0#usize none 0#u64 (by simp)
  exact ⟨fun _ k hk => by simp at hk, fun b hb => by cases hb⟩

/-- A picked candidate is present, not yet chosen, and qualifies. -/
theorem pick_sound (cs : Slice focus.Candidate) (ch : Slice Bool) (t : focus.Thresholds) :
    focus.pick cs ch t ⦃ r => ∀ b, r = some b → elig cs.val ch.val t b.val ⦄ := by
  apply WP.spec_mono (pick_spec cs ch t)
  intro r hr b hb
  subst hb
  exact hr.2.1

/-- No qualifying unchosen candidate has a greater signal; among equal
signals the lower index is picked. -/
theorem pick_maximal (cs : Slice focus.Candidate) (ch : Slice Bool) (t : focus.Thresholds) :
    focus.pick cs ch t ⦃ r => ∀ b, r = some b → ∃ hb : b.val < cs.val.length,
      ∀ k (hk : k < cs.val.length), elig cs.val ch.val t k →
        sigOf cs.val[k] t ≤ sigOf cs.val[b.val] t ∧
        (sigOf cs.val[k] t = sigOf cs.val[b.val] t → b.val ≤ k) ⦄ := by
  apply WP.spec_mono (pick_spec cs ch t)
  intro r hr b hb
  subst hb
  obtain ⟨hbl, _, hall⟩ := hr
  exact ⟨hbl, hall⟩

/-- `decay_weight` never fails. -/
theorem decay_total (a h : U64) : focus.decay_weight a h ⦃ _ => True ⦄ := by
  unfold focus.decay_weight
  simp only [lift, bind_ok]
  repeat' split
  all_goals first
    | simp [WP.spec_ok]
    | (apply WP.spec_bind (halved_spec _); intro b _
       try simp only [lift, bind_ok]
       split <;> simp [WP.spec_ok])

/-- `posterior_bp` never fails. -/
theorem posterior_total (e a al be : U64) : focus.posterior_bp e a al be ⦃ _ => True ⦄ := by
  unfold focus.posterior_bp
  simp only [lift, bind_ok]
  repeat' split
  all_goals simp [WP.spec_ok]

end Proofs.Focus
