---
rfc: "2026-11-26-pace-targets-and-benchmark-probes"
title: "Pace targets and benchmark probes"
status: accepted
implementation: complete
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/6
milestone: "0.1.0-beta.1"
supersedes: []
superseded_by: []
blocked_on:
  - https://github.com/cadence-typing/cadence/issues/27
---

# RFC: Pace targets and benchmark probes

## Summary and motivation

**No existing score changes. Two things are added: a per-track suggested pace that moves by a proved rule, and a weekly unseen-text probe recorded as its own track. A lowered pace changes word difficulty, and that change is recorded.**

Two parts of the handover's trainer (P2) are parsed from policy and never used:

- **Pace hysteresis (L-06).** `trainer::pace::pace_decision` exists with tests and has no caller. It also conflates two quantities: its `current_target` is a `BasisPoints` value that is compared with block accuracy *and* raised by `maximum_raise`, so from the default 9,800 one raise gives 10,000 and every later block would need 100% accuracy. L-06 describes two things: a fixed desired first-pass accuracy (98%) and a suggested *pace* that rises by at most 3% or falls by at most 5%.
- **Benchmark probes (P-03).** `[benchmark]` in `policies/trainer-v2.toml` is parsed; nothing reads it. There is no unseen probe text, so "am I improving" rests on prose practice passages only.

Why an RFC: this changes the session plan policy, adds kernel decisions and a persisted format (schema 5).

Outside this decision: any claim that the probe measures learning (it is a like-for-like measurement, nothing more), a forced cursor or live speed target (P-05 forbids both).

## Observable behaviour

**Pace.** Each typing track has a suggested pace in correct units per minute (shown as WPM; numpad as digits per minute). It is absent until the track has a baseline (the median pace of its qualifying blocks). The start screen and the results show it as a soft goal: "Words goal 52 WPM · steady". After three qualifying blocks in at least two sessions it rises by 3% ("raised"); a block under 95% first-pass accuracy lowers it by 5% ("lowered: more familiar words next time") and the next session's words phase draws a larger share of its target slots from the core band. Nothing on the practice screen moves or hurries.

A *block* is one track's completed slice in one session. It qualifies with at least 100 first-attempt positions and no pause-invalidated measurement.

**Benchmark.** After every seventh completed daily session (a completed session that included prose), the first 120 s of the prose phase is a probe: a purpose-written passage this profile has never seen, captioned "Transfer probe · unseen text · not practice". Results show a "Transfer probe" row with the change since the previous comparable probe, or "not enough comparable probes yet". A probe that was paused, ended early or typed in a synthetic (headless) session is shown and marked "not comparable". A probe passage is retired for the profile once shown.

## Proof obligations

| Rust entry point (kernel) | Theorem | Mutant that must break it | Production caller |
|---|---|---|---|
| `pace::decide(target, desired_accuracy_bp, blocks, policy) -> Decision` with `Decision::{NoBaseline, Steady, Raise(new), Lower(new)}`; replaces `trainer::pace::pace_decision` | `Pace.no_baseline_without_target`; `Pace.raise_bounded`: `target < new <= target + ceil(target*raise_bp/10000)`; `Pace.lower_bounded`: `target - ceil(target*lower_bp/10000) <= new < target`, never below 1; `Pace.raise_needs_evidence`: a raise implies at least `qualifying_blocks` qualifying blocks from at least `minimum_sessions` sessions, each at or above the desired accuracy with at least the position floor, and no attempt-accuracy drop; `Pace.low_block_lowers`: a latest block strictly below the slow-down band gives `Lower`; `Pace.between_bands_steady`; equality at each band is on the documented side; total | `raise-unbounded`, `raise-on-one-session`, `raise-ignores-floor`, `lower-on-equal`, `lower-below-one`, `degradation-ignored` | `trainer::plan::build_session_plan` (per typing track) |
| `pace::block_accuracy_below(first_correct, first_attempted, band_bp) -> bool` | `Pace.below_exact`: exact cross-multiplication, no rounding | `below-rounds` | `pace::decide` |
| `benchmark::due(completed_since_probe, every, unseen_available) -> Due` with `Due::{Probe, NotYet, NoPassage}` | `Benchmark.due_exact`: `Probe` iff `completed_since_probe >= every` and a passage is available; `NoPassage` is never silently `NotYet` | `due-off-by-one`, `no-passage-hidden` | `trainer::plan::build_session_plan` |
| `benchmark::comparable(facts) -> bool`; facts: completed full probe window, paused, ended early, pasted, synthetic, same layout, same strata class | `Benchmark.interrupted_never_comparable`, `Benchmark.synthetic_never_comparable`, `Benchmark.comparable_iff_all` | `paused-comparable`, `synthetic-comparable` | `engine::summary` and the store's probe row |

Tested, not proved: building block evidence from stored tracks; the median baseline; passage strata (punctuation density, capitalisation, mean word length, band mix) and the matching of a probe to the previous one; the planner's familiar-share adjustment; rendering.

## Design and boundaries

- **Kernel.** New modules `pace` and `benchmark` (`kernel` tier). `trainer::pace` is deleted in the same change, with its tests moved to behaviour tests of the caller.
- **Policy `trainer-v4.toml`** (v3 stays): `[pace]` keeps its fields with `maximum_raise` and `maximum_lower` reinterpreted as a share of the pace, plus `familiar_target_share_when_lowered`; `[benchmark]` is unchanged.
- **Schema 5 (`cadence-data`).** `Track::Benchmark` (digest tag 6, persisted name `benchmark`), excluded from the headline like `review`; `pace_targets(session_id, track, target_cpm, decision)`; `benchmark_probes(session_id, passage_id, strata, comparable, reason)`; `sessions.provenance` (`live` or `synthetic`, default `live`), which the headless driver sets (see [Headless driver and replay](2026-11-26-headless-driver-and-replay.md)). Migration 4 to 5 adds tables and the column in one transaction; no backfill.
- **Content.** `corpus/benchmark-en-v1.tsv`: at least 60 purpose-written passages (Apache-2.0 OR MIT, decision 0002), each long enough for 120 s at 100 WPM, with recorded strata; never served by the prose selector. `xtask corpus` checks counts, alphabet and disjointness from practice prose.
- **Planner (startup only).** Reads the last target and blocks per track, calls `pace::decide`, records the decision in the plan; when the words decision is `Lower`, raises the core-band share of target slots for this session. Calls `benchmark::due`; on `Probe` the prose phase is split into a 120 s probe exercise and the remaining prose.
- **Rule 1.** Every pace decision and the familiar-share change are persisted with the plan and shown.

## Compatibility and reversibility

Schema-4 profiles migrate on open with no pace target (baseline forms from history already stored) and no probe history. Older binaries refuse schema 5. Turning the feature off later leaves the tables in place.

## Alternatives

- **Wire `pace_decision` as it is.** Ships the conflation described above. Rejected.
- **Probe from the practice prose bank.** Not unseen once practised; P-03 requires unique, uncontaminated passages.
- **Blocks smaller than a session's track.** Needs per-block persistence that only [observation chunks](2026-11-26-observation-chunks.md) provide; revisit after rc.1.

## Evidence and rollout

#61 pace kernel; #63 benchmark kernel; #64 schema 5, probe bank and planner; #62 pace recorded, shown and applied; #65 results. Reducer and planner tests at each band equality; a seven-session scripted run on a temporary profile shows one probe; a paused probe is stored and marked not comparable.

## Unresolved questions

None blocking. Owner may amend: block = one track per session; 60 passages; the familiar-share value.

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
