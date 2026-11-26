# 0.1.0-alpha.7 release evidence (#53)

Captured on 2026-11-26 with the alpha.7 release build (`target/release/cadence`,
built from `alpha7` after commit `b3234c3`), on temporary profiles, no network.

- `pty-session1.txt`, `pty-session2.txt`: two full default sessions (900 s each)
  driven in a PTY by `drive7.py` (it types the character under the caret, with
  scripted slips: a wrong key then Backspace, about 6% of keys in session 1 and
  3% in session 2). Session 1 ran with `--utc-offset -12:00` and session 2 with
  `--utc-offset +14:00`, so session 2 is the next local day and session 1's
  misses are due. Each file holds the start screen, one exercise per phase,
  the closing review and the results, at 100x32 and 60x18. (Session 2's numpad
  frame was not captured: its entries are shorter than the driver's capture
  point.)
  The run found two defects, fixed before release: the store refused every
  checkpoint after the last scored deadline, so typing in the closing review
  failed to save (`b3234c3`); the practice context line was cut mid-token at
  60 columns (`7ee5a8f`).
- Schema migration: a schema-3 profile made by the alpha.6 build (one
  5-minute session, 182 receipts) was copied and opened by the alpha.7 build
  (`cadence export`): version 3 -> 4, 1 session and 182 receipts intact, no
  UTC offset backfilled; the export reads schema 4.
- `utc-offset-across-midnight.txt`: the migrated copy opened at 15:41 UTC with
  `--utc-offset -16:00` (local day before the stored UTC day: "earlier today",
  never a negative age) and `+09:00` (local day after: "yesterday").
