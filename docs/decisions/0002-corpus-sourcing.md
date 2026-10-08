# Decision 0002 — corpus sourcing and licences (P0)

Date: 2026-10-08 · Status: decided, acquisition deferred to P2

The release corpus is a P2 deliverable; this record fixes the sources,
licences and the reproducible acquisition recipe so P2 cannot drift into
unlicensed content.

## Word forms + frequency bands

- **Source:** `wordfreq` (Robyn Speer et al.), pinned release 3.1.1,
  extracting `top_n_list('en', N)` with N ≥ 25,000, then vetting:
  training-alphabet filter (printable ASCII + `£` per L-02), case-fold
  deduplication preserving the common surface form, minimum length 1.
- **Licence:** code Apache-2.0; bundled frequency data is redistributable
  under CC-BY-SA 4.0 (verified against the project's licence statement on
  2026-10-08). The derived Cadence word pack is published as data under
  CC-BY-SA 4.0 with attribution to wordfreq and its upstream corpus terms;
  application code stays Apache-2.0 OR MIT. Pack manifests (D-03) carry
  the notice text.
- **Frequency bands:** rank-based — band A = ranks 1–2,000 (everyday
  core), band B = 2,001–10,000 (common), band C = 10,001–N (long tail).
  Everyday prominence comes from band weighting in the relevance term
  (L-04); the band boundaries are recorded in the pack manifest, not
  hardcoded at call sites.
- **Count promise:** ≥ 20,000 distinct vetted surface forms in the first
  release pack; the `corpus` gate checks the promised count, dedup
  honesty (no case variants counted twice) and licence notices.

## Prose

- **Natural prose:** Project Gutenberg public-domain texts (pre-1929
  publication). Specific eBook numbers are pinned at acquisition time in
  the pack manifest with per-book notice text. Passages are excerpted,
  inspected for archaic wording/unfamiliar proper nouns (P-04), and
  normalized to the training alphabet.
- **Benchmark probes:** purpose-written unseen passages authored as part
  of this repository (Apache-2.0 OR MIT), matched by the benchmark
  comparability rules (P-03): punctuation, capitalization, word-length
  and frequency strata. Benchmark passages are never reused for practice
  (contamination retirement per L-02/P-03).

## Numpad / number row

Generated content only (D-03/P-04): digit sequences, repetition,
alternation, integer/decimal amounts, operator drill `+ - * /`. String
identity preserved (`005.20` stays `005.20`). No external dataset.

## Rust/shell snippets

Purpose-written short plausible non-executing snippets (repository
licence), added before final P8 qualification per P-04.

## Reproducible acquisition recipe (P2 implements)

1. Pinned Docker image (python + wordfreq==3.1.1 wheel hash recorded).
2. `tools/corpus/dump_words.py` → canonical word list (word, band).
3. SHA-256 of inputs recorded; output pack gets `CorpusVersion` digest.
4. Gutenberg fetch by pinned eBook number; notices archived verbatim.
5. All outputs land under `corpus/` with `manifest.json` (D-03 fields).

Network is used only in this acquisition step, never in qualification.
