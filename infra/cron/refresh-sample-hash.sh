#!/usr/bin/env bash
# Refresh /sample-hash.txt with a recent testnet invoke_host_function hash
# that synthesises into Track B (generated) under `--mode auto`. Track A
# (composed primitives like spending_limit) leaves the playground's Source
# tab empty, which contradicts the on-screen demo. Filtering here keeps the
# sample preset's Source tab populated with real generated Rust.
#
# Honest behaviour: if horizon is unreachable, or if no Track B candidate
# can be found in the batch we scanned, leave the file untouched and exit
# non-zero so systemd marks the run failed. No fake fallbacks.
set -euo pipefail

OUT=/var/www/policy/sample-hash.txt
TMP=$(mktemp -d)
trap "rm -rf $TMP" EXIT

CANDIDATES=$(curl -fsS --max-time 15 \
  "https://horizon-testnet.stellar.org/operations?order=desc&limit=200&include_failed=false" \
  | python3 -c '
import json, sys
ops = json.load(sys.stdin).get("_embedded", {}).get("records", [])
seen = set()
for o in ops:
    if o.get("type") != "invoke_host_function":
        continue
    h = o.get("transaction_hash")
    if h and len(h) == 64 and h not in seen:
        seen.add(h)
        print(h)
')

if [ -z "$CANDIDATES" ]; then
  echo "horizon returned no invoke_host_function ops" >&2
  exit 2
fi

CHOSEN=""
while IFS= read -r HASH; do
  /usr/local/bin/oz-policy-cli record --hash "$HASH" > "$TMP/rec.json" 2>/dev/null || continue
  /usr/local/bin/oz-policy-cli synthesize "$TMP/rec.json" --mode auto --tightness exact > "$TMP/spec.json" 2>/dev/null || continue
  KIND=$(python3 -c "import json; d=json.load(open('$TMP/spec.json')); print(d['policies'][0].get('kind','?'))" 2>/dev/null || echo "?")
  if [ "$KIND" = "generated" ]; then
    CHOSEN="$HASH"
    logger -t refresh-sample-hash "picked $HASH (Track B)"
    break
  fi
done <<< "$CANDIDATES"

if [ -z "$CHOSEN" ]; then
  echo "no Track B candidate found in horizon batch" >&2
  exit 3
fi

printf '%s' "$CHOSEN" > "$TMP/out"
install -m 644 -o caddy -g caddy "$TMP/out" "$OUT"
