# Decision 0005 — archive the handover's root files

Date: 2026-11-24 · Status: accepted

## Context

`MANIFEST.sha256` pins the exact bytes of the handover packet, including the root `README.md`, `AGENTS.md` and `HANDOVER.html`. A public project needs a README written for users, and its engineering rules evolve (for example, adopting tauten). Editing the pinned root files would break contract verification. Leaving them would keep a "handover to an implementing agent" document as the project's front page.

## Decision

- Move `README.md`, `AGENTS.md` and `HANDOVER.html` with **unchanged bytes** to `docs/handover/`, and change only their paths in `MANIFEST.sha256`. Their digests stay the same, so the original contract is still verified.
- The root `README.md` and `AGENTS.md` are now live documents. The frozen rules remain binding where the live `AGENTS.md` does not explicitly supersede them, and supersession requires an RFC.
- `tools/check_packet.py` is itself pinned and is not edited. It still validates the packet: it reads the live root files only for link resolution.
- The eight TLC logs under `evidence/model-checks/` were pinned but never committed, because the `*.log` ignore rule matched them. They are now tracked, so a fresh clone verifies.

## Verification

`cargo xtask verify-contracts` verifies all 48 files at their new paths.
