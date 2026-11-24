# Security policy

## Supported versions

Only the newest release receives fixes. Cadence is a pre-1.0 development preview for Linux x86_64 (including WSL2).

## Reporting a vulnerability

Report privately through GitHub's [security advisory form](https://github.com/cadence-typing/cadence/security/advisories/new). Please do not open a public issue.

Include the `cadence --version` output, the steps to reproduce, and what an attacker gains. You will get an acknowledgement within a week.

## Scope

Cadence runs offline with no network access and no account, and it stores practice history only in your own profile directory. In scope:

- the `cadence` binary, including parsing of its profile database, policy and corpus;
- file handling for `backup`, `export` and `--profile-dir` (permissions, overwrite refusal, path handling);
- terminal escape output, where untrusted corpus or import text could reach the terminal;
- the release assets and [`install.sh`](install.sh).

Release checksums detect corrupted or altered downloads but are **not signatures**. Releases are not signed yet; signing is tracked on the [roadmap](https://github.com/cadence-typing/cadence/milestones).
