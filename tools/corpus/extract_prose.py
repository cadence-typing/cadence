#!/usr/bin/env python3
"""Builds corpus/prose-en-v2.tsv from verbatim public-domain books.

Recipe (plan 0003). Each source is a Project Gutenberg plain-text ebook of a
work in the US public domain. Only the text between the Gutenberg START/END
markers is used; the Project Gutenberg licence header/footer and trademark are
stripped, as the Gutenberg licence permits for public-domain works.

Normalisation is deterministic and lossless in meaning, never a rewrite:
curly quotes become ASCII quotes, em/en dashes become "--"/"-", the ellipsis
character becomes "...", `_italic_` markers are removed and line breaks inside
a paragraph become single spaces. Any paragraph still containing a character
outside printable ASCII, or matching an editorial pattern (headings,
illustrations, footnotes, tables), is rejected rather than edited.

Selection: eligible paragraphs (starting with a capital, ending a sentence) between MINIMUM_UNITS and MAXIMUM_UNITS are
sampled at evenly spaced indices up to PER_BOOK_CAP per book, preserving
order. The original Cadence passages from prose-en-v1.txt are kept.

Output: one passage per line, `source_key<TAB>text`. Passage identity is
derived from the text digest at load time, so ids survive reordering.

Usage:
    python3 -I tools/corpus/extract_prose.py --cache DIR [--download]
Writes corpus/prose-en-v2.tsv and corpus/prose-sources-v2.tsv and prints the manifest JSON fragment.
"""

import argparse
import hashlib
import json
import pathlib
import re
import sys
import urllib.request

# (source key, Gutenberg ebook number, title, author)
SOURCES = [
    ("austen-pride", 1342, "Pride and Prejudice", "Jane Austen"),
    ("austen-emma", 158, "Emma", "Jane Austen"),
    ("doyle-adventures", 1661, "The Adventures of Sherlock Holmes", "Arthur Conan Doyle"),
    ("doyle-hound", 2852, "The Hound of the Baskervilles", "Arthur Conan Doyle"),
    ("shelley-frankenstein", 84, "Frankenstein", "Mary Wollstonecraft Shelley"),
    ("stevenson-treasure", 120, "Treasure Island", "Robert Louis Stevenson"),
    ("thoreau-walden", 205, "Walden", "Henry David Thoreau"),
    ("grahame-willows", 289, "The Wind in the Willows", "Kenneth Grahame"),
    ("wells-time", 35, "The Time Machine", "H. G. Wells"),
    ("wells-worlds", 36, "The War of the Worlds", "H. G. Wells"),
    ("wilde-dorian", 174, "The Picture of Dorian Gray", "Oscar Wilde"),
    ("bronte-jane", 1260, "Jane Eyre", "Charlotte Bronte"),
    ("dickens-two-cities", 98, "A Tale of Two Cities", "Charles Dickens"),
    ("stoker-dracula", 345, "Dracula", "Bram Stoker"),
    ("alcott-little-women", 514, "Little Women", "Louisa May Alcott"),
    ("montgomery-anne", 45, "Anne of Green Gables", "L. M. Montgomery"),
    ("london-call", 215, "The Call of the Wild", "Jack London"),
    ("burnett-garden", 113, "The Secret Garden", "Frances Hodgson Burnett"),
    ("verne-eighty-days", 103, "Around the World in Eighty Days", "Jules Verne"),
]
URL = "https://www.gutenberg.org/cache/epub/{n}/pg{n}.txt"
MINIMUM_UNITS = 160
MAXIMUM_UNITS = 600
PER_BOOK_CAP = 150
REPLACEMENTS = {
    "‘": "'", "’": "'", "“": '"', "”": '"',
    "—": "--", "–": "-", "…": "...", " ": " ",
}
EDITORIAL = re.compile(
    r"(CHAPTER|Chapter [IVXLC0-9]|\[|\]|\*|#|\||Illustration|Footnote|"
    r"gutenberg|Gutenberg|_|  |^[^a-z]*$|[A-Z]{5,})"
)
ENDING = re.compile(r"[.!?][\"')]?$")
OPENING = re.compile(r"^[\"']?[A-Z]")


def fetch(cache: pathlib.Path, number: int, download: bool) -> bytes:
    path = cache / f"pg{number}.txt"
    if not path.exists():
        if not download:
            sys.exit(f"missing cached source {path}; rerun with --download")
        request = urllib.request.Request(
            URL.format(n=number), headers={"User-Agent": "corpus-recipe/1"}
        )
        with urllib.request.urlopen(request, timeout=60) as response:
            path.write_bytes(response.read())
    return path.read_bytes()


def body(text: str) -> str:
    start = re.search(r"\*\*\* ?START OF (THE|THIS) PROJECT GUTENBERG[^\n]*\n", text)
    end = re.search(r"\*\*\* ?END OF (THE|THIS) PROJECT GUTENBERG", text)
    if not start or not end:
        sys.exit("Gutenberg markers not found")
    return text[start.end() : end.start()]


def normalise(paragraph: str) -> str | None:
    text = " ".join(paragraph.split())
    for old, new in REPLACEMENTS.items():
        text = text.replace(old, new)
    text = re.sub(r"_([^_]+)_", r"\1", text)
    text = " ".join(text.split())
    if not (MINIMUM_UNITS <= len(text) <= MAXIMUM_UNITS):
        return None
    if any(not (0x20 <= ord(c) <= 0x7E) for c in text):
        return None
    if EDITORIAL.search(text) or not ENDING.search(text) or not OPENING.search(text):
        return None
    if text.count('"') % 2:
        return None
    return text


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cache", required=True, type=pathlib.Path)
    parser.add_argument("--download", action="store_true")
    args = parser.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)
    root = pathlib.Path(__file__).resolve().parents[2]
    lines = []
    seen = set()
    manifest = []
    original = (root / "corpus" / "prose-en-v1.txt").read_text(encoding="ascii")
    kept = 0
    for paragraph in original.split("\n\n"):
        text = " ".join(paragraph.split())
        if text and text not in seen:
            seen.add(text)
            lines.append(f"cadence-original\t{text}")
            kept += 1
    manifest.append({"key": "cadence-original", "title": "Original Cadence passages",
                     "licence": "Apache-2.0 OR MIT", "passages": kept})
    for key, number, title, author in SOURCES:
        raw = fetch(args.cache, number, args.download)
        text = body(raw.decode("utf-8-sig")).replace("\r\n", "\n")
        eligible = []
        for paragraph in re.split(r"\n\s*\n", text):
            cleaned = normalise(paragraph)
            if cleaned and cleaned not in seen:
                eligible.append(cleaned)
        if len(eligible) > PER_BOOK_CAP:
            step = len(eligible) / PER_BOOK_CAP
            eligible = [eligible[int(i * step)] for i in range(PER_BOOK_CAP)]
        for cleaned in eligible:
            seen.add(cleaned)
            lines.append(f"{key}\t{cleaned}")
        manifest.append({
            "key": key, "title": title, "author": author, "licence": "public domain (US)",
            "url": URL.format(n=number), "sha256": hashlib.sha256(raw).hexdigest(),
            "passages": len(eligible),
        })
    sources = ["cadence-original\tOriginal practice passages\tCadence"]
    sources += [f"{key}\t{title}\t{author}" for key, _, title, author in SOURCES]
    (root / "corpus" / "prose-sources-v2.tsv").write_bytes(
        ("\n".join(sources) + "\n").encode("ascii")
    )
    output = root / "corpus" / "prose-en-v2.tsv"
    data = ("\n".join(lines) + "\n").encode("ascii")
    output.write_bytes(data)
    print(json.dumps({
        "file": "prose-en-v2.tsv", "sha256": hashlib.sha256(data).hexdigest(),
        "passages": len(lines), "units": sum(len(l.split("\t", 1)[1]) for l in lines),
        "sources": manifest,
    }, indent=2))


if __name__ == "__main__":
    main()
