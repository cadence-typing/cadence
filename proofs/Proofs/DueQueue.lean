import Extracted.CadenceKernel
import Proofs.Time

/-!
# The due-review FIFO (`cadence_kernel::review::fifo_*`, #90)

`cadence_trainer::review::DueQueue` owns the ids and their due days and
does what these decisions say: append at the tail, refuse a duplicate or a
full queue, serve the head. Live ids are `ids[head..len]`; an item's rank
(how many are served before it) is its index minus `head`.

Trusted, not proved: that the trainer appends the id at index `len` when
told `Appended`, reads the item at `index` when told `Served`, and never
writes elsewhere (tested in `review.rs`). The liveness half of fairness
(that services keep happening) is `model-proved` by `FeedbackLoop.tla`'s
`DueEventuallyServed` under weak fairness, not proved here.
-/

open Aeneas Aeneas.Std Result
open cadence_kernel

namespace Proofs.Queue

/-- `id` is live in `ids[lo..hi]`. -/
def liveIn (ids : List U64) (lo hi : Nat) (id : U64) : Prop :=
  ∃ k, lo ≤ k ∧ k < hi ∧ ids[k]? = some id

theorem slice_get (ids : Slice U64) (j : Usize) :
    core.slice.Slice.get (core.slice.index.SliceIndexUsizeSlice U64) ids j = ok ids.val[j.val]? := by
  simp [core.slice.Slice.get, core.slice.index.SliceIndexUsizeSlice, core.slice.index.Usize.get,
    Slice.getElem?_Usize_eq]

theorem fifo_contains_loop_spec (ids : Slice U64) (len : Usize) (id : U64) (i : Usize) :
    review.fifo_contains_loop ids len id i ⦃ r => (r = true ↔ liveIn ids.val i.val len.val id) ⦄ := by
  unfold review.fifo_contains_loop
  apply loop.spec_decr_nat
    (fun j : Usize => len.val - j.val)
    (fun j : Usize => i.val ≤ j.val ∧ ∀ k, i.val ≤ k → k < j.val → ids.val[k]? ≠ some id)
  · rintro j ⟨hij, hnone⟩
    unfold review.fifo_contains_loop.body
    split
    · rename_i hlt
      have hlt' : j.val < len.val := by scalar_tac
      have hs1 := Proofs.Time.usize_succ j (by scalar_tac)
      rw [slice_get]
      simp only [bind_ok]
      cases hget : ids.val[j.val]? with
      | none =>
        simp only [lift, bind_ok, WP.spec_ok, hs1]
        refine ⟨⟨by omega, ?_⟩, by omega⟩
        intro k hk hk2
        by_cases hkj : k = j.val
        · subst hkj; simp [hget]
        · exact hnone k hk (by omega)
      | some v =>
        simp only
        by_cases hv : v = id
        · simp only [hv, if_true, WP.spec_ok, true_iff]
          exact ⟨j.val, hij, hlt', hv ▸ hget⟩
        · simp only [hv, if_false, lift, bind_ok, WP.spec_ok, hs1]
          refine ⟨⟨by omega, ?_⟩, by omega⟩
          intro k hk hk2
          by_cases hkj : k = j.val
          · subst hkj; simp [hget, hv]
          · exact hnone k hk (by omega)
    · rename_i hge
      have hge' : len.val ≤ j.val := by scalar_tac
      simp only [WP.spec_ok, Bool.false_eq_true, false_iff]
      rintro ⟨k, hk, hk2, hkv⟩
      exact hnone k hk (by omega) hkv
  · exact ⟨le_refl _, fun k hk hk2 => absurd hk2 (by omega)⟩

/-- The scan is exact: `true` exactly when `id` is live. Total. -/
theorem fifo_contains_spec (ids : Slice U64) (q : review.Fifo) (id : U64) :
    review.fifo_contains ids q id ⦃ r => (r = true ↔ liveIn ids.val q.head.val q.len.val id) ⦄ := by
  unfold review.fifo_contains
  exact fifo_contains_loop_spec ids q.len id q.head

/-- Admission: a live id is refused as a duplicate; otherwise, below the
capacity, the id is appended at the tail and the head stays; at capacity
nothing changes. Total. -/
theorem fifo_admit_spec (ids : Slice U64) (q : review.Fifo) (id : U64) (cap : Usize) :
    review.fifo_admit ids q id cap ⦃ r =>
      (r = review.FifoStep.Duplicate ↔ liveIn ids.val q.head.val q.len.val id) ∧
      ∀ next, r = review.FifoStep.Appended next →
        ¬ liveIn ids.val q.head.val q.len.val id ∧ q.len.val < cap.val ∧
        next.head = q.head ∧ next.len.val = q.len.val + 1 ⦄ := by
  unfold review.fifo_admit
  apply WP.spec_bind (fifo_contains_spec ids q id)
  intro b hb
  by_cases hbt : b = true
  · simp only [hbt, if_true, WP.spec_ok, true_iff, reduceCtorEq, false_implies, implies_true,
      and_true]
    exact hb.mp hbt
  · have hlive : ¬ liveIn ids.val q.head.val q.len.val id := fun h => hbt (hb.mpr h)
    have hbf : b = false := by cases b <;> simp_all
    simp only [hbf, Bool.false_eq_true, if_false]
    split
    · simp only [WP.spec_ok, reduceCtorEq, false_iff, hlive, not_false_eq_true, false_implies,
        implies_true, and_self]
    · rename_i hlt
      have hlt' : q.len.val < cap.val := by scalar_tac
      simp only [lift, bind_ok]
      cases ho : Usize.checked_add q.len 1#usize with
      | none =>
        simp only [WP.spec_ok, reduceCtorEq, false_iff, hlive, not_false_eq_true, false_implies,
          implies_true, and_self]
      | some len =>
        have := Usize.checked_add_bv_spec q.len 1#usize
        rw [ho] at this
        simp only [WP.spec_ok, reduceCtorEq, false_iff, hlive, not_false_eq_true,
          review.FifoStep.Appended.injEq, true_and]
        intro next hnext
        subst hnext
        exact ⟨hlt', rfl, by simp [this.2.1]⟩

/-- Serving: a non-empty queue serves its head and advances it by one; the
live items stay where they are. Total. -/
theorem fifo_serve_spec (q : review.Fifo) :
    review.fifo_serve q ⦃ r =>
      (q.head.val < q.len.val → ∃ next, r = review.FifoStep.Served next q.head ∧
        next.head.val = q.head.val + 1 ∧ next.len = q.len) ∧
      (q.len.val ≤ q.head.val → r = review.FifoStep.Empty) ⦄ := by
  unfold review.fifo_serve
  split
  · rename_i hlt
    have hlt' : q.head.val < q.len.val := by scalar_tac
    simp only [lift, bind_ok]
    cases ho : Usize.checked_add q.head 1#usize with
    | none =>
      have := Usize.checked_add_bv_spec q.head 1#usize
      rw [ho] at this
      exfalso; scalar_tac
    | some head =>
      have := Usize.checked_add_bv_spec q.head 1#usize
      rw [ho] at this
      simp only [WP.spec_ok]
      refine ⟨fun _ => ⟨{ q with head }, by simp, by simp [this.2.1], by simp⟩,
        fun h => absurd h (by omega)⟩
  · rename_i hge
    have hge' : q.len.val ≤ q.head.val := by scalar_tac
    simp only [WP.spec_ok]
    exact ⟨fun h => absurd h (by omega), fun _ => trivial⟩

/-- Live members stay distinct: an admitted id was not live, so the live
list after appending it has no duplicate if it had none before. -/
theorem unique_members (ids : Slice U64) (q : review.Fifo) (id : U64) (cap : Usize)
    (live : List U64) (hlive : ∀ x, x ∈ live ↔ liveIn ids.val q.head.val q.len.val x)
    (hnodup : live.Nodup) :
    review.fifo_admit ids q id cap ⦃ r => ∀ next, r = review.FifoStep.Appended next →
      (live ++ [id]).Nodup ⦄ := by
  apply WP.spec_mono (fifo_admit_spec ids q id cap)
  intro r ⟨_, happ⟩ next hnext
  have ⟨hnot, _, _, _⟩ := happ next hnext
  rw [List.nodup_append]
  refine ⟨hnodup, List.nodup_singleton id, ?_⟩
  intro a ha b hb hab
  rw [List.mem_singleton] at hb
  rw [hb] at hab
  exact hnot ((hlive _).mp (hab ▸ ha))

/-- Serving the head lowers every other live item's rank by exactly one, and
a tail admission leaves every live item's rank as it was: an index `k`
keeps its item, and its rank `k - head` falls by one on a service and is
unchanged by an admission. -/
theorem rank_decreases (q : review.Fifo) (hne : q.head.val < q.len.val) :
    review.fifo_serve q ⦃ r => ∃ next, r = review.FifoStep.Served next q.head ∧
      ∀ k, q.head.val < k → k - next.head.val + 1 = k - q.head.val ⦄ := by
  apply WP.spec_mono (fifo_serve_spec q)
  intro r ⟨hserve, _⟩
  obtain ⟨next, hr, hh, _⟩ := hserve hne
  exact ⟨next, hr, fun k hk => by omega⟩

theorem admission_keeps_rank (ids : Slice U64) (q : review.Fifo) (id : U64) (cap : Usize) :
    review.fifo_admit ids q id cap ⦃ r => ∀ next, r = review.FifoStep.Appended next →
      next.head = q.head ⦄ := by
  apply WP.spec_mono (fifo_admit_spec ids q id cap)
  intro r ⟨_, happ⟩ next hnext
  exact (happ next hnext).2.2.1

/-- `n` services in a row with no admission ahead, the result of the last. -/
def serveTimes : Nat → review.Fifo → Result review.FifoStep
  | 0, q => review.fifo_serve q
  | n + 1, q => do
    let r ← review.fifo_serve q
    match r with
    | review.FifoStep.Served next _ => serveTimes n next
    | _ => ok r

/-- An item with `rank` predecessors is served by the `(rank + 1)`-th
service, given no insertion ahead of it (admissions only append) and that
services happen: that service serves exactly the index `head + rank`. -/
theorem served_within_rank_plus_one (rank : Nat) (q : review.Fifo)
    (h : q.head.val + rank < q.len.val) :
    serveTimes rank q ⦃ r => ∃ next index, r = review.FifoStep.Served next index ∧
      index.val = q.head.val + rank ⦄ := by
  induction rank generalizing q with
  | zero =>
    unfold serveTimes
    apply WP.spec_mono (fifo_serve_spec q)
    intro r ⟨hserve, _⟩
    obtain ⟨next, hr, _, _⟩ := hserve (by omega)
    exact ⟨next, q.head, hr, by simp⟩
  | succ n ih =>
    unfold serveTimes
    apply WP.spec_bind (fifo_serve_spec q)
    intro r ⟨hserve, _⟩
    obtain ⟨next, hr, hh, hl⟩ := hserve (by omega)
    subst hr
    have h' : next.head.val + n < next.len.val := by rw [hl]; omega
    apply WP.spec_mono (ih next h')
    intro r' ⟨next', index, hr', hidx⟩
    exact ⟨next', index, hr', by omega⟩

end Proofs.Queue
