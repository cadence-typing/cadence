import Extracted.CadenceKernel
import Proofs.TautenMachine
import Proofs.Request
import Proofs.Ticket
import Proofs.Barrier

/-!
# Machine-level invariants for the protocol decisions (#91)

`request.admit`, `ticket.admit` and `barrier.causal` are decisions their
drivers ask once per request, result or barrier. Each is placed here in the
smallest machine that applies it as its driver does, against a world that
may deliver any event, and an invariant is proved for every reachable state
by `Tauten.Machine.rinv_of_step`.

Trusted, not proved, as in each decision's own file: that the driver keeps
the state these machines keep (the newest applied id; the epoch, pending
request and completions; the scope's counts) and acts on the answer as
these machines do.
-/

open Aeneas Aeneas.Std Result
open cadence_kernel
open Tauten.Machine

namespace Proofs.Request

/-- The driver's request state: the newest applied id and every id it
applied, in order. -/
structure Applied where
  last : Option U64
  applied : List U64

/-- A request (its id and what the response cache holds for it) is applied
exactly when `request.admit` answers `New`. -/
def appliedMachine : RMachine Applied (U64 × request.Lookup) Unit := ⟨fun x ev =>
  do
    let r ← request.admit x.last ev.1 ev.2
    match r with
    | .New => ok ({ last := some ev.1, applied := x.applied ++ [ev.1] }, ())
    | .Replay => ok (x, ())
    | .Conflict => ok (x, ())
    | .Expired => ok (x, ())⟩

def appliedInit (x : Applied) : Prop := x = { last := none, applied := [] }

/-- Applied ids increase strictly, and none is above the newest. -/
def AppliedInv (x : Applied) : Prop :=
  x.applied.Pairwise (fun a b => a.val < b.val) ∧
  ∀ a ∈ x.applied, ∃ l, x.last = some l ∧ a.val ≤ l.val

/-- Applied once: in every reachable state no request id was applied
twice, whatever ids and cache answers the world delivers. -/
theorem applied_once (x : Applied) (h : RReachable appliedMachine appliedInit x) :
    x.applied.Nodup := by
  have inv : AppliedInv x := by
    refine rinv_of_step appliedMachine appliedInit AppliedInv ?_ ?_ x h
    · intro x hx; subst hx; simp [AppliedInv]
    · intro ⟨last, applied⟩ ⟨id, lookup⟩ ⟨hpw, hle⟩
      simp only at hpw hle
      simp only [appliedMachine]
      apply WP.spec_bind (new_iff_greater last id lookup)
      intro r hr
      cases r with
      | New =>
        have hf : fresh last id := hr.mp rfl
        simp only [WP.spec_ok, AppliedInv]
        refine ⟨?_, ?_⟩
        · rw [List.pairwise_append]
          refine ⟨hpw, List.pairwise_singleton _ _, ?_⟩
          intro a ha b hb
          rw [List.mem_singleton] at hb
          subst hb
          obtain ⟨l, hl, hal⟩ := hle a ha
          subst hl
          simp only [fresh] at hf
          omega
        · intro a ha
          refine ⟨id, rfl, ?_⟩
          rcases List.mem_append.mp ha with ha | ha
          · obtain ⟨l, hl, hal⟩ := hle a ha
            subst hl
            simp only [fresh] at hf
            omega
          · rw [List.mem_singleton] at ha; subst ha; exact le_refl _
      | Replay | Conflict | Expired =>
        simp only [WP.spec_ok, AppliedInv]
        exact ⟨hpw, hle⟩
  rw [List.nodup_iff_pairwise_ne]
  exact inv.1.imp (fun hab heq => by subst heq; omega)

end Proofs.Request

namespace Proofs.Ticket

/-- A job kind's state as the engine's table keeps it: the epoch, the
newest request id handed out, the pending request, and the completions it
accepted (epoch, request), in order. -/
structure Table where
  epoch : U64
  lastRequest : U64
  pending : Option U64
  completed : List (U64 × U64)

/-- What the world does: hand out a request, restart (a later epoch, the
pending request dropped), or deliver a result for any (epoch, request). -/
inductive TableEvent where
  | submit (request : U64)
  | restart (epoch : U64)
  | result (epoch request : U64)

/-- Requests are handed out in increasing order and epochs only rise (the
engine's job table); a result is recorded exactly when `ticket.admit`
accepts it, and then the pending request clears. -/
def tableMachine : RMachine Table TableEvent Unit := ⟨fun x ev =>
  match ev with
  | .submit r =>
    if x.lastRequest.val < r.val then ok ({ x with lastRequest := r, pending := some r }, ())
    else ok (x, ())
  | .restart e =>
    if x.epoch.val < e.val then ok ({ x with epoch := e, pending := none }, ())
    else ok (x, ())
  | .result e r =>
    do
      let a ← ticket.admit e r x.epoch (x.pending.getD 0#u64) x.pending.isSome
      match a with
      | .Accept => ok ({ x with pending := none, completed := x.completed ++ [(e, r)] }, ())
      | .StaleEpoch => ok (x, ())
      | .NotPending => ok (x, ())⟩

def tableInit (x : Table) : Prop :=
  x = { epoch := 0#u64, lastRequest := 0#u64, pending := none, completed := [] }

/-- Completions are of handed-out requests, in strictly increasing request
order, none at a later epoch than the current one; the pending request is
the newest handed out and has not completed. -/
def TableInv (x : Table) : Prop :=
  x.completed.Pairwise (fun a b => a.2.val < b.2.val) ∧
  (∀ c ∈ x.completed, c.1.val ≤ x.epoch.val ∧ c.2.val ≤ x.lastRequest.val) ∧
  (∀ p, x.pending = some p → p = x.lastRequest ∧ ∀ c ∈ x.completed, c.2.val < p.val)

theorem table_step (x : Table) (ev : TableEvent) (h : TableInv x) :
    tableMachine.step x ev ⦃ r => TableInv r.1 ∧ x.completed <+: r.1.completed ∧
      ∀ c, r.1.completed = x.completed ++ [c] →
        c.1 = x.epoch ∧ x.pending = some c.2 ⦄ := by
  obtain ⟨epoch, last, pending, completed⟩ := x
  obtain ⟨hpw, hle, hp⟩ := h
  simp only at hpw hle hp
  cases ev with
  | submit r =>
    simp only [tableMachine]
    split
    · rename_i hlt
      simp only [WP.spec_ok, TableInv]
      refine ⟨⟨hpw, fun c hc => ⟨(hle c hc).1, by have := (hle c hc).2; omega⟩, ?_⟩,
        List.prefix_refl _, fun c hc => by simp at hc⟩
      intro p hpr
      simp only [Option.some.injEq] at hpr
      subst hpr
      exact ⟨rfl, fun c hc => by have := (hle c hc).2; omega⟩
    · simp only [WP.spec_ok, TableInv]
      exact ⟨⟨hpw, hle, hp⟩, List.prefix_refl _, fun c hc => by simp at hc⟩
  | restart e =>
    simp only [tableMachine]
    split
    · rename_i hlt
      simp only [WP.spec_ok, TableInv]
      refine ⟨⟨hpw, fun c hc => ⟨by have := (hle c hc).1; omega, (hle c hc).2⟩,
        fun p hpr => by simp at hpr⟩, List.prefix_refl _, fun c hc => by simp at hc⟩
    · simp only [WP.spec_ok, TableInv]
      exact ⟨⟨hpw, hle, hp⟩, List.prefix_refl _, fun c hc => by simp at hc⟩
  | result e r =>
    simp only [tableMachine]
    apply WP.spec_bind (accept_iff_fresh e r epoch (pending.getD 0#u64) pending.isSome)
    intro a ha
    cases a with
    | Accept =>
      obtain ⟨he, hsome, hpr⟩ := ha.mp rfl
      subst he
      cases pending with
      | none => simp at hsome
      | some p =>
        simp only [Option.getD_some] at hpr
        subst hpr
        obtain ⟨hpl, hlt⟩ := hp p rfl
        subst hpl
        simp only [WP.spec_ok, TableInv, List.prefix_append, true_and, reduceCtorEq,
          false_implies, implies_true, and_true]
        refine ⟨⟨?_, ?_⟩, ?_⟩
        · rw [List.pairwise_append]
          exact ⟨hpw, List.pairwise_singleton _ _, fun a ha b hb => by
            rw [List.mem_singleton] at hb; subst hb; exact hlt a ha⟩
        · intro c hc
          rcases List.mem_append.mp hc with hc | hc
          · exact hle c hc
          · rw [List.mem_singleton] at hc; subst hc; exact ⟨le_refl _, le_refl _⟩
        · intro c hc
          have := List.append_cancel_left hc
          simp only [List.cons.injEq, and_true] at this
          subst this
          exact ⟨rfl, rfl⟩
    | StaleEpoch | NotPending =>
      simp only [WP.spec_ok, TableInv]
      exact ⟨⟨hpw, hle, hp⟩, List.prefix_refl _, fun c hc => by simp at hc⟩

theorem table_inv (x : Table) (h : RReachable tableMachine tableInit x) : TableInv x :=
  rinv_of_step tableMachine tableInit TableInv
    (fun x hx => by subst hx; simp [TableInv])
    (fun x ev hx => WP.spec_mono (table_step x ev hx) (fun _ hr => hr.1)) x h

/-- Accepted fresh: from every reachable state, a result is recorded only
for the current epoch and the pending request. -/
theorem accepted_fresh (x : Table) (h : RReachable tableMachine tableInit x) (ev : TableEvent) :
    tableMachine.step x ev ⦃ r => ∀ c, r.1.completed = x.completed ++ [c] →
      c.1 = x.epoch ∧ x.pending = some c.2 ⦄ :=
  WP.spec_mono (table_step x ev (table_inv x h)) (fun _ hr => hr.2.2)

/-- Completion retained: no step drops or rewrites a recorded completion,
and in every reachable state no request completed twice. -/
theorem completion_retained (x : Table) (h : RReachable tableMachine tableInit x)
    (ev : TableEvent) :
    (x.completed.map Prod.snd).Nodup ∧
    tableMachine.step x ev ⦃ r => x.completed <+: r.1.completed ⦄ := by
  refine ⟨?_, WP.spec_mono (table_step x ev (table_inv x h)) (fun _ hr => hr.2.1)⟩
  have hpw := (table_inv x h).1
  rw [List.nodup_iff_pairwise_ne, List.pairwise_map]
  exact hpw.imp (fun hab heq => by rw [heq] at hab; omega)

end Proofs.Ticket

namespace Proofs.Barrier

/-- A scope's counts as the driver observes them, and the counts every
`Done` answer was given with. -/
structure Scope where
  latest : U64
  outstanding : U64
  pending : U64
  unconsumed : U64
  answers : List (U64 × U64 × U64)

/-- The world changes the counts in any way, or asks a barrier. -/
inductive ScopeEvent where
  | observe (latest outstanding pending unconsumed : U64)
  | ask (through : U64)

def scopeMachine : RMachine Scope ScopeEvent Unit := ⟨fun x ev =>
  match ev with
  | .observe l o p u => ok ({ x with latest := l, outstanding := o, pending := p, unconsumed := u }, ())
  | .ask t =>
    do
      let b ← barrier.causal t x.latest x.outstanding x.pending x.unconsumed
      match b with
      | .Done =>
        ok ({ x with answers := x.answers ++ [(x.outstanding, x.pending, x.unconsumed)] }, ())
      | .Waiting => ok (x, ())
      | .OutOfScope => ok (x, ())⟩

def scopeInit (x : Scope) : Prop :=
  x = { latest := 0#u64, outstanding := 0#u64, pending := 0#u64, unconsumed := 0#u64, answers := [] }

def Drained (a : U64 × U64 × U64) : Prop := a.1.val = 0 ∧ a.2.1.val = 0 ∧ a.2.2.val = 0

/-- Done is drained: in every reachable state, every `Done` a barrier
returned was answered with no outstanding job, pending dispatch or
unconsumed result in its scope, however the counts moved. -/
theorem done_is_drained (x : Scope) (h : RReachable scopeMachine scopeInit x) :
    ∀ a ∈ x.answers, Drained a := by
  refine rinv_of_step scopeMachine scopeInit (fun x => ∀ a ∈ x.answers, Drained a) ?_ ?_ x h
  · intro x hx; subst hx; simp
  · intro ⟨l, o, p, u, answers⟩ ev hinv
    simp only at hinv
    cases ev with
    | observe l' o' p' u' =>
      simp only [scopeMachine, WP.spec_ok]
      exact hinv
    | ask t =>
      simp only [scopeMachine]
      apply WP.spec_bind (no_early_return t l o p u)
      intro b hb
      cases b with
      | Done =>
        obtain ⟨_, ho, hp, hu⟩ := hb.mp rfl
        simp only [WP.spec_ok]
        intro a ha
        rcases List.mem_append.mp ha with ha | ha
        · exact hinv a ha
        · rw [List.mem_singleton] at ha; subst ha; exact ⟨ho, hp, hu⟩
      | Waiting | OutOfScope =>
        simp only [WP.spec_ok]
        exact hinv

end Proofs.Barrier
