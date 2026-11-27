# 0.1.0-rc.2 release evidence (#105)

Captured on 2026-11-27, on temporary HOMEs and profiles only; the gates ran
in the network-less Compose gate container.

- `qualify.txt`: `./ci qualify` of candidate
  `cand-063a733d1b4b608684ee21562debbf69` (commit `5dd7eef`, profile
  `linux-x86_64-v1`): every gate passed except `wsl-terminal` (pending:
  owner-assisted, #98) and `buildbot-recovery` (unsupported: no Buildbot
  controller, #14), the same two as rc.1, so admission is refused and the
  bundle is unpromoted. Later commits add only this README and docs.
- An earlier candidate (`cand-59cb4f57…`, commit `badddf0`) failed the
  `headless` gate: journeys 06, 09 and 12 diverged because the settings view
  now shows `content.numpad_operators` on by default and the Progress break
  note was reworded. They were re-recorded from their request files and the
  diffs reviewed (`8c0c034`: view digests, the version string and that note,
  nothing else).
- `upgrade-rc1-to-rc2.txt` (`upgrade.sh`): a profile written by the
  published rc.1 binary (two sessions with learning evidence) opened by
  rc.2: schema 7 on both sides, every session, sequence, digest and learning
  record unchanged; rc.2 then records a full-keypad session (`12*`,
  Backspace, `+7`, Enter, `-4.50`, Enter) and rc.1 still opens the profile
  and reads all three sessions. The recorded session uses the `numpad-full`
  fixture scenario, which is synthetic and carries the fixture class
  `standard`; real sessions take their class from `session::content_class`
  (`operators` by default, pinned by `operator_drills_are_their_own_class`).
- `performance.json`: the `performance` gate's measurements from the
  qualification (reducer p99 64 µs, frame p99 at 120x40 100 µs,
  input-to-frame p99 174 µs, peak RSS 17.4 MB) on one recorded machine; not
  a hardware promise.
- Proof gate: 80 of 80 claims proved, the seven new `keypad` claims with all
  10 new mutants killed (`docs/TAUTEN.md`, `proofs/claims.json`).
