# Formal assurance with honest scope

## V-01 Tool choice

Use **TLA+** to specify interleaving-sensitive protocols; **TLC** to explore bounded state spaces and kill deliberate faults; **TLAPS** for general inductive safety arguments; **Verus** for selected executable Rust algorithms and their arithmetic/collection invariants. This follows the inspected Strop approach and avoids maintaining a separate Lean implementation of Rust algorithms merely to prove the copy.

Verus is an SMT-based program verifier with a different trusted toolchain from Lean; this document does not claim identical foundational guarantees. Its advantage here is proof of the functions the application actually calls. Keep the verified surface small enough to complete. Lean remains an option only if a concrete mathematical claim cannot be expressed sensibly in the selected stack; it is not a mandatory extra dependency.

Formal claims prove stated software properties under assumptions. They do not prove human learning benefit, arbitrary terminal fidelity, absence of every Rust bug, SQLite/OS durability, or the compiler/hardware stack. A successful finite TLC run is not an unbounded theorem. A verified predicate alone does not prove every caller supplies the right identity.

## V-02 Required claim inventory

| ID | Claim | Required evidence |
|---|---|---|
| VP-01 | Results cannot mutate a different owner generation/request; duplicates cannot apply twice; stale errors cannot replace current results | TLA+/TLC with reorder/restart/reuse/duplicate schedules; TLAPS inductive invariant; same-source Verus acceptance predicate; runtime correspondence tests |
| VP-02 | A retained terminal completion is consumed or explicitly retired; cancellation cannot erase required completion facts; bounded storage | TLA+ result-slot/queue model with weak fairness assumptions; TLC; actual channel/job tests and bounded-memory storm |
| VP-03 | Saved watermark never exceeds committed watermark; retry is idempotent; a changed payload under the same key conflicts | TLA+/TLC; TLAPS protocol safety; actual SQLite kill/reopen/retry tests |
| VP-04 | Causal barriers include descendant jobs and queued results; durable barriers include the captured persistence watermark | TLA+/TLC; TLAPS safety; real engine barrier regressions |
| VP-05 | Headless request reuse cannot apply the same command twice; conflict/expired IDs cannot mutate state | TLA+ small stream model; parser/cache tests; request-admission kernel proof |
| VK-01 | First outcomes are write-once; delete/retry cannot overwrite them; counts bounded; no duplicated committed exercise contribution | Verus on production ledger update kernels; property/fixture tests at the engine boundary |
| VK-02 | Correct throughput/accuracy arithmetic is defined only with valid denominators, bounded and overflow-safe | Verus exact checked arithmetic; independent hand-calculated fixtures |
| VK-03 | Phase allocation respects minimums and sums exactly to the requested budget, or returns infeasible | Verus allocator including tie/remainder handling; generated boundary cases |
| VK-04 | Candidate selection terminates, returns only an eligible optimum within its bounded candidate set, and respects repeat/focus caps | Verus production bounded scan/eligibility update; seeded determinism/property tests |
| VK-05 | Quota counters never overrun; feasible blocks have required lane allocations; fallback/debt is explicit | Verus production quota transition plus complete-block invariant; adversarial tiny-corpus tests |
| VK-06 | Due queue has unique members and monotonic admission order; review cannot serve an ineligible/wrong-track item | Verus queue/selection kernels; persistence constraints and actual-code tests |
| VL-01 | Continuously eligible due work is eventually served under the stated finite-predecessor, no-insertion-ahead and progress assumptions | General ranking/induction argument with checked lemmas; TLA+ temporal property checked under explicit weak fairness |
| VR-01 | Live, headless and replay execute the same reducers/effect admission/rendering; timing/RNG inputs explicit | Dependency/API review, deterministic trace replay, effect/cell equality and controlled divergence tests |

`formal/claims.json` is the machine-readable inventory. An implementation agent must bind each row to source symbols and required tests/proofs in a checked assurance manifest before P7. Do not label a whole row proved when only its trivial arithmetic sublemma exists.

## V-03 Included models and what remains

The packet includes `AsyncAdmission.tla`, `DurableCheckpoint.tla` and `CausalBarrier.tla`, with positive configurations and five deliberately faulty configurations (missing epoch check, missing pending check, premature save ack, duplicate credit, early barrier return). These are small design probes covering VP-01, part of VP-03 and part of VP-04. They do not yet model byte budgets, actual SQL transactions, generation overflow, full NDJSON caches, or learner algorithms. Read the evidence report for the exact bounds/results. Extend them to the production contract; do not preserve a toy model unchanged while adding a different runtime protocol.

P7 must provide the missing result-retention, request-cache and review-fairness models as well as general TLAPS proofs. At minimum prove `Init => Inv` and `Inv /\ Next => Inv'` for admission/checkpoint/barrier safety with unbounded natural-number epochs/sequences and appropriate finite sets. State type/ownership invariants needed for inductiveness, rather than asserting only the final safety property. TLAPS safety does not magically prove fairness/liveness; that row has its own explicit ranking and temporal evidence.

## V-04 Planner proof shape

**Budget:** take validated nonnegative minimum allocations whose sum is ≤ budget. Allocate the remaining integer seconds by quotient/remainder against positive integer weights. The sum of floors is ≤ remaining budget; the unallocated remainder is less than the count of positive-weight phases. Add one second to that many phases in descending fractional remainder, stable phase-ID order on ties. Prove exact sum, minimum preservation, no overflow and determinism. Validate all sums/products before fixed-width execution; a proof over mathematical integers alone is not sufficient for `u64` code.

**Selection:** preconditions bound candidate vector length and ensure candidate records/utility inputs are validated. A scan invariant says the current best, if any, is eligible and at least as good as every previously visited eligible candidate under the total `(utility, seeded_tiebreak, stable_id)` order. Loop index strictly advances and is bounded by vector length. At exit return the best or an explicit no-candidate result. This proves optimality only within the materialized candidate vector, not across all English vocabulary.

**Quotas/caps:** each successful slot consumes one unit from its requested/effective lane counter and updates the selected word/focus counts once. Prove count bounds and consistency between emitted slots and counters. Infeasible fallback returns a tagged decision; it cannot rewrite the requested lane and then claim the original quota was fulfilled. Claim exact 6/2/2 only for complete feasible blocks; retain short prefixes and fallback explanations truthfully.

**Due fairness:** for a fixed continuously eligible item, rank is the number of eligible queued predecessors. A review service removes a predecessor or the item; no new admission increases this rank because admission is tail-only. Repeated successful service therefore reaches the item in at most rank+1 review services. Calendar time requires additional assumptions about enabled tracks and sessions actually being completed; users can stop practising indefinitely. Record these assumptions in the theorem rather than claiming unconditional daily fairness.

**Scoring:** prove append assigns only previously empty first-outcome slots; deletion changes the visible buffer and correction counts but leaves the first-outcome vector unchanged. Prefix-extension can improve the overall ratio by adding new correct positions, so the actual theorem is “correction alone cannot erase prior first-attempt errors,” not “accuracy can never rise.” Bind committed exercise IDs to a consumed set or monotonic frontier to prevent duplicate cumulative contribution.

## V-05 Production linkage and negative calibration

Place verified functions in the owned production modules, compiled as ordinary executable Rust after ghost erasure. Both Verus and normal builds consume exactly those source files. Verify their callers' precondition establishment by types/runtime validation/tests, and formally where practical. The claim manifest records symbol paths, source digests, verifier command/toolchain/solver, theorem names, assumptions, calibration mutants and runtime correspondence fixtures. A function name or comment alone is not a link.

Keep mutants outside shipping code or behind a test-only build configuration whose absence is checked in release. Required examples: ignore epoch; accept consumed request; discard a full result slot; ack before commit; add a retry twice; return a barrier before child registration; clear first-error on Backspace; allow a repeated word past cap; off-by-one budget remainder; starvation through insertion at the head of review.

A model mutant passes calibration only when the expected named invariant/temporal property fails with a valid counterexample, not on any nonzero tool exit. A Verus mutant must invalidate its intended contract; an actual-code mutant must be killed by the named test. Parser errors, missing tools and arbitrary crashes are gate failures, not successful mutation kills. Keep bounded search details and model assumptions in receipts.

## V-06 Practical gate discipline

Separate fast unit/headless feedback from heavier model/proof qualification. Run affected formal lanes while implementing their modules, not after the UI is complete. A private experimental binary may be used for feedback, explicitly unqualified. The final owner bundle requires all declared proof rows; no optional “formal later” switch in the default qualification profile.

Do not add a giant theorem about the whole TUI. Small protocols and real algorithm kernels are valuable if linked to the production path. Track unproved assumptions transparently. An exhausted proof budget is a blocked obligation to refine, not permission to redefine a required theorem as a test.
