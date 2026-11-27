#!/bin/sh
# The handover's final acceptance walkthrough (docs/09-execution.md), steps
# 2 to 7, against a built binary on temporary profiles, headlessly; the
# transcript and captures go to OUT. Step 1 is the package-install gate and
# step 8 an interrupted and resumed `./ci qualify` (see the release README).
#
#   sh tools/walkthrough.sh /path/to/cadence evidence/releases/VERSION/walkthrough
set -eu
BIN=$1
OUT=$2
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
mkdir -p "$OUT"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/cadence-walkthrough.XXXXXX")
trap 'rm -rf "$WORK"' EXIT INT TERM
export HOME="$WORK/home" XDG_CONFIG_HOME="$WORK/home/.config" XDG_DATA_HOME="$WORK/home/.local/share"
mkdir -p "$XDG_CONFIG_HOME/cadence"
T="$OUT/walkthrough.txt"
: > "$T"
step() { printf '\n== %s\n' "$*" >> "$T"; }
run() { printf '$ cadence %s\n' "$*" >> "$T"; "$BIN" "$@" >> "$T" 2>&1; }
fails() { printf '$ cadence %s   (expected to fail)\n' "$*" >> "$T"; if "$BIN" "$@" >> "$T" 2>&1; then echo "walkthrough: '$*' did not fail" >&2; exit 1; fi; }
replay() { run replay "$ROOT/fixtures/journeys/$1" --read-only; }
capture_text() { # requests-file -> the text of every capture response
    python3 -c '
import json,sys
for line in sys.stdin:
    r=json.loads(line)
    t=r.get("result",{}).get("text")
    if t: print(t)' ; }

run --version
step "2. Choose the layout and numpad once; Enter into the daily plan"
replay 01-first-run-setup-and-daily-plan.jsonl
step "3. Make and correct an error; the first-pass history stays honest"
replay 02-mistype-backspace-end-early.jsonl
step "4. Words, number row, numpad and prose; unknown versus verified keypad input"
replay 03-numpad-leading-zero-decimal-enter.jsonl
replay 04-row-and-pad-separate-histories.jsonl
step "5. Leave and reopen: completed progress and review scheduling, once"
# Headless sessions are recorded as synthetic and never enter the live
# recap (by design, #64): the stored commit and chunk watermarks are the
# evidence here, and the PTY suite covers a live launch, quit and reopen.
P="$WORK/profile"
cat > "$WORK/session.jsonl" <<'REQ'
{"v":1,"id":"1","op":"hello","args":{"scenario":"daily-small"}}
{"v":1,"id":"2","op":"action","args":{"name":"Begin"}}
{"v":1,"id":"3","op":"text","args":{"text":"street strong almost string","inter_key_microseconds":200000}}
{"v":1,"id":"4","op":"text","args":{"text":"strung first","inter_key_microseconds":200000}}
{"v":1,"id":"5","op":"quit","args":{"flush":true}}
REQ
cat > "$WORK/reopen.jsonl" <<'REQ'
{"v":1,"id":"1","op":"hello","args":{"scenario":"daily-small"}}
{"v":1,"id":"2","op":"capture","args":{"format":"text"}}
{"v":1,"id":"3","op":"quit","args":{"flush":true}}
REQ
run headless --script "$WORK/session.jsonl" --profile-dir "$P"
run doctor --json --profile-dir "$P"
"$BIN" headless --script "$WORK/reopen.jsonl" --profile-dir "$P" | capture_text > "$OUT/reopened-start-screen.txt"
printf 'start screen after reopening: reopened-start-screen.txt\n' >> "$T"
run headless --script "$WORK/reopen.jsonl" --profile-dir "$P"
run doctor --json --profile-dir "$P"
step "6. A headless trace on an isolated profile replays identically"
cat > "$WORK/trace-requests.jsonl" <<'REQ'
{"v":1,"id":"1","op":"hello","args":{"scenario":"scoring-cat"}}
{"v":1,"id":"2","op":"action","args":{"name":"StartDrill"}}
{"v":1,"id":"3","op":"text","args":{"text":"cxat","inter_key_microseconds":300000}}
{"v":1,"id":"4","op":"capture","args":{"format":"text"}}
{"v":1,"id":"5","op":"quit","args":{"flush":true}}
REQ
run headless --script "$WORK/trace-requests.jsonl" --ephemeral-profile --record "$WORK/trace.jsonl"
run replay "$WORK/trace.jsonl" --read-only
cp "$WORK/trace.jsonl" "$OUT/trace.jsonl"
step "7. An invalid config, theme and import keep the last valid state, with diagnostics"
printf '[session]\nminutes = 7\n' > "$XDG_CONFIG_HOME/cadence/config.toml"
fails config check
printf '[display]\ntheme = "broken.toml"\n' > "$XDG_CONFIG_HOME/cadence/config.toml"
printf '[colors]\ntext = "not-a-colour"\n' > "$XDG_CONFIG_HOME/cadence/broken.toml"
fails config check
printf 'plain line\nan \033[31mescape\033[0m sequence\n' > "$WORK/import.txt"
# The importer keeps clean lines and names each refused one; a dry run
# shows that and creates nothing.
run import "$WORK/import.txt" --name walkthrough --dry-run
run packs
printf '\nwalkthrough: steps 2-7 completed\n' >> "$T"
echo "walkthrough: steps 2-7 completed; transcript $T"
