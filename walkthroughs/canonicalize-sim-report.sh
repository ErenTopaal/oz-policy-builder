#!/usr/bin/env bash
#
# canonicalize-sim-report.sh — strip non-deterministic noise from a SimReport
# JSON document so corpus byte-equality checks pass across runs.
#
# Phase 8 contract: every committed `expected-sim-report.json` is the literal
# output of `oz-policy-cli simulate ...` piped through this canonicalizer.
# Re-running the same simulate command should produce a JSON that, after this
# canonicalizer, is byte-equal to the committed file.
#
# As of Phase 8 corpus-freeze (2026-05-16), the `SimReport` schema is fully
# deterministic by construction:
#   * `spec_id` is derived from `context_rule.name` (stable).
#   * `permit.passed` / `permit.error` are pure functions of (spec, recording, wasm).
#   * `deny_results[*]` are pure functions of the generated deny suite.
#   * `total_vectors` / `passed` are integer counts.
#   * `timestamp_ledger` is sourced from `recording.ledger` (frozen with the
#     recording).
#
# That means this script is currently an identity transform (only normalising
# pretty-printing). The redaction list is preserved for forward-compatibility:
# if a future SimReport schema introduces fields like `simulated_at` /
# `run_id` / `host_version`, add them to `REDACTED_FIELDS` below and the
# canonicalizer will strip them automatically — no corpus rotation required
# for the existing fixtures (they're stable under the strip).
#
# Usage:
#   walkthroughs/canonicalize-sim-report.sh < raw-sim-report.json > canonical.json
#   walkthroughs/canonicalize-sim-report.sh path/to/sim-report.json   # in-place to stdout
#
# Requires: jq (>= 1.6).

set -euo pipefail

REDACTED_FIELDS=(
    "simulated_at"
    "run_id"
    "host_version"
    "wall_clock_ms"
    "captured_at"
)

# Build a jq filter that walks the document and deletes each field, then
# re-pretty-prints with sorted keys for byte-stable output.
del_filters=""
for f in "${REDACTED_FIELDS[@]}"; do
    del_filters+="del(.. | objects | .${f}?) | "
done

if [ $# -eq 0 ]; then
    # Read stdin.
    jq --sort-keys "${del_filters%| } | ."
else
    jq --sort-keys "${del_filters%| } | ." < "$1"
fi
