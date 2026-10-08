# Docker, Buildbot and owner installation

## B-01 Authority and scope

The supplied `reference/shared-buildbot-ci-plan-2026-10-04.md` selects Buildbot, an initial local WSL controller/owned Linux worker, retained project-owned gate commands, resumable candidate-bound qualification and optional publication. Use that direction. Buildkite is background research, not a second implementation. GitHub may remain the Git host; GitHub Actions is not required to accept or install a Cadence build.

This is a new Linux application. Initial support profile `linux-x86_64-v1` declares Linux x86_64 binary execution, Linux PTY/SQLite/filesystem checks, one real WSL-terminal journey and Docker use. ARM64, macOS and native Windows builds are outside this first profile. The shared plan's retained ARM/Windows obligations concern existing projects; do not remove them or copy their enormous matrices into Cadence.

Inspect the current shared CI repository/configuration and its owner entrypoint at P0. Reuse it if present; add a narrow Cadence adapter. If absent, keep all gates runnable locally and record the Buildbot integration as blocked pending the shared bootstrap. Do not quietly mark P8 complete on local tests alone. Do not set up a paid service, repartition an SSD, prune unrelated Docker data or alter shared project releases as part of this handover.

## B-02 Reproducible environment and distribution

Proposed primary artifact: a statically linked `x86_64-unknown-linux-musl` binary with bundled SQLite and built-in base corpus. This suits the offline program and avoids host glibc linkage. Prove actual ELF properties, do not assume `musl` in the target name establishes complete static linkage. Record CPU baseline (`x86-64`, no `target-cpu=native`), minimum kernel/terminal prerequisites and real runtime tests. If the owner's deployment policy requires a GNU/glibc binary, add an explicit qualified recipe and baseline; never relabel a different build as the same artifact. No separate macOS/Windows implementation.

Docker/Compose services: `test`, `headless`, `pty`, `persistence`, `model`, `prove`, `verify`, `package`, `install-smoke`, `app`. Reuse a small number of pinned multi-stage images. Fetch dependencies/tools only in a declared preparation step; qualification execution runs without networking using exact locks/images/corpus inputs. No `latest` tags. P0 selects and verifies actual available Rust/Ratatui/crossterm/Verus/TLAPS/SQLite/Python/Buildbot versions and records digests. The inspected Strop tool pins are reference candidates, not automatic Cadence compatibility evidence.

Product gates execute when the container runs. A successful `RUN cargo test` layer reused by Docker is cached evidence, not a fresh run. It may only be reused under explicit same-candidate receipt policy. Compile test artifacts once per compatible profile/target/recipe and reuse them in isolated test executions; preserve an independent clean build only when a reproducibility claim requires it.

Output directories are scoped and owned by the caller UID/GID. Named Cargo/dependency caches are keyed by toolchain/lock/target/trust scope. Read-only source mounts plus explicit writable outputs are preferred. The shipping `app` container runs non-root, with network disabled and only the profile directory mounted; interactive mode requires `-it`. Container PTY tests allocate their own PTY and do not rely on Buildbot itself running every command under a PTY.

## B-03 Repository command contract

Implement `cargo xtask` subcommands and a small shell wrapper so the shared operator entrypoint does not depend on a host Rust install. Wrappers invoke the pinned Compose service; logic remains in `xtask`/gate tools. Commands listed here are target interfaces, not tools already supplied by this packet.

```sh
./ci feedback
./ci qualify --commit <full-sha> --profile linux-x86_64-v1
./ci status --candidate <candidate-id>
./ci resume --candidate <candidate-id>
./ci gate --candidate <candidate-id> --gate <required-gate-id>
./ci package --candidate <candidate-id>
./ci install-smoke --candidate <candidate-id>
```

`qualify` freezes a clean candidate, persists the record, submits an idempotent Buildbot request and returns its durable request/candidate ID. `status` is an observer; losing it cannot lose the run. `resume` reconciles accepted request IDs and receipts before submitting missing work. It restarts incomplete processes at gate boundaries and reuses valid same-input completed gates. A later source commit must not cancel a frozen qualification.

If no controller is configured, the command returns a specific integration-precondition error and offers the separate local gate commands. Local execution emits the same receipt format, but only the configured authority may promote a qualified bundle. Do not create a competing release scheduler in Rust. Buildbot configuration calls argv-list commands (`ShellCommand`), collects logs/receipts, sets resource locks and propagates nonzero outcomes. It does not reimplement Rust tests/proofs/admission rules as Python conditionals.

## B-04 Candidate and receipt contracts

Candidate canonical inputs: repository/source commit/tree and clean-state check; product version; support profile and complete gate inventory; Cargo/tool/corpus locks; tool/image digests; recipe/validator hashes; scoring/planner/wire/storage schema versions; expected artifacts and consumer/install recipe. Canonical serialization excludes nondeterministic timestamps from the candidate ID and forbids floating-point JSON values. Hash it with a named hash algorithm/version.

Receipt: candidate ID, gate/partition ID, attempt ID, command argv and recipe digest, exact input identities, tool/worker/environment facts, start/end/outcome, output/artifact digests, log/raw-evidence locations and digests, and explicit reuse linkage. Preserve failed attempts. Distinguish pending/running/passed/failed/cancelled/timed-out/unsupported/excluded; only an allowed, complete passing inventory qualifies. Duplicate logical IDs with conflicting evidence, missing/unreadable artifacts or a foreign candidate invalidate aggregation.

Same-candidate evidence reuse only for v1. Cross-candidate reuse requires a future checked input-closure design; a prior pass cannot be relabelled with a new SHA. A changed compiler/prover/corpus/policy/recipe/inventory creates a different candidate and invalidates affected evidence. Unknown dependency means rerun. Admission is an exact set comparison, never “all jobs that reported were green.”

The controller/worker may stop when the owner's machine sleeps. Reconcile after restart; pending work stays pending. Buildbot acknowledgements and candidate persistence must bridge the gap between submitting a request and scheduling work. One heavyweight gate at a time initially; tune concurrency from current measurements. The October 4 disk/swap observations are historical, not permission to assume today's resources or clean storage.

## B-05 Required qualification inventory

| Gate ID | Content |
|---|---|
| `quality` | fmt, Clippy warnings denied, dependency/license audit, boundary/no-placeholder checks |
| `unit-property` | Core/trainer/engine/data/parser invariants and deterministic properties |
| `headless` | Required behavioural journeys and canonical cell frames |
| `pty-linux` | Small actual binary/PTY suite, terminal restoration |
| `persistence-linux` | Native SQLite/process-crash/reopen/migration/lock tests |
| `model` | Full TLA+ model inventory, stated bounds, named fault kills |
| `prove` | Required TLAPS/general proof inventory and calibration |
| `verify` | Same-source Verus kernels, obligation inventory, production mappings |
| `correspondence` | Models/kernels mapped to real paths; event/cancellation/barrier/receipt negative campaigns |
| `corpus` | Licensed pack manifests, count/quality/capacity, unseen benchmark inventory |
| `performance` | Bounded resources, latency distribution and recorded environment |
| `package-install` | Exact candidate bundle, static ELF, fresh-home install/run/replay/backup smoke |
| `wsl-terminal` | Recorded real Windows-terminal→WSL Linux executable journey with honest keypad capability facts |
| `buildbot-recovery` | Duplicate nomination, detach/reconnect, worker/controller restart, no missing-gate admission |

These are logical receipts, not fourteen independently rebuilt containers. Cheap checks share prepared inputs and a single worker. Human terminal qualification may be a clearly recorded owner-assisted evidence step; do not fake it from a Linux-only CI container. Receipt schema must support manual evidence with identity and result fields. It stays required until completed.

## B-06 Owner bundle and install

Bundle contains binary, base corpus if not embedded, licences/notices, version/build-info, config example, manifest/digests, acceptance summary and links/digests for retained gate evidence. Sign the immutable manifest using the shared owner-authorized signing identity outside the build worker; use the shared trusted distribution mechanism if already deployed. No signing key goes into a product repo, worker image or general test job. If signing infrastructure is not ready, the output remains an explicitly unpromoted development bundle, not the final accepted owner release.

Install from the actual bundle into a private versioned directory, verify signature/digests/target before changing the active binary link, and retain the prior executable. Default binary link can be `~/.local/bin/cadence`; document PATH once. Interrupted/corrupt/wrong-architecture installation must not overwrite the working version. Binary rollback does not imply database downgrade safety: refuse a newer schema and provide validated backup/restore guidance.

Fresh-home consumer journey: verify bundle → install → launch offline → complete a short real and headless drill → quit → reopen saved progress → backup/export → reject a tampered/wrong-signer/wrong-target bundle. Use only the installed binary/content, never a workspace symlink. Verify Docker interactive run separately with a persistent volume and non-root ownership.

Public crates.io/GitHub-release/site promotion is optional and outside default acceptance. If later requested, publish the qualified bytes or explicitly verify rebuilt equivalents, download them through the real consumer path and retain per-channel receipts. Never move an existing tag or overwrite a package to repair a partial publication.
