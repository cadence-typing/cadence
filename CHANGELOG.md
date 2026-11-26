# Changelog

User-visible changes per release. Design decisions live in the [RFC registry](docs/rfcs/README.md); requirement-level evidence lives in [STATUS.md](STATUS.md). Every release so far is an **unsigned development preview** for Linux x86_64 (including WSL2).

## [Unreleased]

### Added

- One-line install: `curl -fsSL https://cadence-typing.github.io/install.sh | sh`. It verifies the release checksum, installs for the current user only, never uses `sudo`, and never touches your practice history. `--version`, `--prefix`, `--bin-dir` and `--uninstall` are supported.
- Public home at [cadence-typing.github.io](https://cadence-typing.github.io) and [github.com/cadence-typing/cadence](https://github.com/cadence-typing/cadence), with issues, milestones and an RFC process ([GOVERNANCE.md](GOVERNANCE.md)).

### Verification

- Scoring decisions moved into a `no_std` kernel crate that production calls. 19 Lean theorems over its Aeneas translation are proved, each calibrated by mutants that must break it, covering first-attempt write-once, counters, pace and accuracy arithmetic, active time, track validity and checkpoint admission. No score, timing or saved data changed. [#2](https://github.com/cadence-typing/cadence/issues/2)
- The workspace follows the tauten strictness profile (`cargo tauten check` passes with no baseline). Release builds now enable overflow checks.
- The TLA+ protocol models now run in the quality gate on every change, with their faulty variants required to fail on the named invariant. A new model of the shipped checkpoint save protocol (one save in flight, retry after failure, a clean exit only after the final save is acknowledged) is mapped to the code that implements it. [#28](https://github.com/cadence-typing/cadence/issues/28), [#30](https://github.com/cadence-typing/cadence/issues/30)

- A model of the cross-session feedback loop (observe, save, end or crash, replay through the review ladder, plan and serve the due words) now runs in the quality gate: no miss is lost, nothing promotes without two clean days, a same-session retype never promotes, saved evidence is counted once, the reinforcement queue never takes a due word's slot or holds a word twice, and a due word is served unless the session is killed. [#40](https://github.com/cadence-typing/cadence/issues/40)

### Fixed
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
