# Decision 0001 — toolchain and dependency pins (P0)

Date: 2026-10-08 · Status: verified-on-this-workstation

Actual compatibility was established by building this workspace and
running the `quality` gate on the host and in the pinned Docker image.
These are not copied reference pins.

## Host toolchain (verified)

| Tool | Version | Evidence |
|---|---|---|
| rustc/cargo (workspace default) | 1.97.1 (`rust-toolchain.toml`) | `cargo build --workspace` green; `cargo xtask quality` green |
| Verus | 0.2026.09.06.8dea4a2 at `/home/tarek/verus-toolchain/verus-x86-linux/verus` | `verus --version` executed 2026-10-08 |
| Z3 (Verus solver) | 4.16.0 at `/home/tarek/verus-toolchain/z3-4.16.0-x64-glibc-2.39` | `z3 --version` executed |
| Rust toolchain for Verus | 1.98.0-x86_64-unknown-linux-gnu (installed) | required by the Verus build; production build stays on 1.97.1 |
| TLC (TLA+ toolbox) | jar SHA-256 `db131ddb48e7004d823bef4493df7b35694babe37505b9d9fa5685e7a331f1f1` at `/home/tarek/.local/tla/tla2tools.jar` | digest recomputed 2026-10-08; differs from Strop's pin — this is Cadence's own pinned jar |
| Java (TLC host runtime) | **absent on host** | TLC runs in the Docker `model` service instead (pinned openjdk image to be pinned at P7 with digest) |
| TLAPS | not installed | `prove` gate stays pending until P7 pins it |

## Docker image (verified)

- Base: `rust:1.97.1-slim` pinned by digest
  `sha256:8e8cf8f7fd54a2d23d5a743b3a03f56e26b6c774276c33fa0595111704ebb15c`.
- `rustfmt` and `clippy` components baked in so gate services run with
  `network_mode: none` (verified: `docker compose run quality` passes offline).
- Gate composition: `docker/docker-compose.yml` (B-02 service set);
  dependency fetch happens only in the `prepare` step; `Cargo.lock` is
  committed and `--locked` is used throughout.

## Crate dependencies (verified by full build + clippy/test)

From `Cargo.lock` (committed):

- ratatui 0.29.0, crossterm 0.29.0 (TUI/runtime/driver)
- rusqlite 0.37.0 with `bundled` SQLite (data)
- serde 1.0.228 / serde_json 1.0.145, toml 0.9.12
- proptest 1.8.0 (dev), sha2 0.10.9, rand 0.9.2 + rand_chacha 0.9.0
  (seeded deterministic PRNG basis for L-05)

Licence audit (quality gate extension) is implemented as part of the
`quality` gate's dependency audit when `cargo-deny` is added in a later
packet; today the audit obligation is recorded as pending in STATUS.md.

## Ownership notes

- The Verus binary is a local toolchain, not a repository dependency.
  Same-source Verus kernels (V-05) will compile under plain rustc via the
  `verus!` macro erasure in production modules, starting with P1 kernels.
- The `verify` gate remains pending until the first same-source kernel
  lands; no Verus claim is made by this decision record.
