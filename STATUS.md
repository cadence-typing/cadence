# Cadence implementation and feedback-build status

`0.1.0-alpha.4` is an **unsigned development preview**, not a P8-qualified
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
exercise lists cycle until the deadline. A prose passage typed within the
no-repeat window is labelled as a repeat; cycling of prepared word and numeric
lists is **not yet labelled**. Since alpha.4 the words phase is **adaptive**
(below); warm-up, number-row and numpad content remain cold-start drills.
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

## Alpha.4 learning from mistakes

[Plan 0003](plans/0003-learning-from-mistakes.md) responds to an independent
review that found nothing typed influenced later content and prose repeated
the same six paragraphs every session.

- **Evidence.** The reducer records every first attempt per key and per key
  transition (main keyboard and numpad separately), clean inter-key intervals
  on active time (corrections and pauses break the chain; gaps above 1.5 s are
  excluded from the learner model only, never from scores), per-word outcomes
  and typed prose passages. A corrected slip stays an error.
- **Learner model** (startup only): recency-weighted Beta-prior error rate,
  relative hesitation and uncertainty over the last 60 sessions. A skill
  becomes a focus only above explicit error/hesitation thresholds with enough
  attempts; sparse evidence is uncertain, never mastered.
- **Words phase.** Target slots rotate over up to six focus transitions and
  draw core/common words containing them, ranked by the existing bounded
  utility scan; broad slots keep variety; review slots serve due missed words
  on a 1/3/7/14/30-day ladder (promotion needs clean typing in two sessions on
  different days; an error resets). A word missed in this session returns
  once in a later review slot. The start screen and each exercise name the
  focus; cold start says so.
- **Prose.** 2,643 passages (863k characters): 19 public-domain books with
  recorded Gutenberg sources and digests, plus the 16 original passages.
  Passages typed within 200 sessions are not re-served while unseen ones
  remain; every exercise shows title and author.
- **Seed.** Derived from the session id and day and persisted, so same-day
  sessions differ and each plan is replayable. `--seed` still overrides.
- **Schema 3.** Additive tables for plans, focus, skill observations, word
  outcomes and seen passages. A populated schema 2 migrates in one
  transaction with those tables empty; older receipts keep their exact
  digests. Alpha.3 and earlier refuse a schema-3 profile (`SchemaTooNew`), so
  take a backup before downgrading.

Displayed scores are unchanged by any of this. Targeted words are harder by
design and are labelled; whether this produces faster learning is an
empirical question this preview does not answer.

### Alpha.4 delivery receipt

[Learning-from-mistakes release](https://github.com/tknawara/cadence/releases/tag/v0.1.0-alpha.4)
is bound to clean source `123b044bbb04594010c7c63c34ee23e34c08c6a9`. Offline
Compose quality passed 180 tests, strict Clippy, boundaries and the original
manifest. The release bytes migrated a copy of the owner's schema-2 profile in
an offline container and drove two real PTY sessions that showed in-session
reinforcement and next-session targeting. Uploaded assets were re-downloaded,
checksum-verified and atomically activated at
`~/.local/lib/cadence/0.1.0-alpha.4-123b044bbb04/cadence`; the owner's profile
was backed up with alpha.3 beforehand and opened at schema 3 with all 10
sessions afterwards. Details: [plan 0003](plans/0003-learning-from-mistakes.md).

## Alpha.3 smooth native typography

The [native typography decision](plans/0002-smooth-native-text.md) supersedes
alpha.2's rejected bitmap lettering. Cadence now asks the terminal to render its
own font at **2× size**, using native double-height rows. It does not change font
settings or require a Nerd Font. The old bitmap renderer and font dependency are
removed. Centered landing, transitions, review and the segmented modeline remain.

Startup verifies actual cursor geometry for both double-height halves; environment
variables are not treated as proof. Auto uses native text when detected and two
complete rows fit. Otherwise it uses centered ordinary text with a visible
native-scaling fallback hint. F2 and `--text-size auto|large|normal` remain available.
Text, correction history and timing are independent of the typography preference.

Session review now leads with large correct-speed and first-pass accuracy tiles.
Mixed-session WPM includes all typing-track time but excludes keypad units/time;
keypad-only sessions show correct digits/minute. Accuracy aggregates positions,
not track percentages, and cannot round an error up to 100.0%. Per-track details
remain available below the headline cards.

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
| P2 / P-02–04, VK-03–06 | in_progress | `trainer/{allocation,plan,plan_content,plan_words,learner,learning_policy,policy,review,prose}`, `engine/learning`, `core/learning`; deterministic seed/budget/track tests, bounded eligible scans, learner posterior/decay/threshold tests, weakness-steered target and due-review tests, prose no-repeat tests, reducer evidence/reinforcement tests. Pace hysteresis (`pace.rs`) and benchmark probes remain unwired; numeric tracks are not adapted; retention/benchmark comparisons and 20k vetted vocabulary acceptance remain unfinished. |
| P3 / D-01–02, VP-01–03 | in_progress | `core/{progress,learning}`, `data/{store,schema,records,learning_rows,files,transfer}`, schema 3 with migration/round-trip/regression tests, `engine/storage`, `runtime/writer`. One retained writer/result slot, full-key admission, checkpoints, retry, lock, reopen, backup/export implemented. General job-registry/async planner, immutable observation chunks, full crash/storage campaigns and formal correspondence remain unfinished. |
| P4 / H-01–04, VP-04–05 | pending | No shipped NDJSON driver, causal/durable barriers or replay. Shared cell/row-rendition capture is rendering evidence, not a completed headless protocol. |
| P5 / U-01–05 | in_progress | `engine/{view,summary}`, `tui/{paint,stages,stages/results,stages/headlines,modeline,practice,native_text}` share cells plus native row metadata; `runtime/{presenter,text_protocol}` present and negotiate it. Reducer/summary regressions preserve correction history, weighted metrics, timing and resize pause. Full destinations/settings/layout setup, imports and editable themes remain unfinished. |
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
  in `corpus/`. Prose combines the original AI-assisted passages with verbatim
  public-domain book paragraphs whose sources, digests and extraction recipe
  are recorded in `corpus/NOTICE` and `corpus/manifest.json`; it is practice
  content, not unseen benchmark material.

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


## Alpha.3 delivery receipt

[Native text and session headline release](https://github.com/tknawara/cadence/releases/tag/v0.1.0-alpha.3)
is bound to clean source `7d8bf9a3b435b4f088c50bf5241c8bc9a7954ee9`.
Canonical offline quality passed all 151 tests, formatting, strict Clippy,
boundaries and the original manifest. Static packaging verified no interpreter
or shared libraries. Real Windows Terminal captures covered smooth text and
large summary tiles; deterministic fixture values are labelled as such.

The final binary passed native-protocol/fallback smoke and offline non-root
scratch-container practice/save/reopen. Uploaded bytes were downloaded and
checksum-verified before atomic activation of
`~/.local/lib/cadence/0.1.0-alpha.3-7d8bf9a3b435/cadence` through `~/.local/bin/cadence`.
The installed command preserved earlier corrected-entry counts in the temporary
profile. Alpha.2 and the owner's open trial/history were left intact.

