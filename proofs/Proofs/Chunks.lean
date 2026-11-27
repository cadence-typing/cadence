import Extracted.CadenceKernel

/-!
# Observation-chunk admission (`cadence_kernel::chunks`, #11)

`cadence_data::store::ProgressStore::apply_checkpoint` asks
`chunks.admit` for every incoming chunk of a checkpoint, in sequence order,
and refuses the whole transaction on a `Conflict` or a `Gap`; store open and
`cadence doctor` ask `chunks.watermark_ok` for the derived totals' source
watermark.

Trusted, not proved: the canonical chunk encoding and digest (so
`same_digest`), the SQL, the triggers that abort `UPDATE` and `DELETE`, the
fold that derives the totals, and the migration (tested).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Chunks

/-- The successor test, independent of the code. -/
def isNext (storedLast : Nat) (hasStored : Bool) (incoming : Nat) : Prop :=
  (hasStored = true ∧ incoming = storedLast + 1) ∨ (hasStored = false ∧ incoming = 1)

/-- The decision, exactly. Total. -/
theorem admit_spec (s : U64) (h : Bool) (i : U64) (same : Bool) :
    chunks.admit s h i same ⦃ r =>
      (h = false → (r = chunks.Admit.Append ↔ i.val = 1) ∧
                   (r ≠ chunks.Admit.Append → r = chunks.Admit.Gap)) ∧
      (h = true → i.val ≤ s.val → r = (if same then chunks.Admit.Replay else chunks.Admit.Conflict)) ∧
      (h = true → s.val < i.val → (r = chunks.Admit.Append ↔ i.val = s.val + 1) ∧
                                    (r ≠ chunks.Admit.Append → r = chunks.Admit.Gap)) ⦄ := by
  unfold chunks.admit
  cases h
  · simp only [Bool.false_eq_true, if_false]
    by_cases hi : i = 1#u64
    · have : i.val = 1 := by simp [hi]
      simp [hi, this, WP.spec_ok]
    · have : i.val ≠ 1 := fun hv => hi (by apply UScalar.eq_of_val_eq; simpa using hv)
      simp [hi, this, WP.spec_ok]
  · simp only [if_true]
    by_cases hle : i ≤ s
    · have hle' : i.val ≤ s.val := by scalar_tac
      cases same <;> simp [hle, hle', WP.spec_ok]
    · have hlt : s.val < i.val := by scalar_tac
      simp only [hle, if_false, lift, bind_ok]
      cases ho : U64.checked_add s 1#u64 with
      | none =>
        have := U64.checked_add_bv_spec s 1#u64
        rw [ho] at this
        have hmax : s.val + 1 > U64.max := by simpa using this
        have hne : i.val ≠ s.val + 1 := by scalar_tac
        simp [WP.spec_ok, hne, hlt, Nat.not_le_of_lt hlt]
      | some next =>
        have := U64.checked_add_bv_spec s 1#u64
        rw [ho] at this
        have hn : next.val = s.val + 1 := this.2.1
        by_cases he : i = next
        · have : i.val = s.val + 1 := by rw [he, hn]
          simp [he, WP.spec_ok, this, hlt, Nat.not_le_of_lt hlt, hn]
        · have : i.val ≠ s.val + 1 := fun hv => he (by apply UScalar.eq_of_val_eq; rw [hv, hn])
          simp [he, WP.spec_ok, this, hlt, Nat.not_le_of_lt hlt]

/-- `Append` exactly for the successor of the stored last chunk (or the
first chunk of an empty session). -/
theorem append_iff_next (s : U64) (h : Bool) (i : U64) (same : Bool) :
    chunks.admit s h i same ⦃ r => (r = chunks.Admit.Append ↔ isNext s.val h i.val) ⦄ := by
  apply WP.spec_mono (admit_spec s h i same)
  intro r ⟨hf, hle, hgt⟩
  cases h
  · simp [isNext, (hf rfl).1]
  · by_cases hi : i.val ≤ s.val
    · rw [hle rfl hi]
      have : i.val ≠ s.val + 1 := by omega
      cases same <;> simp [isNext, this]
    · have hlt : s.val < i.val := by omega
      simp [isNext, (hgt rfl hlt).1]

/-- An already stored sequence with the same payload is a replay, which
changes nothing. -/
theorem replay_iff_same (s : U64) (h : Bool) (i : U64) (same : Bool) :
    chunks.admit s h i same ⦃ r =>
      (r = chunks.Admit.Replay ↔ h = true ∧ i.val ≤ s.val ∧ same = true) ⦄ := by
  apply WP.spec_mono (admit_spec s h i same)
  intro r ⟨hf, hle, hgt⟩
  cases h
  · have hg := (hf rfl).2
    by_cases ha : r = chunks.Admit.Append
    · simp [ha]
    · simp [hg ha]
  · by_cases hi : i.val ≤ s.val
    · rw [hle rfl hi]; cases same <;> simp [hi]
    · have hlt : s.val < i.val := by omega
      have hg := (hgt rfl hlt).2
      by_cases ha : r = chunks.Admit.Append
      · simp [ha, hi]
      · simp [hg ha, hi]

/-- A changed payload under a stored sequence is a conflict (VP-03). -/
theorem conflict_on_changed_payload (s : U64) (i : U64) (h : i.val ≤ s.val) :
    chunks.admit s true i false ⦃ r => r = chunks.Admit.Conflict ⦄ := by
  apply WP.spec_mono (admit_spec s true i false)
  intro r ⟨_, hle, _⟩
  simpa using hle rfl h

/-- Anything that is neither stored nor the successor is a gap. -/
theorem gap_refused (s : U64) (h : Bool) (i : U64) (same : Bool)
    (hgap : (h = true ∧ s.val + 1 < i.val) ∨ (h = false ∧ i.val ≠ 1)) :
    chunks.admit s h i same ⦃ r => r = chunks.Admit.Gap ⦄ := by
  apply WP.spec_mono (admit_spec s h i same)
  intro r ⟨hf, _, hgt⟩
  rcases hgap with ⟨ht, hlt⟩ | ⟨hfs, hne⟩
  · subst ht
    have h2 := hgt rfl (by omega)
    exact h2.2 (fun ha => by have := h2.1.mp ha; omega)
  · subst hfs
    exact (hf rfl).2 (fun ha => hne ((hf rfl).1.mp ha))

/-- The derived totals are never ahead of the stored chunks. Total. -/
theorem derived_never_ahead (d s : U64) :
    chunks.watermark_ok d s ⦃ r => (r = true ↔ d.val ≤ s.val) ⦄ := by
  unfold chunks.watermark_ok
  by_cases hle : d ≤ s
  · have : d.val ≤ s.val := by scalar_tac
    simp [hle, this, WP.spec_ok]
  · have : ¬ d.val ≤ s.val := by scalar_tac
    simp [hle, this, WP.spec_ok]

end Proofs.Chunks
