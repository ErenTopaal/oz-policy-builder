#!/usr/bin/env bash
# strip non-deterministic noise from a SimReport JSON for byte-equality checks.
# usage: `canonicalize-sim-report.sh < raw.json > canonical.json`
# add fields to REDACTED_FIELDS to strip them automatically.

set -euo pipefail

REDACTED_FIELDS=(
    "simulated_at"
    "run_id"
    "host_version"
    "wall_clock_ms"
    "captured_at"
)

# build a jq filter that walks the doc, deletes each field, re-prints sorted.
del_filters=""
for f in "${REDACTED_FIELDS[@]}"; do
    del_filters+="del(.. | objects | .${f}?) | "
done

if [ $# -eq 0 ]; then
    # stdin.
    jq --sort-keys "${del_filters%| } | ."
else
    jq --sort-keys "${del_filters%| } | ." < "$1"
fi
