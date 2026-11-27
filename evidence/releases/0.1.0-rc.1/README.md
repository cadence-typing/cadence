# 0.1.0-rc.1 release evidence (#96)

Captured on 2026-11-27, on temporary HOMEs and profiles only; the gates ran
in the network-less Compose gate container.

- `qualify/qualify.log`: `./ci qualify` of the release candidate
  `cand-0bb098eec7aa30aab04ab4062f4dc2cc` (commit `66993ca`, profile
  `linux-x86_64-v1`): every gate passed except `wsl-terminal` (pending:
  owner-assisted, #98) and `buildbot-recovery` (unsupported: no Buildbot
  controller, #14), so admission is refused and the bundle is unpromoted.
  `ACCEPTANCE.txt` is the bundle's copy: each gate's receipt digest, the
  admission decision and the signature state (#13). Later commits add only
  this evidence, the changelog date and docs.
- Walkthrough step 8: `qualify/qualify-1.log`, `qualify-status-interrupted.log`,
  `qualify-2.log` and `qualify-status-final.log` — an earlier candidate
  (`cand-a37e7fd2…`, commit `6960470`, whose quality gate failed `cargo fmt
  --check`) was interrupted by killing its container during `unit-property`;
  `./ci resume` re-ran only the unfinished and failed gates and kept the
  interrupted attempt. `install-from-bundle.txt`: the release candidate's
  bundle (`./ci dist`, no GitHub Actions) installed offline with
  `install.sh --from`.
- Walkthrough steps 2-7: `walkthrough/` (`tools/walkthrough.sh` with the
  installed rc.1 binary): journeys 01-04, leave and reopen with the stored
  watermarks, a recorded trace replayed identically (`trace.jsonl`), and an
  invalid config, an invalid theme file and an import with a terminal
  sequence, each named without changing anything. Step 1 is the
  `package-install` gate.
- `migration-schema6-to-7.txt` (`migration.sh`): a profile written by the
  published beta.3 binary (two sessions with learning evidence) opened by
  rc.1: every session, sequence, digest and learning record reads back
  unchanged, marked legacy with no chunks synthesized; beta.3 then refuses
  the migrated file.
- `performance.json`: the `performance` gate's receipt from the
  qualification (reducer p99 64 µs, frame p99 at 120x40 99 µs,
  input-to-frame p99 176 µs, peak RSS 17.8 MB) on one recorded machine; not a
  hardware promise.
- Proof gate (73 of 73 claims, 191 theorems, 157 mutants killed), TLC gate
  and the assurance manifest: `docs/TAUTEN.md`.
