# 0010: The whole keypad by default

Status: accepted (2026-11-27), issues #101 and #104. Interprets
`docs/01-product.md` P-04, which stays hash-pinned as handed over
(`MANIFEST.sha256`); this record and the
[RFC](../rfcs/2026-11-27-full-numeric-keypad-practice.md) state how it is
read.

## Context

P-04 asks the numpad track to "add `+ - * /` in a selectable operator
drill". beta.3 (#83) implemented that as `content.numpad_operators`, off by
default, under which every entry became `operand operator operand`. The
owner asked on 2026-11-27 for numpad exercises to cover "the entire numpad,
not just the numbers on it".

## Decision

- The operator drill stays selectable: `content.numpad_operators` is the
  selection, and it is **on by default**. `false` gives digits and decimals
  only, from the next session (a proved `NextSession` setting).
- "Operator drill" means a policy-defined mix of five entry families
  (plain digits, decimals, signed, two-operand operations, three-operand
  chains), not a phase made only of operations. The mix is policy v5's
  `[keypad]` table; which family an entry belongs to, the entry grammar
  and which focus skills a family can carry are proved kernel decisions
  (`cadence_kernel::keypad`).
- Operator keys and digit/operator pairs join the numpad focus group, so a
  slow `*` is practised more.
- Entries are strings: nothing is evaluated, and `*` and `/` are shown as
  typed. Physical-keypad origin stays unverified (#98).

## Consequences

- The numpad trend restarts once, at the first full-keypad session
  ("content changed"): pace is still correct digits per minute, but over
  time that now includes operator keys. Words, number-row and prose trends
  continue: the `operators` class enters a comparison only when the numpad
  is compared (`core::comparison::basis`).
- A config that already set `content.numpad_operators = true` now gets the
  mix instead of operations only; those earlier sessions stay in the same
  class as the new ones.
- The frozen handover files are not edited.
