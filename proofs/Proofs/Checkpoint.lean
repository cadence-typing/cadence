import Extracted.CadenceKernel

/-!
# Checkpoint admission (`cadence_kernel::checkpoint`)

`cadence-data`'s `Store::apply_checkpoint` calls `checkpoint.admit` for the
decision "apply, replay, conflict or stale". The theorems say which incoming checkpoints
are credited and that a credited sequence can never be credited again: the
decision-level counterpart of `NoDoubleCredit` in
`formal/DurableCheckpoint.tla`.

Trusted, not proved: that the store reads the receipt table and the latest
sequence correctly before calling, and that the SQL transaction is atomic.
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Checkpoint

/-- The complete decision table of `admit`, for every input. -/
theorem admit_spec (latest : Option U64) (receipt : checkpoint.Receipt) (incoming : U64) :
    checkpoint.admit latest receipt incoming
      ⦃ r => r = match receipt with
        | .Same => checkpoint.Admission.Replay
        | .Different => checkpoint.Admission.Conflict
        | .Absent => match latest with
          | none => checkpoint.Admission.Apply
          | some l => if incoming.val ≤ l.val then checkpoint.Admission.Stale
                      else checkpoint.Admission.Apply ⦄ := by
  unfold checkpoint.admit
  cases receipt <;> cases latest <;> simp [UScalar.le_equiv]
  split <;> simp

/-- `admit` applies exactly when no receipt exists and the incoming sequence
is strictly newer than the latest applied one (or the session is new). -/
theorem admit_apply_iff (latest : Option U64) (receipt : checkpoint.Receipt) (incoming : U64) :
    checkpoint.admit latest receipt incoming
      ⦃ r => (r = checkpoint.Admission.Apply ↔
        (receipt = checkpoint.Receipt.Absent ∧
          ∀ l, latest = some l → l.val < incoming.val)) ⦄ := by
  apply WP.spec_mono (admit_spec latest receipt incoming)
  intro r hr
  subst hr
  cases receipt <;> cases latest <;> simp

/-- A replay, a conflict or a stale request is never applied. -/
theorem refused_never_applies (latest : Option U64) (receipt : checkpoint.Receipt)
    (incoming : U64) (hr : receipt ≠ checkpoint.Receipt.Absent) :
    checkpoint.admit latest receipt incoming
      ⦃ r => r ≠ checkpoint.Admission.Apply ⦄ := by
  apply WP.spec_mono (admit_apply_iff latest receipt incoming)
  intro r h hap
  exact hr (h.mp hap).1

/-- No double credit: once a checkpoint with sequence `incoming` was applied
the store holds `incoming` as its latest sequence (the same transaction writes
`commit_seq`; trusted), and offering the same sequence again, with any receipt
state, is never applied. -/
theorem applied_sequence_never_reapplies (receipt : checkpoint.Receipt) (incoming : U64) :
    checkpoint.admit (some incoming) receipt incoming
      ⦃ r => r ≠ checkpoint.Admission.Apply ⦄ := by
  apply WP.spec_mono (admit_apply_iff (some incoming) receipt incoming)
  intro r hr hap
  have := (hr.mp hap).2 incoming rfl
  omega

/-- Applied sequences of one session are strictly increasing: an applied
checkpoint is newer than the latest one held, so each sequence is credited
at most once. -/
theorem applications_strictly_increase (l incoming : U64) (receipt : checkpoint.Receipt) :
    checkpoint.admit (some l) receipt incoming
      ⦃ r => r = checkpoint.Admission.Apply → l.val < incoming.val ⦄ := by
  apply WP.spec_mono (admit_apply_iff (some l) receipt incoming)
  intro r hr hap
  exact (hr.mp hap).2 l rfl

end Proofs.Checkpoint
