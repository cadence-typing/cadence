# Cadence implementation and feedback-build status

`0.1.0-alpha.6` is an **unsigned development preview**, not a P8-qualified
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
cadence --theme light         # dark (default), light, or terminal
cadence --pack code-rust --minutes 5   # or code-shell, or an imported pack
cadence import /absolute/notes.txt --name notes [--dry-run]
cadence packs
cadence config check
cadence --profile-dir /absolute/private/profile
cadence backup --output /absolute/new-backup.sqlite3
cadence export --output /absolute/new-progress.json
```

Enter starts/continues and submits numpad entries. Esc pauses and opens the
practice controls (Esc/Enter resume, `e` ends the session and shows the
review); losing terminal focus or pasting pauses the same way, and a paste
inserts nothing. F2 cycles Auto → Large → Normal. Ctrl-C during practice ends
the session and shows the review (saved as unfinished); on the review, Enter
saves and exits after the final acknowledgement and prints a one-line recap.
On the start screen `q` or Ctrl-C quits without recording anything. A save
not acknowledged within 2 seconds pauses practice ("Saving is slow; the clock
is stopped") and resumes by itself when it completes. A save failure pauses practice: `r` retries; an explicit
unsuccessful exit retains only acknowledged progress. Final flush has a
10-second watchdog and returns non-success rather than hanging or claiming a
save without an acknowledgement. The normal profile is
`$XDG_DATA_HOME/cadence`, falling back to `~/.local/share/cadence`; relative XDG
values are ignored. Backup/export refuse to overwrite an existing destination.

The default plan allocates 900 seconds, all typed: since alpha.7 the last
60 seconds are a closing review that retypes the session's misses (its own
`review` row, kept out of the headline). The phase clock starts on its first
content key. Ordinary hesitation remains in active time; explicit pauses and
inter-phase screens do not. Prepared exercise lists cycle until the deadline,
and every repeat is labelled ("again", "due review", "repeat: word",
"repeat · list restarted"; a prose passage typed within the no-repeat window
reads "repeat"). Every phase except prose is adaptive (below); prose is
deliberately untargeted and says so.
Numpad strings preserve leading zeros and require Enter; physical-keypad origin
is unverified. Since beta.3 a first run asks the layout, numpad and length;
settings live in `$XDG_CONFIG_HOME/cadence/config.toml` (`cadence config
check`), are edited in the app with `s`, and a layout change starts a
separate history for symbols. Outside practice Tab moves between Today,
Train, Numpad and Progress; `?` lists every key. `cadence import` and
`cadence --pack` practise your own text or the shipped Rust and shell
packs, never compared with standard scores. Benchmark qualification
remains unclaimed.

Progress is cumulative local SQLite data: correction history, separate track
counts, exact durations, and last acknowledged checkpoint survive reopening.
After a kill, reopening reports interrupted/partial history. It does **not**
resume a mid-word exercise or claim an uninterrupted timing trial. Store schema
2 safely refuses a populated historical draft schema 1 rather than inventing
measurements or resetting it. Keep that file and use a new explicit profile
for feedback if such a draft database exists.

## Beta.3 settings, layout and your own text

[RFC: configuration, keyboard layout and the settings surface](docs/rfcs/2026-11-26-configuration-layout-and-settings.md),
[RFC: local imports and code practice](docs/rfcs/2026-11-26-local-imports-and-code-practice.md).

- **Settings.** A typed, validated `config.toml`; first-run setup; an
  in-app overlay whose writes are acknowledged, atomic and never clobber
  an external edit; when each setting applies is the proved
  `settings::takes_effect` (a score-affecting setting never applies now).
  Theme files, high contrast without colour, a numeric key guide.
- **Layout.** Schema 6 records each session's layout and content class;
  the proved `comparison::same_basis` keeps every comparison (review,
  start-screen recap, Progress) within one layout, content class and live
  provenance.
- **Destinations.** Today, Train, Numpad, Progress and help from one typed
  command table that drives dispatch, hints and headless validation.
- **Your text.** A bounded, sanitising importer and pack sessions (imported
  text, shipped Rust and shell), kept out of pace, probes, the learner
  model, the ladder and comparisons.
- **Worker results** (settings, history) pass the proved `ticket::admit`;
  `WorkerResults.tla` models them.

Limits: themes and settings are checked on a few terminals only (P6);
whether code practice transfers to real coding speed is not claimed.

## Beta.2 a headless driver with replay

[RFC: headless driver and replay](docs/rfcs/2026-11-26-headless-driver-and-replay.md).

- **Headless driver.** `cadence headless --stdio|--script` runs the same
  reducer and renderer as the terminal app over NDJSON; repeated request ids
  never apply twice, reused or expired ids and stale revisions are refused
  with stable codes, and its sessions are stored as synthetic, never in
  history or comparisons.
- **Barriers and capture.** Causal barriers wait for every descendant job;
  durable barriers for the save acknowledgement past the captured watermark.
  `capture` returns cells, styles, renditions and cursor from the live
  painter; golden frames pin six screens.
- **Trace and replay.** `--record` traces seal practice text by digest;
  `cadence replay --read-only` reports the first diverging line and field.
  Six journeys replay in CI.

Limits: replay equivalence (VR-01) is tested, not proved; the barrier and
stream models are bounded TLC checks.

## Beta.1 pace goals, transfer probes and a vetted word pack

[RFC: pace targets and benchmark probes](docs/rfcs/2026-11-26-pace-targets-and-benchmark-probes.md),
[decision 0006: word vetting](docs/decisions/0006-word-vetting.md).

- **Pace goals.** Each typing track has a suggested pace (correct units per
  minute; digits per minute on the numpad), starting from the median of its
  qualifying blocks. It rises by at most 3% after enough clean blocks from
  enough sessions and falls by at most 5% after a block below 95%, by the
  proved `pace::decide`. A lowered words goal draws half the next session's
  target words from the most familiar band. Goals show on the start screen
  and the results; nothing on the practice screen moves.
- **Transfer probes.** After every seventh completed session with prose, the
  first 120 s of prose is a probe on a passage this profile has never seen,
  from 60 purpose-written passages in strata classes. Its result is a
  separate row, compared only with the previous comparable probe; a paused,
  early-ended, pasted-into or synthetic probe is stored and marked not
  comparable. Probes never count toward the headline.
- **Vetted word pack.** 22,000 dictionary words (SCOWL), without
  abbreviations, profanity or reviewed slurs; ids of kept words unchanged.
- **Schema 5**: pace targets, probes and session provenance. Back up before
  downgrading: alpha.7 refuses a schema-5 profile.

Limits: whether a pace goal or a probe helps anyone improve is an empirical
question and is not claimed; probe comparisons need several probes.

## Alpha.7 deliberate practice in every phase

[RFC: deliberate practice in every phase](docs/rfcs/2026-11-26-deliberate-practice-in-every-phase.md).

- **Warm-up** serves retention checks (due words above the first ladder
  rung) and says how many; a warm-up miss returns in the words phase.
- **Number row and numpad** each have their own focus digits and digit
  transitions from their own evidence and baseline (policy v3); two of
  every three prepared entries contain a focus skill; a missed entry returns
  once, at least two entries later in the same phase ("again").
- **Words**: a target word does not recur within four exercises.
- **Prose** stays untargeted (its passages never read the learner model) but
  feeds the loop: a slip in a pack word is a word outcome, enters the ladder
  and this session's closing review.
- **Closing review**: a typed, clocked last phase serving the session's
  misses in order, then extra words ("no misses left · extra words"). Its
  figures are a separate row, excluded from the headline and from
  like-for-like comparisons; ending during it still records the session as
  completed (`session::settle`, `Session.review_phase_keeps_completed`;
  `SessionFlow.ReviewNeverUncompletes`).
- **What the UI says**: the start screen names targets per group and due
  reviews; interludes say what the next phase targets; numeric entries are
  captioned "Targeting 7 · 48"; the results name slips from every lane and
  targets per practised track.
- **Local days** follow the local calendar (`--utc-offset`, schema 4); a new
  `review` track. Back up before downgrading: alpha.6 refuses a schema-4
  profile (`SchemaTooNew`).

Limits: retention checks and numeric targeting are only as good as the
recorded evidence; whether any of this helps a learner improve is an
empirical question and is not claimed. Pace targets and benchmark probes
remain unwired (beta.1).

## Alpha.6 proved practice decisions

[RFC: practice decisions in the kernel](docs/rfcs/2026-11-26-practice-decisions-in-the-kernel.md),
[evidence and repeat-gap decisions](docs/rfcs/2026-11-26-evidence-and-repeat-gap-decisions.md),
[slow save pauses practice](docs/rfcs/2026-11-26-slow-save-pauses-practice.md) and
[release bundles from a tag](docs/rfcs/2026-11-26-release-bundles-from-a-tag.md).

- **Every practice decision is a proved kernel function that production
  calls.** Phase allocation (VK-03), candidate utility and the bounded scan
  (VK-04), block quotas and caps (VK-05), the review ladder and reinforcement
  admission (VK-06), integer focus selection (decay, posterior, hesitation,
  qualification, pick), where a missed word is re-served, item labels and list
  cycling, and evidence attribution. The proof gate and its mutants are in
  [docs/TAUTEN.md](docs/TAUTEN.md).
- **The feedback loop is modelled.** `models/FeedbackLoop.tla` (observe,
  checkpoint, end or crash, replay through the ladder, plan, serve) is mapped
  to production with six invariants, a liveness property and seven mutants;
  the save protocol and session flow models gained the slow-save stall.
- **Fixes.** A re-served miss no longer appears twice in one exercise or
  displaces a due review (#43); sampled slots no longer repeat words within
  minutes (#44); a transition counts as correct only when both keys were (#42);
  a timing sample never spans a pause and a zero gap is not timed (#41); a slow
  save pauses instead of invalidating the session (#51).
- **Plans for a given seed changed** (#44 sampling, #38 integer focus); scores
  are computed exactly as before.
- **Releases are built in public CI from the tag** (#3): two clean builds must
  produce identical checksums, and the Lean proof gate runs for the tagged
  commit before the pre-release is published and test-installed.

No schema change.

## Alpha.5 calm flow and honest results

[Plan 0004](docs/rfcs/2026-11-14-calm-flow-and-honest-results.md) adopts an independent
critique of the alpha.4 TUI, captured from real PTY runs.

- **No session-killing accidents.** Focus loss and paste now pause (with the
  reason shown) instead of invalidating the session; input past the bounded
  extra allowance is refused with "Past the end · Backspace to the first
  error" instead of invalidating. Invalidation remains for real invariant
  failures, and its controls never offer an unavailable resume.
- **End early, still see results.** `e` in the pause controls or Ctrl-C during
  practice opens the review; the session is saved as unfinished. The pause
  controls replace the target, so a stopped clock is not reading time. Keys
  shown in hints and controls come from one typed binding table.
- **Interlude grace.** Enter/Esc within 0.7 s of a phase-complete screen
  opening are ignored, so the numpad's last submit cannot skip it.
- **Results that answer "how did I do".** Per-track accuracy %, pace,
  corrections and the change since the last session that had that track;
  headline tiles compare only with the newest session with the same track
  set. One sentence names the most frequent slips ("Most slips: th 9/41 ·
  …") and the focus targeted. The interlude reads "98.0% first-pass · 54 WPM
  · 11 corrections". The start screen shows the last session's pace and
  accuracy, marking one that ended early.
- **Quieter chrome.** "saved" replaces the bold `CKPT n`; live pace and
  accuracy are integers over the current track's completed exercises (they
  move at exercise boundaries, not per key); "phase 2/5" replaces the
  unreachable exercise count; the practice header shows the route;
  upcoming text is no longer DIM; footer boilerplate appears only in the
  first session; a missed space reads "expected space".
- **Content.** A re-served word is named ("again: other's"); a word appears
  at most once per words exercise; prose exercises end at a sentence or
  clause boundary (bound 160 units) instead of mid-clause.

No schema change; scores are computed exactly as before.

### Alpha.5 delivery receipt

[Calm-flow release](https://github.com/tknawara/cadence/releases/tag/v0.1.0-alpha.5)
is bound to clean source `b5917366380fceac6ac56b77a436b9c9a456d69f`. Offline
Compose quality passed 188 tests, strict Clippy, boundaries and the original
manifest. PTY runs on a profile copy covered pause reasons, ending early,
two full sessions and the review at three sizes. Assets were re-downloaded,
checksum-verified and atomically activated at
`~/.local/lib/cadence/0.1.0-alpha.5-b5917366380f/cadence` after a profile
backup. Details: [plan 0004](docs/rfcs/2026-11-14-calm-flow-and-honest-results.md).

## Alpha.4 learning from mistakes

[Plan 0003](docs/rfcs/2026-11-13-learning-from-mistakes.md) responds to an independent
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
sessions afterwards. Details: [plan 0003](docs/rfcs/2026-11-13-learning-from-mistakes.md).

## Alpha.3 smooth native typography

The [native typography decision](docs/rfcs/2026-10-19-smooth-native-text.md) supersedes
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
| P1 / L-02–03, VK-01–02 | implemented, behavior checked | `core/{ledger,metrics,session}`, `engine/{practice,presentation}`; scoring fixtures/properties and live corrected-entry smoke. The scoring decisions now live in `crates/kernel`, which production calls and which carries Lean theorems over its Aeneas translation (first-attempt write-once, counters, pace and accuracy arithmetic, active time, track validity, checkpoint admission); see [docs/TAUTEN.md](docs/TAUTEN.md) for each claim's badge. Ledger plumbing around the kernel stays tested. |
| P2 / P-02–04, VK-03–06 | in_progress | VK-03–06 are proved kernel decisions called by production (`kernel::{plan, select, lanes, review, queue}`, #34–#37, #55) and the focus model is integer (`kernel::focus`, #38/#56); `trainer/{plan,plan_content,plan_words,learner,learning_policy,policy,review,prose}`, `engine/learning`, `core/learning`; deterministic seed/budget/track tests, bounded eligible scans, learner posterior/decay/threshold tests, weakness-steered target and due-review tests, prose no-repeat tests, reducer evidence/reinforcement tests. Since alpha.7 every phase but prose is adaptive: per-group focus for the number row and numpad (#46, #5), warm-up retention checks (#47), numeric and prose misses in the loop (#59, #48), a typed closing review on its own track excluded from the headline (#49, #60), every repeat labelled (#7) and targets named on screen (#50). Since beta.1 pace hysteresis is a proved kernel decision (`kernel::pace`, #61) whose goals are recorded, shown and applied (#62), transfer probes run on unseen purpose-written text with proved due and comparability decisions (`kernel::benchmark`, #63–#65), and the 20k vetted vocabulary bar is met by `words-en-v2` (#8). Nothing in P2 remains open in this milestone; retention comparisons across probes need more recorded probes to say anything (empirical, not a code gap). |
| P3 / D-01–02, VP-01–03 | in_progress | `core/{progress,learning}`, `data/{store,schema,records,learning_rows,files,transfer}`, schema 3 with migration/round-trip/regression tests, `engine/storage`, `runtime/writer`. One retained writer/result slot, full-key admission, checkpoints, retry, lock, reopen, backup/export implemented. General job-registry/async planner, immutable observation chunks, full crash/storage campaigns and formal correspondence remain unfinished. |
| P4 / H-01–04, VP-04–05 | implemented | `crates/driver` (`envelope`, `protocol`, `stream`, `headless`, `capture`, `trace`) and `cadence headless|replay|doctor` (#67–#73): NDJSON protocol v1 with stable error codes and a bounded response cache, causal and durable barriers, canonical frame capture with golden frames, traces with digest-sealed content and read-only replay reporting the first divergence; six journeys replayed by `cargo xtask gate headless` in CI. VP-04/VP-05: `HeadlessBarrier.tla`, `RequestCache.tla` and the proved `barrier::{causal, durable}`, `request::{admit, revision_ok, evict}`. Not qualified on real terminals (P6). |
| P5 / U-01–05 | implemented | `engine/{view,summary,settings,setup,navigation,progress,commands}`, `tui/{paint,stages,stages/results,stages/headlines,modeline,practice,native_text,settings,destinations,progress,theme}`, `runtime/{presenter,text_protocol,config_worker}`, `data/{config,config_write,theme_file,packs}`, `trainer/{import,packs}` (#75–#86): config file and first-run setup, the acknowledged settings overlay with the proved `settings::takes_effect`, layout partitions and the proved `comparison::same_basis`, destinations Today/Train/Numpad/Progress and help from one command table, Progress series with counts, theme files, high contrast and the key guide, numpad operators and separators, sanitised imports and pack sessions kept out of standard scores; render tests at 60×18, 100×32 and 120×40 and PTY evidence (`evidence/releases/0.1.0-beta.3/`). Not qualified on real terminals (P6). |
| P6 / H-05–06 | in_progress | Hermetic Rust regressions and actual PTY/SQLite smoke; no broad terminal qualification or measured p99/memory admission claim. |
| P7 / VP, VK, VL, VR inventory | pending | Packet models remain design probes, now run by the TLC gate in `./ci quality` from `models/claims.toml` (all 3 positive and 5 mutant configurations, pinned TLC 1.7.4; #28); `models/CheckpointSave.tla` maps the shipped save protocol (single-flight, retry, final flush; 4 mutants; #30); `models/FeedbackLoop.tla` maps the cross-session feedback loop (6 invariants, a liveness property, 7 mutants; #40); `models/SessionFlow.tla` maps the session flow (clock, phase window, interlude grace, end early, the typed closing review; 6 mutants), whose state changes all go through the proved `session::step` and status changes through `session::settle` (#31, #49). Lean (Charon/Aeneas) replaces Verus for kernel proofs: the proof inventory for `crates/kernel` is in [docs/TAUTEN.md](docs/TAUTEN.md) (RFC: [2026-11-24-tauten-and-lean-kernel](docs/rfcs/2026-11-24-tauten-and-lean-kernel.md)). VK-03..05 and the VK-06 ladder are proved (`plan::allocate` #34, `select::scan` #35, `lanes::record` #36, `review::ladder_step` #37); VK-06 queue admission is proved (`queue::admit` #55). VP-04/VP-05 are modelled (`models/HeadlessBarrier.tla`, `models/RequestCache.tla`, mapped; beta.2) with their decisions proved (`barrier::{causal, durable}` #71, `request::{admit, revision_ok, evict}` #67); VP-01/VP-02 are modelled (`models/WorkerResults.tla`, mapped; beta.3) with the decision proved (`ticket::admit` #75); VL-01 and VR-01 have no proof (VR-01 replay is tested by six journeys); no TLAPS inventory has passed. |
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
- The word pack (`words-en-v2`, decision 0006, #8) holds 22,000 wordfreq
  forms kept by vetting: each is a SCOWL (size <= 70) dictionary word, not an
  abbreviation, not on the LDNOOBW profanity list or the committed
  hand-reviewed exclusion list (both with common inflections), and not a lone
  letter other than `a`/`i`; a report samples 200 kept and 200 excluded words.
  Automatic lists plus a sampling review are not a guarantee that no
  objectionable word remains, and size 70 admits some rare or archaic words.
  Full upstream notices and CC-BY-SA terms ship
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

