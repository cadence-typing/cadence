import Extracted.CadenceKernel
import Proofs.Time
import Proofs.Lanes

/-!
# The review ladder (`cadence_kernel::review`, VK-06, #37)

`cadence_trainer::review::ladder_step` (called by `trainer::learner::due_words`
for every recorded word outcome) and `review::is_due` adapt the trainer's
`ReviewState` and `UtcDay` to day numbers and ask these functions.

Proved: an error puts the word on the first rung, due the next day, with no
streak; the next due day is always after the outcome's day; clean typing
before the due day (or unscheduled) is practice, never a step; the rung only
rises after a clean day strictly later than a counted one; the rung stays on
the ladder; a completion dated before the counted clean day is refused.

Trusted, not proved: that `due_words` passes each session's day in order and
the policy ladder (tested in `crates/trainer/src/review.rs` and
`learner_tests.rs`).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

set_option linter.unusedTactic false
set_option linter.unnecessarySeqFocus false
set_option linter.unusedSimpArgs false
set_option linter.unreachableTactic false

namespace Proofs.Review

theorem first_rung : review.FIRST_RUNG = 0#usize := by
  unfold review.FIRST_RUNG; rfl

/-- A valid ladder: starts at 1 and every rung is at least one day. -/
def ladderOk (l : List I64) : Prop :=
  ∃ h : 0 < l.length, l[0].val = 1 ∧ ∀ i (hi : i < l.length), 1 ≤ l[i].val

theorem valid_ladder_loop_spec (l : Slice I64) (p : I64) (i : Usize)
    (hi : 1 ≤ i.val ∧ i.val ≤ l.val.length) (hp : 1 ≤ p.val)
    (hk : ∀ k (hk : k < l.val.length), k < i.val → 1 ≤ l.val[k].val) :
    review.valid_ladder_loop l p i
      ⦃ r => r = true → ∀ k (hk : k < l.val.length), 1 ≤ l.val[k].val ⦄ := by
  unfold review.valid_ladder_loop
  have hlen : (Slice.len l).val = l.val.length := Slice.len_val l
  apply loop.spec_decr_nat
    (fun st : I64 × Usize => l.val.length - st.2.val)
    (fun st : I64 × Usize => 1 ≤ st.2.val ∧ st.2.val ≤ l.val.length ∧ 1 ≤ st.1.val ∧
      ∀ k (hk : k < l.val.length), k < st.2.val → 1 ≤ l.val[k].val)
  · rintro ⟨q, j⟩ ⟨hj1, hj2, hq, hall⟩
    simp only at hj1 hj2 hq hall
    unfold review.valid_ladder_loop.body
    simp only [hlen]
    split
    · rename_i hlt
      have hlt' : j.val < l.val.length := by scalar_tac
      have hget : l.val[j.val]? = some l.val[j.val] := List.getElem?_eq_getElem hlt'
      simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
        core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, hget, bind_ok]
      split
      · simp [WP.spec_ok]
      · rename_i hgt
        have hs := Proofs.Time.usize_succ j (by scalar_tac)
        simp only [lift, bind_ok, WP.spec_ok, hs]
        refine ⟨⟨by omega, by omega, by scalar_tac, ?_⟩, by omega⟩
        intro k hk hkj
        by_cases hkj' : k = j.val
        · subst hkj'; scalar_tac
        · exact hall k hk (by omega)
    · rename_i hge
      simp only [WP.spec_ok]
      intro _ k hk
      exact hall k hk (by scalar_tac)
  · exact ⟨hi.1, hi.2, hp, hk⟩

@[step]
theorem valid_ladder_spec (l : Slice I64) :
    review.valid_ladder l ⦃ r => r = true → ladderOk l.val ⦄ := by
  unfold review.valid_ladder
  simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
    core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, bind_ok]
  split
  · simp [WP.spec_ok]
  · rename_i cur hcur
    have hc := List.getElem?_eq_some_iff.mp hcur
    obtain ⟨h0, hc⟩ := hc
    have h0' : 0 < l.val.length := by simpa [first_rung] using h0
    have hc' : l.val[0] = cur := by simpa [first_rung] using hc
    split
    · simp [WP.spec_ok]
    · rename_i hne
      have h1 : cur.val = 1 := by simp at hne; scalar_tac
      apply WP.spec_mono (valid_ladder_loop_spec l 1#i64 1#usize (by simp; omega) (by simp)
        (fun k hk hk1 => by
          have : k = 0 := by simp at hk1; omega
          subst this
          rw [hc']
          omega))
      intro r hr hrt
      exact ⟨h0', by rw [hc']; exact h1, hr hrt⟩

@[step]
theorem rolled_back_spec (st : Option review.Streak) (day : I64) :
    review.rolled_back st day ⦃ r => (r = true ↔ ∃ x, st = some x ∧ day.val < x.day.val) ⦄ := by
  unfold review.rolled_back
  cases st <;> simp [WP.spec_ok]

@[step]
theorem promotes_spec (st : Option review.Streak) (day : I64) :
    review.promotes st day ⦃ r => (r = true ↔
      ∃ x, st = some x ∧ x.day.val < day.val ∧ 1 ≤ x.count.val) ⦄ := by
  unfold review.promotes
  cases st with
  | none => simp [WP.spec_ok]
  | some x =>
    simp only []
    split
    · rename_i h
      simp only [WP.spec_ok]
      have : x.day.val < day.val := by scalar_tac
      simp [this]
    · rename_i h
      simp only [WP.spec_ok]
      have : ¬ x.day.val < day.val := by scalar_tac
      simp [this]

@[step]
theorem next_streak_spec (st : Option review.Streak) (day : I64) :
    review.next_streak st day ⦃ _ => True ⦄ := by
  unfold review.next_streak
  cases st with
  | none => simp [WP.spec_ok]
  | some x => simp only []; split <;> simp [WP.spec_ok, lift]

@[step]
theorem next_rung_spec (rung len : Usize) (p : Bool) (h : rung.val < len.val) :
    review.next_rung rung p len ⦃ r => r.val < len.val ∧
      (p = false → r = rung) ∧ (rung.val < r.val → r.val = rung.val + 1) ⦄ := by
  unfold review.next_rung
  have hs := Proofs.Time.usize_succ rung (by scalar_tac)
  cases p with
  | false => simp [WP.spec_ok]; exact h
  | true =>
    simp only [↓reduceIte, lift, bind_ok]
    split
    · rename_i hlt
      have : rung.val + 1 < len.val := by scalar_tac
      simp only [WP.spec_ok, hs]
      refine ⟨this, fun h => (by cases h), fun _ => (by simp)⟩
    · simp only [WP.spec_ok]
      exact ⟨h, fun h => (by cases h), fun h => (by omega)⟩

/-- What one step promises. -/
def stepPost (s : review.State) (error : Bool) (day : I64) (l : List I64) (r : review.Step) : Prop :=
  match r with
  | .Next s' =>
      s'.rung.val < l.length ∧
      (∃ d, s'.due = some d ∧ day.val < d.val) ∧
      (error = true → s'.rung.val = 0 ∧ s'.streak = none ∧ ∃ d, s'.due = some d ∧ d.val = day.val + 1) ∧
      (error = false → ∃ d, s.due = some d ∧ d.val ≤ day.val) ∧
      (s.rung.val < s'.rung.val → error = false ∧ s'.rung.val = s.rung.val + 1 ∧
        ∃ st, s.streak = some st ∧ st.day.val < day.val) ∧
      (∀ st, s.streak = some st → st.day.val ≤ day.val)
  | .Practice => error = false ∧ (s.due = none ∨ ∃ d, s.due = some d ∧ day.val < d.val)
  | .Rollback => ∃ st, s.streak = some st ∧ day.val < st.day.val
  | .Invalid => True
  | .Overflow => True

theorem ladder_step_spec (s : review.State) (error : Bool) (day : I64) (l : Slice I64) :
    review.ladder_step s error day l ⦃ r => stepPost s error day l.val r ⦄ := by
  unfold review.ladder_step
  have hlen : (Slice.len l).val = l.val.length := Slice.len_val l
  apply WP.spec_bind (valid_ladder_spec l)
  intro b hb
  cases b with
  | false => simp [WP.spec_ok, stepPost]
  | true =>
    obtain ⟨h0, hfirst, hpos⟩ := hb rfl
    simp only [↓reduceIte]
    split
    · simp [WP.spec_ok, stepPost]
    · rename_i hrung
      have hrung' : s.rung.val < l.val.length := by scalar_tac
      apply WP.spec_bind (rolled_back_spec s.streak day)
      intro rb hrb
      cases rb with
      | true =>
        simp only [↓reduceIte, WP.spec_ok, stepPost]
        exact hrb.mp rfl
      | false =>
        have hnrb : ∀ x, s.streak = some x → x.day.val ≤ day.val := by
          intro x hx
          by_contra hlt
          have := hrb.mpr ⟨x, hx, by omega⟩
          cases this
        simp only [Bool.false_eq_true, ↓reduceIte]
        cases error with
        | true =>
          simp only [↓reduceIte, core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
            core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, bind_ok]
          have hg : l.val[review.FIRST_RUNG.val]? = some l.val[0] := by
            simpa [first_rung] using List.getElem?_eq_getElem h0
          rw [hg]
          simp only []
          have hc := I64.checked_add_bv_spec day l.val[0]
          cases e : I64.checked_add day l.val[0] with
          | none => simp only [lift, e, bind_ok, WP.spec_ok, stepPost]
          | some dd =>
            rw [show core.num.checked_add_IScalar day l.val[0] = I64.checked_add day l.val[0] from rfl, e] at hc
            simp only [lift, e, bind_ok, WP.spec_ok, stepPost]
            obtain ⟨_, _, hv, _⟩ := hc
            have hl0 := hpos 0 h0
            refine ⟨by simpa using h0, ⟨dd, rfl, by omega⟩, fun _ => ⟨by simp, by simp, dd, rfl, by omega⟩,
              fun h => (by cases h), fun h => (by simp at h), hnrb⟩
        | false =>
          simp only [Bool.false_eq_true, ↓reduceIte]
          cases hdue : s.due with
          | none => simp [WP.spec_ok, stepPost, hdue]
          | some d =>
            simp only []
            split
            · rename_i hlt
              simp only [WP.spec_ok, stepPost]
              exact ⟨by simp, Or.inr ⟨d, hdue, by scalar_tac⟩⟩
            · rename_i hge
              have hge' : d.val ≤ day.val := by scalar_tac
              apply WP.spec_bind (promotes_spec s.streak day)
              intro pr hpr
              apply WP.spec_bind (next_rung_spec s.rung (Slice.len l) pr (by scalar_tac))
              intro rg ⟨hrg1, hrg2, hrg3⟩
              have hrgl : rg.val < l.val.length := by scalar_tac
              simp only [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice,
                core.slice.index.Usize.get, bind_tc_ok, Slice.getElem?_Usize_eq, bind_ok]
              rw [List.getElem?_eq_getElem hrgl]
              simp only []
              have hp := hpos rg.val hrgl
              have hc := I64.checked_add_bv_spec day l.val[rg.val]
              cases e : I64.checked_add day l.val[rg.val] with
              | none => simp only [lift, e, bind_ok, WP.spec_ok, stepPost]
              | some dd =>
                rw [show core.num.checked_add_IScalar day l.val[rg.val] = I64.checked_add day l.val[rg.val] from rfl, e] at hc
                simp only [lift, e, bind_ok]
                obtain ⟨_, _, hv, _⟩ := hc
                have post : ∀ st', stepPost s false day l.val
                    (review.Step.Next { rung := rg, streak := st', due := some dd }) := by
                  intro st'
                  simp only [stepPost]
                  refine ⟨hrgl, ⟨dd, rfl, by omega⟩, fun h => (by cases h), fun _ => ⟨d, hdue, hge'⟩,
                    fun hlt => ⟨by simp, hrg3 hlt, ?_⟩, hnrb⟩
                  cases pr with
                  | false => have := hrg2 rfl; subst this; omega
                  | true =>
                    obtain ⟨x, hx, hxd, _⟩ := hpr.mp rfl
                    exact ⟨x, hx, hxd⟩
                cases pr with
                | true =>
                  simp only [↓reduceIte, WP.spec_ok]
                  exact post none
                | false =>
                  simp only [Bool.false_eq_true, ↓reduceIte]
                  apply WP.spec_bind (next_streak_spec s.streak day)
                  intro ns _
                  simp only [WP.spec_ok]
                  exact post (some ns)

/-- An error puts the word on the first rung, due the next day. -/
theorem error_resets (s : review.State) (day : I64) (l : Slice I64) :
    review.ladder_step s true day l ⦃ r => ∀ s', r = .Next s' →
      s'.rung.val = 0 ∧ s'.streak = none ∧ ∃ d, s'.due = some d ∧ d.val = day.val + 1 ⦄ := by
  apply WP.spec_mono (ladder_step_spec s true day l)
  intro r hr s' he
  subst he
  exact hr.2.2.1 rfl

/-- The next due day is always after the outcome's day. -/
theorem due_is_future (s : review.State) (e : Bool) (day : I64) (l : Slice I64) :
    review.ladder_step s e day l ⦃ r => ∀ s', r = .Next s' →
      ∃ d, s'.due = some d ∧ day.val < d.val ⦄ := by
  apply WP.spec_mono (ladder_step_spec s e day l)
  intro r hr s' he
  subst he
  exact hr.2.1

/-- Clean typing before the due day (or with no due day) is practice: only
a due clean completion moves the word. -/
theorem early_clean_is_practice (s : review.State) (day : I64) (l : Slice I64) :
    review.ladder_step s false day l ⦃ r => ∀ s', r = .Next s' →
      ∃ d, s.due = some d ∧ d.val ≤ day.val ⦄ := by
  apply WP.spec_mono (ladder_step_spec s false day l)
  intro r hr s' he
  subst he
  exact hr.2.2.2.1 rfl

/-- The rung rises by exactly one, and only after a clean completion on a
day strictly later than a counted one. -/
theorem promotion_needs_two_days (s : review.State) (e : Bool) (day : I64) (l : Slice I64) :
    review.ladder_step s e day l ⦃ r => ∀ s', r = .Next s' → s.rung.val < s'.rung.val →
      e = false ∧ s'.rung.val = s.rung.val + 1 ∧
        ∃ st, s.streak = some st ∧ st.day.val < day.val ⦄ := by
  apply WP.spec_mono (ladder_step_spec s e day l)
  intro r hr s' he
  subst he
  exact hr.2.2.2.2.1

/-- The rung stays on the ladder: the top rung does not wrap. -/
theorem rung_bounded (s : review.State) (e : Bool) (day : I64) (l : Slice I64) :
    review.ladder_step s e day l ⦃ r => ∀ s', r = .Next s' → s'.rung.val < l.val.length ⦄ := by
  apply WP.spec_mono (ladder_step_spec s e day l)
  intro r hr s' he
  subst he
  exact hr.1

/-- A completion dated before the counted clean day never moves the word. -/
theorem rollback_refused (s : review.State) (e : Bool) (day : I64) (l : Slice I64) :
    review.ladder_step s e day l ⦃ r => ∀ s', r = .Next s' →
      ∀ st, s.streak = some st → st.day.val ≤ day.val ⦄ := by
  apply WP.spec_mono (ladder_step_spec s e day l)
  intro r hr s' he
  subst he
  exact hr.2.2.2.2.2

/-- Due exactly on or after the due day. Total. -/
theorem is_due_spec (due : Option I64) (today : I64) :
    review.is_due due today ⦃ r => (r = true ↔ ∃ d, due = some d ∧ d.val ≤ today.val) ⦄ := by
  unfold review.is_due
  cases due <;> simp [WP.spec_ok]

end Proofs.Review
