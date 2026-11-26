import Extracted.CadenceKernel

/-!
# Headless barriers (`cadence_kernel::barrier`, #71)

`cadence-driver`'s `barrier` operation asks `barrier.causal` with the
latest admitted event and the counts of its scope's outstanding jobs,
pending dispatches and unconsumed results, and `barrier.durable` with the
acknowledged checkpoint and the captured watermark.

Trusted, not proved: that the driver counts the scope correctly (children
inherit their parent's causal group; tested) and that the engine assigns
groups (`models/HeadlessBarrier.tla` checks the interleavings, bounded).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Barrier

/-- No early return: `Done` exactly when the scope is admitted and has no
outstanding job, pending dispatch or unconsumed result. Total. -/
theorem no_early_return (through latest outstanding pending unconsumed : U64) :
    barrier.causal through latest outstanding pending unconsumed ⦃ r =>
      (r = .Done ↔ through.val ≤ latest.val ∧ outstanding.val = 0 ∧
        pending.val = 0 ∧ unconsumed.val = 0) ⦄ := by
  unfold barrier.causal
  by_cases h : through > latest
  · have : ¬ through.val ≤ latest.val := by scalar_tac
    simp [h, this, WP.spec_ok]
  · have : through.val ≤ latest.val := by scalar_tac
    simp only [h, ite_false]
    by_cases ho : outstanding = 0#u64 <;> by_cases hp : pending = 0#u64 <;>
      by_cases hu : unconsumed = 0#u64 <;> simp_all [WP.spec_ok]

/-- A barrier through an event not yet admitted is out of scope. -/
theorem future_refused (through latest outstanding pending unconsumed : U64) :
    barrier.causal through latest outstanding pending unconsumed ⦃ r =>
      (r = .OutOfScope ↔ latest.val < through.val) ⦄ := by
  unfold barrier.causal
  by_cases h : through > latest
  · have : latest.val < through.val := by scalar_tac
    simp [h, this, WP.spec_ok]
  · have : ¬ latest.val < through.val := by scalar_tac
    simp only [h, ite_false]
    by_cases ho : outstanding = 0#u64 <;> by_cases hp : pending = 0#u64 <;>
      by_cases hu : unconsumed = 0#u64 <;> simp_all [WP.spec_ok]

/-- A durable barrier is done only when its causal scope is done and an
acknowledged checkpoint covers the watermark. Total. -/
theorem durable_covers_watermark (c : barrier.Barrier) (acked : U64) (has_ack : Bool)
    (watermark : U64) :
    barrier.durable c acked has_ack watermark ⦃ r => r = .Done →
      c = .Done ∧ has_ack = true ∧ watermark.val ≤ acked.val ⦄ := by
  unfold barrier.durable
  cases c with
  | OutOfScope => simp [WP.spec_ok]
  | Waiting => simp [WP.spec_ok]
  | Done =>
    simp only
    cases has_ack with
    | false => simp [WP.spec_ok]
    | true =>
      by_cases h : acked >= watermark
      · have : watermark.val ≤ acked.val := by scalar_tac
        simp [h, this, WP.spec_ok]
      · simp [h, WP.spec_ok]

end Proofs.Barrier
