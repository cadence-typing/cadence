#!/usr/bin/env python3
"""Checks corpus/benchmark-en-v1.tsv (#64): count, alphabet, length, unique
ids and no sentence shared with the practice prose bank."""
import re, sys
from pathlib import Path

root = Path(__file__).resolve().parents[2]
lines = (root / "corpus/benchmark-en-v1.tsv").read_text().splitlines()
prose = (root / "corpus/prose-en-v2.tsv").read_text()

def sentences(text):
    return {s.strip().lower() for s in re.split(r"(?<=[.!?])\s+", text) if len(s.strip()) > 20}

practice = set()
for line in prose.splitlines():
    if "\t" in line:
        practice |= sentences(line.split("\t", 1)[1])

errors, ids = [], set()
for n, line in enumerate(lines, 1):
    parts = line.split("\t")
    if len(parts) != 2:
        errors.append(f"line {n}: expected 2 tab-separated columns")
        continue
    pid, text = parts
    if not re.fullmatch(r"probe-\d{3}", pid) or pid in ids:
        errors.append(f"line {n}: bad or duplicate id {pid!r}")
    ids.add(pid)
    if len(text) < 1000:
        errors.append(f"{pid}: {len(text)} units, need at least 1000")
    if any(not (" " <= c <= "~") for c in text):
        errors.append(f"{pid}: characters outside printable ASCII")
    if text != text.strip() or "  " in text:
        errors.append(f"{pid}: leading/trailing or double spaces")
    shared = sentences(text) & practice
    if shared:
        errors.append(f"{pid}: shares a sentence with the practice bank: {sorted(shared)[0][:60]}")
if len(lines) < 60:
    errors.append(f"{len(lines)} passages, need at least 60")
print("\n".join(errors) or f"ok: {len(lines)} passages")
sys.exit(1 if errors else 0)
