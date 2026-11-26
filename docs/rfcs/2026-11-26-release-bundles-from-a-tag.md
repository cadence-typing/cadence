---
rfc: "2026-11-26-release-bundles-from-a-tag"
title: "Release bundles built from a tag in public CI"
status: accepted
implementation: complete
authors:
  - claude-fable (grooming agent)
created: 2026-11-26
updated: 2026-11-26
issue: https://github.com/cadence-typing/cadence/issues/3
milestone: "0.1.0-alpha.6"
supersedes: []
superseded_by: []
blocked_on: []
---

# RFC: Release bundles built from a tag in public CI

Answers the unresolved question of [Public distribution](2026-11-24-public-distribution.md).

## Summary and motivation

Release bytes for alpha.1 to alpha.5 were built with `./ci dist` on the owner's machine and uploaded by hand. Nothing public shows which commit and which gates produced a published asset, and the first public CI run built a different tarball digest than the published alpha.5 asset (different commits; see #3). From alpha.6 on, pushing a tag `vX.Y.Z[-pre]` builds, checks and publishes the release in GitHub Actions with the same offline Compose commands the owner runs locally.

Outside this decision: signing (#13), Buildbot qualification (#14), other platforms. A release made this way is still an **unsigned development preview**, and `BUNDLE.txt` keeps saying so. This workflow publishes; it does not qualify or promote a bundle (handover B-06).

Why an RFC: release policy is costly to undo once users install from it.

## Observable behaviour

Before: assets appear on a release when someone uploads them. After:

```sh
git tag -a v0.1.0-alpha.6 -m "Cadence 0.1.0-alpha.6" && git push origin v0.1.0-alpha.6
```

The `release` workflow then:

1. refuses the tag unless it equals `v` + the workspace version in `Cargo.toml`, `CHANGELOG.md` has a section for that version, the tagged commit is on `main`, and no release exists for the tag (a tag is never moved and an asset is never overwritten);
2. runs `./ci prepare` (networked), then offline `./ci quality` (fmt, Clippy, tests, boundaries, contract hashes, TLC gate) and `./ci dist`;
3. runs the Lean proof gate for the tagged commit by calling `proofs.yml` as a reusable workflow;
4. builds the bundle a second time from a second clean checkout and compares `SHA256SUMS`; a difference fails the release;
5. creates a GitHub **pre-release** with `cadence-<version>-x86_64-unknown-linux-musl.tar.gz`, `cadence-linux-x86_64` and `SHA256SUMS` (the names `install.sh` expects), with the changelog section as notes plus the source commit, gate image digest and both workflow run URLs;
6. installs the published release with `install.sh --version <tag>` in a clean unprivileged container and checks that the installed binary's SHA-256 equals the built one.

`workflow_dispatch` with `dry_run: true` runs steps 1 to 4 on any ref and publishes nothing.

## Proof obligations

None. No scoring, timing or kernel decision changes. The workflow makes the proof gate and the TLC gate preconditions of a release, so a release cannot ship a claim whose gate did not run on that commit.

## Design and boundaries

- `.github/workflows/release.yml` (new). Jobs: `check-tag`, `build` (matrix of two clean checkouts), `proofs` (`uses: ./.github/workflows/proofs.yml`, which gains `workflow_call`), `publish` (`needs` all three; the only job with `contents: write`), `verify-published`. Actions are pinned by commit as in `check.yml`.
- `cargo xtask release-check --tag <tag>` (edge tooling in `xtask`) owns the tag, version and changelog comparison and extracts the release notes, so the rule lives in the repository and not in workflow YAML.
- Reproducibility inputs, stated in `BUNDLE.txt` and the release notes: source tree at the tag, `Cargo.lock`, `rust-toolchain.toml`, the gate image built from `docker/Dockerfile` (its digest is recorded), policy and corpus digests, and a fixed archive timestamp taken from the tagged commit (`SOURCE_DATE_EPOCH`). `xtask dist` sets the tar mtime, owner and ordering from those inputs.
- No secret is needed: publishing uses the workflow's `GITHUB_TOKEN`.
- `check.yml` is unchanged; its `installer` job keeps installing the newest release on every push.

## Compatibility and reversibility

Asset names and layout are unchanged, so `install.sh` and existing installs are unaffected. Earlier releases stay as they are. Reverting means deleting the workflow and uploading by hand again. A bad release is superseded by a new version, never repaired in place.

## Alternatives

- **Keep building on the owner's machine.** No public evidence of what was built. Rejected.
- **Wait for Buildbot (#14).** Buildbot is the qualification route for a promoted `0.1.0` and needs shared infrastructure that does not exist yet. Previews should not wait for it; the two do not conflict because this workflow promotes nothing.
- **Publish on every push to `main`.** Makes every commit a release; rejected.

## Evidence and rollout

1. `xtask release-check` with tests (matching tag, mismatched version, missing changelog section).
2. `release.yml`; a `dry_run` on `main` passes steps 1 to 4 and shows two identical `SHA256SUMS`.
3. The alpha.6 tag is the first real run (#57): the release exists, `verify-published` passes, and `curl -fsSL https://cadence-typing.github.io/install.sh | sh` installs it.

If step 4 shows a nondeterminism that cannot be removed, the implementer records the cause in the decision log, changes the step to record both digests without failing, and says so in the release notes. The claim is then "built in public CI from the tag", not "reproducible".

## Unresolved questions

None blocking. Owner may amend: whether a release should be created as a draft for manual publication (default: published directly as a pre-release), and who may push tags (default: anyone with write access; no tag protection rule is added).

## Decision log

- 2026-11-26: accepted by default under the owner's 2026-11-26 rule; owner may amend.
