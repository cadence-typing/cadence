---
rfc: "2026-11-27-full-numeric-keypad-practice"
title: "Full numeric keypad practice"
status: accepted
implementation: not-started
authors:
  - tknawara
  - claude-fable (design agent)
created: 2026-11-27
updated: 2026-11-27
issue: https://github.com/cadence-typing/cadence/issues/101
milestone: "0.1.0-rc.2"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Full numeric keypad practice

Amends two sections of earlier records without replacing them: the operator drill of [Configuration, keyboard layout and the settings surface](2026-11-26-configuration-layout-and-settings.md) (#83) and the numpad focus group of [Deliberate practice in every phase](2026-11-26-deliberate-practice-in-every-phase.md) (#46).

## Summary and motivation

**What counts as an error, a correct digit, a pace and a clean entry does not change. What changes is the content of the numpad track, which keys can become a numpad focus, and which sessions the numpad trend compares.**

The owner asked (2026-11-27): "For the numpad exercises can they also include - and + and * , ...etc. like the entire numpad not just the numbers on it?"

Traced through the code at `8afecde`:

| Keypad key | Drilled today (default config) | With `content.numpad_operators = true` (#83) | Can be a focus skill |
|---|---|---|---|
| `0`–`9` | Yes | Yes | Yes |
| `.` (or `,`) | Yes, at most one per entry | Yes | Yes |
| Enter | Yes: submits every entry | Yes | No (it is not a target unit) |
| `+ - * /` | **No** | Yes, but every entry becomes `operand operator operand` | **No**: `trainer::learner::group_of` admits digits and the separator only |
| Leading sign (`-12.5`) | No | No | — |
| More than one operator (`12+7-3`) | No | No | — |

So the whole keypad is reachable only through a setting that is off by default and that replaces the digit drills instead of joining them, and a learner who is slow on `*` is never given more `*`. The generator is `crates/trainer/src/numeric.rs::numpad_exercise`: a seeded draw of one of five digit or decimal shapes (`uniform_numpad`), optionally joined to a second one by a seeded operator.

The decision: by default the numpad track drills the whole keypad as a policy-defined mix of five entry families; operator keys and digit/operator transitions join the numpad focus group; the shape of an entry and the mix are kernel decisions with Lean claims.

Outside the decision: verifying that a key came from the physical keypad (#98 stays the owner's), arithmetic (entries are strings, nothing is evaluated and there is no `=`), NumLock-off navigation keys, parentheses, percent, and any change to the number-row track.

An issue alone is not enough because this changes the session plan policy, the learner model's focus groups, a comparison rule and adds kernel decisions.

## Observable behaviour

Before, a numpad phase (default config):

```
numpad · Exact entries. Leading zeros count.
4821     07.50     3333     00412.75     5656
```

After (default config; the same phase, same length, Enter still submits each entry):

```
numpad · Exact keypad entries: digits, decimals and + - * /. Leading zeros count.
4821     12+7     07.50     -45.20     6*3/2     3333     84/2.5
```

- Each entry is typed exactly and submitted with Enter. Enter on an incomplete entry does nothing, a wrong key is a first-attempt error at that position, Backspace corrects it and the error stays counted: all as today.
- `*` and `/` are shown as those characters, never as × or ÷, because those are what the keys send and what is compared.
- Start screen and review name operator skills when they are weak: "numpad 7+ · *", "Most slips: numpad *4".
- `content.numpad_operators = false` gives digits and decimals only, from the next session. Its default becomes `true`.
- Results still report keypad pace as correct **digits** per minute. Operators, signs and separators count toward accuracy and clean-entry and take time, so full-keypad pace is lower than digits-only pace for the same hands. For that reason the numpad trend starts again at the first full-keypad session ("content changed"); the words, number-row and prose trends continue unbroken.
- Help and `STATUS.md` keep saying that physical-keypad origin is unverified. A `+` typed on the main row during a numpad phase is recorded in the numpad lane, exactly as a main-row `7` is today.

## Proof obligations

Written before the design. No existing claim loses a theorem or a mutant, and none gains an assumption.

**Unchanged proved decisions**, still called by the same production code: `first::settle` and `counters::*` (what is an error), `ledger::complete` (when Enter may submit), `ratio::digits_per_minute` (pace), `ledger::clean` (clean entry), `settings::takes_effect` (the setting keeps its `Kind::NumpadOperators` and stays `NextSession`), `comparison::same_basis` (its inputs change, see "Comparison"; the function and its four theorems do not), `focus::pick` and `focus::qualifies` (which candidate becomes a focus), `lanes::*` and `evidence::*`.

**New kernel module `crates/kernel/src/keypad.rs`** (issue #102). Every function is total, integer-only and loop-free except `well_formed`, which is one `while` over `get`.

| Rust entry point | Decision | Lean theorems (`proofs/Proofs/Keypad.lean`) | Mutants that must break one |
|---|---|---|---|
| `keypad::class(code: u32) -> Class` | Which characters are keypad keys: `Digit` (`0`–`9`), `Separator` (`.` `,`), `Operator` (`+ - * /`), `Outside` | `class_spec` (the exact sets; total), `digit_iff_ascii_digit` (a key is a digit exactly when its code is 48–57, so no operator or separator ever enters the digit numerator) | `operator-is-digit` (`+` classed `Digit`); `comma-outside` (`,` classed `Outside`) |
| `keypad::step(state, class, sign) -> State` and `keypad::accepting(state)` | The entry grammar, one key at a time | `step_spec` (the table below), `reject_absorbing`, `outside_rejects`, `no_adjacent_operators` (an operator after an operator or after a sign rejects), `one_separator_per_operand`, `accepting_spec` (only `Whole` and `Fraction` accept: an entry never ends in an operator, a sign or a separator) | `trailing-operator-accepted`; `second-separator-accepted`; `adjacent-operators-accepted` |
| `keypad::well_formed(codes: &[u32]) -> bool` | Whether a whole string may be served as a keypad entry | `well_formed_spec` (equals `accepting` of the fold of `step` over `class`; total; the empty string is rejected) | `well-formed-skips-last` (loop bound off by one) |
| `keypad::family_at(position: u64, weights: Weights, operators: bool) -> Family` | Which family an entry of the phase belongs to | `family_at_spec` (with `r = position mod total`, the family whose cumulative weight interval holds `r`, so each whole cycle serves each family exactly its weight), `disabled_never_served` (a zero weight is never returned while another is positive), `operators_off_digits_only` (with `operators = false` only `Plain` or `Decimal`), `empty_mix_is_plain` (no positive admissible weight returns `Plain`; total) | `family-boundary-off-by-one` (`<` for `<=`); `operators-off-ignored` |
| `keypad::carries(family, first, second, pair) -> bool` | Whether a focus skill can be embedded in a family | `carries_spec` (`Plain` carries digit skills only; a separator needs `Decimal`, `Signed`, `Operation` or `Chain`; an operator needs `Signed`, `Operation` or `Chain`; nothing carries `Outside` or an operator/operator pair) | `plain-carries-operator` |
| `keypad::focus_admissible(first, second, pair, operators) -> bool` | Whether a numpad-lane skill may be a numpad focus | `focus_admissible_spec` (never `Outside`; never an `Operator` when `operators = false`; otherwise every keypad key and pair) | `focus-ignores-operators-off` |

The grammar `step` decides (`sign` is true when the operator is `+` or `-`):

| State | Digit | Separator | Operator | Outside |
|---|---|---|---|---|
| `Start` | `Whole` | `Reject` | `Signed` if `sign`, else `Reject` | `Reject` |
| `Signed` | `Whole` | `Reject` | `Reject` | `Reject` |
| `Whole` | `Whole` | `Point` | `Operator` | `Reject` |
| `Point` | `Fraction` | `Reject` | `Reject` | `Reject` |
| `Fraction` | `Fraction` | `Reject` | `Operator` | `Reject` |
| `Operator` | `Whole` | `Reject` | `Reject` | `Reject` |
| `Reject` | `Reject` | `Reject` | `Reject` | `Reject` |

That is `sign? operand (operator operand)*` with `operand = digits (separator digits)?`. `005.20`, `-12.5`, `12+7` and `4.5*2/3` are entries; `12+`, `1..2`, `.5`, `5.`, `3*-2` and the empty string are not. Every entry the generator makes today is accepted.

**Host assumptions.** The Charon/Aeneas translation and the Lean kernel, as for every other claim (`proofs/TRUST.md`). `class` takes a `u32` code point; that `char as u32` in the caller is the character's code is Rust's definition, not proved.

**Tested, not proved.**
- That production calls these functions on the right facts: `KeypadEntry::new` passes the entry's code points to `well_formed`; `plan_content` passes the cycle position and the policy weights to `family_at`. Covered by trainer tests and `consumer_boundaries.rs`.
- The seeded permutation of positions inside a cycle (the order differs per seed; a permutation cannot change the per-cycle counts, which a test pins).
- The generator: that each family's builder produces an entry of that family, operand lengths, operator choice, focus embedding, separator replacement, the no-repeat scan. A wrong builder cannot serve a malformed entry (the constructor refuses it) but could serve a well-formed entry of another family; tests pin the family.
- Policy parsing and validation, the settings default, the mapping of a session onto its comparison basis (`core::comparison::basis`), rendering, fixtures.
- Physical-keypad origin: not established, as before.

## Design and boundaries

Crates and tiers: `cadence-kernel` (kernel) gains `keypad`; `cadence-core` and `cadence-trainer` (core) call it; `cadence-engine` (core) and `cadence-tui`, `cadence` (edge) change text and defaults. Dependencies still point inward. Nothing new on the input → reduce → render path: entries are prepared with the plan, as today.

### The keypad alphabet as a typed domain

`keypad::Class` is the single definition of the keypad alphabet. `core::alphabet::TrainingChar::is_digit` and `trainer::learner::group_of` call `keypad::class` instead of holding their own character tests (AGENTS: never a second copy). Keypad keys that send no character with NumLock on (NumLock itself) or that are not content (Enter) are outside the alphabet; Enter stays the submit command.

`trainer::numeric::KeypadEntry` is a newtype over `Vec<TrainingChar>` whose only constructor is `KeypadEntry::new(text) -> Result<Self, NumericError>`, which runs `keypad::well_formed`. `numpad_exercise` returns `KeypadEntry`, so a malformed entry cannot reach a plan.

### Content: five families

`numpad_exercise(rng, used, family, focus, options)` builds one entry of the given family. Entries are built with `.`; the configured separator replaces it last, as today.

| Family | Shape | Examples | Built from |
|---|---|---|---|
| `Plain` | A digit group: a run, one repeated digit, or a repeated pair | `4821`, `3333`, `5656` | today's `uniform_numpad` cases 0, 2, 3 |
| `Decimal` | Whole and two-digit fraction, sometimes with leading zeros | `07.50`, `00412.75` | today's cases 1, 4 |
| `Signed` | `+` or `-`, then a `Plain` run or a `Decimal` | `-45.20`, `+7` | new |
| `Operation` | operand, one operator, operand | `12+7`, `84/2.5` | today's operator drill |
| `Chain` | three operands, two operators | `6*3/2`, `12+7-3` | new |

Operands inside `Operation` and `Chain` are digit runs of `minimum_operand_digits..=maximum_operand_digits`, one in three a decimal, so longer entries stay typeable in the phase's rhythm. The four operators are drawn uniformly by the plan's seeded source; a test pins that all four appear.

The all-operations behaviour of #83 is deleted, not kept behind a second flag. `NumericOptions::operators` now means "operator families are allowed".

### Mix policy (trainer policy v5)

`policies/trainer-v5.toml` adds one table. Every number is a named, validated field of a typed `KeypadPolicy`; none appears in code.

```toml
[keypad]
# Entries of each family in one cycle of numpad entries. The cycle is their sum.
plain_entries_per_cycle = 3
decimal_entries_per_cycle = 2
signed_entries_per_cycle = 1
operation_entries_per_cycle = 3
chain_entries_per_cycle = 1
# Digits in one operand of an operation or a chain.
minimum_operand_digits = 1
maximum_operand_digits = 4
```

and raises `numpad_focus_skill_count` from 3 to 4, because the group grows from 11 keys to 15.

Validation (named bounds in the policy module, refusal names the field): the cycle is `1..=MAXIMUM_KEYPAD_CYCLE` (20); `plain + decimal > 0`, so switching operators off never leaves the phase empty; operand digits are `1..=MAXIMUM_OPERAND_DIGITS` (6) with minimum ≤ maximum. Policies v1 to v4 have no `[keypad]` table and parse to the mix that reproduces their content.

The schedule: for entry `i` of the phase, the cycle number is `i / total` and the position is a seeded permutation of `0..total` for that cycle; the family is `keypad::family_at(position, weights, operators)`. So half of a default cycle is digit and decimal work and half uses an operator key, the order is interleaved rather than blocked, and every whole cycle has exactly the policy's counts.

There is no unlocking course. `docs/01-product.md` asks for every key to be available at once with a short observation period instead; the mix is the same on the first day, and the learner model then shifts the targeted share toward what is weak.

### Learner model: weak keypad keys are targeted

- Evidence is already recorded per key and per transition in lane `numpad` for every content key of a numpad phase (`engine::practice`, lane chosen by track), including operators when they were served. Nothing changes there.
- `learner::group_of` admits to `FocusGroup::Numpad` every numpad-lane skill for which `keypad::focus_admissible` holds: digits, the separator, operators, and pairs of them (`7+`, `*4`, `.5`, `-1`). With operators off, operator skills are not admissible, so a focus is never one the session cannot serve. The group's error-rate and interval baselines are computed over the same members, so they include operator keys once those have evidence.
- `focus::pick` (proved) still chooses among the candidates; `numeric_target_share` (2 of 3) still says how many entries are targeted.
- A targeted entry keeps its scheduled family. Its focus is the next focus skill, in rotation, that `keypad::carries` for that family; if none does, the entry is untargeted. So the proved per-cycle mix holds for targeted entries too, at the cost that fewer than 2 of 3 entries may be targeted when every focus is an operator and the family is `Plain`.
- Embedding: a digit skill goes into an operand (`embed`, as today); a separator skill into a decimal operand; an operator unit becomes the entry's operator or sign; `7+` ends the left operand in `7` before `+`; `*4` starts the right operand with `4`.
- A missed entry still returns once after `numeric_reinforcement_gap_entries` (#59); it is a whole entry string and needs no change.

### Scoring and errors

Unchanged, by the same proved functions. Every character of an entry is a unit: a mistyped operator is a first-attempt error; accuracy and clean-entry count all units; correct digits per minute counts digit units only over the phase's active time. Enter is not a unit.

### Comparison

`ContentClass::Operators` already exists, is persisted (`sessions.content_class`, schema CHECK) and already separates operator sessions from standard ones. It now means "the numpad entries may contain operators". Two rules follow:

1. `app::session::content_class` is unchanged: a session with a numpad phase and operators allowed is `operators`.
2. `core::comparison::basis` lets the class `Operators` into a basis only when the compared tracks include `Track::Numpad`; otherwise the basis carries `Standard`. Without this, switching the default would break the words, number-row and prose trends of every daily session, whose content did not change.

`kernel::comparison::same_basis` and its theorems are untouched; rule 2 is in the tested mapping in front of it.

### Display and input

`engine::presentation::phase_purpose` and the Numpad destination text name the full keypad; targets and review slips show operator skills through the existing `skill_label`. No new widget.

Input is unchanged: the terminal reports a keypad `+` as the character `+`, the same as the main row's, unless a keyboard-protocol extension Cadence does not require is active; keypad Enter arrives as Enter. Lane is decided by the track, never by the key's origin, and the source annotation stays `Unverified` (`docs/02-learning.md`). With NumLock off the keypad sends navigation keys, which are not content and are ignored while typing, as today.

### Persistence and protocol

No schema change and no new persisted value: `content_class = 'operators'` is already allowed; skills are stored as characters with their lane, and operator skills are already stored when served; sessions record their policy digest, so v5 sessions are distinguishable. The headless protocol gains no request or field; one fixture scenario, `numpad-full`, is added.

## Compatibility and reversibility

- **Existing profile.** History loads unchanged. The first session after the upgrade has operators in its numpad phase (the default changed). The numpad trend restarts there; other tracks' trends continue. The whole-session "last session" comparison breaks once at the switch.
- **A config that sets `content.numpad_operators`.** `false` keeps digits and decimals. `true` used to mean every entry is an operation and now means the mix; earlier `operators` sessions stay in the same class as the new ones. Both count digits per minute over time that includes operator keys; the mix differs, and the changelog says so.
- **Focus list.** With policy v5 the numpad group may hold four skills. `MAXIMUM_FOCUS_SKILLS` (16) still covers 6 + 3 + 4.
- **Downgrade.** The store written by rc.2 has no new schema version, content class or skill encoding, so it holds nothing an rc.1 binary has not seen. A downgrade test is not part of this work.
- **Backup and export.** Unchanged formats.
- **Undo.** Set the default back to `false` and ship policy v4: content returns to digits and decimals; recorded operator evidence stays in the store, inadmissible as a focus. The kernel module and its claims would remain (claims are never weakened).
- **Frozen contract.** `docs/01-product.md` P-04 says "add `+ - * /` in a selectable operator drill". The setting remains the selection; a decision record (`docs/decisions/0010`) states that it is on by default and that the drill is a mix. The frozen files are not edited.

## Alternatives

- **Leave it: tell the owner about `content.numpad_operators`.** Costs nothing, but that drill has no digits-only entries, no signs, no chains, and never targets a weak operator. The request is for the keypad as a whole.
- **Flip the default only.** One line, but the numpad phase would become all operations, operator keys would still never be a focus, and every track's trend would break (the class is session-wide today).
- **A new setting `content.numpad_keys = digits | full` and a new class `keypad`.** Cleaner names, but a new kernel `Kind`, a rebuilt `sessions` table to widen the CHECK constraint, a schema version and a config migration, all to rename something that already has the right meaning.
- **A second lane or identity for keypad `+` versus main-row `+`.** Lanes already separate them by track, which is the only fact Cadence has. Identity by physical key needs the keyboard protocol evidence of #98; if the owner's terminal turns out to report keypad origin, a later RFC can annotate evidence as `VerifiedKeypad` without changing this design.
- **Evaluated arithmetic (`12+7=19`).** Makes the drill mental arithmetic and puts thinking time into typing pace. Entries stay strings.
- **Staged unlocking (digits, then decimals, then operators).** Contradicts the product contract's "all keys at once"; the targeted share already concentrates practice on weak keys.
- **Families and grammar in the trainer, tested only.** Less work, but the shape of what is served and the allocation of a phase's entries are trainer decisions, which AGENTS puts in the kernel with claims.
- **Draw the family by seeded weighted chance instead of a schedule.** Simpler, but a short phase could by chance contain no operator at all, and nothing could be proved about the mix.

Evidence that would change the choice: the owner finding operators in half the entries too many or too few (change the v5 weights, no code), or a terminal that reports keypad origin (add verified annotation).

## Evidence and rollout

Four issues in dependency order, each safe to land alone:

1. **#102 Kernel `keypad`.** The claims above recorded `proved` by `cargo tauten proofs` (translation, `lake build`, axiom audit, every mutant killed). No caller, no behaviour change.
2. **#103 Trainer.** Policy v5, `KeypadEntry`, the five families, the schedule and operator focus skills, with generator, schedule, targeting and policy tests. The default is still off, so shipped behaviour is unchanged.
3. **#104 Default and surfaces.** The default flips; the per-track basis rule; screens, help, fixture, golden frames, PTY smoke; scoring fixtures pin that a mistyped operator is an error and not a digit.
4. **#105 Release.** Decision record, STATUS, changelog, RFC fields, rc.2 evidence.

Finish line: on a fresh profile `cadence --track numpad --minutes 5` shows entries using every keypad character key within the first cycles; after a session with slow `*`, the next start screen names it under numpad targets; `content.numpad_operators = false` restores digits and decimals; Progress shows the other tracks' trends unbroken across the upgrade.

## Unresolved questions

None blocks acceptance. Resolved by default, for the owner to amend:

- **Default mix weights** (3 plain, 2 decimal, 1 signed, 3 operation, 1 chain): half of the entries use an operator key. A policy edit changes it.
- **Signs**: both `+` and `-` may lead an entry; a sign never follows an operator (`3*-2` is not an entry).
- **`*` and `/` glyphs**: shown as typed.
- **Old `operators` sessions** stay comparable with new full-keypad sessions rather than being split into a third class.
- **Keypad `=`, `%`, parentheses, NumLock-off keys**: not drilled; most keypads have none of the first three.
- **Whether the owner's terminal delivers every keypad operator key** is asked in #98's script; the design does not depend on the answer.

## Decision log

- 2026-11-27: Accepted by default (owner rule of 2026-11-26 for RFCs written by the project's agents) following the owner's request quoted above. Scheduled in milestone `0.1.0-rc.2`, tracking issue #101, implementation issues #102 to #105. The owner amends afterwards.
