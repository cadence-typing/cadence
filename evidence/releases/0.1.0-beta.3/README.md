# 0.1.0-beta.3 release evidence (#87)

Captured on 2026-11-27, no network, on temporary HOMEs and profiles only.

- `screens-pty.txt` (`screens-pty.py`): the release binary in a real PTY at
  60x18 and 100x32: first-run setup, Today, Train, Numpad, Progress, the
  settings overlay, and a `--pack code-shell` session with its review
  ("Your own text · not compared with standard practice").
- `first-run-pty.txt` (`first-run-pty.py`): setup answered (UK, no
  numpad), the written `config.toml`, the rebuilt start screen ("numpad
  off", 15:00 allocated), no setup on the second run, and the settings
  overlay reading the file back; `settings_audit` holds layout us -> uk and
  numpad true -> false at session 1.
- `progress-pty.txt`: Progress reading the store through the writer.
- `code-pack-pty.txt` (`code-pack-pty.py`): a Rust snippet typed with its
  newlines, the captioned review, and the session stored as content class
  `code` with its pack digest.
- Schema migration: a copy of the schema-5 profile from the beta.1 evidence
  (two sessions, 1,128 receipts) opened by the beta.3 build
  (`cadence export`): `migration-schema5-before.txt` and
  `migration-schema6-after.txt` — version 5 -> 6, every table's row count
  unchanged, `settings_audit` added and empty, both sessions read as layout
  `unspecified` and class `standard`; the new export reads schema 6.
- Headless gate: all twelve H-05 journeys replay, and a tampered copy of
  journey 11 fails at its first divergence (lines 17 and 15).
- Proof gate, TLC gate and corpus gate: `docs/TAUTEN.md`.
