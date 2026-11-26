# 0006: Word vetting

Status: accepted (2026-11-26), issue #8. Supersedes the "alphabet-filtered
frequency tokens, not a vetted lexicon" state of decision 0002 for the word
pack; the acquisition recipe of 0002 is unchanged.

## What "vetted" means here

A practice word is a dictionary word of English in ordinary use. It is not a
proper name, an abbreviation, a lone letter other than `a` and `i`, a slur or
profanity. Every exclusion is decided by a pinned list or by a committed,
hand-reviewed list; nothing is decided ad hoc at build time.

## Lists

| Class | Decided by | Pinned input | Licence of the input |
|---|---|---|---|
| Dictionary word | SCOWL 2020.12.07, `english-words`, `american-words`, `english-contractions`, sizes <= 70 | sha256 `5587667c…59cc` | SCOWL notice: permissive, redistribution allowed with the notice |
| Abbreviation | SCOWL `english-`/`american-abbreviations` <= 70 (excluded unless also a dictionary word) | same archive | same |
| Proper name, nonword, foreign word, misspelling | absence from the SCOWL word lists above (names are capitalised there) | same archive | same |
| Profanity | LDNOOBW `en`, commit `5faf2ba`, with common inflections | sha256 `af851ece…e8bd` | CC-BY-4.0 |
| Slurs and distressing words the lists keep | `corpus/vetting/excluded-reviewed.txt`, hand-reviewed, with common inflections | committed | (Cadence) |
| Lone letters | rule | — | — |

No list with a suitable licence covers slurs completely; that class is the
committed reviewed list, extended when a report sample or a user finds one.
The result is distributed under CC-BY-SA 4.0, the licence of the frequency
data; CC-BY-4.0 and the SCOWL notice are compatible with that.

## Ids

Word ids are the alphabet-filtered wordfreq ranks (decision 0002). Every
word kept from `words-en-v1` keeps its id, so stored outcomes and review
ladders keep their meaning; removed ids are never reused. The pack draws
further down the ranks (to rank 28,006) until 22,000 words are kept, above
the 20,000 of P-04. A removed word that is still due is dropped from
planning.

## Recipe

`./ci prepare` fetches the pinned sources (`tools/corpus/fetch_vetting_sources.py`,
verifying digests). `tools/corpus/dump_words.py 40000` (wordfreq 3.1.1)
gives the ranked dump; `tools/corpus/vet_words.py` rebuilds
`corpus/words-en-v2.tsv` and `corpus/vetting/report-v2.txt` offline: counts
kept and excluded per reason, band sizes, and 200 random kept and 200 random
excluded words (seed 6) for review. Digests are in `corpus/manifest.json`.

## What this is not

Automatic lists plus a sampling review are not a guarantee that no
objectionable word remains, and SCOWL size 70 admits some rare or archaic
words. The report's samples were reviewed for the first release of the pack.
