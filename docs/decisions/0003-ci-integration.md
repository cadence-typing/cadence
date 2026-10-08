# Decision 0003 — CI integration posture (P0)

Date: 2026-10-08 · Status: inspected, integration deferred to P8

## What was inspected (2026-10-08)

- The shared plan at `/home/tarek/workspace/SHARED-BUILDBOT-CI-PLAN.md`
  (canonical copy; identical in substance to the packet's
  `reference/shared-buildbot-ci-plan-2026-10-04.md`).
- The workstation: WSL2 kernel `6.18.33.2-microsoft-standard-WSL2`,
  x86_64; Docker Desktop server 29.7.2; Compose v5.5.0. These match the
  plan's 2026-10-04 observations.
- No Buildbot controller or worker is currently provisioned on this
  workstation. No `ci-infra` repository exists yet. The plan explicitly
  records that no services were provisioned by the research.

## Consequence for Cadence

- All gates are runnable locally through the repository command contract
  (`./ci`, `cargo xtask`) per B-03. Local execution emits receipts with
  the same format but does not promote a qualified bundle.
- The `buildbot-recovery` gate is **blocked** pending the shared
  bootstrap, and stays blocked until the controller/worker exist. It is
  never converted to a pass from local evidence.
- P8 adds a narrow Cadence adapter to the shared operator flow (one
  heavyweight gate at a time initially, per the plan's disk/swap
  caution); no competing Rust release scheduler is built.

## Repository command surface (implemented at P0)

- `./ci quality` / `./ci gate <id>` / `./ci gates` — Compose-routed gate
  execution (offline after `prepare`).
- `cargo xtask quality|boundary|verify-contracts|gate|gates` — logic
  owners. Unimplemented gates fail with their work-packet name.
- `qualify`, `status`, `resume`, `package`, `install-smoke` operator
  subcommands land with P8's candidate/receipt machinery.
