#!/bin/sh
# Cadence installer.
#
#   curl -fsSL https://cadence-typing.github.io/install.sh | sh
#   curl -fsSL https://cadence-typing.github.io/install.sh | sh -s -- --version v0.1.0-alpha.5
#   sh install.sh --from /path/to/bundle-directory     (offline, #94)
#
# Downloads a release bundle from GitHub, checks it against the release's
# SHA256SUMS, and installs it for the current user only: the bundle goes in
# ~/.local/lib/cadence/<tag> and ~/.local/bin/cadence links to it.
# Never uses sudo and never touches your practice history
# (~/.local/share/cadence). Checksums detect corrupted or altered downloads;
# they are not signatures.
#
# Everything runs inside main(), so a truncated download of this script
# does nothing.

set -eu

REPO_DEFAULT="cadence-typing/cadence"
TARGET="x86_64-unknown-linux-musl"

say() { printf 'cadence-install: %s\n' "$*"; }
err() { printf 'cadence-install: error: %s\n' "$*" >&2; exit 1; }

usage() {
    cat <<'EOF'
Install Cadence, an offline typing trainer for the Linux terminal.

Usage: install.sh [options]

Options:
  --version TAG     Install this release tag (default: the newest release,
                    including previews). Env: CADENCE_VERSION
  --from DIR        Install offline from a directory holding a release's
                    SHA256SUMS and cadence-VERSION-x86_64-unknown-linux-musl.tar.gz
  --prefix DIR      Where release bundles live
                    (default: ~/.local/lib/cadence). Env: CADENCE_PREFIX
  --bin-dir DIR     Where the `cadence` link goes (default: ~/.local/bin).
                    Env: CADENCE_BIN_DIR
  --uninstall       Remove the link and the bundles this script installed.
                    Practice history in ~/.local/share/cadence is kept.
  -h, --help        Show this help.

Supported: Linux x86_64, including WSL2. Needs curl or wget, tar, and
sha256sum.
EOF
}

need() { command -v "$1" >/dev/null 2>&1 || err "'$1' is required but was not found"; }

download() { # url dest
    if command -v curl >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget --https-only -q -O "$2" "$1"
    else
        err "curl or wget is required"
    fi
}

latest_tag() { # repo tmpdir
    # /releases/latest skips prereleases, so take the newest published entry.
    download "https://api.github.com/repos/$1/releases?per_page=1" "$2/releases.json" ||
        err "could not query GitHub releases for $1 (try --version TAG)"
    tag=$(sed -n 's/^ *"tag_name": *"\([^"]*\)".*/\1/p' "$2/releases.json" | head -n 1)
    [ -n "$tag" ] || err "no published release found for $1"
    printf '%s\n' "$tag"
}

uninstall() {
    link="$BIN_DIR/cadence"
    if [ -L "$link" ]; then
        case "$(readlink "$link")" in
            "$PREFIX"/*) rm -f "$link"; say "removed $link" ;;
            *) say "left $link alone: it does not point into $PREFIX" ;;
        esac
    fi
    for dir in "$PREFIX"/v*; do
        [ -f "$dir/.installed-by-cadence-install" ] || continue
        rm -rf "$dir"
        say "removed $dir"
    done
    rmdir "$PREFIX" 2>/dev/null || true
    say "practice history (\${XDG_DATA_HOME:-~/.local/share}/cadence) was not touched"
}

main() {
    REPO="${CADENCE_REPO:-$REPO_DEFAULT}"
    VERSION="${CADENCE_VERSION:-}"
    PREFIX="${CADENCE_PREFIX:-${HOME:?HOME is not set}/.local/lib/cadence}"
    BIN_DIR="${CADENCE_BIN_DIR:-$HOME/.local/bin}"
    UNINSTALL=0
    FROM=""

    while [ "$#" -gt 0 ]; do
        case "$1" in
            --version) [ "$#" -ge 2 ] || err "--version needs a value"; VERSION="$2"; shift 2 ;;
            --version=*) VERSION="${1#*=}"; shift ;;
            --prefix) [ "$#" -ge 2 ] || err "--prefix needs a value"; PREFIX="$2"; shift 2 ;;
            --prefix=*) PREFIX="${1#*=}"; shift ;;
            --bin-dir) [ "$#" -ge 2 ] || err "--bin-dir needs a value"; BIN_DIR="$2"; shift 2 ;;
            --bin-dir=*) BIN_DIR="${1#*=}"; shift ;;
            --from) [ "$#" -ge 2 ] || err "--from needs a directory"; FROM="$2"; shift 2 ;;
            --from=*) FROM="${1#*=}"; shift ;;
            --uninstall) UNINSTALL=1; shift ;;
            -h|--help) usage; exit 0 ;;
            *) err "unknown option '$1' (see --help)" ;;
        esac
    done

    case "$PREFIX" in /*) ;; *) err "--prefix must be an absolute path" ;; esac
    case "$BIN_DIR" in /*) ;; *) err "--bin-dir must be an absolute path" ;; esac

    if [ "$UNINSTALL" -eq 1 ]; then
        uninstall
        exit 0
    fi

    os=$(uname -s)
    arch=$(uname -m)
    [ "$os" = "Linux" ] || err "Cadence supports Linux (including WSL2) only; this is $os"
    case "$arch" in
        x86_64|amd64) ;;
        *) err "Cadence ships x86_64 builds only; this machine is $arch" ;;
    esac
    need tar
    need sha256sum
    need mktemp

    tmp=$(mktemp -d "${TMPDIR:-/tmp}/cadence-install.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT INT TERM

    if [ -n "$FROM" ]; then
        [ -d "$FROM" ] || err "--from $FROM is not a directory"
        if [ -z "$VERSION" ]; then
            found=$(cd "$FROM" && ls cadence-*-"$TARGET".tar.gz 2>/dev/null | head -n 1)
            [ -n "$found" ] || err "$FROM holds no cadence-*-$TARGET.tar.gz"
            VERSION="v${found#cadence-}"
            VERSION="${VERSION%-$TARGET.tar.gz}"
        fi
    elif [ -z "$VERSION" ]; then
        VERSION=$(latest_tag "$REPO" "$tmp")
    fi
    case "$VERSION" in
        v*) ;;
        *) VERSION="v$VERSION" ;;
    esac
    case "$VERSION" in
        */*|*..*) err "invalid version '$VERSION'" ;;
    esac
    bare="${VERSION#v}"
    name="cadence-$bare-$TARGET"
    archive="$name.tar.gz"
    base="https://github.com/$REPO/releases/download/$VERSION"

    if [ -n "$FROM" ]; then
        say "installing Cadence $VERSION from $FROM"
        cp "$FROM/SHA256SUMS" "$tmp/SHA256SUMS" 2>/dev/null || err "$FROM has no SHA256SUMS"
        cp "$FROM/$archive" "$tmp/$archive" 2>/dev/null || err "$FROM has no $archive"
    else
        say "installing Cadence $VERSION from github.com/$REPO"
        download "$base/SHA256SUMS" "$tmp/SHA256SUMS" ||
            err "release $VERSION has no SHA256SUMS (does the tag exist?)"
        download "$base/$archive" "$tmp/$archive" ||
            err "could not download $archive"
    fi

    expected=$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
    [ -n "$expected" ] || err "SHA256SUMS does not list $archive"
    actual=$(sha256sum "$tmp/$archive" | awk '{ print $1 }')
    [ "$expected" = "$actual" ] ||
        err "checksum mismatch for $archive (expected $expected, got $actual)"
    say "checksum ok ($actual)"

    mkdir -p "$tmp/x"
    tar -xzf "$tmp/$archive" -C "$tmp/x" --no-same-owner
    bundle="$tmp/x/$name"
    [ -f "$bundle/cadence" ] || err "archive does not contain $name/cadence"
    got=$(sha256sum "$bundle/cadence" | awk '{ print $1 }')
    if [ -f "$bundle/cadence.sha256" ]; then
        inner=$(awk '{ print $1 }' "$bundle/cadence.sha256")
        [ "$inner" = "$got" ] || err "bundled binary does not match its cadence.sha256"
    fi
    # The release's own digest of the executable, when it lists one.
    standalone=$(awk '$2 == "cadence-linux-x86_64" { print $1 }' "$tmp/SHA256SUMS")
    if [ -n "$standalone" ]; then
        [ "$standalone" = "$got" ] || err "bundled binary does not match SHA256SUMS"
    fi
    # An x86-64 ELF executable: magic, 64-bit, little-endian, e_machine 0x3e.
    elf=$(od -An -tx1 -N20 "$bundle/cadence" | tr -d ' \n')
    case "$elf" in
        7f454c46020101*) ;;
        *) err "the bundled binary is not a 64-bit little-endian ELF executable" ;;
    esac
    machine=$(od -An -tx1 -j18 -N2 "$bundle/cadence" | tr -d ' \n')
    [ "$machine" = "3e00" ] || err "the bundled binary is not for x86_64 (ELF machine $machine)"
    chmod 0755 "$bundle/cadence"
    printf '%s\n' "$VERSION" > "$bundle/.installed-by-cadence-install"

    mkdir -p "$PREFIX" "$BIN_DIR"
    dest="$PREFIX/$VERSION"
    staged="$PREFIX/.$VERSION.partial"
    previous="$PREFIX/.$VERSION.previous"
    rm -rf "$staged" "$previous"
    mv "$bundle" "$staged"
    # The working version stays in place until the new one is ready: an
    # existing directory of the same version is moved aside, not removed,
    # and the link is replaced by a rename, never left dangling.
    if [ -e "$dest" ]; then
        mv "$dest" "$previous"
    fi
    mv "$staged" "$dest"
    [ "${CADENCE_INSTALL_FAULT:-}" != "before-link" ] || err "test fault: stopped before the link"
    link_tmp="$BIN_DIR/.cadence.link.$$"
    ln -s "$dest/cadence" "$link_tmp"
    # A rename over the old link (GNU mv -T); elsewhere, a replaced link.
    mv -T "$link_tmp" "$BIN_DIR/cadence" 2>/dev/null || {
        rm -f "$link_tmp"
        ln -sfn "$dest/cadence" "$BIN_DIR/cadence"
    }
    rm -rf "$previous"

    say "installed $dest"
    say "linked $BIN_DIR/cadence"
    case ":${PATH:-}:" in
        *":$BIN_DIR:"*) say "run: cadence" ;;
        *)
            say "$BIN_DIR is not on your PATH. Add this to your shell profile:"
            # shellcheck disable=SC2016 # print a literal $PATH for the user's profile
            printf '\n    export PATH="%s:$PATH"\n\n' "$BIN_DIR"
            ;;
    esac
}

main "$@"
