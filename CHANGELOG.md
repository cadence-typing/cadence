# Changelog

User-visible changes per release. Design decisions live in the [RFC registry](docs/rfcs/README.md); requirement-level evidence lives in [STATUS.md](STATUS.md). Every release so far is an **unsigned development preview** for Linux x86_64 (including WSL2).

## [Unreleased]

## [0.1.0-rc.2] — The whole numeric keypad (2026-11-27)

Design: [RFC: full numeric keypad practice](docs/rfcs/2026-11-27-full-numeric-keypad-practice.md), [decision 0010](docs/decisions/0010-full-keypad-by-default.md). Tracking: [#101](https://github.com/cadence-typing/cadence/issues/101).

- **Numpad practice uses the whole keypad by default:** digits, decimals, `+ - * /` and Enter. Each cycle of ten entries serves 3 digit groups (`4821`, `3333`), 2 decimals (`07.50`), 1 signed entry (`-45.20`), 3 operations (`12+7`, `84/2.5`) and 1 chain (`6*3/2`), interleaved in a seeded order. Entries are typed exactly and submitted with Enter, as before; nothing is evaluated. [#103](https://github.com/cadence-typing/cadence/issues/103), [#104](https://github.com/cadence-typing/cadence/issues/104)
- Weak operator keys are targeted: `*`, `+`, `7+` or `*4` can become numpad focus skills and are named on the start screen and in the review ("numpad 7+"). [#103](https://github.com/cadence-typing/cadence/issues/103)
- `content.numpad_operators = false` keeps digits and decimals only. If you had set it to `true` (beta.3's operations-only drill), you now get the mix. [#104](https://github.com/cadence-typing/cadence/issues/104)
- Scoring is unchanged: a mistyped operator is a first-attempt error that stays counted after you correct it, and keypad pace is still correct **digits** per minute, now over time that includes operator keys. So your numpad trend starts again at your first full-keypad session ("content changed here"); words, number-row and prose trends continue unbroken. [#104](https://github.com/cadence-typing/cadence/issues/104)
- The keypad alphabet, the entry grammar (`sign? operand (operator operand)*`), the per-cycle mix and which focus skills each kind of entry can carry are proved kernel decisions: 80 of 80 claims proved, with 10 new mutants that must break them. [#102](https://github.com/cadence-typing/cadence/issues/102)
- Trainer policy v5 (`policies/trainer-v5.toml`). No store schema change: rc.1 profiles open unchanged. [#103](https://github.com/cadence-typing/cadence/issues/103)

## [0.1.0-rc.1] — Evidence you can count once, gates you can rerun (2026-11-27)

Design: [RFC: append-only observation chunks](docs/rfcs/2026-11-26-observation-chunks.md), [RFC: protocol proofs as kernel machines](docs/rfcs/2026-11-26-protocol-proofs-as-kernel-machines.md), [decision 0009](docs/decisions/0009-lean-for-verus-and-tlaps.md). The release candidate: still an unpromoted development bundle (promotion needs #13, #14 and #98).

- **Store schema 7.** Migration is automatic; learning evidence is now stored as append-only observation chunks, one per closed exercise, that the database itself refuses to change or delete. Each checkpoint writes only what is new, so saving no longer gets slower as a session grows, and a retried save can never count evidence twice. Sessions recorded before stay exactly as they were (`legacy`). beta.3 cannot open a schema-7 profile; back up before downgrading. [#11](https://github.com/cadence-typing/cadence/issues/11)
- `cadence doctor --json` reports each session's commit, chunk and derived watermarks; `cadence doctor --rebuild-derived` recomputes your totals from the chunks and lists any that differed; `export` lists every chunk. [#11](https://github.com/cadence-typing/cadence/issues/11)
- `cadence restore --from FILE` restores a backup with the writer closed: it validates the backup before replacing anything, keeps the replaced database as `progress.sqlite3.previous`, and survives being interrupted. Opening a database written by a newer Cadence now says how to recover. [#88](https://github.com/cadence-typing/cadence/issues/88), [#94](https://github.com/cadence-typing/cadence/issues/94)
- The save protocol is a proved kernel machine: single flight, acknowledgements that never go backwards, a named failed tail and a clean exit only after the final acknowledgement hold in every reachable state, not just to a model checker's bound. [#89](https://github.com/cadence-typing/cadence/issues/89)
- Due reviews are served first in, first out by a proved queue: no duplicate is admitted and an item with `n` reviews ahead of it is served by the `(n+1)`-th service. [#90](https://github.com/cadence-typing/cadence/issues/90)
- An assurance manifest binds every handover claim (VP-01..05, VK-01..06, VL-01, VR-01) to its theorems, models and runtime tests, and the `model`, `prove`, `verify` and `correspondence` gates check it. [#91](https://github.com/cadence-typing/cadence/issues/91)
- Gates you can rerun: a real-PTY suite (#92), crash campaigns that kill the app around every checkpoint statement and migration step (#88), measured latency and memory budgets (#15), a fresh-home offline install that rejects tampered and wrong-architecture bundles (#94), and `./ci qualify`, `status` and `resume` with a receipt for every gate attempt (#93). The installer gains `--from DIR` for offline installs and never leaves the `cadence` link dangling.
- Dependencies: a licence and advisory audit in every quality run; `chrono` (unmaintained) is replaced by `jiff`. [#95](https://github.com/cadence-typing/cadence/issues/95)

## [0.1.0-beta.3] — Your settings, your layout, your text (2026-11-27)

Design: [RFC: configuration, keyboard layout and the settings surface](docs/rfcs/2026-11-26-configuration-layout-and-settings.md), [RFC: local imports and code practice](docs/rfcs/2026-11-26-local-imports-and-code-practice.md).

- **Store schema 6.** Migration is automatic; it records each session's layout and content class, and an audit of score-affecting settings. beta.2 cannot open a schema-6 profile (`SchemaTooNew`), so back up before downgrading. Sessions recorded before read as layout `unspecified`, adopted by the first layout you choose. [#77](https://github.com/cadence-typing/cadence/issues/77)
- `~/.config/cadence/config.toml`: typed, validated settings with defaults; flags still win; `cadence config check` prints each value and where it came from, or names the bad line. [#76](https://github.com/cadence-typing/cadence/issues/76)
- A first run asks the layout (US, UK or other), whether you have a numpad and the session length; with no numpad its time goes to the other phases ("numpad off"). [#78](https://github.com/cadence-typing/cadence/issues/78)
- The keyboard layout partitions your history: switching layout keeps letter history and starts number-row symbols fresh, and no comparison ever crosses a layout change. [#77](https://github.com/cadence-typing/cadence/issues/77)
- Settings (`s`): saved only once the file is written; display settings apply at once, anything that affects practice or scores from the next session (a proved rule); an edit made elsewhere is never overwritten, and a bad file keeps your last valid settings. `r` reloads. [#75](https://github.com/cadence-typing/cadence/issues/75), [#79](https://github.com/cadence-typing/cadence/issues/79)
- Destinations Today, Train, Numpad and Progress, with `?` help; every key hint comes from one command table, so none names a key that does nothing. [#80](https://github.com/cadence-typing/cadence/issues/80)
- Progress: per-track trends only across sessions on the same layout and content, with counts, or "not enough comparable sessions"; it says where and why a series stops. [#81](https://github.com/cadence-typing/cadence/issues/81)
- Theme files with eleven roles, high-contrast mode that works without colour, and an optional key guide for numeric practice. [#82](https://github.com/cadence-typing/cadence/issues/82)
- Numpad operator drills and a `,` decimal separator; operator drills are compared only with each other. [#83](https://github.com/cadence-typing/cadence/issues/83)
- `cadence import` sanitises your own text (terminal sequences, controls and bidirectional overrides refused by line and column), and `cadence --pack` practises it, or the shipped Rust and shell packs (Enter types a newline); none of it is compared with or feeds standard practice. [#84](https://github.com/cadence-typing/cadence/issues/84), [#85](https://github.com/cadence-typing/cadence/issues/85)
- **Security:** the bundled SQLite is 3.54.1 (rusqlite 0.41), fixing RUSTSEC-2026-0459 / CVE-2026-119375, where a database opened with a crafted `-wal` file could return uninitialised memory. Every earlier release bundled an affected SQLite; take care opening a profile or backup someone else wrote with an older build. [#100](https://github.com/cadence-typing/cadence/issues/100)
- Headless: all twelve journeys of the test inventory replay in CI, and a tampered trace fails at its first divergence. [#86](https://github.com/cadence-typing/cadence/issues/86)

## [0.1.0-beta.2] — A headless driver with replay (2026-11-26)

Design: [RFC: headless driver and replay](docs/rfcs/2026-11-26-headless-driver-and-replay.md). For tools and tests; practice itself is unchanged.

- `cadence headless --stdio` (or `--script FILE`) drives the same reducer as the terminal app over NDJSON requests, on a profile directory or a throwaway one (`--ephemeral-profile`). Requests carry an id; a repeated id with the same request returns the stored response and never applies twice, a reused id with a different request or an expired one is refused with a stable error code, and an expected `state_revision` that is stale is refused before anything changes. Sessions recorded this way are stored as synthetic and never count toward your history or comparisons. [#67](https://github.com/cadence-typing/cadence/issues/67), [#68](https://github.com/cadence-typing/cadence/issues/68), [#69](https://github.com/cadence-typing/cadence/issues/69)
- `capture` returns the screen as cells, styles, row renditions and cursor, painted by the live renderer; golden frames pin the start, practice, numpad, results, save-error and too-small screens at two sizes. [#70](https://github.com/cadence-typing/cadence/issues/70)
- Barriers: a causal barrier returns only when every job started by the request and its descendants has delivered; a durable barrier also waits for the save acknowledgement at or past the captured watermark. [#71](https://github.com/cadence-typing/cadence/issues/71)
- `--record FILE` writes a trace (practice text sealed by digest unless `--record-content`), and `cadence replay FILE --read-only` re-executes it and reports the first diverging line and field. `cadence doctor --json` prints environment facts. [#72](https://github.com/cadence-typing/cadence/issues/72)
- Six recorded journeys replay in CI (`cargo xtask gate headless`); see [docs/headless-debugging.md](docs/headless-debugging.md). [#73](https://github.com/cadence-typing/cadence/issues/73)
- Request admission, revision checks, cache eviction and barrier readiness are proved kernel decisions, and `RequestCache.tla` and `HeadlessBarrier.tla` model the stream and barrier protocols. [#67](https://github.com/cadence-typing/cadence/issues/67), [#71](https://github.com/cadence-typing/cadence/issues/71)

## [0.1.0-beta.1] — Pace goals, transfer probes and a vetted word pack (2026-11-26)

Design: [RFC: pace targets and benchmark probes](docs/rfcs/2026-11-26-pace-targets-and-benchmark-probes.md), [decision 0006](docs/decisions/0006-word-vetting.md).

- **Store schema 5.** Migration is automatic; it adds pace goals, transfer probes and each session's provenance. alpha.7 cannot open a schema-5 profile (`SchemaTooNew`), so back up before downgrading. [#64](https://github.com/cadence-typing/cadence/issues/64)
- Each typing track has a pace goal, starting from your own qualifying blocks: "Words goal 52 WPM · raised". It rises at most 3% after clean blocks over several sessions and falls at most 5% after a sloppy block; after a lowering, half the next session's target words are familiar ones. The decision is a proved kernel function. [#61](https://github.com/cadence-typing/cadence/issues/61), [#62](https://github.com/cadence-typing/cadence/issues/62)
- After every seventh completed session with prose, a 2-minute transfer probe on unseen, purpose-written text ("Transfer probe · unseen text · not practice") measures how your skill carries over; the results compare it only with your previous comparable probe and say when one was paused or ended early. [#63](https://github.com/cadence-typing/cadence/issues/63), [#64](https://github.com/cadence-typing/cadence/issues/64), [#65](https://github.com/cadence-typing/cadence/issues/65)
- The word pack is vetted: 22,000 dictionary words without abbreviations, profanity or reviewed slurs. Words you have history on keep their identity; a due word that left the pack is named once and dropped. Plans change. [#8](https://github.com/cadence-typing/cadence/issues/8)

## [0.1.0-alpha.7] — Deliberate practice in every phase (2026-11-26)

Deliberate practice in every phase. Design: [RFC](docs/rfcs/2026-11-26-deliberate-practice-in-every-phase.md).

- **Store schema 4.** Migration is automatic; it adds the session's UTC offset and a `review` track. alpha.6 cannot open a schema-4 profile (`SchemaTooNew`), so back up before downgrading. [#45](https://github.com/cadence-typing/cadence/issues/45)
- Days follow your local calendar, so the review ladder's "tomorrow" is tomorrow where you are; `--utc-offset +HH:MM` overrides the detected zone, and the start screen says when none was found. [#52](https://github.com/cadence-typing/cadence/issues/52)
- The number row and the numpad each target their own weak digits and digit pairs (policy v3): two of every three entries contain one ("Targeting 7 · 48"); a missed entry comes back once, at least two entries later in the same phase ("again"). [#46](https://github.com/cadence-typing/cadence/issues/46), [#5](https://github.com/cadence-typing/cadence/issues/5), [#59](https://github.com/cadence-typing/cadence/issues/59)
- Warm-up checks familiar words you have recovered ("Familiar words · 2 retention checks"); a word missed in warm-up returns in the words phase. [#47](https://github.com/cadence-typing/cadence/issues/47)
- A target word does not recur within four exercises. [#58](https://github.com/cadence-typing/cadence/issues/58)
- Prose stays untargeted and says so; a slip in a word that is in the word pack now counts for that word and returns in the closing review. [#48](https://github.com/cadence-typing/cadence/issues/48)
- The last minute is a typed closing review of this session's misses (then "no misses left · extra words"), with its own results row that never enters the headline or like-for-like comparisons; ending during it still records the session as completed. [#49](https://github.com/cadence-typing/cadence/issues/49), [#60](https://github.com/cadence-typing/cadence/issues/60)
- Every repeat is labelled: "again", "due review", "repeat: word", "repeat · list restarted". [#7](https://github.com/cadence-typing/cadence/issues/7)
- The start screen, interludes, numeric captions and results say what each phase targeted; "Most slips" covers every lane. [#50](https://github.com/cadence-typing/cadence/issues/50)
- Plans for a given seed differ from alpha.6.

## [0.1.0-alpha.6] — Proved practice decisions (2026-11-26)

Practice decisions (phase split, word choice, quotas, the review ladder, the reinforcement queue, slot substitution, labels, the learner's focus and its evidence) are now proved kernel functions that the app calls. No schema change. **Plans for a given seed differ from alpha.5** (broad words are sampled without replacement and the learner model computes in integers), so an alpha.5 seed does not reproduce its alpha.5 plan. Design: [RFC: practice decisions in the kernel](docs/rfcs/2026-11-26-practice-decisions-in-the-kernel.md), [RFC: evidence and repeat-gap decisions](docs/rfcs/2026-11-26-evidence-and-repeat-gap-decisions.md), [RFC: a slow save pauses practice](docs/rfcs/2026-11-26-slow-save-pauses-practice.md).

### Added

- One-line install: `curl -fsSL https://cadence-typing.github.io/install.sh | sh`. It verifies the release checksum, installs for the current user only, never uses `sudo`, and never touches your practice history. `--version`, `--prefix`, `--bin-dir` and `--uninstall` are supported.
- Public home at [cadence-typing.github.io](https://cadence-typing.github.io) and [github.com/cadence-typing/cadence](https://github.com/cadence-typing/cadence), with issues, milestones and an RFC process ([GOVERNANCE.md](GOVERNANCE.md)).

### Verification

- Releases are now built in public CI from a version tag: the quality gate, two clean builds that must produce identical checksums, and the Lean proof gate run before a pre-release is published and test-installed. Bundles add `BUNDLE.txt` with their reproducibility inputs. [#3](https://github.com/cadence-typing/cadence/issues/3)

- Scoring decisions moved into a `no_std` kernel crate that production calls. 19 Lean theorems over its Aeneas translation are proved, each calibrated by mutants that must break it, covering first-attempt write-once, counters, pace and accuracy arithmetic, active time, track validity and checkpoint admission. No score, timing or saved data changed. [#2](https://github.com/cadence-typing/cadence/issues/2)
- The workspace follows the tauten strictness profile (`cargo tauten check` passes with no baseline). Release builds now enable overflow checks.
- The TLA+ protocol models now run in the quality gate on every change, with their faulty variants required to fail on the named invariant. A new model of the shipped checkpoint save protocol (one save in flight, retry after failure, a clean exit only after the final save is acknowledged) is mapped to the code that implements it. [#28](https://github.com/cadence-typing/cadence/issues/28), [#30](https://github.com/cadence-typing/cadence/issues/30)

- A model of the cross-session feedback loop (observe, save, end or crash, replay through the review ladder, plan and serve the due words) now runs in the quality gate: no miss is lost, nothing promotes without two clean days, a same-session retype never promotes, saved evidence is counted once, the reinforcement queue never takes a due word's slot or holds a word twice, and a due word is served unless the session is killed. [#40](https://github.com/cadence-typing/cadence/issues/40)

### Fixed
- A slow save no longer ends the session. When a save is not acknowledged within 2 seconds, practice pauses with "Saving is slow; the clock is stopped" and resumes by itself when the save completes; a failed save takes the usual retry path, and a pause you made yourself stays paused. Before, the session was marked invalid with no qualified score although the save later succeeded. [#51](https://github.com/cadence-typing/cadence/issues/51)
- Broad and cold-start word slots no longer repeat words within minutes: they take the next unplanned word of the seeded, shuffled pack instead of redrawing from a sliding window of 256. Plans for a given seed change. [#44](https://github.com/cadence-typing/cadence/issues/44)
- A transition now counts as correct only if both of its keys were: typing `x`, Backspace, `t`, `h` records `th` as a miss, as the learning spec says. Before, a slip on the first key of a pair was invisible to the transition. Learner evidence only; scores are unchanged. [#42](https://github.com/cadence-typing/cadence/issues/42)
- A timing sample never spans a pause (requested, focus loss, paste, resize or a failed save) and two keys at the same tick are not timed. Before, the time before and after a pause was joined into one interval. Learner evidence only. [#41](https://github.com/cadence-typing/cadence/issues/41)
- The learner model that picks what each session targets now computes in integers with proved kernel functions instead of floating point: older sessions' evidence halves every 7 sessions (linearly in between), the error estimate, hesitation and the focus ranking are exact, and the same history gives the same plan on every platform. A focus near a threshold may differ by a session from before. [#38](https://github.com/cadence-typing/cadence/issues/38), [#56](https://github.com/cadence-typing/cadence/issues/56)
- A missed word re-served in the same session no longer appears twice in one exercise or takes the place of a due review: it goes to a free review slot, else a broad slot, and otherwise waits for the next exercise. [#43](https://github.com/cadence-typing/cadence/issues/43)
- Exercise labels ("due review", "again: word", a prose "repeat") and cycling through a phase's prepared exercises are now proved kernel decisions; the app now also records when a phase's list starts over (shown from #7). Captions are unchanged. [#39](https://github.com/cadence-typing/cadence/issues/39)
- Whether a missed word is queued for one more repetition in the same session (not already queued, queue not full, the word's repetitions not used up) is now a proved kernel decision. Behaviour is unchanged. [#55](https://github.com/cadence-typing/cadence/issues/55)
- The review ladder (an error returns a word to the 1-day rung; clean typing before the word is due is practice; two clean days on or after the due day promote one rung; the top rung does not wrap; a completion dated before the counted clean day is refused) is now a proved kernel decision. Due words are unchanged. [#37](https://github.com/cadence-typing/cadence/issues/37)
- Block quotas and repetition caps (6 target, 2 review, 2 broad slots; at most twice per block and twice in a row per word; at most three target slots per focus) are now one proved kernel decision used both to choose and to record a slot, instead of two copies. Plans are unchanged. [#36](https://github.com/cadence-typing/cadence/issues/36)
- Which word fills a practice slot (the eligible candidate with the highest weighted utility, then the seeded tie-break, then the word id) is now decided by a proved kernel function. The same seed and history give the same plan as before. [#35](https://github.com/cadence-typing/cadence/issues/35)
- The phase split of a session (each phase's minimum, then the remaining seconds by weight, leftover seconds to the largest remainders) is now a proved kernel function: the seconds always sum to the budget, every phase keeps its minimum and an infeasible budget is reported. The split is unchanged. [#34](https://github.com/cadence-typing/cadence/issues/34)
- Every decision in a typing step (whether a key is admitted under the buffer bound, where it lands, the write-once first outcome, the per-word flags and completion) is now a proved kernel function. No score changed. [#32](https://github.com/cadence-typing/cadence/issues/32)
- Exercise counters are merged into a track by a proved kernel function, and an exercise can be credited only once (checked on every merge). No score changed. [#33](https://github.com/cadence-typing/cadence/issues/33)
- Every session state change (start, pause, resume, deadline, next phase, end early, finish) now goes through one proved decision table. Behaviour is unchanged; a failed save during the review or the final save now keeps that state instead of an internal paused state. [#31](https://github.com/cadence-typing/cadence/issues/31)
- Active time and the phase deadline are now decided by proved kernel functions that the app calls. Previously the proved active-time function had no production caller and the app used its own copy. No score or timing changed. [#29](https://github.com/cadence-typing/cadence/issues/29)

- The TLC model-check logs pinned by `MANIFEST.sha256` are now committed, so contract verification passes on a fresh clone.

## [0.1.0-alpha.5] — Calm flow and honest results (2026-11-14)

- Switching windows or pasting pauses practice and says why, instead of invalidating the session. A paste inserts nothing.
- Typing far past an uncorrected error is refused ("Past the end · Backspace to the first error") instead of invalidating the session.
- End early and still see your results: `e` in the pause controls or Ctrl-C during practice opens the review, and the session is saved as unfinished.
- Enter or Esc within 0.7 s of a phase-complete screen opening is ignored, so the last numpad Enter cannot skip it.
- The review answers "how did I do": per-track accuracy, pace, corrections and the change since your last comparable session, plus your most frequent slips.
- Quieter chrome; `--theme dark|light|terminal`. A missed word that returns is named; prose exercises end at sentence boundaries.
- No schema change. Design: [RFC](docs/rfcs/2026-11-14-calm-flow-and-honest-results.md).

## [0.1.0-alpha.4] — Learning from mistakes (2026-11-13)

- Every first attempt is recorded per key and key transition, with clean inter-key timing. A corrected mistake stays an error.
- The words phase targets up to six weak transitions once evidence clears explicit thresholds. Missed words return later in the session and then on a 1/3/7/14/30-day review ladder.
- Prose grew to 2,643 passages from 19 public-domain books plus original passages, with a 200-session no-repeat window.
- Each session has its own persisted seed, so plans differ and can be replayed.
- **Store schema 3.** Migration is automatic; alpha.3 cannot open a schema-3 profile, so back up before downgrading. Design: [RFC](docs/rfcs/2026-11-13-learning-from-mistakes.md).

## [0.1.0-alpha.3] — Smooth text and session headlines (2026-10-19)

- Native 2× terminal text (DEC double-height rows) replaces bitmap lettering, with a probed fallback to centered normal text.
- Large correct-speed and first-pass accuracy tiles on the review. Accuracy aggregates positions and never rounds an error up to 100.0%. Design: [RFC](docs/rfcs/2026-10-19-smooth-native-text.md).

## [0.1.0-alpha.2] — Visual focus (2026-10-19)

- Centered landing, session route, between-phase cards and per-track review; a segmented modeline.
- F2 and `--text-size auto|large|normal`. Below 60×18, practice pauses. Design: [RFC](docs/rfcs/2026-10-19-visual-focus.md).

## [0.1.0-alpha.1] — First static Linux build (2026-10-18)

- A 15-minute guided session with warm-up, words, number row, numpad and prose; shorter presets and single-track practice.
- First-attempt errors survive correction; precise active time, where pauses and transitions are excluded but hesitation is not.
- Local SQLite progress with checkpoint recovery, a writer lock, `backup` and `export`.
- A statically linked musl binary with embedded content; no runtime libraries required.

[Unreleased]: https://github.com/cadence-typing/cadence/compare/v0.1.0-alpha.5...HEAD
[0.1.0-alpha.5]: https://github.com/cadence-typing/cadence/releases/tag/v0.1.0-alpha.5
[0.1.0-alpha.4]: https://github.com/cadence-typing/cadence/releases/tag/v0.1.0-alpha.4
[0.1.0-alpha.3]: https://github.com/cadence-typing/cadence/releases/tag/v0.1.0-alpha.3
[0.1.0-alpha.2]: https://github.com/cadence-typing/cadence/releases/tag/v0.1.0-alpha.2
[0.1.0-alpha.1]: https://github.com/cadence-typing/cadence/releases/tag/v0.1.0-alpha.1
