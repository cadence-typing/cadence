---
rfc: "2026-11-26-local-imports-and-code-practice"
title: "Local text imports and Rust/shell practice, kept apart from scores"
status: accepted
implementation: complete
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/10
milestone: "0.1.0-beta.3"
supersedes: []
superseded_by: []
blocked_on:
  - https://github.com/cadence-typing/cadence/issues/77
---

# RFC: Local text imports and Rust/shell practice, kept apart from scores

## Summary and motivation

The product contract (P-04, P5) includes practising your own text and short Rust/shell snippets; neither exists. #10 asked for an RFC on two risks: untrusted text reaching the terminal, and imported practice polluting the measurements. This RFC answers both and schedules the work.

Outside this decision: recursive directory import, watching files, fetching URLs (rule 7: offline), executing or linting snippets (never), wider alphabets than L-02.

## Observable behaviour

```sh
cadence import notes.txt --name notes --dry-run     # preview only
cadence import notes.txt --name notes [--normalize-typography]
cadence packs                                       # list packs with counts and origin
cadence --pack notes [--minutes 5]                  # practise a pack
cadence --pack rust                                 # shipped code packs: rust, shell
```

- **Import** reads one explicit file (or `-` for stdin). Limits before any large allocation: 10 MiB, 200,000 lines, 2,000 units per line. It prints accepted, normalized and rejected counts and the first rejections with line and column. `--dry-run` writes nothing. Without `--normalize-typography`, a smart quote, dash or non-breaking space is a rejection, not a silent change; with it, the mapping and counts are shown and the original bytes are kept beside the pack.
- **Refused always**: ESC and every other C0 control except newline and tab, DEL, C1 controls, bidirectional controls (U+202A to U+202E, U+2066 to U+2069), and any character outside the training alphabet. Tabs become four spaces (documented code-pack policy). A file with a refused character is imported only if at least one passage survives, and the report says how many were dropped.
- **Packs** live in `$XDG_DATA_HOME/cadence/packs/<name>/` with a manifest (name, content digest, normalizer version, counts, source file name, never its directory). Importing the same name with different content is refused unless `--replace`.
- **Practice** of a pack is a single-track session. In a code pack, Enter types the newline unit and there is no auto-advance on a wrong character, as everywhere.
- **Kept apart**: a pack session is stored with content class `imported` or `code`. It has its own results row and history. It never enters the headline comparison with standard sessions, the pace decision, benchmark comparability (a probe is not served in a pack session), the review ladder or the learner model's focus selection. The results say so: "Your own text · not compared with standard practice".
- Imported text is never executed, evaluated, sent anywhere or written to logs.

## Proof obligations

No new kernel function. The separation is carried by `comparison::same_basis` ([configuration RFC](2026-11-26-configuration-layout-and-settings.md)), whose theorem `Comparison.never_across_class` this RFC relies on, and by `benchmark::comparable`. This RFC adds one obligation to each caller: a pack session passes its content class, shown by a runtime test per caller (headline, pace evidence, learner history, ladder).

Tested, not proved (parsers and I/O, per AGENTS.md): the sanitizer and normalizer. Their evidence is a property test over arbitrary bytes ("no accepted passage contains a refused code point; accepted plus rejected lines equals input lines; output length is bounded by the stated expansion") with persisted regression seeds, and a render-path test that paints every accepted fixture and finds no control byte in the cell buffer.

## Design and boundaries

- **`cadence-trainer` (`core`)**: `import::normalize(bytes, options) -> ImportReport` is pure (no I/O); `PassagePack` generalises the prose bank for imported and code passages.
- **`cadence-data` (`edge`)**: pack directory, manifest, atomic write, digest check; the import job runs on the worker and its result is admitted by `ticket::admit`.
- **`cadence` app (`edge`)**: `import`, `packs`, `--pack`.
- **Content**: `corpus/code-rust-v1.tsv` and `corpus/code-shell-v1.tsv`, at least 150 short snippets each, written for this repository (Apache-2.0 OR MIT), plausible and non-destructive in appearance; listed in `corpus/manifest.json`.
- **Schema**: no version of its own. Schema 6 (#77) adds `sessions.content_class` with the values `standard`, `imported` and `code`, and the nullable `sessions.pack_digest`; this RFC is their first writer of a non-default value.

## Compatibility and reversibility

Additive. Deleting a pack directory removes the pack; its past sessions stay with their class and digest. Older binaries refuse the newer schema.

## Alternatives

- **Let imported sessions feed the learner model.** Real typing evidence, but L-06 forbids inference across incompatible content classes, and one pasted log file would retarget the words phase. Default no; owner may amend for key-level evidence only.
- **Normalize typography silently.** Changes what the learner typed against; rejected (L-02 asks for a preview).
- **Strip refused characters and keep the line.** Hides that the practice text differs from the file; a rejected line with a diagnostic is honest.

## Evidence and rollout

#84 import, sanitizer and preview; #85 pack practice, code packs and the separation tests; journey 12 of H-05 in #86. Finish line: the final-acceptance step "apply an invalid import; retain last valid state with useful diagnostics" passes headlessly.

## Unresolved questions

None blocking. Owner may amend: the limits; whether key-level evidence from packs may feed focus selection.

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
