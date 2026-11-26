#!/usr/bin/env python3
"""Validate the Cadence code corpus files (code-rust-v1.tsv, code-shell-v1.tsv)."""
import os
import re
import sys

MIN_SNIPPETS = 150
MIN_CHARS, MAX_CHARS = 20, 300
MAX_LINES, MAX_LINE_LEN = 8, 72

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
FILES = [
    ("rust", os.path.join(ROOT, "corpus", "code-rust-v1.tsv")),
    ("shell", os.path.join(ROOT, "corpus", "code-shell-v1.tsv")),
]


class Bad(Exception):
    pass


def decode(raw, where):
    out = []
    i = 0
    while i < len(raw):
        c = raw[i]
        if c == "\\":
            nxt = raw[i + 1] if i + 1 < len(raw) else ""
            if nxt == "n":
                out.append("\n")
            elif nxt == "\\":
                out.append("\\")
            else:
                raise Bad(f"{where}: backslash must be followed by 'n' or '\\' (col {i + 1})")
            i += 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def check(prefix, path):
    try:
        with open(path, "rb") as fh:
            data = fh.read()
    except OSError as e:
        raise Bad(f"{path}: cannot read: {e}")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as e:
        raise Bad(f"{path}: not valid UTF-8: {e}")
    if b"\r" in data:
        raise Bad(f"{path}: contains CR; LF line ends required")
    if not text.endswith("\n"):
        raise Bad(f"{path}: file must end with a newline")
    lines = text[:-1].split("\n")
    idre = re.compile(rf"^{prefix}-(\d{{3,}})$")
    seen = set()
    for n, line in enumerate(lines, 1):
        where = f"{path}:{n}"
        if line.count("\t") != 1:
            raise Bad(f"{where}: expected exactly one tab, found {line.count(chr(9))}")
        sid, raw = line.split("\t")
        m = idre.match(sid)
        if not m or len(m.group(1)) != 3 or sid != f"{prefix}-{n:03d}":
            raise Bad(f"{where}: id {sid!r} should be {prefix}-{n:03d}")
        body = decode(raw, where)
        for ch in body:
            if ch != "\n" and not (0x20 <= ord(ch) <= 0x7E):
                raise Bad(f"{where}: ({sid}) disallowed character {ch!r}")
        if not MIN_CHARS <= len(body) <= MAX_CHARS:
            raise Bad(f"{where}: ({sid}) length {len(body)} not in {MIN_CHARS}..{MAX_CHARS}")
        parts = body.split("\n")
        if len(parts) > MAX_LINES:
            raise Bad(f"{where}: ({sid}) {len(parts)} lines, max {MAX_LINES}")
        if parts[0].strip() == "" or parts[-1].strip() == "":
            raise Bad(f"{where}: ({sid}) blank first or last line")
        for k, p in enumerate(parts, 1):
            if p != p.rstrip(" "):
                raise Bad(f"{where}: ({sid}) trailing space on snippet line {k}")
            if len(p) > MAX_LINE_LEN:
                raise Bad(f"{where}: ({sid}) snippet line {k} has {len(p)} chars, max {MAX_LINE_LEN}")
        if body in seen:
            raise Bad(f"{where}: ({sid}) duplicate snippet text")
        seen.add(body)
    if len(lines) < MIN_SNIPPETS:
        raise Bad(f"{path}: only {len(lines)} snippets, need at least {MIN_SNIPPETS}")
    return len(lines)


def main():
    ok = True
    for prefix, path in FILES:
        try:
            n = check(prefix, path)
            print(f"OK {os.path.relpath(path, ROOT)}: {n} snippets")
        except Bad as e:
            print(f"FAIL {e}", file=sys.stderr)
            ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
