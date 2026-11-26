import Extracted.CadenceKernel

/-!
# Headless request admission (`cadence_kernel::request`, #67)

`cadence-driver`'s `stream::admit` asks `request.admit` for every request
with the newest applied id and what the response cache holds for the id,
`request.revision_ok` for its expected state revision, and `request.evict`
before inserting a response into the bounded cache.

Trusted, not proved: that the driver looks the id up in the cache it keeps
(tested in `cadence-driver`) and applies a request exactly when the answer is
`New` (`models/RequestCache.tla` checks the interleavings, bounded).
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Request

/-- The newest applied id, as a number (none: below every id). -/
def fresh (last : Option U64) (id : U64) : Prop :=
  match last with
  | none => True
  | some l => l.val < id.val

/-- `New` exactly when the id is greater than every applied id. Total. -/
theorem new_iff_greater (last : Option U64) (id : U64) (lookup : request.Lookup) :
    request.admit last id lookup ⦃ r => (r = .New ↔ fresh last id) ⦄ := by
  unfold request.admit fresh
  cases last with
  | none => simp [WP.spec_ok]
  | some l =>
    by_cases h : id > l
    · have : l.val < id.val := by scalar_tac
      simp [h, this, WP.spec_ok]
    · have : ¬ l.val < id.val := by scalar_tac
      cases lookup <;> simp [h, this, WP.spec_ok]

/-- A cached id with the same digest replays. -/
theorem replay_iff_same (last : Option U64) (id : U64) (lookup : request.Lookup) :
    request.admit last id lookup ⦃ r =>
      (r = .Replay ↔ ¬ fresh last id ∧ lookup = .SameDigest) ⦄ := by
  unfold request.admit fresh
  cases last with
  | none => cases lookup <;> simp [WP.spec_ok]
  | some l =>
    by_cases h : id > l
    · have : l.val < id.val := by scalar_tac
      cases lookup <;> simp [h, this, WP.spec_ok]
    · have : ¬ l.val < id.val := by scalar_tac
      cases lookup <;> simp [h, this, WP.spec_ok]

/-- A cached id with another digest is a conflict. -/
theorem conflict_iff_other (last : Option U64) (id : U64) (lookup : request.Lookup) :
    request.admit last id lookup ⦃ r =>
      (r = .Conflict ↔ ¬ fresh last id ∧ lookup = .OtherDigest) ⦄ := by
  unfold request.admit fresh
  cases last with
  | none => cases lookup <;> simp [WP.spec_ok]
  | some l =>
    by_cases h : id > l
    · have : l.val < id.val := by scalar_tac
      cases lookup <;> simp [h, this, WP.spec_ok]
    · have : ¬ l.val < id.val := by scalar_tac
      cases lookup <;> simp [h, this, WP.spec_ok]

/-- Only a new id mutates: an old id never applies again, whatever the
cache holds (a missing entry has expired, never re-executes). -/
theorem only_new_mutates (last : Option U64) (id : U64) (lookup : request.Lookup) :
    request.admit last id lookup ⦃ r => r ≠ .New →
      ¬ fresh last id ∧ (lookup = .Miss → r = .Expired) ⦄ := by
  unfold request.admit fresh
  cases last with
  | none => cases lookup <;> simp [WP.spec_ok]
  | some l =>
    by_cases h : id > l
    · have : l.val < id.val := by scalar_tac
      cases lookup <;> simp [h, this, WP.spec_ok]
    · have : ¬ l.val < id.val := by scalar_tac
      cases lookup <;> simp [h, this, WP.spec_ok]

/-- A stale expected revision is refused: false exactly when one is given
and it differs from the current revision. Total. -/
theorem stale_refused (expected : U64) (present : Bool) (current : U64) :
    request.revision_ok expected present current ⦃ r =>
      (r = false ↔ present = true ∧ expected ≠ current) ⦄ := by
  unfold request.revision_ok
  cases present <;> by_cases h : expected = current <;> simp [h, WP.spec_ok]

/-- Evicting when full keeps a cache of positive capacity bounded: after
inserting one more, the length is at most the capacity. Total. -/
theorem cache_bounded (len cap : Usize) (h : len.val ≤ cap.val) (hc : 0 < cap.val) :
    request.evict len cap ⦃ e =>
      (if e then len.val - 1 + 1 else len.val + 1) ≤ cap.val ⦄ := by
  unfold request.evict
  by_cases hl : len ≥ cap
  · have : cap.val ≤ len.val := by scalar_tac
    simp [hl, WP.spec_ok]; omega
  · have : len.val < cap.val := by scalar_tac
    simp [hl, WP.spec_ok]; omega

end Proofs.Request
