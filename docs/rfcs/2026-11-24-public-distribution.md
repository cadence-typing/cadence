---
rfc: "2026-11-24-public-distribution"
title: "Public distribution: install script, website and project process"
status: accepted
implementation: in-progress
authors:
  - tknawara
created: 2026-11-24
updated: 2026-11-26
issue: "https://github.com/cadence-typing/cadence/issues/1"
milestone: "0.1.0-alpha.6"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Public distribution: install script, website and project process

## Summary and motivation

Cadence has been an owner-only feedback build. Its releases were private prereleases on `tknawara/cadence`, installed by hand. The handover reserved public distribution until after owner installation ([docs/08-build-release.md](../08-build-release.md)). The owner has now asked to open the project:

- a public repository in the `cadence-typing` GitHub organization;
- a one-line installer;
- a website on GitHub Pages without a custom domain;
- the RFC, issue and milestone process used by the owner's other projects.

Out of scope: signing, package-manager distribution, macOS, Windows and ARM64. Those remain separate decisions.

## Observable behaviour

```sh
curl -fsSL https://cadence-typing.github.io/install.sh | sh
curl -fsSL https://cadence-typing.github.io/install.sh | sh -s -- --version v0.1.0-alpha.5
curl -fsSL https://cadence-typing.github.io/install.sh | sh -s -- --uninstall
```

The script:

- refuses anything other than Linux x86_64;
- resolves the newest published release (prereleases included, since every release so far is one) or the requested tag;
- downloads the versioned tarball and `SHA256SUMS`, then checks the tarball against them and the binary against the bundle's own `cadence.sha256`;
- installs the whole bundle, with notices and licenses, to `~/.local/lib/cadence/<tag>`, which matches the owner's existing hand-installed layout;
- atomically points `~/.local/bin/cadence` at it, and prints a `PATH` hint when needed.

It never uses `sudo`, never writes outside the two chosen directories, and never touches `$XDG_DATA_HOME/cadence`. Uninstall removes only bundles the script itself marked.

The website at <https://cadence-typing.github.io> is static HTML with no trackers. The Pages workflow copies `install.sh` from this repository's `main` at deploy time, so there is one source of truth.

## Proof obligations

None. No scoring, timing or kernel decision changes.

## Design and boundaries

- `install.sh` lives at the repository root and is POSIX `sh`. CI runs ShellCheck on it and runs it against a published release.
- The website lives in `cadence-typing/cadence-typing.github.io`.
- Process: [GOVERNANCE.md](../../GOVERNANCE.md), [CONTRIBUTING.md](../../CONTRIBUTING.md), [SECURITY.md](../../SECURITY.md), [CHANGELOG.md](../../CHANGELOG.md), issue forms, labels, release milestones plus `Later`, and this registry. The former `plans/` were moved here.
- The handover's frozen `README.md`, `AGENTS.md` and `HANDOVER.html` move to `docs/handover/` with unchanged bytes, and `MANIFEST.sha256` records the new paths ([decision 0005](../decisions/0005-handover-archive.md)). The live README and AGENTS.md can then evolve.
- GitHub Actions runs the same commands as the Docker gate: `fmt`, strict Clippy, tests, and contract verification. The Docker Compose route stays canonical, and Buildbot qualification is unchanged.

## Compatibility and reversibility

Existing hand installs under `~/.local/lib/cadence/<version>-<sha>/` are left alone; the installer only re-points the `~/.local/bin/cadence` link. Profiles are untouched. Making the repository public cannot be fully undone, because forks and caches persist. That is the owner's explicit decision.

## Alternatives

- **Project pages at `tknawara.github.io/cadence`.** Simpler, but it ties the project to a personal account, unlike the owner's other projects.
- **Serving `install.sh` from `raw.githubusercontent.com`.** Works, but the URL is long and couples users to the branch layout.
- **Signed releases first.** Better integrity, but signing is not yet authorized ([SECURITY.md](../../SECURITY.md)). Checksums are stated as what they are.

## Evidence and rollout

1. Installer behaviour was checked against the alpha.5 bundle with a local download shim: default install, pinned version, unknown tag, invalid flags, checksum mismatch, idempotent reinstall, and uninstall preserving a foreign install. After publication, it must also be checked against the live release in a clean container.
2. Transfer the repository to `cadence-typing`, make it public, enable Pages, and republish release assets there if the transfer does not carry them.
3. Seed labels, milestones and issues from the [STATUS.md](../../STATUS.md) requirement ledger.

## Unresolved questions

- Should future releases be built by a tag-triggered workflow, so that release bytes come from public CI rather than the owner's machine? This is tracked as an issue; it interacts with the Buildbot plan.

## Decision log

- 2026-11-24: The owner chose the `cadence-typing` organization and asked for the repository to be made public once this work is done.
- 2026-11-26: Accepted by default under the owner's rule adopted from Writfold (AGENTS.md, "Issues, milestones and RFCs"); the owner may amend. The org avatar upload remains on #1.
