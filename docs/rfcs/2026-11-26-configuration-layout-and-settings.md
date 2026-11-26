---
rfc: "2026-11-26-configuration-layout-and-settings"
title: "Configuration, keyboard layout and the settings surface"
status: accepted
implementation: not-started
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/9
milestone: "0.1.0-beta.3"
supersedes: []
superseded_by: []
blocked_on:
  - https://github.com/cadence-typing/cadence/issues/4
---

# RFC: Configuration, keyboard layout and the settings surface

## Summary and motivation

Cadence is configured by CLI flags only. There is no config file, the layout is fixed to US (`LayoutSupportsPound::US` at every call site), there is one screen flow (start, practice, results) and no way to see progress over time. The handover's P5 (U-01 to U-05, P-01, D-01) asks for a first-run choice of layout and numpad, a validated config file, a settings overlay, four destinations and user themes. This RFC fixes the persisted and decision-bearing parts; visual details follow `docs/04-ui.md` and the existing RFCs for typography and calm flow.

Why an RFC: a new persisted format (`config.toml`, schema 6), a CLI contract (`cadence config check`), and new decisions (when a setting takes effect; which results may be compared; admission of worker results).

Outside this decision: imports and code practice ([separate RFC](2026-11-26-local-imports-and-code-practice.md)), mouse support, a filesystem watcher (U-04: manual reload is enough).

## Observable behaviour

- **First run** (no config and no sessions): one screen asks for layout (US QWERTY, UK QWERTY, other: text only), whether there is a numpad, and the session length. Enter accepts the defaults (US, numpad present, 15 minutes). An existing profile is asked once on upgrade and keeps its history under the chosen layout.
- **`$XDG_CONFIG_HOME/cadence/config.toml`** (default `~/.config/cadence/config.toml`). Keys: `session.minutes` (5, 10, 15, 20, 30), `session.tracks`, `session.emphasis`, `keyboard.layout`, `keyboard.numpad`, `keyboard.decimal_separator`, `practice.accuracy_target` (bounded), `practice.live_pace`, `content.breadth`, `content.packs`, `display.theme`, `display.text_size`, `display.key_guide` (`auto`, `always`, `off`), `display.high_contrast`. Precedence: defaults, then the file, then CLI flags. An unknown key or invalid value is an error naming file, line, key and the accepted range; omitted keys take defaults; an invalid file never silently becomes defaults.
- **`cadence config check [FILE]`** prints the effective values with their source, or the diagnostics, and exits non-zero on any error.
- **Destinations**: Today, Train, Numpad, Progress; settings and help are overlays (`?` outside practice). During practice every printable key is input, as today.
- **Settings overlay** edits the keys above. "Saved" appears only after the write is acknowledged. Theme, text size, key guide and contrast apply at once; duration, tracks, emphasis, accuracy target, breadth and packs apply to the next session; layout and numpad apply to the next session and start a separate skill partition. Nothing changes the exercise being typed.
- **Reload settings** re-reads the file; an invalid file keeps the previous settings and shows one diagnostic.
- **Progress** shows per-track accuracy and pace over comparable sessions only, with counts, and says "not comparable" or "not enough sessions" instead of drawing a trend across a layout change, synthetic sessions or different content classes.
- **Themes**: `display.theme` may name a file in the config directory with the eleven roles of U-02; an invalid theme keeps the current palette and reports one diagnostic.

Scores and timing do not change. A layout change never rewrites history; UK adds `£` and the UK symbol positions to number-row content.

## Proof obligations

| Rust entry point (kernel) | Theorem | Mutant | Production caller |
|---|---|---|---|
| `ticket::admit(result_epoch, result_request, current_epoch, pending_request, has_pending) -> Admit` with `Admit::{Accept, StaleEpoch, NotPending}` (VP-01, VP-02) | `Ticket.accept_iff_fresh`: `Accept` iff the epochs are equal, a request is pending and its id equals the result's; `Ticket.at_most_once`: after an accept the same result is `NotPending`; `Ticket.stale_error_never_replaces`: failure results obey the same rule | `no-epoch-check`, `no-pending-check` (the two faults of `formal/AsyncAdmission`) | `engine` admission of config-write, reload and import results |
| `settings::takes_effect(kind) -> Effect` with `Effect::{Now, NextSession}` | `Settings.score_affecting_never_now`: every kind that can change targets, timing or scoring is `NextSession`; listed exhaustively, no wildcard | `layout-applies-now` | `engine` settings reducer |
| `comparison::same_basis(a, b) -> bool` over layout revision, content class, provenance and track set | `Comparison.never_across_layout`, `Comparison.never_with_synthetic`, `Comparison.never_across_class`, `Comparison.same_basis_iff_all` | `layout-ignored`, `synthetic-compared` | `engine::review` (replaces the "same track set" test), Progress, `benchmark::comparable` facts |

TLA+: `formal/AsyncAdmission` gets a living successor `models/WorkerResults.tla` (epoch and request tickets, a single retained result slot, restart and reuse) with invariants `AcceptedFresh`, `AtMostOnce`, `CompletionRetained` and mutants `no-epoch`, `no-pending`, `drop-full-slot`; registered `mapped` to `ticket::admit` and the worker.

Tested, not proved: TOML parsing and diagnostics, atomic file replacement, theme validation, rendering at 60x18, 100x32 and 120x40.

## Design and boundaries

- **`cadence-data` (`edge`)**: `config` module (typed `Config`, parse with `toml_edit` so in-app edits keep comments, write to a temporary file and rename, source digest check before writing so an externally edited file is not clobbered). Schema 6: `sessions.layout` (`us`, `uk`, `custom`, or `unspecified` for sessions before this change) and `sessions.content_class` (`standard`; other values come with the imports RFC); `settings_audit(at_session, key, old, new)` for score-affecting keys. Skill observations are read per layout; `unspecified` history is adopted by the first chosen layout (letters are identical on US and UK).
- **`cadence-runtime` (`edge`)**: the worker gains config read and write jobs; each result carries its ticket.
- **`cadence-engine` (`core`)**: screens `Today`, `Train`, `Numpad`, `Progress`, overlays `Settings`, `Help`, `Setup`; one typed binding table drives dispatch, footer hints, help and headless `action` validation (U-03).
- **`cadence-core`**: `Layout` replaces `LayoutSupportsPound` at call sites; `TrainingChar::new` takes the layout.
- **`cadence-tui`**: theme adapter loads roles; no colour literals in widgets.
- Config and settings never touch the reduce path with I/O: the reducer emits a command, the worker returns a fact.

## Compatibility and reversibility

No config file means today's behaviour plus the first-run question. Schema-5 profiles migrate with `layout = unspecified`. Older binaries refuse schema 6. Removing a key later needs a deprecation diagnostic, not a silent ignore.

## Alternatives

- **Flags only.** Cannot express layout or persist a choice.
- **Settings in SQLite.** Not hand-editable; D-01 puts preferences in the config directory.
- **Apply every setting immediately.** Would change targets mid-exercise (U-04 forbids).

## Evidence and rollout

#75 job tickets and `WorkerResults.tla`; #76 config file and `config check`; #77 schema 6 and layout partition; #79 settings overlay; #78 first-run setup; #80 destinations and help; #81 Progress; #82 themes and key guide; #83 numpad operators and separator; #86 headless journeys 1 and 9. Render tests at the three sizes for each new screen.

## Unresolved questions

None blocking. Owner may amend: the accuracy-target bounds (default 95.0% to 99.5%), adoption of `unspecified` history by the first chosen layout, and `toml_edit` as the dependency.

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
