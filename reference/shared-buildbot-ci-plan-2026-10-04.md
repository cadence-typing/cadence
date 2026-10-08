# Shared CI and release plan — Buildbot, owned Linux workers, optional publication

Updated: 2026-10-04.
Canonical shared location: `/home/tarek/workspace/SHARED-BUILDBOT-CI-PLAN.md`.
Applies initially to Gripsack, Strop, Rootle, and their release consumers/sites.

## 1. Read this first: decisions, authority, and current status

This is the cross-workspace implementation handoff requested by the owner. It records the discussion and the owner's latest decisions; it does not claim that any CI infrastructure or product migration has been implemented.

### Owner decisions

1. **Buildbot is the selected direction.** Keep project-owned build/gate commands; use an existing scheduler rather than inventing a CI engine.
2. **Local WSL is the selected initial host.** Run the Buildbot controller and initial Linux x86_64 worker on the owner's existing Windows/WSL workstation. No rented machine, Mac setup, or cloud subscription is part of this deployment.
3. **macOS is to be dropped as a supported target across the projects.** This supersedes the earlier proposal to maintain a native Mac worker. The owner is not an active Mac user and does not need this target now.
4. **The owner is the main user; breaking the existing delivery workflow is acceptable.** Public registries, release tags, Homebrew, and website deployment need not be on the critical path to using an accepted build.
5. **Required gates still determine acceptance.** Changing the supported platform inventory is explicit owner policy, not permission to turn missing evidence into success. Preserve the substantive obligations for the retained targets.
6. **Buildkite is a researched alternative, not the chosen deployment.** Retain its pricing comparison for a future owner decision; do not introduce a subscription or hosted capacity into the local rollout.
7. **Optimize for an easier and faster release cycle, not a new infrastructure project.** One operator entrypoint should nominate a candidate, schedule the required work, expose durable status/resumption, and produce an installable qualified bundle. Infrastructure complexity must earn its place through an observed need.
8. **Disk cleanup is owner-managed and investigation is deferred.** The owner also has an external **1 TB SSD** available for possible CI storage (corrected from the earlier “1 GB” message). Its connection, actual device identity, contents, filesystem, and usable capacity have not been inspected. Do not clean or format disks as part of this planning work.

### What this document does not authorize or claim

- No services, accounts, VPNs, SSH access, firewall rules, runner registrations, subscriptions, or machines have been provisioned by this research.
- No existing workflows, installers, package versions, tags, branch protections, or secrets have been changed.
- The macOS removal is an approved direction, **not an already completed cutover**. Until the new platform policy and every affected consumer are migrated together, existing candidate requirements remain what their recorded policy says.
- Do not silently apply this policy to an in-flight release. Gripsack's dated 0053 handoff described a separate 0.44.1 release effort. Refresh its current state and coordinate the cutover with that release's owner; never cancel runs, move tags, or overwrite published packages as a shortcut.
- Linux ARM64 and Strop's Windows work were **not** removed by the owner. Their obligations remain separate from the macOS decision.
- The existing Gripsack instruction excluding fuzz-engine/corpus replay remains in force for this work. Record the named exclusion honestly; do not dispatch those lanes without a new owner instruction.

An agent working from another repository should read this document, then the relevant repository's current plans and gate producers. Do not treat the older Gripsack checkout used for this conversation as the latest release source.

## 2. Problem and evidence

The source handoff is [Gripsack plan 0053](gripsack/plan/0053-release-ci-research-handoff.md). It distinguishes publication defects, partial registry publication, queueing, repeated preparation, artifact identity, native-platform constraints, filesystem ownership, and unreliable observation.

Read-only GitHub API observation of candidate `f8c68f6a80e08ad511896274c19e3d14d6919b03` found:

| Observation | Result |
|---|---:|
| Full CI, creation to final update | 14h31m02s |
| CI inventory | 203 jobs; 202 successful, one explicitly waived/skipped fuzz job |
| Caller canary, job creation to execution | 7h19m27s |
| Caller canary execution | 32m20s |
| Linux persistence partitions | 96; median wait 9h22m40s; median execution 27m46s |
| macOS persistence partitions, historical supported scope | 96; median wait 12h37m12s; median execution 12m11s |
| Persistence execution sum | 66.11 runner-hours, **not CPU-hours** |
| Historical Mac persistence compilation sum | About 2.82 runner-hours |
| Historical Mac persistence test-step sum | About 14.01 runner-hours |
| Formal verifier lane execution | 4h20m19s |

Evidence: [CI run 37134187816](https://github.com/gripsack-dev/gripsack/actions/runs/37134187816), [caller run 37134189898](https://github.com/gripsack-dev/gripsack/actions/runs/37134189898). All 203 returned CI jobs carried the same candidate SHA. These are historical measurements, not qualification of a later patch or measurements of Buildbot/Buildkite.

Limits:

- The provider-side cause of the queue was not established. Do not assert a billing/concurrency/outage cause without evidence.
- Linux persistence uses a compound Compose build-and-test step, so these API timings do not separate compilation from test execution.
- No paired cold/warm, artifact-transfer, or replacement-worker benchmark has been performed.
- Removing macOS removes a deliberately withdrawn product obligation. It does not make Linux tests, formal work, or Linux ARM qualification free.
- A green source checkout did not cover the exact publication invocation in the recorded failure. Moving hosts alone does not fix this.

## 3. Target architecture

GitHub may remain the Git host. GitHub Actions should cease to be the indispensable scheduler and source of release state once the replacement is qualified.

```text
Owner / automation
       |
       | nominate exact source and qualification profile
       v
Buildbot controller — durable request IDs, queue, UI, logs
       |
       | commands over authenticated worker connections
       v
Owned Linux x86_64 worker(s) — existing Compose/native commands
       |
       +---- other retained native capabilities, when provisioned
       |
       v
Candidate-bound artifacts + gate receipts + complete-inventory admission
       |
       v
Qualified bundle
       +---- owner installation / owner channel
       |
       +---- optional registry publisher
                     |
                     v
             downloaded-package verification
                     |
                     v
             website / example / tap promotion
```

Buildbot workers initiate their connection to the controller. **They do not need publicly exposed inbound SSH.** SSH is an optional administration/debugging channel, not the scheduler's execution mechanism. Workers still need access to their source and declared dependency/artifact endpoints.

### Three distinct activities

- **Feedback:** cheap checks on changes; superseded feedback may be cancelled/coalesced.
- **Qualification:** nominate a frozen candidate and run all gates required by its recorded support/assurance profile. Preserve successful same-input work.
- **Publication:** optional promotion of an already qualified bundle to specific distribution channels, with channel-specific verification.

An experimental build is not an accepted release. A private accepted release must satisfy its declared inventory; being private is not a test waiver.

### Operator experience is an acceptance criterion

The normal path must be a single release/qualification request, not manually launching each gate, watching several unrelated runs, collecting checksums in chat, or rebuilding everything after one failure. The operator must be able to leave and return to the same candidate and see its exact next action. Public publication remains separate and optional.

Start with one local controller, one bounded worker, and existing gate commands. Do not add autoscaling, a cluster, a remote network, or another CI provider merely because the design permits them. Report both elapsed qualification time and hands-on operator steps during the pilot; a faster test run that needs more manual coordination does not meet the owner's goal. No speedup is claimed until measured.


### Keep the scheduler replaceable

Build/gate commands and receipt producers live in project repositories. Shared infrastructure owns scheduling, worker bootstrap, evidence storage, status presentation, and promotion policy. Do not rewrite Rust/TS/proof/e2e logic into Buildbot configuration.

A future versioned `ci-infra` repository may hold pinned controller/worker dependencies, Buildbot configuration, bootstrap code, and the shared receipt adapter. It is not created by this document. This root-level plan remains the shared discovery point until a deliberate move is agreed.

If Buildkite is chosen later, replace the controller adapter rather than the project's gates or acceptance semantics. Do not operate two competing release authorities indefinitely.

## 4. Owned workers: what the owner actually maintains

An owned worker is a machine/environment under the owner's administration running a small worker service, plus the required build tools, workspace/cache storage, and resource limits. It need not be a rented server or a separate physical machine.

Maintenance consists of:

- Service startup/reconnection and controlled shutdown.
- Pinned build environments and deliberate updates, separate from emergency host security maintenance.
- Disk/cache quotas, usable memory, and bounded concurrency.
- Clean workspaces and isolation from personal credentials/data.
- Controller database/configuration and promoted artifact backups.
- Periodic restore and reconnect checks.

There is no obligation to provide enterprise uptime. A sleeping or rebooting worker may delay work; it must not cause missing gates to become green.

### Initial placement after the macOS decision

- **Controller:** this workstation's WSL environment, using a dedicated controller account and private state. Start with SQLite for a small single-controller deployment; use a supported backup procedure, not an arbitrary copy of a live database.
- **Linux x86_64 worker:** the same physical workstation, in a dedicated CI environment/account with bounded access. Prefer its own Docker engine in a CI distro/VM over unrestricted use of the owner's general-purpose Docker Desktop daemon.
- **Mac Studio:** not part of the required setup. No macOS worker, Remote Login, VPN enrollment, or sleep-policy change is needed for the now-removed target.
- **Native Linux ARM64:** remains a separate capability if retained. An ARM64 Linux VM on the Mac Studio is an optional way to supply Linux-on-ARM execution without supporting macOS as a product target, but it requires deliberate setup and qualification. It is not assumed available or automatically included in this rollout.
- **Strop native Windows:** the Windows host is a possible future worker, not already-qualified Windows evidence. A Linux worker under WSL does not replace Windows/MSVC, native input, GUI, WSL-discovery, or hardware-specific checks.

A separate WSL distro/user reduces accidental contamination; it is **not** a hostile-code security boundary equivalent to a separate machine. WSL distributions and Windows integration share host trust. Do not run unreviewed public-fork code on this workstation. Keep the controller's private state inaccessible to worker accounts; never mount it into build containers.

If the pilot uses the existing Docker Desktop daemon, label it a trusted-code, non-publishing pilot. Docker access is privileged access. Do not run broad container/volume cleanup or let jobs access unrelated personal containers.

### Local capability observation, 2026-10-04

Only read-only commands were run. These observations are environment facts, not a passed gate:

| Item | Observed |
|---|---|
| Kernel/architecture | `6.18.33.2-microsoft-standard-WSL2`, `x86_64` |
| Docker server | `29.7.2`, Docker Desktop, x86_64, overlayfs |
| Docker-reported capacity | 32 logical CPUs; 32,404,889,600 memory bytes (about 30.2 GiB) |
| Docker context / Compose | `default`; Compose `v5.5.0` |
| WSL service manager | `systemctl is-system-running` returned `running` |
| Memory snapshot | About 30 GiB total; 15 GiB available |
| Swap snapshot | About 7.7 GiB of 8 GiB used |
| Workspace filesystem | Linux ext4; 87 GiB free; 91% used |
| Windows C: backing storage | 95 GiB free; 95% used |
| Administration tools | Windows `wsl.exe` and PowerShell reachable; `tailscale` not found on this shell's PATH |

Near-full swap is not proof of active thrashing, but it argues against immediately saturating all 32 reported CPUs. Start with **one heavyweight gate at a time**, bounded Cargo/prover parallelism, and measure peak RSS, CPU use, swapping, and disk growth before increasing concurrency.

Disk allocation is a first implementation prerequisite. The ext4 guest and Windows backing disk both need headroom. Do not interpret the guest's nominal one-terabyte capacity as physically available free storage. Do not prune the owner's existing Docker caches, builds, or files without permission; establish CI-owned storage and retention deliberately.

### External 1 TB SSD: proposed storage, not a formatting instruction

The owner's SSD is a useful candidate for CI workspaces, dependency caches, Docker layers, and temporary build output. Keep its use optional until the owner connects it and its device identity, existing data, health, filesystem, free capacity, and WSL accessibility have been checked.

- Prefer Linux-native ext4-backed storage for the CI environment. A dedicated WSL distribution/virtual disk stored on the external drive is a candidate layout; avoid running the filesystem-sensitive builds directly in a Windows-mounted `/mnt/<drive>` directory.
- Reformatting the physical SSD is not automatically necessary to host a WSL virtual disk. Choose the layout after inspecting the actual device and supported mounting/import options. Any destructive operation requires explicit approval of the identified disk and confirmation that its data is expendable or backed up.
- Establish CI-only paths, cache quotas, and retention. A larger disk must not merely postpone unbounded cache growth.
- Keep small controller state on reliable internal storage initially, with a supported backup. Back up promoted bundles and acceptance evidence separately; disposable caches are not the release archive.
- A missing/disconnected drive must stop admission of affected jobs, not silently write into a fallback directory on the full internal disk. Verify the expected storage identity before execution and stop jobs before safe disconnection.
- Measure the actual SSD/interface performance and available space before increasing concurrency. Do not assume the advertised capacity or “external SSD” label establishes a performance result.


## 5. WSL qualification and availability boundary

WSL2 is a real Linux kernel environment and can be a useful x86_64 Linux worker. The existing Docker-first commands are a good starting point, but each required feature must be exercised before admitting the worker.

- Keep checkouts, scratch homes, permission fixtures, caches, and filesystem-fault fixtures on Linux-native storage, not `/mnt/c`, SMB, or another Windows-backed filesystem.
- With Docker Desktop, record the **actual test container mount/filesystem and daemon host**, not only the filesystem of the source checkout. A bind mount/export path can have a different behavior from an in-container native volume.
- Exercise the actual confinement prerequisites, namespaces, seccomp/capability behavior, process supervision, SSH/PTY behavior, and Docker/BuildKit provider behavior used by each repository.
- Label the receipts with WSL kernel, Docker Desktop/engine version, container image digest, architecture, filesystem/mount type, and granted capabilities.
- Software fault injection and process-crash tests remain those kinds of evidence. They do not establish bare-metal power-loss behavior, arbitrary filesystems, native systemd-manager equivalence, or KVM/nested-virtualization capability.
- If an obligation needs a facility unavailable under this worker, use another admitted Linux environment or leave that obligation blocked. Do not label WSL unsupported merely because it is virtualized, and do not label it universally equivalent to every Linux host either.

Use systemd supervision with explicit account, HOME, PATH, working directory, logging, and restart policy. Also arrange and verify how Windows starts the required WSL distribution after reboot. Microsoft documents that systemd services do not by themselves keep a WSL instance alive. Closing an observer terminal, stopping a distro, Windows sleep, Windows update/restart, and Docker Desktop restart need separate lifecycle checks.

For an owner-operated setup it is acceptable to start a qualification session while the workstation is awake and let later jobs wait when it is unavailable. Do not disable sleep, disk encryption, or Windows security settings merely to make CI look always-on. Any always-on policy is an explicit owner choice.

### Network defaults

Initially keep the UI/API on loopback and avoid router port forwarding. Windows-to-WSL localhost access is supported, but verify the actual topology used by the worker environment.

If a remote worker is later added, prefer a private LAN/VPN connection with authenticated worker credentials and narrowly scoped access. Tailscale or WireGuard are optional transports, not required CI dependencies. Restrict who can reach the controller's worker endpoint and administrative UI. Tailscale's initial allow-all policy is not a substitute for deliberate access rules; VPN rules also do not automatically restrict unrelated LAN access.

No network scans, SSH host enrollment, or remote login changes were performed. The Mac Studio's OS, RAM, free disk, availability, and network access were not inspected because it is no longer needed for macOS support.

## 6. Platform policy and complete macOS cutover

### Supported scope

| Scope | Decision |
|---|---|
| Linux x86_64 | Initial owned-worker capability and retained target |
| Linux ARM64 | Retained where currently supported; native worker still needs provision/qualification or an explicit separate owner scope decision |
| macOS ARM64 / Intel | Owner-authorized removal across the projects; implementation pending |
| Strop Windows work | Not removed; preserve its actual current commitments and distinguish protocol checks from full GUI/hardware claims |
| Mac hardware as a Linux ARM host | Optional infrastructure choice, not renewed macOS product support |

The initial x86_64 worker is not by itself a complete replacement for every retained platform. Existing native ARM capacity may remain as an explicitly transitional provider lane. Complete GitHub Actions retirement is blocked until every retained capability has a replacement or the owner explicitly changes that support obligation.

### Cutover acceptance checklist

Perform a coordinated clean cutover per project; do not merely add `if: false` or mark Darwin jobs skipped.

1. **Record the support-policy revision and first affected candidate/version.** Update current plans, support tables, compatibility/evidence ledgers, required inventories, and release criteria. Keep historical reports and immutable published assets truthful.
2. **CI and aggregation:** remove obsolete macOS jobs, runner labels, preparation, native fixtures that only support the removed platform, and required-job entries. Update the admission checker and its behavior-based calibration together. Missing retained Linux work must still fail.
3. **Gripsack persistence:** the historical matrix is 2 platforms × 6 scenarios × 2 faults × 8 shards = 192 partitions. After the explicit platform cutover, the corresponding retained Linux x86_64 inventory is **96 logical partitions**, each still covering all assigned measured cuts and both drift states. Change the declared policy and validator, not just the YAML. Linux ARM obligations elsewhere remain separate.
4. **Release production:** remove Darwin core/helper artifacts, platform-specific packaging/qualification, helper pins and asset expectations that are obsolete under the cutover. Replace magic artifact-count checks with the exact retained target inventory. Preserve Linux x64/ARM artifacts unless separately changed.
5. **Install/update/provisioning:** installers, self-updaters, helper bootstrap, manifests/catalogs, target selection, and package/tap integrations must agree. A removed platform should receive an explicit unsupported-target result before mutation; do not leave dead download URLs or silently select a wrong-architecture binary.
6. **Product code/dependencies:** remove now-obsolete maintained macOS implementations, platform dependencies, launchd/VZ integration, conditionals, and aliases whose only purpose was the removed support. Keep genuinely shared logic, vendor sources, and immutable historical schemas/fixtures where they remain necessary. Do not mass-delete every `darwin` string or reinterpret retained state.
7. **Public surfaces:** README, install instructions, support claims, release templates, example repositories, sites, Homebrew casks/taps, and machine-readable catalogs must reflect the same support boundary. Unsupported future Mac updates must not overwrite an existing working installation.
8. **Schema/history discipline:** if removing a target changes a wire or persisted contract, follow that repository's schema/versioning and migration rules. Read-only historical compatibility is not the same thing as advertising current runtime support.
9. **Native evidence:** retain all native Linux/ARM and Windows obligations that still apply. Cross-compilation or emulation does not manufacture native execution evidence.
10. **Proof/runtime preservation:** platform removal does not waive retained platform-independent theorems, calibrations, source attribution, real-caller journeys, or installation integrity.

Restore macOS only through a future explicit support decision, complete implementation, and actual native qualification. Do not retain an untested hidden release path “just in case.”

## 7. Shared execution and scheduling contract

Each project declares gates as ordinary commands with:

- A stable gate identity and required worker capabilities.
- Dependencies on other gates/artifacts.
- Declared inputs and exact expected outputs/evidence.
- CPU/memory/concurrency budgets and execution timeout.
- Side-effect class: qualification, staging, or publication.
- Its support/assurance profile and any explicitly authorized exclusions.

The shared scheduler transports and schedules this contract. It does not implement a second version of the project's validation rules.

### Reuse existing producers

Starting points, to refresh against the implementation candidate:

- Gripsack: `scripts/check_ci_gate.py`, delivery/evidence checkers, Compose gates, native binary and reproducibility checkers, helper distribution scripts, and actual caller/installer journeys.
- Strop: `verification/freeze.py`, its source-bound evidence producers, `.github/scripts/publish-order.py`, and `.github/scripts/release-catalog.py`.
- Rootle: Compose test/e2e/model commands, native/PTY checks, external provider conformance, native artifact verification, and publication workflow.

Strop already derives its Cargo publication order from dependency edges. Generalize or reuse that approach instead of maintaining more ordered crate-name lists.

### Bounded scheduling and artifact preparation

- Freeze versions before building anything intended for promotion. A later version bump may change embedded bytes and invalidates the relevant artifact/evidence.
- Build once **per required component, profile, platform, and build recipe**. Debug-suite artifacts, release binaries, helpers, and SDK packages are not interchangeable.
- Preserve deliberate independent clean builds required for reproducibility; do not use the same cached binary as both rounds.
- Prepare candidate-bound Linux images/tool bundles, then execute many logical partitions through a bounded pool. A logical receipt need not require a fresh machine or a fresh compile.
- Keep fixture homes, process groups, scratch trees, Docker resources, and report destinations isolated per attempt/partition. Regrouping must not share state that the existing tests deliberately isolate.
- Permit cheap feedback to progress without letting it starve a nominated qualification. Do not cancel a frozen release qualification or publication because a later development commit appears.
- Admission uses the exact required inventory, not matrix roll-up color, artifact counts alone, or “all jobs that happened to report succeeded.”

## 8. Candidate, receipts, and resumption

### Candidate record

One immutable candidate records:

- Repository identity, commit and tree; dirty development snapshots cannot masquerade as release candidates.
- Core/SDK/helper/component versions and compatibility relationships.
- Source/input manifest, dependency locks, compiler/prover/runtime pins, images and resolved environment identities.
- Gate/publisher/validator recipe hashes and support-policy hash.
- Required platform/gate/partition inventory.
- Caller, site, installer, and other consumer revisions where relevant.
- Expected artifact identities, with produced digests attached through immutable result records.

A candidate ID is derived from canonical recorded inputs, not a mutable branch, `latest`, or a human version alone.

### Gate receipt

Record candidate ID; gate and logical partition; source; worker capability/environment identity; exact command/tool versions; attempt; outcome; timestamps; output digests; and durable raw-evidence locations/digests. Record reuse explicitly with the producing receipt and the input-equivalence decision. Retain failed attempts.

Outcomes distinguish pending, running, passed, failed, cancelled, timed out, unsupported, and explicitly excluded. Required missing/unknown work is not success. A reused result is accepted only through checked identity and evidence policy, not by copying a prior green status.

### State model

```text
frozen -> qualifying -> qualified -> owner-promoted
                         |
                         +-> publishing -> partial
                                      \-> published-unverified -> complete
```

Publication state is per registry/package/version/digest and also aggregated for the compatible release set. Some uploads succeeding must not make the whole set complete.

### Recovery rules

- Buildbot request IDs, attempts, and durable candidate state outlive the CLI/assistant session. Observers can detach and reattach.
- Buildbot's database stores requests/state, but its default in-memory message queue does not make every trigger durable across controller restart. Persist the candidate, obtain an acknowledged request ID, and reconcile incomplete requests after restart.
- Restart incomplete gates at a meaningful boundary. Do not promise arbitrary subprocesses resume mid-instruction.
- Reuse completed same-input gates; reject stale, foreign-source, duplicate, missing, or unreadable evidence.
- A lost watcher connection is an observation error. Query the existing request before submitting a replacement.
- Before retrying an uncertain publication, inspect the registry. Match expected source/package bytes or an explicitly defined equivalent package identity; `already exists` alone proves nothing.
- Retry transport/rate-limit/index-propagation conditions under a bounded policy. Authentication, compilation, version, and digest failures require correction, not blind retries.
- Never move an existing release tag or replace an existing package to recover a run.

## 9. Cache and evidence invalidation

Initially permit retained gate evidence only within the same immutable candidate. Cross-candidate proof/test reuse is a later checked optimization: it needs complete declared input closures, calibrated invalidation, preserved producer identity, and a new consumer-candidate binding. A prior pass is never relabelled with a new SHA.

| Changed input | Required reconsideration |
|---|---|
| Rust source/features/Cargo locks | Dependent builds and runtime/native checks; formal gates that consume the changed production source/specification/toolchain |
| TypeScript/schema/embedded frontend | SDK tests/package, affected embedded-core builds, compatibility and real caller journeys |
| Helper source/locks/packaging/pins | Helper artifacts and native/reproducibility evidence; dependent core artifacts and helper-consuming journeys |
| Compiler/prover/image/runtime pin | Every build/proof/test whose assumptions consume that pin |
| Host OS/kernel/filesystem/capabilities | Corresponding concrete native/containment/persistence checks; do not infer a change to unrelated mathematics |
| Gate script/inventory/validator | The affected gate and admission/calibration evidence |
| Publisher/installer/signing policy | Exact preflight, negative identity/verification cases, channel installation/update checks; product builds too if code is embedded |
| Documentation/site-only inputs | Documentation/site checks only when a checked input closure proves executable/package inputs unchanged |

Unknown dependency means rerun. Cache keys include relevant target, profile, toolchain, lock, source/recipe inputs, and trust scope. Separate trusted and untrusted cache namespaces. Never share a writable Cargo target directory between concurrent incompatible builds.

Some existing Compose gates execute during image construction. A cached successful layer is retained evidence, not a fresh test execution. Record that distinction or move qualification to explicit execution without weakening its checks. Time-dependent checks such as vulnerability advisories have a freshness policy separate from byte-identical build reuse.

## 10. Owner distribution and optional public publication

The default deliverable is a qualified immutable bundle containing the retained-platform binaries, applicable helpers/SDK packages, manifests, checksums, evidence, and an authorized signature. A mutable owner-channel pointer may identify an immutable bundle; it must not silently mutate the bundle.

Private distribution is a genuine consumer journey:

- Install the binary/package from the actual bundle in a fresh private home.
- For Gripsack, install the packed SDK instead of a source-directory link; exercise deliberate pinning, the embedded frontend, frozen locks, and applicable helper provisioning.
- Migrate installer/updater/helper endpoint resolution to the owner channel where needed. Do not assume current hard-coded public URLs already support private bundles. Preserve digest/trust checks; a test-only mirror override is not proof of default production bootstrap.
- Validate the compatible core/SDK/helper set before changing the channel pointer.
- Keep the prior installed build usable on failed verification/promotion. Application data rollback remains governed by the project's actual compatibility rules, not by a generic promise to downgrade binaries safely.

Public publication is optional but not approximate:

1. Non-publishing preflight uses the actual working directories, relative arguments, output ownership/layouts, package contents, and native dependency/SBOM checks.
2. Qualify/promote the same bytes where possible. If a tool repackages/rebuilds on upload, retain inputs and verify the uploaded package against the qualified expected identity; do not assert equality without checking it.
3. Publish according to a validated dependency DAG with per-package receipts.
4. Retrieve actual registry packages/native assets and exercise real installation/default provisioning. Local package success is not registry-install success.
5. Advance compatible consumer pins, site/tap/catalog promotion, and public completion status only after their required verification. Partial publication remains visible and resumable.

### Trust policy

Gripsack 0053 records a policy tied to authorized GitHub-hosted release provenance. Leaving Actions requires an explicit verifier/signing-policy migration.

Use an owner-authorized signing identity whose private key is unavailable to general build workers, and pin the verification key/policy on consuming machines. Bind repository, source, recipe/policy, artifacts, and acceptance evidence. Signing authenticates the authorized producer/approval; it is not an independent theorem that compilation was correct.

Do not route self-built artifacts through an Actions signing job to imply GitHub-hosted compilation. Update installers/verifiers, examples, negative tests, and published trust claims together. Old signatures and published packages remain immutable history.

## 11. Storage, security, and operations

- The controller is an execution authority. Protect its configuration, worker credentials, request permissions, and administrative endpoints. Project code is executed on workers, not imported as arbitrary controller configuration.
- Workers have no release-signing key, registry publish token, personal browser/keychain profile, personal SSH agent, or unrelated production access.
- Use fresh workspaces/private fixture homes and scoped artifact-upload credentials. Docker-socket access and raw native shell execution require an explicit trusted-worker boundary.
- Do not expose public fork builds to persistent owner hardware. Future external contributions need a separate disposable/untrusted execution pool and cache scope.
- Retain promoted artifacts/manifests/acceptance evidence for the release's supported lifetime. Start with 30–90 days for non-promoted attempt logs, plus enough failure history for investigation.
- Cache eviction must never delete the only copy of a qualified artifact or acceptance receipt. Logs/prover caches/traces do not belong in source control.
- Keep at least one durable copy of promoted bundles and controller backups outside the CI worker's writable storage/failure domain. A mounted writable backup is not independent protection.
- Test restore, not just backup creation. A single-controller service is acceptable initially; do not introduce a cluster merely to avoid a documented restore procedure.
- Minimum tool versions, clock/timestamp handling, output UID/GID, output directory creation, available disk, and resource budgets belong in preflight. Never broad-chown/chmod the owner's repository or HOME to repair an output mount.

## 12. Buildkite alternative: actual costs and limitations

This is a researched alternative, not a subscription decision. Prices were checked on 2026-10-04 against the primary sources below. All quoted Buildkite prices are **USD, not GBP**, before applicable taxes/conversion and extra metered usage.

### Controller versus compute

| Arrangement | Buildkite charge model | What the owner still operates |
|---|---|---|
| Buildbot + owned WSL workers | No Buildbot subscription; local hardware/electricity/storage costs remain | Controller, workers, backups |
| Buildkite Free + owned workers | $0 within plan limits | Workers and durable release storage |
| Buildkite Pro + owned workers | $30 per active user/month; 10 self-hosted agents included | Workers and durable release storage; no controller administration |
| Buildkite Pro + hosted Linux | $30 per active user/month **plus metered compute** after the included allowance | Pipeline/gate policy, credentials, release evidence/retention |

The Pro subscription is per active user in a Buildkite organization, not per repository. One active user can manage these project pipelines under that organization; it is not $30 for each project. Separate active automation identities may count as users; process/AI-agent count is not the billing definition. Check the actual account/organization quotas before purchase.

**Correction to the early free-tier suggestion:** Free has a four-hour job timeout. The observed Gripsack verifier ran for 4h20m19s. Free is not a proven drop-in home for the current long lanes. Pro removes the general pipeline timeout, but hosted machine lifetime limits still apply: Linux currently up to eight hours; macOS up to four hours unless support changes the limit. Do not split a proof gate by omitting calibration or weakening its aggregate.

### Linux-only hosted cost after dropping macOS

Pro includes **4,000 Linux vCPU-minutes per month**, shared across the organization, not per project. Additional Linux AMD64 usage is **$0.004 per vCPU-minute**:

| Hosted Linux shape | Resources | Effective job-minute price | Job-hour price before the monthly inclusion |
|---|---|---:|---:|
| Small | 2 vCPU / 4 GB | $0.008 | $0.48 |
| Medium | 4 vCPU / 16 GB | $0.016 | $0.96 |
| Large | 8 vCPU / 32 GB | $0.032 | $1.92 |

Usage is metered to the second; startup/shutdown periods are excluded. Choose a shape from measured memory and runtime, not price per minute alone.

For **one active Pro user**, with all jobs on Linux Medium, the arithmetic is:

```text
monthly USD = 30 + max(0, total_job_hours * 60 * 4 - 4000) * 0.004
```

Hypothetical combined usage across **all projects**, not a forecast or provider benchmark:

| Monthly Linux Medium job-hours | Hosted compute | Platform + compute |
|---:|---:|---:|
| 10 | $0.00 | $30.00 |
| 20 | $3.20 | $33.20 |
| 50 | $32.00 | $62.00 |
| 100 | $80.00 | $110.00 |
| 200 | $176.00 | $206.00 |

Job-hours are summed across parallel jobs. Ten one-hour jobs consume ten job-hours even if they finish in one wall-clock hour. The included 4,000 vCPU-minutes cover 16h40m of Medium execution per month.

The earlier Mac-cost concern no longer applies to the planned product target set. For the historical comparison only, Mac Medium was $0.12/job-minute ($7.20/hour); 17.44 hours at that rate is $125.57. That was a rate-only calculation assuming unchanged GitHub job durations, **not** a Buildkite benchmark or part of the Linux-only budget.

### Reasons “everything on Pro” is still not established

- **Hosted Linux ARM64 is Enterprise-only/on request** in the current documentation. Keeping native Linux ARM64 means owned/other-provider ARM capacity or a separately quoted plan; do not represent it as included in the $30 Pro subscription.
- **Hosted Windows is in private preview**, using Windows Server 2022; it does not currently support Windows Server 2025, custom base images, caches, or interactive desktop access. It is not a demonstrated replacement for Strop's existing native/hardware requirements.
- Pro currently permits 256 combined hosted Linux vCPUs. A 4-vCPU shape permits up to 64 concurrent machines, not unlimited capacity. Actual queue behavior remains unmeasured.
- Buildkite's matrix limit is currently 50 jobs per step. Gripsack's retained 96 logical partitions must be represented as multiple steps, generated jobs, or bounded partition batches without changing the inventory.
- Hosted cache support does not waive cold-home checks or evidence retention. Pro artifact retention is currently 30 days, while build retention is 90 days. Preserve promoted bundles elsewhere before expiration.
- Pro lists 250 GB/month artifact storage and 2.5 TB/month downloads; additional storage/downloads and optional test-ingestion/package-registry usage may add charges. Do not treat the documentation's illustrative artifact overage rates as a customer quote.
- Spend alerts are informational, **not spending caps**. Bound admission/concurrency/runtime and observe actual billing. Confirm any required hard budget control before enabling unattended paid workloads.
- Buildkite offers special open-source pricing by application; do not assume eligibility, acceptance, or a discount before confirmation.

Decision rule: the chosen deployment is **local Buildbot on WSL with owned Linux capacity**. This Buildkite comparison is reference material, not a parallel implementation or purchasing task. If the owner later decides against controller administration, Buildkite Pro can replace that layer while retaining the same worker/gate/receipt contracts. Fully hosted Linux AMD64 would need a separate usage pilot and retained-platform decision. No estimated runtime improvement is promised.

## 13. Phased implementation and observable acceptance

These are executable work packets for future agents. They are not completed tasks merely because this plan exists.

### P0 — Support inventory and storage readiness

- Confirm the current source/release state of each project and appoint one integration owner for shared infrastructure.
- Record the macOS withdrawal in each project's current support/evidence policy; identify the first candidate using that profile. Inventory all producers/consumers from section 6 before editing.
- Keep Linux ARM/Windows obligations explicit. Identify their existing capacity; leave unavailable required work blocked rather than silently falling back to cross-only evidence.
- Allocate CI-owned disk/cache/artifact space and an independent backup location. Observe container mount semantics and resource headroom.
- Select pinned Buildbot/Python/worker dependencies and private account/service boundaries; do not reuse undocumented personal shell environments.

**Exit:** a reviewed target/gate inventory and resource/trust boundary, with no ambiguous Mac claims or silently removed retained targets.

### P1 — Local controller and one Linux x86_64 worker

- Bootstrap the controller/worker as supervised services; initially private and manual-triggered, with no publication credentials.
- Use a committed Rootle candidate for the first narrow end-to-end pilot. Run its existing Linux commands, package an artifact, consume it through a real CLI/PTY journey, and retain command/output/hash receipts.
- Demonstrate stop/start of the observer and worker, controller restart, duplicate request handling, and state recovery. Verify Windows/WSL startup behavior separately.
- Apply the agreed Mac cutover to Rootle before claiming its new supported release profile complete. Missing retained ARM work remains an explicit blocker.

**Exit:** the actual product path runs on owned Linux with durable, source-bound evidence and no dependence on an Actions observer session. A one-platform pilot is not the full release certificate.

### P2 — Complete platform cutovers and port the gate inventories

- Complete the section 6 cutover across Gripsack, Strop, Rootle and their affected consumers/sites, with each project's changelog/status/support documentation updated.
- Run Gripsack's retained Linux persistence inventory through prepared images and bounded slots; verify every measured cut/drift/partition receipt.
- Port retained Rust/TS/e2e/formal/native/helper/caller gates without changing their semantics. Keep named fuzz exclusions explicit.
- Supply every retained native capability, or obtain a separate explicit owner support decision. Do not retire the last required ARM/Windows execution facility prematurely.
- Measure cold/warm setup, compilation, proof/test execution, queue, transfer, peak memory, disk growth, and total time on exact candidates. Compare inventories and outputs, not just elapsed wall time.

**Exit:** complete retained-target qualification from the new system, with consumer-visible platform removal and no orphaned Mac artifact/pin/install paths.

### P3 — Qualified owner bundles

- Freeze the compatible artifact set, implement the owner-channel installation/update path where the products require it, and sign its immutable manifest through the separate authorized identity.
- Exercise installed packages, helper bootstrap, trust/compatibility failures, fresh private homes, frozen consumers, and retained-byte reuse.
- Verify the prior installation remains usable when publication or verification fails.
- Restore controller state and a promoted bundle from backup on a clean environment.

**Exit:** the owner can install and use an accepted release without public registry publication or GitHub Actions, with honest provenance and complete retained-platform evidence.

### P4 — Optional public publisher and Actions retirement

- Reuse the publication DAG, exact preflight, immutable bundles, per-package receipts, and partial-publication state.
- Exercise actual registry downloads/installers and compatible consumer/site promotion when public publication is requested.
- Migrate authorized signing/verifier policy and required checks deliberately. Keep existing releases immutable.
- Remove obsolete workflow paths and duplicated authority only after every retained gate and consumer has moved. Do not leave two competing “release complete” definitions.

**Exit:** complete optional public releases can be resumed safely, and no required execution still depends on the retired Actions workflow.

## 14. Required failure demonstrations before acceptance-authority cutover

| Case | Required result |
|---|---|
| Artifact from another source/candidate or wrong helper pins | Rejected before promotion or execution through an admitted install path |
| Missing/duplicate/overlapping retained persistence partitions | Aggregate fails; all required cuts and drift states remain necessary |
| Cancelled, timed-out, unsupported, skipped, or unreadable required gate | Cannot become green |
| Same version but unexpected registry bytes | Stop; do not accept `already exists` or overwrite |
| Lost response after successful upload | Reconcile actual registry identity and resume only missing work |
| Watcher/API failure | Preserve run identity and unknown observation; no fabricated job outcome or unnecessary rerun |
| Controller or worker restart during qualification | Completed receipts remain usable; incomplete work is reconciled/restarted honestly |
| Restart between trigger and scheduler processing | Candidate survives; missing acknowledged work is detectable/resubmittable |
| Source/lock/toolchain/recipe/inventory change | Affected evidence invalidates; unrelated reuse requires checked input closure |
| Cached image/gate layer | Report retained evidence accurately, not fresh execution |
| Package consumer retains a local source link | Clean installed-package journey fails until the real package is used |
| Wrong signer/repository/source/ref/policy | Verification rejects without mutating the existing install |
| Removed macOS target | Current consumer reports unsupported before mutation; no new Mac artifact advertised; historical releases unchanged |
| Wrong platform substitution | No x64 emulation/cross-build receipt admitted as native ARM; no WSL receipt admitted as native Windows |
| Root-owned output directory / insufficient disk | Clear preflight failure or scoped CI-output repair; no broad owner-repository mutation |
| Backup restore / expired SaaS artifact | Promoted bundle and acceptance evidence remain retrievable independently of temporary CI storage |

These demonstrations supplement the product gates; they do not replace required theorems or native evidence.

## 15. Agent coordination and evidence record

One integration owner owns controller configuration, shared receipt contracts, artifact storage, and the cutover decision. Project agents own their gate/consumer migrations. Agree stable candidate/gate/partition IDs and capability vocabulary before independent edits. Every PR/patch should state which policy revision it consumes.

Never use chat memory or a temporary tool artifact as the sole release record. Store raw run evidence outside source control with retained URIs and digests; maintain compact references here or in the future infrastructure repository.

| Item | Status at this handoff | Evidence / next action |
|---|---|---|
| Shared architecture and owner decisions | Recorded | This document |
| Historical Gripsack timing inventory | Observed read-only | Runs linked in section 2 |
| Local WSL/Docker/resource inventory | Observed read-only | Commands and values in section 4 |
| Local WSL hosting choice | Owner confirmed | One local controller and initial Linux x86_64 worker; no paid hosting in the rollout |
| Disk cleanup / external 1 TB SSD | Owner-managed cleanup; SSD proposed, not inspected | Deferred investigation; identify and inspect the connected device before layout or formatting decisions |
| Buildkite price arithmetic | Calculated from published rates | Section 12; hypothetical usage, not a benchmark |
| Mac Studio inspection/network setup | Not performed; not required for removed target | Optional Linux ARM hosting would be a separate task |
| macOS support removal | Owner approved; implementation pending | Complete section 6 and per-project consumers |
| Buildbot controller/worker deployment | Not started | P0/P1 |
| Owned-worker product qualification | Not run | P1/P2 |
| Owner signing/distribution migration | Not implemented | P3 |
| Public publisher migration | Not implemented | P4 |
| Purchase/subscription/network changes | None | Require a separate implementation/spend decision |

## 16. Source map and primary references

### Local project starting points

- [Gripsack research handoff](gripsack/plan/0053-release-ci-research-handoff.md).
- [Gripsack assurance and owner-authority contract](gripsack/plan/0048-review-response-0.42.0.md); consult the matching active source revision before implementation.
- [Inspected Gripsack candidate CI](https://github.com/gripsack-dev/gripsack/blob/1024283e19ebccc330c1ff19107430be64ecfdbc/.github/workflows/ci.yml), [core publisher](https://github.com/gripsack-dev/gripsack/blob/1024283e19ebccc330c1ff19107430be64ecfdbc/.github/workflows/release-core.yml). These links are source snapshots, not an assertion that this remains the latest revision.
- [Strop CI](strop/.github/workflows/ci.yml), [Windows lane](strop/.github/workflows/windows.yml), [release](strop/.github/workflows/release.yml), [candidate producer](strop/verification/freeze.py), [publication DAG](strop/.github/scripts/publish-order.py).
- [Rootle CI](rust/rootle/.github/workflows/ci.yml), [release](rust/rootle/.github/workflows/release.yml).
- Consumer/site workspaces include `gripsack-dev.github.io/`, `gripsack-site/`, `example-env-typescript/`, `stropdev.github.io/`, and `rust/rootledev.github.io/`. Determine the active source/deployment repository before editing; do not update two historical website copies blindly.

### Controller, OS, and networking

- [Buildbot architecture and worker-initiated connections](https://docs.buildbot.net/current/manual/introduction.html).
- [Buildbot worker concurrency and lifecycle](https://docs.buildbot.net/current/manual/configuration/workers.html).
- [Buildbot database and message-queue persistence boundaries](https://docs.buildbot.net/current/manual/configuration/global.html).
- [Buildbot service startup and worker accounts](https://docs.buildbot.net/current/manual/installation/misc.html).
- [Microsoft: systemd under WSL, including lifetime limitations](https://learn.microsoft.com/en-us/windows/wsl/systemd).
- [Microsoft: WSL networking](https://learn.microsoft.com/en-us/windows/wsl/networking).
- [Tailscale access policy defaults and limits](https://tailscale.com/kb/1018/acls).
- [Apple Remote Login](https://support.apple.com/guide/mac-help/allow-a-remote-computer-to-access-your-mac-mchlp1066/mac), researched before the Mac target was removed; no configuration was applied.

### Optional managed alternative

- [Buildkite pricing, active-user definition, inclusions and compute rates](https://buildkite.com/pricing/).
- [Buildkite self-hosted/hosted architecture](https://buildkite.com/docs/pipelines/architecture).
- [Buildkite service limits and timeouts](https://buildkite.com/docs/platform/limits).
- [Buildkite Linux hosted agents and ARM64 plan restriction](https://buildkite.com/docs/agent/buildkite-hosted/linux).
- [Buildkite Windows private preview and limitations](https://buildkite.com/docs/agent/buildkite-hosted/windows).
- [Buildkite hosted execution and metering](https://buildkite.com/docs/agent/buildkite-hosted).
- [Buildkite artifact billing; illustrative rates are not a quote](https://buildkite.com/docs/platform/artifact-storage-and-transfer-billing).
- [Buildkite spend alerts do not cap usage](https://buildkite.com/docs/platform/pricing-and-plans).
