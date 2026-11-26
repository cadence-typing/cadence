---
rfc: "2026-10-19-smooth-native-text"
title: "Smooth native 2× terminal text"
status: accepted
implementation: complete
authors:
  - tknawara
created: 2026-10-19
updated: 2026-11-24
issue: "https://github.com/cadence-typing/cadence/issues/22"
milestone: "0.1.0-alpha.3"
supersedes: []
superseded_by: []
blocked_on: []
legacy_id: "plan/0002"
---

> Backfilled on 2026-11-24 from `plans/0002-*.md`; shipped in `0.1.0-alpha.3`. The text below is the original record and describes its time. It amends the typography section of [visual focus](2026-10-19-visual-focus.md).

# Smooth native text: alpha.3

Status: released and installed as `0.1.0-alpha.3`, including the requested summary tiles.

The owner rejected alpha.2's coarse 8×8 bitmap lettering. Replace it, rather
than smoothing those pixels or imposing a terminal font/profile change.

## Decision and evidence

Use DEC double-height/double-width rows: `ESC #3` for the upper half, `ESC #4`
for the lower half, `ESC #5` to return a row to normal. The same real text and
styles occupy adjacent halves. The terminal rasterizes its own outline font
at 2× size. These are whole-row attributes, not arbitrary regional font zoom.

- [VT510 DECDHL specification](https://vt100.net/docs/vt510-rm/DECDHL.html):
  paired rows, identical characters, half-width logical column space.
- [Windows Terminal support](https://github.com/microsoft/terminal/issues/11595):
  shipped in Preview 1.17.1023, with ConPTY propagation and native rendering.
- [Windows Terminal rendering pipeline](https://github.com/microsoft/terminal/blob/main/src/renderer/atlas/README.md):
  DirectWrite glyph runs and native double-height glyph halves.
- Installed Windows Terminal reports `1.24.261001001`. A separate, disposable
  terminal window showed genuinely smooth normal/mint/error text. Its native
  PrintWindow client capture was inspected, not simulated with enlarged cells.
  At 73 columns, cursor reports were column 73 normally and column 36 for each
  double-height half, exactly the required odd-width clamp.

Sixel would add raster assets, font packaging and image lifecycle complexity.
Kitty OSC 66 is not available universally. Native DEC rows solve this machine's
actual problem with its own font and no graphics transport or new dependency.
Remove the bitmap renderer and `font8x8` completely; do not retain a retro mode.

## Implementation contract

- Probe the empty alternate screen at startup. Require a normal-width cursor
  report and the expected logical half-width report for **both** native halves.
  Use Crossterm's existing parser and bounded cursor query, not a custom parser.
  Restore the probe row after success, unsupported replies, timeout or failure.
  Environment variables alone are not capability evidence.
- Admit the capability as typed `SetTextSupport` through the reducer, independent
  of `TextSize`. Normal-only before admission; never alter target, first outcomes,
  clocks, difficulty, checkpoints or schema because typography changes.
- One shared pure painter produces Ratatui cells **and row renditions**. Native
  text is literal text, not block characters. Normal-sized chrome remains normal.
  Partial-width viewport painting cannot change whole-row attributes.
- A narrow retained presenter uses Ratatui's wide-cell diff and the established
  Crossterm backend. Invalidate/redraw a row when its rendition changes, filter
  hidden logical columns, reset attributes on resize and before alternate-screen
  teardown. No per-frame row allocation and no separate demo rendering path.
- Auto selects native text when detected and two complete rows fit; its smaller
  2× geometry no longer needs the bitmap mode's 80×24 threshold. `--text-size`
  and F2 stay available. Unsupported terminals use centered ordinary text with
  an explicit native-scaling fallback hint, never a pixelated approximation.
- Delete the duplicate small context line needed to decipher bitmap glyphs.
  Keep error guidance, caret visibility, Enter submission and pause/save hints.

## Verification and delivery

Exercise real Cadence in the same dedicated Windows Terminal: landing, practice,
correction, F2 native/normal, pause, resize and phase boundaries. Inspect native
client-area screenshots; verify line attributes do not leak into chrome or the
shell. Exercise unsupported queries and honest corrected-entry persistence in
isolated PTYs. Keep regression coverage for protocol state changes and capability
admission; do not replace real terminal rendering with screenshot-text tests.

Run canonical offline Compose quality and static musl packaging, retain source
identity/checksums, publish a new alpha.3 prerelease and atomically install its
verified bytes. Preserve alpha.2 and the owner's history. This remains an unsigned
feedback build; no formal or broad terminal qualification is claimed.

## Integration evidence

- Real Cadence (not the prototype) rendered native landing and practice in the
  dedicated Windows Terminal window. Its client-area PrintWindow captures were
  inspected. The font is smooth; ordinary chrome retains single-height rows.
- Native-protocol PTY fixtures exercised correction, numpad submission, F2 in
  both directions, below-minimum pause and restoration. SQLite retained first-pass
  3/4, attempts 4/5, one deletion. A rejecting terminal stayed normal and received
  no double-height drawing commands.
- Keys queued **during** cursor negotiation survived: `9494 Enter` committed
  4/4 first-pass and 4/4 attempts. An earlier smoke injected before raw-mode
  admission (visible canonical echo); that was not counted as a negotiation pass.
- Six new protocol regressions cover native-half evidence, missing/failed replies,
  cleanup, rendition changes, hidden columns and resize repaint. The existing
  display-control regression also covers capability changes. Host strict Clippy
  and all 145 workspace tests passed after integration.
- The real terminal window has been left available for the owner's trial.
  Further scripted practice uses separate temporary PTYs, not that window or
  the owner's ordinary history. No Strop repository or terminal settings changed.

## Session headline tiles (folded into alpha.3)

The owner requested large session speed and accuracy while trying the native
renderer. The original `design/initial-concept.html` Progress view was inspected
in Chromium: two thin-bordered panels, restrained mint figures and explicitly
separate keypad history. Its illustrative benchmark bars are not copied.

- `engine/summary.rs` aggregates real report counts and durations, never averages
  track rates or percentages. Mixed-session speed is correct WPM across typing
  tracks, including their ordinary hesitation; numpad-only sessions show correct
  digits/minute. Numpad digits/time never get blended into typing WPM.
- First-pass accuracy includes every track's first-attempt positions and uses
  checked, truncating `BasisPoints`. One-decimal formatting truncates again:
  9999/10000 cannot become a displayed perfect 100.0%. Empty/invalid measurements
  stay unavailable; counter exhaustion returns a typed error, not a better score.
- `tui/stages/headlines.rs` renders equal-width speed/accuracy cards. Their native
  number bands share a row pair; panel edges align to that same two-cell grid.
  The full per-track table remains underneath. At constrained sizes the cards
  and final action retain priority, with an explicit resize hint if details
  cannot fit. No score or stored-schema change accompanies the presentation.
- A deterministic content/clock fixture exercised the production reducer,
  shared painter and native presenter: 54.0 typing WPM, 91.6% first-pass accuracy,
  plus separate 15.0 keypad DPM. These are fixture results, not the owner's score.
  Real Windows Terminal client captures were inspected at roomy and compact
  sizes, below the minimum, and after returning to ordinary shell rows.
- Five aggregation regressions and one numeric-display boundary regression bring
  the host suite to 151 passing tests; strict Clippy passed. The throwaway capture
  driver was removed after smoke proof. The separate summary test window was
  closed without closing or signalling the owner's trial window.

## Candidate quality receipt

Canonical offline Docker Compose `./ci quality` passed after removing the capture
driver: formatting, warnings-denied Clippy, all 151 tests, architecture boundaries,
and verification of all 48 original contract files against `MANIFEST.sha256`.
The final native summary capture uses the balanced, grid-aligned card layout.

## Release and installation receipt

- [Published alpha.3](https://github.com/tknawara/cadence/releases/tag/v0.1.0-alpha.3)
  with the static executable, notice-complete bundle, checksums and native summary
  captures. Screenshot fixture values are explicitly distinguished from owner data.
- Clean source: `7d8bf9a3b435b4f088c50bf5241c8bc9a7954ee9`.
  Binary SHA-256:
  `ac308e3ece036f26ab739b25a6e74524859f52fb001cf7ffc4dcb38c81f08327`.
  Canonical `./ci dist` verified ELF64/x86_64, no interpreter and no shared-library
  requirements.
- The final static executable passed native-protocol and fallback practice smoke.
  It also practised/saved in a scratch container as UID/GID 1000, networking off,
  read-only root and all capabilities dropped. Corrected keypad counts stayed
  3/4 first-pass, 4/5 attempts, one deletion; reopening/export succeeded.
- Uploaded assets were downloaded, checked against `SHA256SUMS`, and matched to
  the local candidate. Installation was atomic at `~/.local/bin/cadence` →
  `~/.local/lib/cadence/0.1.0-alpha.3-7d8bf9a3b435/cadence`. Alpha.2 remains intact.
- The installed command reopened that temporary history, exercised native/normal
  switching and exited successfully without changing earlier counts. Normal owner
  history was not used. The owner's already-running trial was not interrupted;
  the new executable and summary apply on the next launch.

