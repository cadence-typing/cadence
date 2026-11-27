#!/bin/sh
# rc.1 -> rc.2 (#101): no schema change. A profile written by rc.1 (two
# sessions with learning evidence) is opened by rc.2 and every session,
# sequence, digest and learning record reads back unchanged; rc.2 then
# records a full-keypad session (numpad-full) and rc.1 still opens the
# profile. Temporary profiles only.  usage: upgrade.sh RC1 RC2 OUT_FILE
set -eu
R1=$1; R2=$2; OUT=$3
WORK=$(mktemp -d "${TMPDIR:-/tmp}/cadence-upgrade.XXXXXX")
trap 'rm -rf "$WORK"' EXIT INT TERM
P="$WORK/profile"
cat > "$WORK/words.jsonl" <<'REQ'
{"v":1,"id":"1","op":"hello","args":{"scenario":"daily-small"}}
{"v":1,"id":"2","op":"action","args":{"name":"Begin"}}
{"v":1,"id":"3","op":"text","args":{"text":"street strong almost string","inter_key_microseconds":200000}}
{"v":1,"id":"4","op":"advance","args":{"microseconds":2500000}}
{"v":1,"id":"5","op":"text","args":{"text":"strung first stream","inter_key_microseconds":200000}}
{"v":1,"id":"6","op":"quit","args":{"flush":true}}
REQ
cat > "$WORK/keypad.jsonl" <<'REQ'
{"v":1,"id":"1","op":"hello","args":{"scenario":"numpad-full"}}
{"v":1,"id":"2","op":"action","args":{"name":"Begin"}}
{"v":1,"id":"3","op":"text","args":{"text":"12*","inter_key_microseconds":250000}}
{"v":1,"id":"4","op":"key","args":{"key":"Backspace","modifiers":[],"kind":"press","source":"unverified"}}
{"v":1,"id":"5","op":"text","args":{"text":"+7","inter_key_microseconds":250000}}
{"v":1,"id":"6","op":"key","args":{"key":"Enter","modifiers":[],"kind":"press","source":"unverified"}}
{"v":1,"id":"7","op":"text","args":{"text":"-4.50","inter_key_microseconds":250000}}
{"v":1,"id":"8","op":"key","args":{"key":"Enter","modifiers":[],"kind":"press","source":"unverified"}}
{"v":1,"id":"9","op":"quit","args":{"flush":true}}
REQ
{
  echo "# rc.1: $("$R1" --version)"
  echo "# rc.2: $("$R2" --version)"
  for n in 1 2; do "$R1" headless --script "$WORK/words.jsonl" --profile-dir "$P" | tail -n 1; done
  "$R1" export --output "$WORK/before.json" --profile-dir "$P"
  echo "## rc.2 opens the rc.1 profile"
  "$R2" doctor --json --profile-dir "$P" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("schema", d["schema"])'
  "$R2" export --output "$WORK/after.json" --profile-dir "$P"
  python3 - "$WORK/before.json" "$WORK/after.json" <<'PY'
import json, sys
before, after = (json.load(open(p)) for p in sys.argv[1:3])
key = lambda s: (s["session_id"], s["sequence"], s["digest"])
print("export schema", before["schema_version"], "->", after["schema_version"])
assert before["schema_version"] == after["schema_version"], "schema changed"
assert [key(s) for s in before["sessions"]] == [key(s) for s in after["sessions"]], "a session changed"
assert all(s["learning"] == t["learning"] for s, t in zip(before["sessions"], after["sessions"])), "evidence changed"
print("sessions", len(after["sessions"]), "unchanged: sequence, digest and learning records")
PY
  echo "## rc.2 records a full-keypad session (12*, Backspace, +7, Enter, -4.50, Enter)"
  "$R2" headless --script "$WORK/keypad.jsonl" --profile-dir "$P" | python3 -c '
import json,sys
rows=[json.loads(l) for l in sys.stdin]
bad=[r for r in rows if not r.get("ok")]
assert not bad, bad
print("requests", len(rows), "all ok; saved", rows[-1]["result"]["saved"])'
  "$R2" export --output "$WORK/rc2.json" --profile-dir "$P"
  python3 - "$WORK/rc2.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
s = d["sessions"][-1]
p = s["progress"]
skills = [k for k in s["learning"].get("skills", [])] if isinstance(s["learning"], dict) else []
print("newest session", s["session_id"], "provenance", s["provenance"], "content_class", p.get("content_class") if isinstance(p, dict) else None)
PY
  echo "## rc.1 opens the profile rc.2 wrote (no new schema)"
  "$R1" export --output "$WORK/back.json" --profile-dir "$P"
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); print("rc.1 reads", len(d["sessions"]), "sessions")' "$WORK/back.json"
} > "$OUT" 2>&1
echo "upgrade evidence: $OUT"
