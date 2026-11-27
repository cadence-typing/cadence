# Dependencies

Every direct dependency has a named purpose and an owner for its updates
(AGENTS.md). Versions are pinned by `Cargo.lock`; `deny.toml` is the policy
the `quality` gate enforces with a pinned cargo-deny (#95): an allow-list of
licences, crates.io as the only source, no wildcard requirements, and no
RustSec advisory against the RustSec snapshot `./ci prepare` fetched.
Duplicate versions are reported, not refused.

The owner of updates for every crate below is the Cadence maintainers
(`cadence-typing`); an update is a pull request that keeps `./ci quality`
green and, for anything that touches the terminal, SQLite or parsing, names
the change it brings in the commit message.

| Crate | Used by | Purpose |
|---|---|---|
| `serde`, `serde_json` | core, data, driver, app, xtask | Persisted records, the portable export, the headless NDJSON protocol and manifests |
| `rusqlite` (bundled SQLite, `backup`) | data | The progress store: WAL, `synchronous=FULL`, online backup; the bundled SQLite must stay at or above 3.54.0 (RUSTSEC-2026-0459, tested) |
| `toml` | trainer | The trainer policy files (`policies/*.toml`) |
| `toml_edit` | data | `config.toml` with line numbers for diagnostics, and comment-preserving rewrites |
| `ratatui` | tui, driver | The shared renderer and the in-memory buffer frames are captured from |
| `unicode-width` | tui, driver | Cell widths of what is drawn (pinned to ratatui's) |
| `crossterm` | runtime | Terminal input, raw mode and the alternate screen |
| `signal-hook` | runtime | SIGTERM and SIGWINCH as events |
| `sha2` | core, data, trainer, driver, app | Content, checkpoint, config and pack digests |
| `rand`, `rand_chacha` | trainer | The seeded, replayable plan generator |
| `jiff` (system time zone only) | app | The local UTC offset read once at session start (replaced chrono, unmaintained: RUSTSEC-2026-0543) |
| `proptest` | tests only | Property tests with persisted regression seeds |
| `tempfile` | tests only | Temporary profiles and homes |

Allowed licences (`deny.toml`): Apache-2.0 and MIT (the project's own and
most of the ecosystem), Unicode-3.0 (unicode-ident's data tables, alongside
MIT OR Apache-2.0), and Zlib for `foldhash` alone (ratatui's hasher). Any
other licence fails the gate.
