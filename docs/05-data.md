# Local persistence, privacy and corpus delivery

## D-01 XDG layout

| Location | Contents | Deletion consequence |
|---|---|---|
| `$XDG_CONFIG_HOME/cadence` (default `~/.config/cadence`) | `config.toml`, user theme files | Preferences reset |
| `$XDG_DATA_HOME/cadence` (default `~/.local/share/cadence`) | `progress.sqlite3`, imported packs, content manifests, explicit backups | Learning/imported data lost; never treat as cache |
| `$XDG_STATE_HOME/cadence` (default `~/.local/state/cadence`) | Bounded operational logs, transient UI state, explicit debug traces | Diagnostic/session-view state lost |
| `$XDG_CACHE_HOME/cadence` (default `~/.cache/cadence`) | Rebuildable corpus indexes, generated previews | Rebuilt automatically |
| `$XDG_RUNTIME_DIR/cadence` when available | Optional local attach socket in a future mode | Ephemeral; never store progress here |

The first release needs stdio headless mode, not an always-running daemon or socket. Put the profile writer's advisory lock adjacent to its database so all writers to the same profile share one lock regardless of XDG runtime path. Respect caller-supplied profile directories. New private directories use restrictive modes; do not chmod pre-existing parent directories or HOME. Existing malformed data must not be silently replaced by an empty profile.

## D-02 Database and writes

SQLite via `rusqlite` with the bundled library is the proposed adapter. One writer connection owns mutations off the UI thread. Use WAL on supported local filesystems, foreign keys, bounded busy timeout, explicit transactions and a deliberate durability setting (FULL for accepted checkpoints). Record actual SQLite version and compile flags. Do not claim that filesystem/OS hardware failures are formally verified.

Logical tables: schema version; profiles/layout revisions; sessions and phase slices; immutable normalized observation chunks; exercise/target identities; per-session commit receipts; learner derived state with source watermark; review queue; benchmarks and exclusions; corpus manifests; settings audit relevant to scoring. Primary/unique keys enforce session/commit/event identity. SQL migrations are numbered, transactional where supported, covered by previous-supported-schema fixtures and preceded by a restorable backup when destructive. Reject a newer unsupported schema before writes.

Store target identity and normalized correctness/timing facts sufficient to recompute the documented learner model. Do not retain raw mistyped strings or imported plaintext in operational logs. Imported packs themselves are intentional user data. Full text-bearing replay traces require explicit opt-in and a clear destination. Do not promise full replay from the normal compact history database if it omits raw inputs/effect payloads.

Checkpoint at exercise/phase completion, pause, graceful shutdown and a maximum named running interval (initially 2 seconds). Batch observations; no transaction per key. A normal crash may lose at most the unacknowledged tail since the last completed checkpoint; report the recovered watermark. It may not corrupt prior accepted history or reapply committed facts. If sustained disk failure/slow writes prevents the promised checkpoint cadence, stop advancing scored practice and expose the problem rather than extending the loss window invisibly.

Database backup uses SQLite's online backup facility (or a documented exclusive, checkpointed closed-database flow), not an arbitrary copy of a live `.sqlite3` file while ignoring WAL. Export portable JSON with schema/version, IDs and needed content metadata; imports reject duplicates/conflicts predictably. Restore happens with the writer closed and validates the backup before replacing anything; retain the previous file until replacement succeeds. There is no automatic destructive reset command hidden inside repair.

## D-03 Corpus supply chain and indexes

Every shipped pack contains a manifest: stable pack ID/version, source URL and revision/date, exact redistribution licence/notice, normalization recipe version/hash, source/content digests, alphabet, language, word/prose counts, frequency source and difficulty labels. Do not infer that a popular public word list is freely redistributable. A frequency data source may have a different licence from the word list.

Choose sources and pin them during P0/P2; include notices in the installed bundle. Implementation must perform a content audit and produce the promised pack before calling corpus acceptance passed. The package contains only synthetic example text and contracts; it does not pretend to ship 20,000 vetted words.

Use immutable compact pack data and indexes by frequency band, length, allowed character class and focused n-gram. Corpus load/decompression/index creation runs off-thread. An index is keyed by pack digest, alphabet/normalizer version and index format; a stale/corrupt cache is rebuilt, not trusted. Pack change invalidates planner inputs but does not rewrite old exercise targets or word identities. Content-addressed IDs include normalized content and relevant language/profile namespace; ID collisions are checked, not assumed impossible at the API boundary.

Import takes an explicit file/stream chosen by the user. Bound bytes (initially 10 MiB per import), lines, token lengths, normalization expansion and preview size before allocating. Disallow terminal escape/control sequences from render paths; newlines are structural, tabs expand under a documented code-pack policy. Report rejected/normalized counts. No recursive source-tree scanning by default. Do not execute shell/code content, invoke file-defined hooks, or follow instructions in practice text.

## D-04 Offline and operational constraints

Normal start, practice, progress, local import and export must work with network disabled. No update check on each launch. Optional manual software update distribution is outside the trainer process's learning loop. Containers mount only the chosen profile/config paths; never require the Docker socket or the user's whole home. The app operates as a non-root user with correct output ownership.

Disk use is measured and visible in diagnostics. Cache cleanup affects only Cadence-owned cache paths and cannot remove the sole copy of progress/imports/qualified bundles. Data retention changes require an explicit migration/setting; do not silently delete old learning evidence to meet an arbitrary quota. Default operational logs rotate; content trace retention is separately bounded and documented.
