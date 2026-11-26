#!/usr/bin/env python3
"""Cadence word vetting (decision 0006, #8).

Rebuilds corpus/words-en-v2.tsv from pinned inputs, offline:
  1. the wordfreq dump (tools/corpus/dump_words.py 40000, wordfreq 3.1.1),
     whose ids are the alphabet-filtered ranks (v1 ids are its first 25,000);
  2. SCOWL 2020.12.07 final lists, sizes <= 70: english/american words and
     contractions are dictionary words; abbreviations are excluded unless
     also a word;
  3. LDNOOBW `en` at a pinned commit: profanity;
  4. corpus/vetting/excluded-reviewed.txt: hand-reviewed exclusions (slurs
     and offensive terms the lists miss).
A word is kept when it is a dictionary word, not profanity, not reviewed
out, and not a lone letter other than `a` and `i`. Ids and bands of kept
words are unchanged; kept words are taken in rank order until TARGET.

Usage: vet_words.py DUMP SCOWL_FINAL_DIR LDNOOBW REVIEWED OUT_TSV OUT_REPORT
"""

import glob
import random
import sys
from collections import Counter

TARGET = 22_000


def scowl(directory, prefixes):
    out = set()
    for prefix in prefixes:
        for path in glob.glob(f"{directory}/{prefix}.*"):
            if int(path.rsplit(".", 1)[1]) <= 70:
                with open(path, encoding="latin-1") as handle:
                    out |= {line.strip() for line in handle if line.strip()}
    return out


SUFFIXES = ("'s", "s", "es", "ed", "ing", "er", "ers", "y")


def listed(word, terms):
    """The word, or the word less a common inflection, is a listed term."""
    if word in terms:
        return True
    return any(word.endswith(suffix) and word[: -len(suffix)] in terms for suffix in SUFFIXES)


def main(dump, final, ldnoobw, reviewed, out_tsv, out_report):
    words = scowl(final, ["english-words", "american-words", "english-contractions", "american-contractions"])
    abbreviations = scowl(final, ["english-abbreviations", "american-abbreviations"])
    with open(ldnoobw, encoding="utf-8") as handle:
        profanity = {line.strip().lower() for line in handle if line.strip()}
    with open(reviewed, encoding="utf-8") as handle:
        reviewed_out = {
            line.strip() for line in handle if line.strip() and not line.startswith("#")
        }
    kept, excluded, reasons = [], [], Counter()
    with open(dump, encoding="utf-8") as handle:
        for line in handle:
            if len(kept) >= TARGET:
                break
            rank, word, band = line.rstrip("\n").split("\t")
            if len(word) == 1 and word not in ("a", "i"):
                reason = "lone letter"
            elif listed(word, profanity):
                reason = "profanity (LDNOOBW)"
            elif listed(word, reviewed_out):
                reason = "hand-reviewed exclusion"
            elif word in words:
                reason = None
            elif word in abbreviations:
                reason = "abbreviation"
            else:
                reason = "not a dictionary word (names, nonwords, foreign, misspellings)"
            if reason is None:
                kept.append((rank, word, band))
            else:
                excluded.append((rank, word, reason))
                reasons[reason] += 1
    if len(kept) < 20_000:
        print(f"only {len(kept)} words kept", file=sys.stderr)
        return 1
    with open(out_tsv, "w", encoding="utf-8") as handle:
        for rank, word, band in kept:
            handle.write(f"{rank}\t{word}\t{band}\n")
    sampler = random.Random(6)
    bands = Counter(band for _, _, band in kept)
    with open(out_report, "w", encoding="utf-8") as handle:
        handle.write("Cadence word vetting report (words-en-v2, decision 0006)\n\n")
        handle.write(f"kept: {len(kept)} (core {bands['core']}, common {bands['common']}, tail {bands['tail']})\n")
        handle.write(f"highest rank read: {kept[-1][0]}\n")
        handle.write(f"excluded: {len(excluded)}\n")
        for reason, count in sorted(reasons.items()):
            handle.write(f"  {reason}: {count}\n")
        handle.write("\n200 random kept words (seed 6):\n")
        handle.write(" ".join(w for _, w, _ in sampler.sample(kept, 200)) + "\n")
        handle.write("\n200 random excluded words (seed 6), with reasons:\n")
        for _, word, reason in sampler.sample(excluded, 200):
            handle.write(f"  {word}\t{reason}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(*sys.argv[1:7]))
