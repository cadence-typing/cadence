# 0007: Headless contract revisions for beta.2

Status: accepted (2026-11-26), issues #68 and #70. Revises the handover
contracts in `contracts/` (decision 0005); `MANIFEST.sha256` records the new
digests.

## Changes

- `contracts/frame.schema.json` gains `row_renditions`: an array of at most
  80 entries, one of `normal`, `double_height_top`, `double_height_bottom`.
  The shared painter decides each row's native line rendition (large text,
  alpha.3), so a canonical frame without it would not pin what the terminal
  shows. Optional: a frame from a consumer that never emits renditions still
  validates. (#70)
- `contracts/headless-response.schema.json` narrows `error.code` from any
  non-empty string to the closed set of stable codes the driver emits
  (`MalformedJson` … `BarrierTimeout`, `cadence_driver::envelope::Code`).
  Clients may match on the code; a new code is a contract change recorded
  here. (#68)

`contracts/headless-request.schema.json` is unchanged.

## Compatibility

Both are protocol v1 changes made before any release shipped the headless
driver (first shipped in 0.1.0-beta.2), so no released client depends on
the earlier forms.
