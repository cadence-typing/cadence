---
rfc: "2026-10-19-visual-focus"
title: "Centered visual focus and large practice text"
status: accepted
implementation: complete
authors:
  - tknawara
created: 2026-10-19
updated: 2026-11-24
issue: null
milestone: null
supersedes: []
superseded_by: []
blocked_on: []
legacy_id: "plan/0001"
---

> Backfilled on 2026-11-24 from `plans/0001-*.md`; shipped in `0.1.0-alpha.2`. The text below is the original record and describes its time. Superseded in part: the bitmap typography was replaced by [smooth native text](2026-10-19-smooth-native-text.md). The centered layout, route and modeline remain in force.

# Visual focus: alpha.2

Historical status: shipped as `0.1.0-alpha.2`. Its bitmap typography is superseded
by [smooth native text in alpha.3](2026-10-19-smooth-native-text.md).
Scope: terminal UI only, plus typed display preferences and release/install.
Training policy, target identity, scoring, phase allocations and persisted
history schema remain unchanged. This is still an unsigned feedback build.

## Research and decisions

1. **Landing and inter-phase screens.** Replace the left-aligned diagnostic
   layout with a centered composition: Cadence wordmark, one strong heading,
   session route/timeline, concise phase purpose, useful history/result facts,
   and one keyboard call to action. A transition card celebrates the completed
   phase, not a fabricated learning gain. It shows the finished phase's actual
   first-pass/attempt counts and corrections, then previews the next phase.
   Summary follows the same visual language. Thin borders, mint focus,
   charcoal panels, readable neutral text; no animation during practice.
2. **Modeline.** Borrow the segmentation, left/right groups and width-dependent
   information priority of [oh-my-pi's presets][omp] and [Helix's statusline][hx].
   Use a clear mode badge (READY / TYPE / PAUSED / NEXT / REVIEW / SAVING / ERROR),
   phase context, first-pass accuracy, correctly measured throughput, active or
   remaining time, and durability. Labels derive from typed engine facts,
   never by parsing notice strings. At 60 columns, retain mode, phase/time and
   save failure first; optional metrics disappear by a documented priority.
   A separate quiet hint row advertises only currently available actions.
   No private-use Powerline/Nerd Font characters or copied upstream code.
3. **Large centered writing.** Native font size belongs to the terminal.
   [Kitty OSC 66][kitty] provides true per-region scaling from Kitty 0.40, but
   requires negotiated terminal support and special multicell overwrite rules.
   There is no evidence of that capability on this Windows-Terminal/WSL setup;
   environment hints are not a support test. [Windows Terminal zoom][wt] scales
   the pane, not a portable application-owned region. Do not change the user's
   terminal settings or silently emit unsupported escape sequences.

   Ship a portable **large-cell mode** instead: the established
   [font8x8 0.3.1 glyph tables][font] drawn as 4×4 terminal-cell characters using
   standard quadrant block elements. This changes visual size, not the
   terminal's font setting. Preserve lowercase, uppercase, punctuation and £.
   Unsupported glyphs or insufficient geometry fall back to normal text,
   explicitly rather than drawing blanks. A small normal-size context line
   provides legibility and the exact active text alongside the large canvas.

   [tui-big-text 0.7][big] demonstrates this technique. Its implementation
   uses BASIC_FONTS only and builds/clones styled line vectors. Cadence will use
   its underlying established font8x8 data directly with a small bounded
   painter: no per-glyph string allocation, no homegrown font tables, no terminal
   parser or graphics protocol. Dependency purpose: portable typography;
   version pinned, licence/provenance included by the existing bundle producer;
   update/security ownership stays with the Cadence maintainer.

## Layout contract

- Default `--text-size auto`: large at ≥80×24 when the practice body is at
  least 76×18 and holds two complete glyph rows; otherwise centered normal.
  A diagnostic banner can reduce the available body and select normal text.
- `--text-size large|normal` selects a preference. Large still falls back safely
  when geometry/glyph support cannot satisfy the contract. F2 cycles
  Auto → Large → Normal without changing scores, target or clocks.
- Both render sizes center the visible text block horizontally and vertically.
  Wrap on expected-target word boundaries, not mistyped characters; never
  manufacture extra input spaces. Oversized words may wrap at unit boundaries.
- Long targets keep the caret and nearby context visible. Paging does not imply
  text completion and never re-generates the target. The last complete numpad
  entry remains visible while waiting for Enter.
- Correct/error/cursor/upcoming units remain distinct. Error meaning includes
  a shape/underline plus an explicit expected-character hint, not colour alone.
- Shared painter remains a pure function of immutable PreparedView + palette +
  geometry; native and TestBackend use the same implementation.
- 100×32 reference, 120×40 wide, 60×18 compact; 59×18 / 60×17 / 1×1 show the
  small-terminal fallback. Resize admission pauses practice below the minimum;
  returning to a supported size requires explicit safe resume, preserving input.
- A persistent 2-row modeline/hints region replaces the repeated status/footer.
  Save errors/invalidation notices remain visible; no decorative layout may
  conceal an unacknowledged write or advertise a successful save prematurely.

## Engine and module boundaries

- `engine/view.rs`: typed display preference, UI mode, durability, ordered phase
  overview and actual phase/summary numbers. `presentation.rs` prepares these
  bounded values. F2 and CLI preference are admitted through the reducer.
- `tui/stages.rs`, `tui/stages/results.rs`: landing/interlude and summary.
- `tui/modeline.rs`: segmented width-aware status and action hints.
- `tui/practice.rs`, `tui/glyphs.rs`: centered target layout and portable glyphs.
- `tui/paint.rs`: composition only; semantic colours stay in `theme.rs`.
- `runtime/live.rs`: F2 normalization; `app/cli.rs`: validated `--text-size`.
  No terminal font configuration writes and no changes to data storage.

## Verification and delivery

Exercise all screens through the actual shared reducer/painter and the actual
binary in private PTYs. Inspect rendered cell captures at reference/wide/minimum
sizes plus errors, pause, numpad awaiting Enter, transition and save failure.
Prove size changes/resizes preserve target, input, first outcomes and timing;
exercise unsupported-glyph/tiny-viewport fallback. Run existing regressions,
fmt and strict Clippy after integration. Do not substitute source-text tests or
assertions about copied colour defaults for real behaviour.

Build the static musl `0.1.0-alpha.2` bundle with unchanged schema 2. Verify no
PT_INTERP/DT_NEEDED, licence notices/checksums, private-profile save/reopen, and
network-disabled non-root execution. Push coherent checkpoints; publish a new
prerelease without overwriting alpha.1. Download/verify the published bytes and
atomically point the local executable at the versioned install, retaining the
previous installation. Record actual evidence and limitations after smoke.

[omp]: https://github.com/can1357/oh-my-pi/blob/main/packages/tui/src/status-line/presets.ts
[hx]: https://docs.helix-editor.com/editor.html#editorstatusline-section
[kitty]: https://sw.kovidgoyal.net/kitty/text-sizing-protocol/
[wt]: https://learn.microsoft.com/en-us/windows/terminal/customize-settings/profile-appearance
[font]: https://docs.rs/font8x8/0.3.1/font8x8/
[big]: https://docs.rs/tui-big-text/0.7.0/tui_big_text/

## Execution evidence

- Implemented one shared stage/practice/modeline painter and reducer-owned
  display preferences. Summary is separated by responsibility in
  `stages/results.rs`; glyph painting and target wrapping avoid per-glyph
  strings and per-frame row-vector allocations.
- Native debug-binary PTY captures inspected landing, large and normal practice,
  corrected input, pause/resize recovery, keypad submission and save failure at
  100×32 and 60×18. A real SQLite rejection showed ERROR / SAVE ERROR and retry
  controls; removing the rejection and pressing `r` produced an acknowledged
  checkpoint. Corrected `9494` persisted first-pass 3/4, attempts 4/5 and one
  deletion, unchanged from alpha.1 semantics.
- Production reducer/shared-painter captures inspected interludes and actual
  per-track summary at reference, wide and compact sizes. A throwaway driver
  exercised 80×23 normal versus 80×24 large, 59×18 / 60×17 / 1×1 fallback,
  and renderer-only unsupported-glyph fallback. The latter is a boundary fixture,
  not a claim that this ASCII-first preview admits arbitrary Unicode.
- Captures before and after removing the row allocation were byte-identical.
  The throwaway Rust capture driver was removed after these smoke runs.
- Native font-scaling capability, the owner's particular terminal font, and
  physical-keypad provenance remain unqualified. Software checks do not prove
  human learning gains. Release/install receipts accompany the prerelease.

- Canonical Docker Compose `./ci prepare` and offline `./ci quality` passed:
  formatting, warnings-denied Clippy, workspace tests, architecture boundaries
  and verification of all 48 original contract files against the manifest.

## Release and installation receipt

- [Published prerelease](https://github.com/tknawara/cadence/releases/tag/v0.1.0-alpha.2),
  including static executable, notice-complete tarball, checksums and three
  actual released-binary terminal captures.
- Clean candidate: `5f932654ab8812360a4b167067c1b3b639624db5`.
  Binary SHA-256:
  `dd7236b3cffb446ee6f97f1bab574ed083a6df59dac9e7222f6fe2b4271f65b3`.
  `./ci dist` verified ELF64/x86_64 with no interpreter or shared-library needs.
- Offline Compose quality: 139 passing tests, formatting and strict Clippy.
  Static-binary native smoke observed a natural 24-second warm-up transition;
  SQLite retained exactly 24,000,000 active microseconds and its partial facts.
- The same binary practised and saved in a scratch container as UID/GID 1000,
  with no networking, read-only root and all capabilities dropped. Corrected
  keypad counts remained 3/4 first-pass, 4/5 attempts, one deletion, schema 2.
- Uploaded candidate assets were downloaded and checksum-verified before
  installation and public publication. The installed executable was exercised
  against the temporary existing profile: history appeared, F2 worked, and
  earlier counts remained intact after closing.
- Active link: `~/.local/bin/cadence` →
  `~/.local/lib/cadence/0.1.0-alpha.2-5f932654ab88/cadence`.
  Activation was atomic; `0.1.0-alpha.1-32ebebc99dc8` remains available.
  All practice smoke runs used explicit temporary profiles and homes, never
  the owner's normal history.

