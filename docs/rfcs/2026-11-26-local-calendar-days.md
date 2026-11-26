---
rfc: "2026-11-26-local-calendar-days"
title: "Review days are local-calendar days with a recorded offset"
status: accepted
implementation: not-started
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/52
milestone: "0.1.0-alpha.7"
supersedes: []
superseded_by: []
blocked_on:
  - https://github.com/cadence-typing/cadence/issues/45
---

# RFC: Review days are local-calendar days with a recorded offset

Amends the "Schema 4" paragraph of [Deliberate practice in every phase](2026-11-26-deliberate-practice-in-every-phase.md): the 3 to 4 migration also adds one nullable column.

## Summary and motivation

`crates/app/src/main.rs` computes `day = SystemTime::now() / 86_400`, a UTC day. That value is `sessions.day`, the day the review ladder steps on, the "different days" test for promotion, an input to the plan seed and the source of "earlier today / N days ago". The contract (L-06) says "1, 3, 7, 14 and 30 local-calendar days ... record calendar/timezone identity", and `engine/src/storage.rs` already documents the field as "Local calendar day of this session".

Away from UTC the difference is visible: for a learner at UTC-8 an evening session and the next morning's session share a UTC day (no promotion, "earlier today"), and two sessions minutes apart across 16:00 local count as two days and can promote a word. This is #52, filed `needs-owner` with two options; this RFC records option (a), restoring the contract, as the default.

## Observable behaviour

- A session's day is the learner's local calendar day at session start. Promotion needs clean recalls on two different local days. "Earlier today" means the same local day.
- If no local time zone can be determined, days are counted in UTC and the start screen says so once: "Days are counted in UTC (no local time zone found)".
- `--utc-offset +HH:MM` overrides detection (containers, tests).
- Sessions recorded before this change keep their stored day.
- Scores and timing do not change.

## Proof obligations

New module `cadence_kernel::calendar`.

| Rust entry point | Theorem | Mutant that must break it | Production caller |
|---|---|---|---|
| `calendar::local_day(unix_seconds: u64, offset_seconds: i32) -> Day` with `Day::{Number(i64), OffsetOutOfRange, Overflow}` | `Calendar.local_day_spec`: on `Number(d)`, `86400*d <= unix_seconds + offset_seconds < 86400*(d+1)`; `Calendar.local_day_monotone`: for a fixed offset, a later time never gives an earlier day; `Calendar.offset_bounded`: an offset beyond +/-18 h is `OffsetOutOfRange`; total | `day-ignores-offset` (today's behaviour), `day-rounds-up`, `offset-unbounded` | `cadence::main` (the only place that reads the wall clock), replacing the division |
| `calendar::days_ago(today: i64, then: i64) -> u32` | `Calendar.days_ago_never_negative`: 0 when `then >= today` | `days-ago-wraps` | `engine::review` recap text |

The ladder is unchanged: `review::ladder_step` (#37) already takes day numbers from the caller and refuses a backward day (`Review.rollback_refused`). A local day can step back by one after travel west or a clock change; sessions are replayed in `(day, session)` order as today, so the ladder sees non-decreasing days.

Tested, not proved: reading the offset from the system (edge), the migration, and that every consumer of "day" receives the kernel's value.

## Design and boundaries

- **Offset source (`cadence` app, `edge`).** One pinned dependency with the named purpose "local UTC offset at session start": `chrono` with default features off and `clock` on (pure-Rust reading of `TZ` and `/etc/localtime`, no libc call, so it works in the static musl binary and offline). The implementer verifies it in the Compose image and in the static bundle; `jiff` is the fallback choice. The offset is read once at session start through a capability, never on the reduce path.
- **Schema 4 (`cadence-data`).** The 3 to 4 migration of #45 adds `sessions.utc_offset_minutes INTEGER NULL`. `NULL` means "recorded as a UTC day before alpha.7". New sessions store the local day in `sessions.day` and the offset used. `SessionProgress` carries the offset; its digest encoding adds the field only when present, so existing receipts keep their digests.
- **Export** writes `utc_offset_minutes` (or `null`).
- **Seed.** `session_seed` keeps its inputs (session id and day); the day is now the local day.

## Compatibility and reversibility

Existing profiles migrate with every stored day unchanged. Around the changeover a learner away from UTC can see one review come due a day earlier or later than it would have. Reverting means storing UTC days again with offset 0; the column stays.

## Alternatives

- **(b) Amend L-06 to say UTC days** by a decision record. No code, but promotion and "today" stay wrong for most learners. Rejected as the default.
- **Store an IANA zone name.** More identity than the ladder needs; the offset at session start is what decided the day.
- **Call libc `localtime_r`.** Needs `unsafe` in an owned crate; AGENTS.md forbids it.

## Evidence and rollout

One issue (#52) after #45: kernel function and claims; then the app and store. Tests with an injected clock and offset: 23:30 and 00:30 local on either side of midnight are different days at UTC-8 and UTC+10; two sessions across UTC midnight inside one local day are the same day; an offset change between sessions keeps replay order; a migrated schema-3 profile keeps its days and has `NULL` offsets; digest fixtures for existing receipts are unchanged.

## Unresolved questions

For the owner (#52 stays `needs-owner` until answered; work proceeds on the default): option (a) versus (b); `chrono` as the dependency.

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
