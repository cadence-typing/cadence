# Cadence implementation and feedback-build status

`0.1.0-alpha.2` is an **unsigned development preview**, not a P8-qualified
release. Earlier commit messages overstated P0–P2 completion: passing tests,
ASCII filtering, and documentation of intended proofs did not discharge the
handover's proof, vocabulary-vetting, adaptation, or qualification obligations.
This ledger supersedes those completion claims; the original handover stays
unchanged in `docs/01-*` through `docs/10-*` and `MANIFEST.sha256`.

## Try the feedback binary

Linux x86_64, including WSL2; at least 60×18 cells, no Nerd Font required.
Download the versioned tarball and `SHA256SUMS` from the matching GitHub
prerelease. Check `sha256sum --check SHA256SUMS` for the files downloaded,
extract the archive, then run `./cadence` inside it. The archive contains the
same executable as the standalone `cadence-linux-x86_64` asset, plus notices.
Checksums detect byte changes; they are **not signatures**.

```sh
cadence
cadence --minutes 5 --track numpad
cadence --minutes 5 --track words --seed 42
cadence --text-size normal    # auto (default), large, or normal
cadence --profile-dir /absolute/private/profile
cadence backup --output /absolute/new-backup.sqlite3
cadence export --output /absolute/new-progress.json
```

Enter starts/continues and submits numpad entries. Esc pauses/resumes;
F2 cycles Auto → Large → Normal. Ctrl-C saves and quits.
Summary Enter/Esc finishes the process after the final
save acknowledgement. A save failure pauses practice: `r` retries; an explicit
unsuccessful exit retains only acknowledged progress. Final flush has a
10-second watchdog and returns non-success rather than hanging or claiming a
save without an acknowledgement. The normal profile is
`$XDG_DATA_HOME/cadence`, falling back to `~/.local/share/cadence`; relative XDG
values are ignored. Backup/export refuse to overwrite an existing destination.

The default plan allocates 900 seconds (840 typing + 60 dismissible review).
The phase clock starts on its first content key. Ordinary hesitation remains
in active time; explicit pauses and inter-phase screens do not. Prepared
exercise lists cycle until the deadline, visibly labelled as repeat practice.
The planner is currently **cold-start mixed practice, not learned adaptation**.
Numpad strings preserve leading zeros and require Enter; physical-keypad origin
is unverified. This preview supports ASCII-first practice, not full first-run
layout configuration, imports, benchmark qualification, or the full settings UI.

Progress is cumulative local SQLite data: correction history, separate track
counts, exact durations, and last acknowledged checkpoint survive reopening.
After a kill, reopening reports interrupted/partial history. It does **not**
resume a mid-word exercise or claim an uninterrupted timing trial. Store schema
2 safely refuses a populated historical draft schema 1 rather than inventing
measurements or resetting it. Keep that file and use a new explicit profile
for feedback if such a draft database exists.

## Alpha.2 visual refresh

The [researched proposal](plans/0001-visual-focus.md) covers centered landing,
phase transitions and review screens; an editor-style segmented modeline; and
large, centered practice text. Large characters use portable 4×4 terminal-cell
glyphs, **not native font scaling** or terminal configuration changes. Auto
normally selects large text at 80×24 or above; a diagnostic banner can reduce
the available canvas and select normal text. Forced large falls back visibly
if its geometry or glyph requirements cannot be met.
Normal text is also centered; both modes follow the caret without changing the
target, input, first-pass history or difficulty.

The modeline prioritizes mode, durability and time. `CKPT n` names the last
acknowledged checkpoint, **not a promise that every current key is saved**.
Shrinking below 60×18 pauses through the reducer; restoring the window leaves
practice paused until Esc. Stage screens and explicit pauses do not accumulate
active practice time. No scoring-policy or stored-schema change accompanies
this refresh.

## Requirement ledger

| Packet / claims | Status | Production mapping, evidence, remaining obligation |
|---|---|---|
| P0 / A-01, B-02 | in_progress | Pinned Rust image/toolchain, Cargo lock, musl C compiler, architecture feedback check. Proof-tool compatibility, full dependency/security audit and shared CI admission remain unqualified. |
| P1 / L-02–03, VK-01–02 | implemented, behavior checked | `core/{ledger,metrics,session}`, `engine/{practice,presentation}`; scoring fixtures/properties and live corrected-entry smoke. These are not same-source Verus proofs. |
| P2 / P-02–04, VK-03–06 | in_progress | `trainer/{allocation,plan,plan_content,policy,review,pace}`; deterministic seed/budget/track tests and bounded eligible scans. Cold-start selection only; learner adaptation, retention/benchmark comparisons and 20k vetted vocabulary acceptance remain unfinished. |
| P3 / D-01–02, VP-01–03 | in_progress | `core/progress`, `data/{store,schema,records,files,transfer}`, `engine/storage`, `runtime/writer`. One retained writer/result slot, full-key admission, checkpoints, retry, lock, reopen, backup/export implemented. General job-registry/async planner, immutable observation chunks, full crash/storage campaigns and formal correspondence remain unfinished. |
| P4 / H-01–04, VP-04–05 | pending | No shipped NDJSON driver, causal/durable barriers or replay. TestBackend is shared rendering evidence, not a completed headless protocol. |
| P5 / U-01–05 | in_progress | `engine/view`, `tui/{paint,stages,stages/results,modeline,practice,glyphs}` share one painter for centered stages and text, typed modeline facts and F2/CLI text-size preferences. Reducer tests preserve corrections/timing across display changes and resize pause. Full destinations/settings/layout setup, imports and editable themes remain unfinished. |
| P6 / H-05–06 | in_progress | Hermetic Rust regressions and actual PTY/SQLite smoke; no broad terminal qualification or measured p99/memory admission claim. |
| P7 / VP, VK, VL, VR inventory | pending | Packet models remain design probes. No TLAPS/Verus production proof inventory has passed. |
| P8 / B-03–06 | blocked for qualification | Static feedback-bundle producer and owner trial installation exist. Buildbot recovery, authorized signing, complete inventory admission and owner terminal/keypad qualification are not completed. |

## Evidence and limits

- Workspace behavioral tests cover exact timing boundaries, corrections,
  numeric submission, bounded selection, malformed policy/identity rejection,
  checkpoint freshness, SQL rollback/idempotency, lock/schema/data validation,
  backup/export and private file permissions. Tests use temporary profiles.
- Native PTY smoke exercised numpad `9 x Backspace 4 9 4 Enter`: committed
  first-pass **3/4**, attempt **4/5**, one deletion. Quit/reopen, acknowledgement
  followed by SIGKILL/reopen, concurrent-writer rejection, actual SQLite backup,
  portable export and SIGINT terminal restoration were observed. A real SQLite
  rejection produced a visible save error, unsuccessful explicit exit, terminal
  restoration, and preservation of the prior acknowledged checkpoint.
- Alpha.2 native PTY captures exercised 100×32 and 60×18 landing/practice,
  large/normal switching, visible corrected input, completed keypad entries,
  below-minimum pause/recovery, save-error priority and successful retry after
  a real SQLite rejection. The corrected keypad trial again persisted 3/4
  first-pass, 4/5 attempts and one deletion. Shared production reducer/painter
  captures additionally exercised 120×40, interludes and per-track review.
- Release packaging inspects ELF64/x86_64 headers and requires **no PT_INTERP
  and no DT_NEEDED entries**. Bundles retain exact lock/policy/corpus hashes,
  source commit/clean flag, compiler identity and binary SHA-256. Packaging is
  not a substitute for release qualification.
- The word pack contains 25,000 alphabet-filtered wordfreq tokens, not a
  professionally vetted dictionary. It may include names, abbreviations,
  offensive words or nonwords. Full upstream notices and CC-BY-SA terms ship
  in `corpus/`; practice prose is explicitly original AI-assisted material,
  not verified literary excerpts or unseen benchmark content.

## Reproduce

```sh
./ci prepare                 # networked image/dependency preparation only
./ci quality                 # offline fmt, strict Clippy, Rust tests, boundaries
./ci dist                    # offline musl build, ELF inspection, notices, tarball
cargo xtask gates            # qualification obligations, not green-by-omission
```

`quality` is a feedback command; `gate quality` and `gate corpus` explicitly
refuse full qualification because their audits/inventories are incomplete.
Build outputs/caches live only in `dist/` and `.cadence-build/`, owned by the
caller UID/GID. No unrelated Docker resources, CI service, or owner history is
modified by these commands. Qualification and software proofs do not establish
human learning effectiveness or promise a WPM gain.

## Alpha.2 delivery receipt

[Release and screenshots](https://github.com/tknawara/cadence/releases/tag/v0.1.0-alpha.2)
bind the static binary to clean source
`5f932654ab8812360a4b167067c1b3b639624db5`. Compose quality passed 139 tests,
strict Clippy, formatting, boundaries and the original contract manifest.
Compose dist verified no ELF interpreter or shared-library dependencies.
Native static-binary practice and a real-time phase transition passed; an
offline, non-root scratch-container trial persisted the honest corrected-entry
counts. Uploaded assets were downloaded and checksum-verified.

Installed atomically at `~/.local/bin/cadence`, pointing to
`~/.local/lib/cadence/0.1.0-alpha.2-5f932654ab88/cadence`; alpha.1 is retained.
The installed command reopened the temporary saved profile, showed its history,
switched text size, and preserved earlier counts. Normal owner history was not
used for verification. Detailed receipts and limits are in the proposal.

