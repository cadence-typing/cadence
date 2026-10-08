#!/usr/bin/env python3
"""Cadence corpus acquisition (decision 0002).

Extracts the top-N English surface forms from the pinned wordfreq
release, vets them against the Cadence V1 training alphabet, and writes
a canonical TSV consumed by the Rust pack loader.

Determinism: wordfreq's top_n_list is deterministic for a pinned
release; the output carries the tool version and a SHA-256 recorded by
the caller. Licences: wordfreq code Apache-2.0; frequency data CC-BY-SA
4.0 — the derived pack inherits CC-BY-SA 4.0 with attribution (see
corpus/NOTICE).
"""

import sys

from wordfreq import top_n_list

CORE_LIMIT = 2_000
COMMON_LIMIT = 10_000
DEFAULT_COUNT = 25_000


def band_for(rank: int) -> str:
    if rank <= CORE_LIMIT:
        return "core"
    if rank <= COMMON_LIMIT:
        return "common"
    return "tail"


def vetted(word: str) -> bool:
    # V1 training alphabet, lowercased word practice: a-z plus the
    # apostrophe. Case variants are folded by wordfreq's ranking.
    return all("a" <= character <= "z" or character == "'" for character in word)


def main() -> int:
    count = int(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_COUNT
    emitted = 0
    for word in top_n_list("en", count * 2):
        if emitted >= count:
            break
        if not vetted(word):
            continue
        emitted += 1
        print(f"{emitted}\t{word}\t{band_for(emitted)}")
    return 0 if emitted == count else 1


if __name__ == "__main__":
    raise SystemExit(main())
