# Working rules for Cadence

These rules express the owner's requested quality constraints for the future project. They do not change instructions in reference repositories.

## Product and architecture

1. Training correctness is the product. Never improve displayed scores by forgiving corrected mistakes, excluding ordinary hesitation, or changing difficulty without recording it.
2. All application and UI state mutations pass through the engine's typed reducer. Workers return facts; widgets render snapshots. No sibling component mutation, interior-mutable global app state, or worker access to `&mut AppState`.
3. Input admission → reduction → presentation preparation → render must not await, perform filesystem/database/network I/O, launch a process, or do corpus-wide work.
4. Every asynchronous result carries the complete identity and dependency versions it was computed against. Check successful and failed outcomes. Cancellation does not prove freshness.
5. Headless, replay and live mode call the same reducer and renderer. Fixtures replace effect ports, never product logic. Do not add a demo-only practice path.
6. Ordered semantic events and durable acknowledgements cannot be silently dropped or coalesced. Only explicitly replaceable notifications/snapshots may coalesce, with their truth retained elsewhere.
7. Offline means the practice process starts and works with networking disabled, after installing the bundle. All normal history is local.

## Rust and maintainability

- Strong domain types for IDs, generations, revisions, units, positions, counts, probabilities and byte/cell/text lengths. No primitive soup, sentinel numbers, boolean mode combinations or stringly typed events.
- No naked policy numbers. Named, typed constants or validated policy fields own budgets, timeouts, limits, weights and thresholds; document units and rationale. Arithmetic identities `0`/`1`, enum encodings with a schema, and explicit test examples are fine. Naming `MAGIC_500` is not compliance.
- Checked arithmetic at all external-input/counter boundaries. Generations and revisions never wrap; retire an exhausted slot or stop the operation with a typed error. Saturation is allowed only when clipping is the specified operation, not to conceal lost data.
- Production libraries return typed errors. `anyhow` is limited to CLI/xtask aggregation. No `unwrap`, `expect`, `panic!`, unchecked indexing, ignored results, or warning suppression in normal application logic. Unreachable paths need a typed invariant and an explicit reviewable justification.
- `unsafe` is forbidden in owned application crates initially. If an unavoidable terminal/platform seam emerges, isolate it in one narrow module with an invariant, safe alternative assessment and tests; do not weaken workspace policy globally.
- Use `cargo fmt --check` and Clippy with warnings denied. Review targeted `#[expect(..., reason = ...)]` only for actual documented false positives; no blanket lint disabling.
- Aim for modules around 300–400 lines; 600 lines triggers review and decomposition; 800 lines requires an explicit exceptional justification in the same change. Tests count. Generated fixed tables are exempt and carry a generator/provenance link.
- Split by responsibility, not arbitrary line ranges. Functions with load-bearing branch order should usually become typed state transitions or decision tables.
- Prefer small concrete interfaces and established dependencies. Do not create a framework, plugin system, home-grown database, CI scheduler, allocator, terminal parser or PRNG.
- No hot-path full-state serialization, whole-corpus scan, unbounded vectors/channels, per-key SQL transaction or per-key allocation of the entire target text.
- No compatibility shim left behind when an internal interface changes: migrate every caller and delete the old implementation together. Persisted/wire schema compatibility is explicit and must never be silently dropped.
- Dependency versions and features are pinned by committed locks. Dependencies must have a named purpose, compatible licensing and documented security/update ownership. Do not use floating `latest` in qualification recipes.

## Verification and evidence

- Test contracts, behaviours and failure boundaries. Prefer the cheapest layer that exercises the real code. No coverage-percentage target, source-text tests pretending to be behaviour tests, or sleeps as deterministic synchronization.
- Tests run with an explicit temporary profile; never read/write the owner's real HOME, history or credentials. No network in product gates.
- Every required formal property has a named production mapping, positive run and negative calibration. Tool absence, parse failure, timeout, skipped proof or zero obligations is a failure/blocked result.
- Do not substitute a verified toy copy for the function used by the application. Same-source Verus kernels must be called by production. Differential testing is useful evidence, not a universal refinement proof.
- No `assume`, `admit`, `sorry`, unreviewed `external_body`, or circular lemma in claimed theorems. List tool/library/OS assumptions and approved trusted adapters.
- Preserve failing attempts and exact candidate identities. A green cache entry or successful subset is not complete qualification.
- Docker Compose is the canonical reproducible gate route. Host Rust is optional for iteration. Buildbot schedules the same repository commands; proof/test logic does not live in controller configuration.
- Do not mutate shared CI infrastructure, disk layouts, public releases or other projects as an incidental fix. Implement the Cadence adapter and report missing shared prerequisites concretely.

## Work completion

Maintain a compact status ledger mapping requirement IDs to implementation, tests, proof claims and receipts. Finish all authorised P0–P8 work; do not replace required behaviour with a TODO and describe the feature as complete. Report actual limitations and blocked gates precisely. Human-learning effectiveness is evaluated empirically; software proofs do not certify a guaranteed WPM gain.
