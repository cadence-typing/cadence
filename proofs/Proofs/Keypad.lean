import Extracted.CadenceKernel
import Proofs.Time
import Proofs.Focus

/-!
# The numeric keypad (`cadence_kernel::keypad`, #101, #102)

`trainer::numeric::KeypadEntry::new` serves an entry only when
`keypad.well_formed` accepts it; `trainer::plan_content` takes each numpad
entry's family from `keypad.family_at`; `trainer::learner::group_of` admits
a numpad focus with `keypad.focus_admissible`; `core::alphabet` and the
learner classify keys with `keypad.class`.

Proved: the keypad alphabet is exactly `0`–`9`, `.` `,` and `+ - * /`, and a
key counts as a digit exactly when it is `0`–`9`; the entry grammar is the
table of the RFC (`sign? operand (operator operand)*`), so an entry never
ends in an operator, a sign or a separator, never has two operators in a
row and never two separators in one operand; `well_formed` is that grammar
run over the whole string; `family_at` returns the family whose run of the
cycle holds `position mod cycle`, never a family of weight zero, only
`Plain` or `Decimal` with operators off, and `Plain` for an empty mix;
`carries` and `focus_admissible` are the RFC's tables, and with operators
off no operator skill is admissible. All total.

Trusted, not proved: that callers pass `char as u32` codes, the policy
weights and the cycle position (tested: `trainer` keypad tests and
`consumer_boundaries.rs`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Keypad

/-! ## The alphabet -/

/-- The keypad class of a code, from the character sets. -/
def classOf (v : Nat) : keypad.Class :=
  if 48 ≤ v ∧ v ≤ 57 then .Digit
  else if v = 46 ∨ v = 44 then .Separator
  else if v = 43 ∨ v = 45 ∨ v = 42 ∨ v = 47 then .Operator
  else .Outside

/-- `+` or `-`. -/
def signOf (v : Nat) : Bool := decide (v = 43 ∨ v = 45)

theorem class_eq (c : U32) : keypad.class c = ok (classOf c.val) := by
  unfold keypad.class keypad.class_other classOf
  simp only [keypad.DIGIT_ZERO, keypad.DIGIT_NINE, keypad.POINT, keypad.COMMA,
    keypad.PLUS, keypad.MINUS, keypad.TIMES, keypad.DIVIDE]
  split_ifs <;> simp_all <;> scalar_tac

theorem is_sign_eq (c : U32) : keypad.is_sign c = ok (signOf c.val) := by
  unfold keypad.is_sign signOf
  simp only [keypad.PLUS, keypad.MINUS]
  split_ifs <;> simp_all <;> scalar_tac

/-- The class of every code is the class of its character set. Total. -/
theorem class_spec (c : U32) : keypad.class c ⦃ k => k = classOf c.val ⦄ := by
  rw [class_eq]; simp [WP.spec_ok]

/-- A key is a digit exactly when its code is `0`–`9` (48–57), so no
operator or separator is ever counted as a digit. Total. -/
theorem digit_iff_ascii_digit (c : U32) :
    keypad.class c ⦃ k => (k = .Digit ↔ 48 ≤ c.val ∧ c.val ≤ 57) ⦄ := by
  rw [class_eq]
  simp only [WP.spec_ok, classOf]
  split_ifs <;> simp_all

/-! ## The entry grammar -/

/-- The RFC's grammar table. -/
def stepOf : keypad.State → keypad.Class → Bool → keypad.State
  | .Start, .Digit, _ => .Whole
  | .Start, .Operator, true => .Signed
  | .Signed, .Digit, _ => .Whole
  | .Whole, .Digit, _ => .Whole
  | .Whole, .Separator, _ => .Point
  | .Whole, .Operator, _ => .Operator
  | .Point, .Digit, _ => .Fraction
  | .Fraction, .Digit, _ => .Fraction
  | .Fraction, .Operator, _ => .Operator
  | .Operator, .Digit, _ => .Whole
  | _, _, _ => .Reject

/-- An entry may end only after a digit. -/
def acceptOf : keypad.State → Bool
  | .Whole => true
  | .Fraction => true
  | _ => false

theorem step_eq (s : keypad.State) (k : keypad.Class) (b : Bool) :
    keypad.step s k b = ok (stepOf s k b) := by
  cases s <;> cases k <;> cases b <;> rfl

theorem accepting_eq (s : keypad.State) : keypad.accepting s = ok (acceptOf s) := by
  cases s <;> rfl

/-- `step` is the RFC's table. Total. -/
theorem step_spec (s : keypad.State) (k : keypad.Class) (b : Bool) :
    keypad.step s k b ⦃ r => r = stepOf s k b ⦄ := by
  rw [step_eq]; simp [WP.spec_ok]

/-- Nothing leaves `Reject`. -/
theorem reject_absorbing (k : keypad.Class) (b : Bool) :
    keypad.step .Reject k b ⦃ r => r = .Reject ⦄ := by
  rw [step_eq]; cases k <;> cases b <;> simp [WP.spec_ok, stepOf]

/-- A key outside the keypad rejects the entry. -/
theorem outside_rejects (s : keypad.State) (b : Bool) :
    keypad.step s .Outside b ⦃ r => r = .Reject ⦄ := by
  rw [step_eq]; cases s <;> cases b <;> simp [WP.spec_ok, stepOf]

/-- An operator right after an operator or a sign rejects the entry. -/
theorem no_adjacent_operators (s : keypad.State) (b : Bool)
    (h : s = .Operator ∨ s = .Signed) :
    keypad.step s .Operator b ⦃ r => r = .Reject ⦄ := by
  rw [step_eq]; rcases h with h | h <;> subst h <;> cases b <;> simp [WP.spec_ok, stepOf]

/-- A second separator in one operand rejects the entry. -/
theorem one_separator_per_operand (s : keypad.State) (b : Bool)
    (h : s = .Point ∨ s = .Fraction) :
    keypad.step s .Separator b ⦃ r => r = .Reject ⦄ := by
  rw [step_eq]; rcases h with h | h <;> subst h <;> cases b <;> simp [WP.spec_ok, stepOf]

/-- Only `Whole` and `Fraction` accept: an entry never ends in an operator, a
sign or a separator. Total. -/
theorem accepting_spec (s : keypad.State) :
    keypad.accepting s ⦃ b => (b = true ↔ s = .Whole ∨ s = .Fraction) ⦄ := by
  rw [accepting_eq]; cases s <;> simp [WP.spec_ok, acceptOf]

/-- The grammar run over a list of codes. -/
def runOf : keypad.State → List U32 → keypad.State
  | s, [] => s
  | s, c :: cs => runOf (stepOf s (classOf c.val) (signOf c.val)) cs

theorem run_from_loop_spec (cs : Slice U32) (i : Usize) (s : keypad.State)
    (hi : i.val ≤ cs.val.length) :
    keypad.run_from_loop cs i s ⦃ r => r = runOf s (cs.val.drop i.val) ⦄ := by
  unfold keypad.run_from_loop
  have hlen : (Slice.len cs).val = cs.val.length := Slice.len_val cs
  apply loop.spec_decr_nat
    (fun st : Usize × keypad.State => cs.val.length - st.1.val)
    (fun st : Usize × keypad.State => st.1.val ≤ cs.val.length ∧
      runOf st.2 (cs.val.drop st.1.val) = runOf s (cs.val.drop i.val))
  · rintro ⟨j, t⟩ ⟨hj, ht⟩
    simp only at hj ht
    unfold keypad.run_from_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < cs.val.length := by scalar_tac
      have hget : cs.val[j.val]? = some cs.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok,
        class_eq, is_sign_eq, step_eq]
      have hs1 := Proofs.Time.usize_succ j (by scalar_tac)
      simp only [lift, bind_ok, WP.spec_ok, hs1]
      rw [List.drop_eq_getElem_cons hlt'] at ht
      simp only [runOf] at ht
      refine ⟨⟨by omega, ht⟩, by omega⟩
    · rename_i hge
      simp only [WP.spec_ok]
      have : cs.val.length ≤ j.val := by scalar_tac
      rw [List.drop_eq_nil_of_le this] at ht
      simp only [runOf] at ht
      exact ht
  · exact ⟨hi, rfl⟩

/-- `well_formed` is the grammar run over the whole string from `Start`,
accepted only after a digit; so the empty string is rejected. Total. -/
theorem well_formed_spec (cs : Slice U32) :
    keypad.well_formed cs ⦃ b => b = acceptOf (runOf .Start cs.val) ⦄ := by
  unfold keypad.well_formed keypad.run_from
  have h := run_from_loop_spec cs 0#usize .Start (by simp)
  apply WP.spec_bind h
  intro r hr
  rw [hr, accepting_eq]; simp [WP.spec_ok]

/-! ## The family schedule -/

/-- The weights a session may serve. -/
def effOf (w : keypad.Weights) (op : Bool) : keypad.Weights :=
  if op then w else { w with signed := 0#u64, operation := 0#u64, chain := 0#u64 }

/-- The cycle length in `Nat`. -/
def sumOf (w : keypad.Weights) : Nat :=
  w.plain.val + w.decimal.val + w.signed.val + w.operation.val + w.chain.val

/-- The weight of a family. -/
def weightOf (w : keypad.Weights) : keypad.Family → Nat
  | .Plain => w.plain.val
  | .Decimal => w.decimal.val
  | .Signed => w.signed.val
  | .Operation => w.operation.val
  | .Chain => w.chain.val

theorem admissible_eq (w : keypad.Weights) (op : Bool) :
    keypad.admissible w op = ok (effOf w op) := by
  unfold keypad.admissible effOf; cases op <;> rfl

theorem cycle_spec (w : keypad.Weights) :
    keypad.cycle w ⦃ o => match o with
      | some t => t.val = sumOf w
      | none => U64.max < sumOf w ⦄ := by
  unfold keypad.cycle sumOf
  have d1 := U64.checked_add_bv_spec w.plain w.decimal
  simp only [lift, bind_ok]
  split
  · rename_i e; rw [e] at d1; simp at d1; simp [WP.spec_ok]; scalar_tac
  rename_i a e1
  rw [e1] at d1; obtain ⟨_, ha, _⟩ := d1
  have d2 := U64.checked_add_bv_spec a w.signed
  split
  · rename_i e; rw [e] at d2; simp at d2; simp [WP.spec_ok]; scalar_tac
  rename_i b e2
  rw [e2] at d2; obtain ⟨_, hb, _⟩ := d2
  have d3 := U64.checked_add_bv_spec b w.operation
  split
  · rename_i e; rw [e] at d3; simp at d3; simp [WP.spec_ok]; scalar_tac
  rename_i c e3
  rw [e3] at d3; obtain ⟨_, hc, _⟩ := d3
  have d4 := U64.checked_add_bv_spec c w.chain
  cases e4 : U64.checked_add c w.chain with
  | none => rw [e4] at d4; simp at d4; simp [WP.spec_ok]; scalar_tac
  | some t => rw [e4] at d4; obtain ⟨_, ht, _⟩ := d4; simp [WP.spec_ok]; scalar_tac

/-- With `r = position mod cycle`, the family whose run of consecutive
positions (plain, decimal, signed, operation, chain) holds `r`: so a whole
cycle serves each family exactly its weight. -/
theorem family_at_spec (p : U64) (w : keypad.Weights) (op : Bool)
    (hfit : sumOf (effOf w op) ≤ U64.max) (hpos : 0 < sumOf (effOf w op)) :
    keypad.family_at p w op ⦃ f =>
      let e := effOf w op
      let r := p.val % sumOf e
      (f = .Plain ↔ r < e.plain.val) ∧
      (f = .Decimal ↔ e.plain.val ≤ r ∧ r < e.plain.val + e.decimal.val) ∧
      (f = .Signed ↔ e.plain.val + e.decimal.val ≤ r ∧
        r < e.plain.val + e.decimal.val + e.signed.val) ∧
      (f = .Operation ↔ e.plain.val + e.decimal.val + e.signed.val ≤ r ∧
        r < e.plain.val + e.decimal.val + e.signed.val + e.operation.val) ∧
      (f = .Chain ↔ e.plain.val + e.decimal.val + e.signed.val + e.operation.val ≤ r) ⦄ := by
  unfold keypad.family_at
  rw [admissible_eq]
  simp only [bind_ok]
  apply WP.spec_bind (cycle_spec (effOf w op))
  intro o ho
  cases o with
  | none => simp at ho; omega
  | some t =>
    simp only at ho
    have hr := U64.checked_rem_bv_spec p t
    simp only [lift, bind_ok]
    cases e : U64.checked_rem p t with
    | none => rw [e] at hr; simp at hr; omega
    | some r =>
      rw [e] at hr; obtain ⟨_, hrv, _⟩ := hr
      rw [ho] at hrv
      have hlt : p.val % sumOf (effOf w op) < sumOf (effOf w op) := Nat.mod_lt _ hpos
      have s1 := Proofs.Focus.sat_sub_val r (effOf w op).plain
      have s2 := Proofs.Focus.sat_sub_val (core.num.U64.saturating_sub r (effOf w op).plain)
        (effOf w op).decimal
      have s3 := Proofs.Focus.sat_sub_val (core.num.U64.saturating_sub
        (core.num.U64.saturating_sub r (effOf w op).plain) (effOf w op).decimal) (effOf w op).signed
      simp only [sumOf] at hlt hrv hfit hpos
      dsimp only
      split_ifs <;> simp [WP.spec_ok, sumOf] <;> scalar_tac

/-- `family_at` never fails. -/
theorem family_at_total (p : U64) (w : keypad.Weights) (op : Bool) :
    keypad.family_at p w op ⦃ _ => True ⦄ := by
  unfold keypad.family_at
  rw [admissible_eq]
  simp only [bind_ok]
  apply WP.spec_bind (cycle_spec (effOf w op))
  intro o _
  cases o with
  | none => simp [WP.spec_ok]
  | some t =>
    simp only [lift, bind_ok]
    cases U64.checked_rem p t with
    | none => simp [WP.spec_ok]
    | some r => dsimp only; split_ifs <;> simp [WP.spec_ok]

/-- With operators off, only `Plain` or `Decimal` is served. Total. -/
theorem operators_off_digits_only (p : U64) (w : keypad.Weights) :
    keypad.family_at p w false ⦃ f => f = .Plain ∨ f = .Decimal ⦄ := by
  unfold keypad.family_at
  rw [admissible_eq]
  simp only [bind_ok]
  apply WP.spec_bind (cycle_spec (effOf w false))
  intro o ho
  cases o with
  | none => simp [WP.spec_ok]
  | some t =>
    simp only at ho
    have hr := U64.checked_rem_bv_spec p t
    simp only [lift, bind_ok]
    cases e : U64.checked_rem p t with
    | none => simp [WP.spec_ok]
    | some r =>
      rw [e] at hr; obtain ⟨hne, hrv, _⟩ := hr
      have tpos : 0 < t.val := by
        rcases Nat.eq_zero_or_pos t.val with h | h
        · exfalso; simp_all
        · exact h
      have hlt : r.val < t.val := by rw [hrv]; exact Nat.mod_lt _ tpos
      have s1 := Proofs.Focus.sat_sub_val r (effOf w false).plain
      simp only [effOf, sumOf] at ho s1 ⊢
      simp only [Bool.false_eq_true, if_false] at ho s1 ⊢
      split_ifs <;> simp [WP.spec_ok] <;> scalar_tac

/-- A family whose admissible weight is zero is never served while the mix
has a positive weight. -/
theorem disabled_never_served (p : U64) (w : keypad.Weights) (op : Bool)
    (hfit : sumOf (effOf w op) ≤ U64.max) (hpos : 0 < sumOf (effOf w op)) :
    keypad.family_at p w op ⦃ f => 0 < weightOf (effOf w op) f ⦄ := by
  apply WP.spec_mono (family_at_spec p w op hfit hpos)
  intro f hf
  have hlt : p.val % sumOf (effOf w op) < sumOf (effOf w op) := Nat.mod_lt _ hpos
  simp only [sumOf] at hlt
  obtain ⟨h1, h2, h3, h4, h5⟩ := hf
  simp only [sumOf] at h1 h2 h3 h4 h5 hlt
  cases f <;> simp_all [weightOf] <;> omega

/-- An empty mix serves `Plain`. -/
theorem empty_mix_is_plain (p : U64) (w : keypad.Weights) (op : Bool)
    (h0 : sumOf (effOf w op) = 0) :
    keypad.family_at p w op ⦃ f => f = .Plain ⦄ := by
  unfold keypad.family_at
  rw [admissible_eq]
  simp only [bind_ok]
  apply WP.spec_bind (cycle_spec (effOf w op))
  intro o ho
  cases o with
  | none => simp [WP.spec_ok]
  | some t =>
    simp only at ho
    have hr := U64.checked_rem_bv_spec p t
    simp only [lift, bind_ok]
    cases e : U64.checked_rem p t with
    | none => simp [WP.spec_ok]
    | some r =>
      rw [e] at hr; obtain ⟨_, hrv, _⟩ := hr
      exfalso; simp_all <;> scalar_tac

/-! ## Focus skills -/

/-- The RFC's table: which family can carry which focus skill. -/
def operatedOf : keypad.Family → Bool
  | .Operation => true
  | .Chain => true
  | _ => false

def fractionalOf : keypad.Family → Bool
  | .Plain => false
  | _ => true

def signedOf : keypad.Family → Bool
  | .Signed => true
  | _ => false

def carriesOf (f : keypad.Family) (a b : Nat) (pair : Bool) : Bool :=
  if pair then
    match classOf a, classOf b with
    | .Digit, .Digit => true
    | .Digit, .Separator => fractionalOf f
    | .Separator, .Digit => fractionalOf f
    | .Digit, .Operator => operatedOf f
    | .Operator, .Digit => operatedOf f || (signedOf f && signOf a)
    | _, _ => false
  else
    match classOf a with
    | .Digit => true
    | .Separator => fractionalOf f
    | .Operator => operatedOf f || (signedOf f && signOf a)
    | .Outside => false

theorem carries_eq (f : keypad.Family) (a b : U32) (pair : Bool) :
    keypad.carries f a b pair = ok (carriesOf f a.val b.val pair) := by
  unfold keypad.carries carriesOf
  simp only [class_eq, bind_ok]
  cases f <;> cases pair <;> cases classOf a.val <;> cases classOf b.val <;>
    simp [is_sign_eq, fractionalOf, operatedOf, signedOf]

/-- `carries` is the RFC's table: nothing carries an outside key or an
operator pair, `Plain` carries no separator or operator, `Decimal` no
operator, and `Signed` only a leading `+` or `-`. Total. -/
theorem carries_spec (f : keypad.Family) (a b : U32) (pair : Bool) :
    keypad.carries f a b pair ⦃ r => r = carriesOf f a.val b.val pair ⦄ := by
  rw [carries_eq]; simp [WP.spec_ok]

/-- A numpad focus is admissible exactly when some family the session may
serve carries it: `Decimal` or, with operators on, `Operation`. Never an
outside key; never an operator with operators off. Total. -/
theorem focus_admissible_spec (a b : U32) (pair op : Bool) :
    keypad.focus_admissible a b pair op ⦃ r =>
      (r = true ↔ carriesOf .Decimal a.val b.val pair = true ∨
        (op = true ∧ carriesOf .Operation a.val b.val pair = true)) ∧
      (classOf a.val = .Outside → r = false) ∧
      (op = false → pair = false → classOf a.val = .Operator → r = false) ⦄ := by
  unfold keypad.focus_admissible
  simp only [carries_eq, bind_ok]
  cases op <;> cases pair <;> cases ha : classOf a.val <;> cases hb : classOf b.val <;>
    simp [WP.spec_ok, carriesOf, ha, hb, fractionalOf, operatedOf, signedOf]

end Proofs.Keypad
