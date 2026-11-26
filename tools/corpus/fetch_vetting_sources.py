#!/usr/bin/env python3
"""Fetches the pinned word-vetting sources (decision 0006, #8) into
target/corpus-sources/, verifying each SHA-256. Run by `./ci prepare`
(networked); every other step is offline."""
import hashlib
import pathlib
import sys
import urllib.request

SOURCES = [
    (
        "scowl-2020.12.07.tar.gz",
        "https://downloads.sourceforge.net/project/wordlist/SCOWL/2020.12.07/scowl-2020.12.07.tar.gz",
        "5587667caa20c4891390c2d42dbb4d5c4c3f41bee77af1457ece3ba23fb859cc",
    ),
    (
        "ldnoobw-en.txt",
        "https://raw.githubusercontent.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/en",
        "af851ecef1d5f212caba17339b12ac39cc2fef7d78c74876f67237644fcee8bd",
    ),
]


def main() -> int:
    root = pathlib.Path(__file__).resolve().parents[2] / "target" / "corpus-sources"
    root.mkdir(parents=True, exist_ok=True)
    for name, url, digest in SOURCES:
        path = root / name
        if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != digest:
            with urllib.request.urlopen(url, timeout=120) as response:
                data = response.read()
            if hashlib.sha256(data).hexdigest() != digest:
                print(f"{name}: digest mismatch", file=sys.stderr)
                return 1
            path.write_bytes(data)
        print(f"{name}: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
