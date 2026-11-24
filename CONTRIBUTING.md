# Contributing to Cadence

Thanks for helping. Cadence is a small, owner-led project; [GOVERNANCE.md](GOVERNANCE.md) says who decides what.

## Before you start

- **Bugs:** open an issue with the [bug form](https://github.com/cadence-typing/cadence/issues/new?template=bug.yml). Include `cadence --version`, your terminal and its size, and the smallest steps that reproduce the problem. Never attach your practice database. If a profile is needed, reproduce with `--profile-dir` pointing at a throwaway directory.
- **Features and changes:** open an issue describing the outcome you want before writing code. New issues get `needs-triage` until the owner decides.
- **Security:** follow [SECURITY.md](SECURITY.md). Do not open a public issue.
- **Trivial fixes**, such as typos or a broken link, can go straight to a pull request.

## When you need an RFC

Write an RFC before changing any of these:

- scoring, timing or what counts as an error;
- the learning model or session plan policy;
- a persisted format (the SQLite schema, export or backup);
- the headless protocol or a CLI contract;
- a kernel decision or its proof premises;
- a dependency, platform or release policy that would be costly to undo.

The process is in the [RFC registry](docs/rfcs/README.md). Ordinary bug fixes that restore the existing contract need only an issue and a regression test.

## Building and testing

Linux x86_64 or WSL2. Docker is the canonical route, and no host Rust is needed:

```sh
./ci prepare      # networked: build the gate image and fetch locked dependencies
./ci quality      # offline: rustfmt, strict Clippy, all tests, boundaries, contract hashes
./ci dist         # offline: static musl bundle in dist/
```

With a host toolchain (pinned by `rust-toolchain.toml`):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked
cargo test --workspace --locked
cargo xtask verify-contracts
cargo tauten check            # strictness profile; see docs/TAUTEN.md
```

Tests must use an explicit temporary profile. They never read or write your real `~/.local/share/cadence`.

## Pull requests

- Link the issue with `Refs #N`, or `Closes #N` when the agreed outcome is complete.
- Follow [AGENTS.md](AGENTS.md). It applies to human contributors too: typed errors, no `unwrap`, checked arithmetic, no naked policy numbers, and modules split by responsibility.
- Never weaken, delete or `#[ignore]` a test to make a change pass.
- For a user-visible change, add an entry under `[Unreleased]` in [CHANGELOG.md](CHANGELOG.md). For an assurance change, update [STATUS.md](STATUS.md) and the claim badges together.
- State what you verified and how, and what you did not verify.

By contributing, you agree that your contribution is dual-licensed under [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at the user's option, like the rest of Cadence.
