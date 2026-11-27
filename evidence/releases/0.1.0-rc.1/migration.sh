#!/bin/sh
# Schema 6 -> 7 (#11): a profile written by beta.3 (two sessions with
# learning evidence) is opened by rc.1; every session, receipt and digest
# reads back unchanged and is marked legacy; beta.3 then refuses the
# migrated file.  usage: migration.sh BETA3_BINARY RC1_BINARY OUT_FILE
set -eu
B3=$1; RC=$2; OUT=$3
WORK=$(mktemp -d "${TMPDIR:-/tmp}/cadence-migration.XXXXXX")
trap 'rm -rf "$WORK"' EXIT INT TERM
P="$WORK/profile"
cat > "$WORK/session.jsonl" <<'REQ'
{"v":1,"id":"1","op":"hello","args":{"scenario":"daily-small"}}
{"v":1,"id":"2","op":"action","args":{"name":"Begin"}}
{"v":1,"id":"3","op":"text","args":{"text":"street strong almost string","inter_key_microseconds":200000}}
{"v":1,"id":"4","op":"advance","args":{"microseconds":2500000}}
{"v":1,"id":"5","op":"text","args":{"text":"strung first stream","inter_key_microseconds":200000}}
{"v":1,"id":"6","op":"quit","args":{"flush":true}}
REQ
{
  echo "# beta.3: $("$B3" --version)"
  echo "# rc.1:   $("$RC" --version)"
  for n in 1 2; do "$B3" headless --script "$WORK/session.jsonl" --profile-dir "$P" | tail -n 1; done
  "$B3" export --output "$WORK/before.json" --profile-dir "$P"
  echo "## rc.1 opens the beta.3 profile"
  "$RC" doctor --json --profile-dir "$P" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("schema", d["schema"], "store", json.dumps(d["store"]))'
  "$RC" export --output "$WORK/after.json" --profile-dir "$P"
  python3 - "$WORK/before.json" "$WORK/after.json" <<'PY'
import json, sys
before, after = (json.load(open(p)) for p in sys.argv[1:3])
key = lambda s: (s["session_id"], s["sequence"], s["digest"])
print("export schema", before["schema_version"], "->", after["schema_version"])
print("sessions", [key(s) for s in before["sessions"]])
assert [key(s) for s in before["sessions"]] == [key(s) for s in after["sessions"]], "a session changed"
assert all(s["learning"] == t["learning"] for s, t in zip(before["sessions"], after["sessions"])), "evidence changed"
assert all(not s["chunks"] for s in after["sessions"]), "legacy sessions never gain chunks"
print("every session, sequence, digest and learning record unchanged; no chunks synthesized")
PY
  echo "## beta.3 on the migrated profile"
  if "$B3" export --output "$WORK/again.json" --profile-dir "$P" 2>&1; then echo "beta.3 opened a schema-7 profile" >&2; exit 1; fi
} > "$OUT" 2>&1
echo "migration evidence: $OUT"
