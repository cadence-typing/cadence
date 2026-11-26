---
rfc: "2026-11-26-slow-save-pauses-practice"
title: "A slow save pauses practice instead of invalidating the session"
status: accepted
implementation: not-started
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/51
milestone: "0.1.0-alpha.6"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: A slow save pauses practice instead of invalidating the session

## Summary and motivation

**How time is measured changes in one case: while a checkpoint acknowledgement is overdue, the clock is stopped and nothing is scored. Today that case ends the session as invalid.**

`Engine::watchdog` (`crates/engine/src/storage.rs`) returns an error when a checkpoint has been outstanding for more than `CHECKPOINT_CADENCE_US` (2 s) while the session is running. The reducer turns the error into `invalidate`: "no qualified score", and resume is refused for the rest of the session. `late_success_ack_is_consumed_even_when_watchdog_invalidates` (`crates/engine/tests/plan_session.rs`) pins it. Every checkpoint is a `synchronous=FULL` transaction that rewrites all learning rows of the session, so one slow fsync (WSL2, a busy disk, a laptop waking) ends a 15-minute session although the write then succeeds and nothing was lost.

The handover asks for something else (D-02): "If sustained disk failure/slow writes prevents the promised checkpoint cadence, stop advancing scored practice and expose the problem rather than extending the loss window invisibly." Alpha.5 already made focus loss and paste pause instead of invalidate; a slow acknowledgement is the remaining session-killing accident. This is #51, filed `needs-owner`; this RFC records the recommended default so work can proceed.

Outside this decision: making checkpoints cheaper ([observation chunks](2026-11-26-observation-chunks.md)), and the 10-second final-flush watchdog, which is unchanged.

## Observable behaviour

Before: after 2 s without an acknowledgement, "Measurement stopped ... no qualified score"; the session cannot continue.

After:

- At the same 2 s bound practice pauses with the reason "Saving is slow · the clock is stopped". Keys typed while paused are not applied, as in any pause.
- When the acknowledgement arrives and succeeded, practice resumes by itself and the notice clears. The loss window never exceeded what was already unacknowledged at the pause.
- When it arrives as a failure, today's save-failed path takes over (`r` retries).
- If the learner had also paused for another reason (Esc, focus loss), that reason stays and the acknowledgement does not resume practice.
- `e` (end early) and Ctrl-C work during the stall as in any pause; the final flush keeps its own bound.
- A clean interval never spans the stall ([evidence RFC](2026-11-26-evidence-and-repeat-gap-decisions.md)).

## Proof obligations

### TLA+ first: `models/CheckpointSave.tla`, `models/SessionFlow.tla`

| Model | Change | Invariant | Mutant configuration that must violate it |
|---|---|---|---|
| `CheckpointSave` | new action `Stall` (an unanswered checkpoint passes the bound while practice runs) and `CatchUp` (its success acknowledgement is received) | `NoScoreWhileOverdue`: no content event is credited while a checkpoint is overdue; `StallLosesNothing`: a stall never changes `durable`, `lastAck` or credits | `score-while-overdue`, `stall-drops-tail` |
| `SessionFlow` | pause reason `saving`; `CatchUp` resumes only a pause whose reason is `saving` | `PauseKeepsReason` (existing) and `CatchUpKeepsOtherPauses`: a pause with another reason is never ended by an acknowledgement | `ack-resumes-any-pause` |

Registry rows gain the production symbols `cadence_kernel::session::step` (events below) and the runtime tests named under Evidence.

### Lean: `cadence_kernel::session::step`

| Change | Theorem | Mutant |
|---|---|---|
| `Event::SaveStalled`: `Running -> Paused`, `Clock::Stop`, `Outcome::Unchanged`; refused in every other state | `Session.stall_stops_clock`, `Session.stall_changes_no_outcome` | `stall-keeps-clock`, `stall-marks-unfinished` |
| `Event::SaveCaughtUp`: `Paused -> Running`, `Clock::Resume`; refused elsewhere | `Session.caught_up_only_from_paused`; the existing `Session.clock_runs_only_running` and `Session.review_is_terminal_for_typing` must still build with their mutants failing | `caught-up-from-review` |
| `session::overdue(elapsed_us, bound_us) -> bool` | `Session.overdue_spec`: true iff `elapsed_us > bound_us` | `overdue-inclusive` |

Tested, not proved: that the engine sends `SaveCaughtUp` only when its recorded pause reason is the stall (the kernel table has no reason; `SessionFlow` models it), and the watchdog's use of the clock.

## Design and boundaries

`cadence-kernel` owns the two events and the bound comparison. `cadence-engine` (`core`): `watchdog` raises `SaveStalled` through `Engine::transition` and sets `PauseReason::SavingSlow`; `save_result` on success raises `SaveCaughtUp` when the pause reason is `SavingSlow`. The invalidation branch and its message are deleted. `cadence-tui` shows the reason like the other pause reasons. No I/O is added to the reduce path.

## Compatibility and reversibility

No persisted format changes. A session that would have been stored as invalid is now stored as an ordinary session with a pause. Reverting restores the invalidation and needs `[[amend]]` entries for the added events.

## Alternatives

- **Keep invalidation and raise the bound** (for example to 10 s). Still kills a session on a slow disk, later. Rejected as the default.
- **Do nothing visible and keep typing.** Extends the loss window invisibly; D-02 forbids it.
- **Require Esc to resume after the acknowledgement.** Consistent with other pauses, but the learner did nothing to cause this one; automatic resume is the default. Owner may amend.

## Evidence and rollout

One issue (#51), in this order: the two models with their mutants; the kernel events with claims; the engine change. Runtime tests: the pinned test is rewritten to expect a pause at the bound, no score change during the stall and a resume on the late success (called out as a contract change, not a weakened test); a stall followed by a failed acknowledgement reaches the save-failed state; an Esc pause during a stall is not resumed by the acknowledgement. A PTY run with a save delayed by a test-only fault port shows the notice.

## Unresolved questions

For the owner (#51 stays `needs-owner` until answered; work proceeds on the default): automatic resume versus Esc to resume; whether the bound should stay 2 s.

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
