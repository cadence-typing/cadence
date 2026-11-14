# 0004 — Calm flow and results that answer "how did I do" (alpha.5)

Status: accepted by the owner's request of 2026-11-14 ("critique the TUI and
the flow… anything you adopt, plan and execute, cut a release and install").

## Findings adopted (independent TUI/flow review of alpha.4, `e08bfc7`)

Captured from the installed binary in private PTYs at 100x30, 80x24 and
60x18 with throwaway profiles.

1. Focus loss or a paste turns the session `INVALID` for good; the footer
   still says "Esc resumes" and the notice leaks "invalid Engine:".
2. Typing on past an uncorrected error fills the extra-input allowance and
   then invalidates the session ("typed input exceeded the bounded buffer").
3. There is no way to end early and still see results; Ctrl-C abandons
   straight to the shell and prints nothing. Pause is a single footer line,
   not the U-03 practice controls, and the target stays readable while the
   clock is stopped.
4. The Enter that submits the last numpad entry can land after the deadline
   and silently dismiss the phase-complete screen.
5. Results show raw fractions (`1867/1935 1935/2003`) without percentages, do
   not compare with an earlier session, do not say which skills slipped, and
   omit the focus skills plan 0003 promised there. The interlude shows
   "First-pass 542/553 Attempts 553/564".
6. The modeline gives a bold `CKPT n` the most emphasis, shows a one-decimal
   live WPM that jumps every keystroke (against P-05), and an `ex 1/59`
   denominator nobody reaches.
7. Upcoming text — the text being read ahead — is painted Muted *and* DIM;
   the light and terminal palettes are unreachable.
8. Permanent footer boilerplate; a missed space reads "expected ‘ ’".
9. "missed earlier, again" does not name the word; targeted blocks repeat
   words ("ain't … ain't", "thinks … thinks").
10. The start screen does not say how the last session went.
11. Prose chunks end mid-clause and swap every few seconds.

## Changes

Reducer/flow (engine):
- Focus loss and paste become an ordinary pause with a typed
  `PauseReason` (`Requested`, `FocusLost`, `PasteRejected`); the paste is
  not inserted, the exercise is not interrupted and no score is changed.
  Invalidation remains for real invariant failures; its notice names the
  reason without the internal error kind, and the controls never offer a
  resume that is not available.
- Input beyond the extra-input bound is refused (not recorded), never
  session-fatal. Extra units past the target render as a distinct
  `UnitSpan::Extra` and the footer says "Past the end · Backspace to the
  first error".
- End early: in pause, `e` ends the session and opens the review; Ctrl-C
  during practice or an interlude does the same. Status is recorded as
  `abandoned` (unfinished), the review says "Ended early" and the open
  exercise is marked interrupted exactly as before. Enter, Esc or Ctrl-C on
  the review saves and exits. SIGTERM still exits directly.
- The pause controls replace the target while paused (no pre-reading on a
  stopped clock) and list the available keys from one typed binding table
  (`commands.rs`) that also drives the footer hints.
- Phase-complete ignores Enter/Esc for a named grace interval after it
  opens, so a keystroke already in flight cannot skip it.
- After the terminal is restored, one recap line is printed.

Results and history:
- `HistorySummary` carries a bounded list of recent completed sessions'
  exact track totals (already stored; no schema change). The review
  compares with the most recent completed session with the same track set
  ("+2.1 WPM · −0.6 pts vs yesterday", or "no comparable session yet").
  Never blends numpad into WPM; never compares different track sets.
- Per-track rows show first-pass accuracy %, correct pace, corrections and
  the change against that track in the most recent session that had it;
  raw fractions remain in a detail column on wide terminals.
- One observation sentence from this session's recorded first attempts:
  "Most slips: th 9/41 · ou 6/30" plus the plan's focus skills.
- Interlude: "98.0% first-pass · 54 WPM · 11 corrections"; a deadline in
  mid-exercise reads "Time's up — the partial line still counts".
- Start screen: "Last session · 54 WPM · 97.0% · yesterday".

Chrome:
- Durability is a quiet "saved"/"local"; only a failure or an unsaved
  volatile run is emphasised. Live pace and accuracy are integers for the
  current track's completed exercises, updated at exercise boundaries.
  "phase 2/5" replaces the exercise denominator; the header shows the
  route on practice screens; "FP" → "acc"; one "F2 text size" hint.
- Upcoming text uses readable Text colour without DIM; typed text uses the
  Correct role. `--theme dark|light|terminal` selects the palette.
- First-session-only "Follow the caret" footer; "Enter submits" on numpad;
  "expected space" for a missed space.

Content:
- Reinforced words are named ("again: other's").
- `maximum_word_occurrences_per_block = 1` and consecutive = 1 in
  `trainer-v2.toml` (the corpus capacity gate re-checks it).
- Prose chunks prefer a sentence end, then a clause end, then a word
  boundary, never below half the bound; `prose_exercise_units` 120 → 160.

## Not adopted

- Skip-phase from the pause controls: it would change the plan mid-session
  without a persisted record; `--track` already selects tracks.
- Next-line preview for words (conflicts with boundary-time reinforcement).
- Theme picker UI and the four U-01 destinations.

## Verification

Engine tests: focus loss and paste pause and resume without invalidation;
over-bound input refused and session still completes; `e` and Ctrl-C reach
the review with abandoned status, then exit after the final ack; interlude
grace; comparison selection (same track set only). TUI render tests at
100x30, 80x24, 60x18 for the pause panel, review deltas and slips. Trainer
tests: sentence-aware chunking keeps every unit and respects bounds; blocks
contain no repeated word. Canonical offline Compose quality, static musl
dist, GitHub prerelease, checksum-verified atomic install, PTY smoke on a
copy of the owner's profile. No schema change.

## Integration evidence (alpha.5 delivery)

- Clean source `b5917366380fceac6ac56b77a436b9c9a456d69f`; canonical offline
  Compose quality passed (fmt, strict Clippy, 188 tests, boundaries, 48
  contract hashes). Static musl bundle: binary SHA-256
  `d58b25079c090bbd79840a24c962964006a2c9280d59497efcf629b43462407d`.
- PTY runs of the candidate on a copy of the owner's profile: focus loss and
  paste paused with their reasons and hid the target; Ctrl-C opened the
  review ("ended early, saved as unfinished") at 100x30, 80x24 and 60x18;
  Enter saved and printed the recap line. Two full 5-minute sessions showed
  every interlude (the 0.7 s grace absorbed the driver's in-flight Enter),
  named re-served words, per-track and like-for-like changes, the slips
  sentence, and the next start screen's last-session line. A prose-only run
  showed chunks ending at sentence boundaries with attribution.
- The PTY run exposed "−0.0 pts" for a sub-precision drop and clipped review
  lines at 80 and 60 columns; both were fixed with tests before release.
- Uploaded assets were downloaded, checksum-verified (tarball binary
  identical to the standalone asset), installed at
  `~/.local/lib/cadence/0.1.0-alpha.5-b5917366380f/` and activated by an
  atomic symlink swap after `cadence backup` of the owner's profile. The
  installed binary exported it at schema 3 with all 11 sessions.
