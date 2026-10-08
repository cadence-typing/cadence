# Product and daily practice

## P-01 Purpose

Tarek wants one terminal-native place to practise every day, including the number row and numpad, with strong accuracy, useful speed gains and automatic familiarity with word patterns. His platform is Linux/WSL with Docker. He uses a Keychron Q6 and a custom top numpad key mapped to Backspace. The actual active keyboard layout is not known; do not assume UK key positions solely from residence.

The first-run experience asks only what affects correct exercises: layout (US/UK QWERTY initially, with a custom text-only fallback), presence of a numpad, and optionally session duration. Defaults otherwise work. All alphabetic keys are available immediately. A brief initial observation period replaces a lengthy beginner unlocking course. Finger guidance is optional and described as a suggestion; the terminal does not observe fingers.

## P-02 Fifteen-minute daily plan

The default plan contains 900 seconds of allocated session time:

| Phase | Seconds | Purpose |
|---|---:|---|
| Warm-up | 60 | Familiar words at an easy pace; timing-quality check |
| Adaptive words | 300 | Weak transitions/words, due review and unfamiliar vocabulary |
| Number row | 120 | Digit reaches in text and neutral identifier contexts |
| Numpad | 180 | Digit sequences, decimals, zero, corrections and entry submission |
| Prose transfer | 180 | Natural sentences with matched punctuation difficulty |
| Review | 60 | Three useful findings and the next session's focus |

Typing allocations total 840 seconds; review totals 60. Transition screens do not add mandatory exercises. Pause, explicit instruction reading and interruption may lengthen wall time; the app must label active time versus elapsed time honestly. Review can be dismissed immediately. At a phase deadline, end the scored slice at that event boundary, retain the partial exercise, and offer the next phase; do not force a final long word or add hidden overtime. A partial word contributes observed characters and accuracy but not a completed-word success.

The plan freezes policy, layout, corpus identity and initial learner revision. Text for the current exercise is immutable. Generate the next exercise ahead of time using committed or explicitly provisional local observations; each accepted exercise records its own basis. If preparation is late, show a brief preparing state or use an explicitly recorded safe fallback corpus block. Never block input on a planner task.

Changing duration creates a new plan at the next session boundary. For 5–30 minute presets, allocate an exact integer budget using documented phase minimums and largest-remainder apportionment. A 5-minute session still includes both numeric tracks unless the user explicitly disables one. Reject an impossible custom configuration; do not quietly remove a selected track. Proof obligations cover the allocation algorithm, not the educational optimality of these durations.

## P-03 Initial and ongoing adaptation

First session: common real words and short neutral prose, then simple numeric sequences. Do not infer speed targets from other users. Until enough clean observations exist, sample broadly and explain “building your baseline.” Warm-up timing is recorded separately from core progress. Every enabled track gets exposure; a weak numpad does not reduce main-keyboard difficulty.

Daily sessions mix difficult material with familiar material, avoid exhaustion loops, and let the user pause or stop without losing completed work. Returning after a break increases review exposure within the same time budget rather than issuing a backlog demand. The scheduler proposes a session, not a compulsory debt list.

Benchmark: once per seven completed daily sessions, replace the first 120 seconds of the prose phase with a standardized unseen-text probe, leaving 60 seconds for normal prose. Match punctuation, capitalization, word-length/frequency strata and duration to the previous comparable probe. Keep passages unique for the profile. Mark practice on a benchmark passage as contamination and retire it from future benchmark use. Interrupted or pasted-input probes are retained but excluded from the comparable benchmark series. The UI must say when there is not enough comparable evidence.

Numeric transfer: use fresh amounts/sequences from the same generator class, never the identical list every day. Track clean-entry rate and numeric throughput independently. Do not compare a decimals drill to an integers drill as a single speed trend.

## P-04 Corpus and modes

Mandatory first release: adaptive real words, natural prose, number row, numpad and user-selected single-track practice. Add Rust/shell and explicit local text import before final P8 qualification; these are requested scope, not an excuse to block the first working slice.

The main word pack targets at least 20,000 distinct vetted English surface forms in the shipped release, with an expansion target around 50,000 when quality/licensing permit. “All words” means broad, expandable coverage, not a claim to include every dictionary entry. Frequency bands must keep everyday words prominent while sampling the long tail. Do not manufacture size by counting capitalization variants, duplicates, arbitrary identifiers or inflections the source does not support. Record exact counts, normalization and source licences. If suitable content cannot be licensed, mark the corpus gate blocked rather than ship ten words and claim completion.

Prose must be natural, edited and usable offline. Use licensed/public-domain material or purpose-written passages; inspect readability, unfamiliar proper nouns, archaic wording and punctuation. The handover contains examples, not the release corpus. Rust/shell snippets should be short, plausible and non-executing. Imported snippets are never executed, evaluated or sent elsewhere.

Numpad content covers digits 0–9, repetition, alternation, integer/decimal amounts and Enter; add `+ - * /` in a selectable operator drill. Preserve string identity: `005.20` must remain exactly that string, never be converted to the number 5.2. Start with decimal point; alternate separators are explicit pack/profile settings. Number-row symbols follow the configured layout.

## P-05 Boundaries

No accounts, cloud synchronization, leaderboard, multiplayer, social features, subscriptions, AI coaching API, background keylogger or automatic repository scanning. These are not needed for the authorised product. No forced streak guilt or constant score animation. Themes are meaningful but secondary: ship dark, light and terminal-default palettes; custom semantic-role palettes are small configuration data.

Measure progress through accuracy and transfer, not daily badges. Explain uncertainty and lack of data. Target thresholds and the 15-minute mix are tunable product hypotheses. The app cannot promise that 15 minutes is globally optimal or that a specific speed gain is guaranteed.
