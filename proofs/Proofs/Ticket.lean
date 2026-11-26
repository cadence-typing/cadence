import Extracted.CadenceKernel

/-!
# Worker result admission (`cadence_kernel::ticket`, #75)

`cadence-engine`'s job table asks `ticket.admit` for every non-save worker
result with the result's ticket, the current epoch of its job kind and the
pending request. Success and failure results go through the same call.

Trusted, not proved: that the engine raises the epoch on restart and clears
the pending request after an accept (tested; `models/WorkerResults.tla`
checks the interleavings, bounded).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Ticket

/-- `Accept` exactly when the epochs are equal, a request is pending and its
id is the result's. Total. -/
theorem accept_iff_fresh (re rr ce pr : U64) (has : Bool) :
    ticket.admit re rr ce pr has ⦃ r =>
      (r = .Accept ↔ re = ce ∧ has = true ∧ pr = rr) ⦄ := by
  unfold ticket.admit
  by_cases he : re = ce <;> cases has <;> by_cases hp : pr = rr <;> simp_all [WP.spec_ok]

/-- At most once: once the pending request is cleared after an accept, the
same result (or any other) is never accepted again. -/
theorem at_most_once (re rr ce pr : U64) :
    ticket.admit re rr ce pr false ⦃ r => r ≠ .Accept ⦄ := by
  unfold ticket.admit
  by_cases he : re = ce <;> simp_all [WP.spec_ok]

/-- A result from an earlier epoch, failure or success, never replaces
anything: it is `StaleEpoch` whatever is pending. -/
theorem stale_error_never_replaces (re rr ce pr : U64) (has : Bool) (h : re ≠ ce) :
    ticket.admit re rr ce pr has ⦃ r => r = .StaleEpoch ⦄ := by
  unfold ticket.admit
  simp [h, WP.spec_ok]

end Proofs.Ticket
