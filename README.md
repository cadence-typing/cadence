<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/logo-wordmark-dark.svg">
    <img src="assets/brand/logo-wordmark.svg" width="320" alt="Cadence">
  </picture>
</p>

<p align="center"><b>Fifteen honest minutes of typing practice a day, in your Linux terminal.</b></p>

<p align="center">
  <a href="https://cadence-typing.github.io">Website</a> ·
  <a href="#install">Install</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="docs/rfcs/README.md">RFCs</a> ·
  <a href="https://github.com/cadence-typing/cadence/milestones">Roadmap</a>
</p>

Cadence is an offline typing trainer. Run `cadence`, press Enter, and it walks you through a guided session: a warm-up, adaptive words, the number row, the numpad and real prose. It remembers what you got wrong and brings it back. It works without an account, a network connection or telemetry. Your history is a SQLite file in your home directory.

> **Status: unsigned development preview** (`0.1.0-alpha.5`). Linux x86_64 and WSL2 only, in a terminal of at least 60×18. What is finished, tested and proved is tracked precisely in [STATUS.md](STATUS.md).

## Install

```sh
curl -fsSL https://cadence-typing.github.io/install.sh | sh
```

The script downloads the newest release from GitHub and checks it against the release's `SHA256SUMS`. It installs the bundle to `~/.local/lib/cadence/` and links `~/.local/bin/cadence`. It never uses `sudo` and never touches your practice history. To read the script before running it:

```sh
curl -fsSLO https://cadence-typing.github.io/install.sh
less install.sh
sh install.sh                        # or: sh install.sh --version v0.1.0-alpha.5
sh install.sh --uninstall            # removes what it installed; keeps your history
```

You can also download the tarball and `SHA256SUMS` from [Releases](https://github.com/cadence-typing/cadence/releases) and run `sha256sum --check`. Checksums detect corrupted or altered downloads; **they are not signatures**.

## Use

```sh
cadence                                   # today's 15-minute session
cadence --minutes 5 --track numpad        # a short single-track drill
cadence --minutes 5 --track words --seed 42
cadence --theme light                     # dark (default), light, terminal
cadence --text-size normal                # auto (default), large, normal
cadence --pack code-rust --minutes 5      # practise Rust (or code-shell, or your own pack)
cadence import /path/notes.txt --name notes   # your own text, sanitised; --dry-run to preview
cadence packs                             # list imported packs
cadence config check                      # validate ~/.config/cadence/config.toml
cadence backup --output ~/cadence-backup.sqlite3
cadence export --output ~/cadence-progress.json
```

The first run asks three things (keyboard layout, whether you have a numpad, session length) and writes them to `~/.config/cadence/config.toml`, which you can also edit by hand; `cadence config check` names any bad line. Outside practice, Tab or the arrows move between **Today** (the guided session), **Train** (one track, your length), **Numpad** and **Progress** (trends only across comparable sessions, with their counts).

| Key | Does |
|---|---|
| Enter | start, continue, submit a numpad entry (and type a newline in code) |
| Esc | pause; while paused, `e` ends the session and shows your review |
| Ctrl-C | end practice and show the review (saved as unfinished) |
| `s` | settings (start screen or paused): display changes apply at once, the rest next session |
| `?` | every key for the screen you are on |
| F2 | cycle text size: auto → large → normal |

During practice every printable key is practice input. Switching windows or pasting pauses practice; a paste never inserts text. Progress lives in `$XDG_DATA_HOME/cadence` (usually `~/.local/share/cadence`). Use `--profile-dir` to keep a separate profile. Imported text and code packs are never compared with, and never feed, your standard scores.

## What makes it honest

- **A corrected mistake is still a mistake.** First-attempt accuracy is what's scored; fixing a typo with Backspace doesn't erase it.
- **Time is real.** Hesitation counts as practice time; only explicit pauses and between-phase screens don't.
- **Adaptation is labelled.** When the words phase targets your weak key transitions or brings back a missed word, it tells you. Harder targeted words are harder by design.
- **No promises about learning.** Cadence measures carefully. Whether it makes you faster is an empirical question, and the software claims no guaranteed gain.

## How it's built

Rust and [Ratatui](https://ratatui.rs). There is one state owner with a typed reducer and immutable presentation snapshots, and no I/O on the keypress → render path. The same reducer and renderer drive the live TUI, the headless driver and replay. Release builds are static musl binaries with no runtime library dependencies.

| Crate | Role |
|---|---|
| [`core`](crates/core) | domain types, sessions, ledger, scoring, progress |
| [`trainer`](crates/trainer) | session planner, learner model, corpus, review ladder |
| [`engine`](crates/engine) | reducer, practice state, presentation snapshots |
| [`data`](crates/data) | SQLite profile, schema migrations, backup and export |
| [`tui`](crates/tui) | rendering: practice, stages, review, modeline |
| [`runtime`](crates/runtime) | terminal lifecycle, input, durable writer |
| [`driver`](crates/driver) | headless driver and capture |
| [`app`](crates/app) | the `cadence` binary (composition root) |

**Assurance**, labelled honestly:

- **Proved:** the scoring decisions live in a small `no_std` crate, [`crates/kernel`](crates/kernel), which production calls directly. It has 65 proved claims (160 Lean theorems) over its [Charon/Aeneas](https://github.com/AeneasVerif/aeneas) translation, for example:
  - a corrected mistake stays an error;
  - accuracy never exceeds 100%;
  - active time is the closed segments plus the open one, exactly, never counts a pause and never runs backwards;
  - pace is exactly `units·60 s / (5·active)` and never overflows;
  - a checkpoint is never credited twice;
  - once the review shows, nothing leads back to typing, and ending early never marks a session completed.

  Each claim passes an axiom audit, and every one of its 53 deliberate source mutants must break it. The inventory, and what remains only tested, is in [docs/TAUTEN.md](docs/TAUTEN.md).
- **Model-checked:** the protocol designs (async admission, causal barriers, durable checkpoints) have bounded [TLA+ models](formal/), run by the quality gate on every change ([registry](models/claims.toml)); deliberately faulty variants must produce named counterexamples. The checkpoint save protocol ([`CheckpointSave.tla`](models/CheckpointSave.tla)) and the session flow ([`SessionFlow.tla`](models/SessionFlow.tla)) are mapped to the shipped code; the handover's three models are design probes.
- **Tested:** [independent scoring fixtures](fixtures/scoring.json), property tests, 193 tests in all, plus real-PTY smoke runs.
- **Profile:** the code follows the [tauten](https://github.com/tauten-dev/tauten) strictness profile: kernel/core/edge tiers, no ambient I/O outside the edge, checked arithmetic, typed errors. `cargo tauten check` passes with no baseline.

Proofs cover the decisions they name, not the whole program, and they do not show that practice makes you faster.

## Develop

```sh
./ci prepare     # build the pinned gate image and fetch dependencies (networked)
./ci quality     # offline: fmt, strict Clippy, all tests, boundaries, contract hashes
./ci dist        # offline: the static release bundle in dist/
cargo +1.98.0 tauten check     # strictness profile
cargo +1.98.0 tauten proofs    # Lean gate: extract, build, axiom audit, mutants (Docker, ~20 GB image)
```

Start with [CONTRIBUTING.md](CONTRIBUTING.md), then [AGENTS.md](AGENTS.md) for the engineering rules (they apply to people too) and [GOVERNANCE.md](GOVERNANCE.md) for how decisions are made. Designs go through [RFCs](docs/rfcs/README.md). The original implementation contract is archived in [docs/handover/](docs/handover/README.md).

## License

Code: [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option. Word list: derived from [wordfreq](https://github.com/rspeer/wordfreq) (CC BY-SA 4.0). Prose: public-domain books and original passages. Sources and notices are in [`corpus/`](corpus/).
