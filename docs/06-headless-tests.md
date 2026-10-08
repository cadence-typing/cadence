# Headless driving, replay and a focused test suite

## H-01 Same product, inspectable without a terminal

Deliver these CLI shapes:

```sh
cadence
cadence doctor --json
cadence config check
cadence headless --stdio --profile-dir /absolute/private/test-profile \
  --clock virtual --services fixture --fixture-root /absolute/fixtures
cadence headless --script journey.jsonl --profile-dir /absolute/private/test-profile \
  --clock virtual --services fixture --fixture-root /absolute/fixtures
cadence replay recording.jsonl --read-only --capture-dir /absolute/captures
cadence export --output /absolute/progress.json
cadence backup --output /absolute/backup.sqlite3
```

`headless` is a separate subcommand, not a mock app. The engine, input ownership, effect request constructors, generation checks and Ratatui draw are shared with live mode. `TestBackend` replaces only the terminal output adapter. Virtual time and fixture services are explicit capabilities. Native-service headless mode can debug the actual data/import workers with an isolated profile and real clock; it cannot claim deterministic virtual timing if it reads the uncontrolled OS.

Headless never chooses the real profile implicitly. Require `--profile-dir`, or an explicit `--ephemeral-profile` that returns its private location. Persist fixture/synthetic provenance; fixture sessions cannot contaminate normal progress/benchmarks. Debug commands must not give a general “set any internal field” bypass.

## H-02 NDJSON protocol v1

Each line is one UTF-8 JSON request, maximum 1 MiB. Envelope: `v:1`, `id` as a decimal string representing a positive bounded u64, `op`, `args`, and optional `expected_state_revision` as a decimal string. JSON integers for dimensions/small durations must be checked; u64 IDs/revisions stay strings to avoid JavaScript precision loss. Unknown operations/fields, invalid enum values, unsupported versions, oversized lines and malformed JSON fail with stable codes and no state mutation.

Requests are processed in stream order. IDs strictly increase for new requests. Keep a bounded response cache of 1,024 completed requests, keyed by ID and canonical request digest. Repeating the same ID and same request returns its original result without another mutation. Same ID/different request returns `RequestIdConflict`; an old evicted ID returns `RequestExpired`, never re-executes. Malformed envelopes without a usable ID get a protocol error and no cache mutation. Reconnect creates a new stream scope; replay/trace identity is separate. This is not an exactly-once distributed network RPC claim.

Response: `{v,id,ok,result}` or `{v,id,ok:false,error:{code,message,details}}`. Successful results include `state_revision`, `event_seq` and (when relevant) `durable_commit_seq`, all strings. `ok` means the requested synchronous operation was applied/validated; it does not imply async jobs or persistence completed. stdout contains protocol records only; stderr contains bounded diagnostics. Machine consumption must not parse decorative CLI output.

Operations:

| Operation | Meaning |
|---|---|
| `hello` | Negotiate version and report build, services, clock, limits, capability facts; select named fixture scenario only in fixture mode |
| `key` | One normalized key event with modifiers, press/repeat/release kind and source evidence |
| `text` | Explicit synthetic content expansion, one shared-path key event per training unit; configurable inter-key microseconds in virtual mode; never maps live paste into typing |
| `advance` | Advance virtual clock by checked duration and deliver due clock events; forbidden with real clock |
| `resize` | Same viewport event as live resize |
| `action` | A documented typed user action, validated by the same command dispatcher |
| `barrier` | Await a precisely scoped causal work boundary; timeout is failure |
| `snapshot` | Structured state/read-model observation without mutation |
| `capture` | Shared renderer to canonical cells/styles/cursor plus optional readable text |
| `deliver` | Release a named pending fixture completion, or its configured stale/fault variant; fixture mode only |
| `quit` | Normal finish/flush lifecycle; returns saved/failed outcome; EOF also follows documented shutdown policy |

Normative request shape is [the schema](../contracts/headless-request.schema.json). `hello` is the first accepted request. `expected_state_revision` is checked before mutation: mismatches return `StaleState`; never guess which screen an agent meant to act on. Snapshot/capture responses list discoverable available actions and input owner so the agent can drive actual UI states.

Schema validation is necessary but not sufficient: parse decimal ID strings with checked u64 conversion and reject overflow. Validate operation-specific action arguments, capability restrictions and state preconditions before dispatch. A `text` request validates its complete expansion and resource budget before admitting its first key; it must not partly type a malformed request.

In virtual mode, `key` arrives at the current tick. `text` places its first unit at the current tick, then advances by `inter_key_us` before each later unit, delivering due clock events before the unit at that tick. Neither command adds an implicit delay after its last unit. Use `advance` for leading/trailing delays. Checked time arithmetic rejects overflow. Fixture transcripts must state this convention; request processing duration never becomes practice time. Live mode timestamps ingress using the monotonic clock as specified in the learning contract.

Fixture scenarios are registered immutable assets (for example `scoring-cat` and `daily-small`), each with pack identity, seed, services and initial state. `deliver` refers to a real pending fixture job ID exposed by snapshot; it cannot manufacture a completion for arbitrary production state. Known adverse variants deliberately exercise full freshness guards and are tagged as fixtures in traces.

## H-03 Barrier semantics

Two scopes:

- `causal`: captures all work caused by requests through a specified event sequence, including descendants spawned when their completions are consumed. It completes when those jobs/results/pending dispatches are drained. It excludes later unrelated actions and periodic UI clocks.
- `durable`: includes the causal scope and requests/awaits a checkpoint covering the captured session watermark. The result identifies that watermark. A later queued key is not accidentally declared persisted.

Every effect gets an origin/causal group inherited by follow-ups. Jobs stay outstanding through cancellation acknowledgement and result consumption, not merely until a worker computes a value. Register child work before marking its parent drained. A barrier must not return in the gap between a parent completion and child registration. Virtual mode pumps deterministic service deliveries; it does not sleep until “the screen seems quiet.” Native mode waits on actual completion signals with a bounded watchdog and returns pending job IDs on timeout.

Reject `through_event_seq` beyond the latest admitted event, or outside this driver session. Event sequence zero denotes the initial state. Barrier observation does not advance virtual practice time; watchdog time is a separate wall-clock safety bound. A durable barrier explicitly admits its checkpoint request and includes that request in its own scope.

Limit each request to one active barrier in the sequential v1 driver; this avoids speculative concurrent RPC complexity. Control/read streams continue only as explicitly allowed. A fixture barrier with undelivered jobs fails with their IDs at its watchdog bound rather than silently auto-succeeding. The “auto” fixture service policy, where enabled for ordinary examples, deterministically produces registered fixture results and is recorded in hello.

## H-04 Captures and replay diagnostics

Canonical frame: dimensions, row-major cells (symbol, display width/continuation marker, foreground/background semantic or resolved colours, modifiers), cursor visibility/position and prepared-view revision. Plain text is a convenience view that omits continuation cells beneath wide glyphs. Golden tests compare meaningful cells/styles/cursor, not ANSI escape encodings. No fake screenshots drawn independently of Ratatui.

Validate cross-field frame invariants in Rust: exactly `columns × rows` cells, in-bounds visible cursor, valid wide-cell continuation pairs, and no continuation crossing a row boundary. The JSON schema describes field types and limits; it does not establish these relationships by itself.

`snapshot` exposes session phase, input owner, active target identity (content only when permitted), counters with numerators/denominators, active/elapsed time, planner basis/reason, pending jobs/keys, saved watermark and available actions. Avoid exposing unstable collection internals as the stable protocol. Version the wire shape. Debug replay reports the earliest diverging state/effect/frame and retains the exact seed, input tape and capability configuration.

## H-05 Test pyramid and initial inventory

**Pure unit/property tests:** Rust `proptest` over scoring, typed conversions, budget apportionment, lane quotas, review state, parser limits, clock transitions and ID rejection. Assert meaningful invariants and independently computed edge examples. Use persisted regression seeds. Testing random data alone does not replace boundary fixtures.

**Deterministic headless integration:** approximately 12 named journeys, each exercises the actual binary or public driver and shared renderer:

1. Fresh profile → first-run choices → daily plan → first exercise; all initial figures genuinely unknown.
2. Mistype/backspace/correct/restart: first outcomes preserved, attempt counts correct, abandoned trial visible.
3. Numpad leading zero, decimal and Enter: exact string behaviour and clean-entry tracking.
4. Number-row/numpad identical content: histories separate; legacy capability marked unverified.
5. Virtual phase deadlines, manual pause/resume, focus interruption and exact-boundary key.
6. Late planner success/failure after session restart/view generation reuse: no stale mutation.
7. Save → lost acknowledgement → retry → reopen: one durable contribution, no double learning update.
8. Parent worker completion spawns child: causal/durable barrier cannot return early.
9. Theme/config changes: valid update, invalid config, comment preservation/conflict, same learning state.
10. Tiny terminal resize/recovery, Unicode UI labels, cursor/error styles, no panic/clipping beyond viewport.
11. Full trace → replay: same state/effects/cells; one tampered/missing event fails at the first divergence.
12. Unseen benchmark and synthetic/imported contamination exclusions; sparse evidence explanations.

Keep each journey short; do not build an enormous screenshot matrix. Use a representative canonical frame for Today, practice, numpad, progress, settings, small-terminal and save-error states in dark/light/terminal themes, with boundary geometry tests in the cheap renderer layer. A fix adds a targeted regression when it protects a real contract.

**Native persistence integration:** real SQLite temporary homes; process killed before commit, after commit before ack, and migration interruption; incompatible schema, locked writer and full/unwritable storage. Fault hooks are explicit test-only adapter hooks, excluded from release builds. These are process-crash tests, not proof of power-loss safety.

**Real PTY smoke:** 3 tests using `portable-pty` (or justified Rust PTY crate): launch/type/quit with alt-screen/raw-mode restoration; SIGWINCH resize with non-empty new frame; abnormal exit/SIGINT cleanup and terminal capability negotiation/legacy fallback. Use event/output predicates and watchdog deadlines, never arbitrary sleeps or detailed speed assertions. Capability-specific parsing gets cheap byte-fixture tests; manually qualify one real WSL terminal and one Linux terminal. PTY simulation cannot prove every terminal's keypad reporting.

**Agent conformance:** a thin Python example may issue NDJSON via `subprocess` for convenience, but expected scoring, state rules and production test authority remain Rust. Maintaining Python e2e is not required. No pyte screen scraping as the default behavioural oracle.

## H-06 Performance evidence

Record corpus size, build profile, CPU/kernel/container/terminal and load. Initial budgets on the admitted worker: reducer p99 ≤1 ms, normal frame preparation+draw p99 ≤8 ms at 120×40, input-arrival-to-frame p99 ≤33 ms under ordinary practice with a background save/index job. Treat these as measured acceptance targets with baseline calibration, not universal hardware promises. No sleep-based unit assertions. A reproducible regression beyond the budget requires optimisation or an explicit revised documented budget, not deleting the benchmark. Bound memory/backlog under a controlled result storm; overload must preserve history and report invalid timing.
