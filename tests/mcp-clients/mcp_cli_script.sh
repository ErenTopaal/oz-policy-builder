#!/usr/bin/env bash
# Drives the same scripted MCP session as `crates/oz-policy-mcp/tests/stdio_smoke.rs`
# through the Anthropic `mcp-cli` reference tool, then writes the
# transcript to ./oz-policy-mcp-mcp-cli-transcript.json. Phase 5
# Stream D's cross-client matrix job consumes this script alongside the
# Claude Desktop / Cursor / Cline / Continue drivers and asserts the
# five resulting transcripts are byte-equal (modulo UUID redaction —
# the redactor mirrors the one in `stdio_smoke.rs`).
#
# Required environment:
#   OZ_POLICY_MCP_BIN    — absolute path to the `oz-policy-mcp` binary
#                          (default: ./target/release/oz-policy-mcp)
#   MCP_CLI              — path to mcp-cli (default: $(which mcp-cli))
#   OZ_POLICY_MCP_TOKEN  — unused for STDIO; set for HTTP variants
#
# Exit codes:
#   0 — transcript written successfully
#   1 — missing binary / unmet prereq
#   2 — mcp-cli call failed
#   3 — assertion mismatch on tool list

set -euo pipefail

BIN="${OZ_POLICY_MCP_BIN:-$(pwd)/target/release/oz-policy-mcp}"
MCP_CLI="${MCP_CLI:-$(command -v mcp-cli 2>/dev/null || true)}"
OUT="${OUT:-oz-policy-mcp-mcp-cli-transcript.json}"
BLEND_HASH="5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e"

if [[ ! -x "$BIN" ]]; then
  echo "FATAL: oz-policy-mcp binary not found at $BIN" >&2
  echo "Build it first: cargo build --release -p oz-policy-mcp" >&2
  exit 1
fi
if [[ -z "$MCP_CLI" ]] || [[ ! -x "$MCP_CLI" ]]; then
  echo "FATAL: mcp-cli not on PATH. Install: pip install mcp-cli" >&2
  exit 1
fi

# `mcp-cli` invokes the server, sends the requested method/params, and
# prints the JSON-RPC response on stdout. The flags below match the
# session covered by `stdio_smoke.rs`.
#
# Each call below runs in a separate `mcp-cli` invocation, so the in-
# memory store doesn't persist across calls — that means the
# tools/call inputs that need a prior recording_id / spec_id can't be
# chained automatically in a multi-process shell harness. The
# byte-equality comparison in the CI matrix job is therefore restricted
# to the *first four* requests (initialize / tools/list /
# resources/list / prompts/list) — the tool/call steps are validated
# end-to-end by the in-process `stdio_smoke.rs` Rust integration test.
#
# Pass `--transport stdio --server-cmd "$BIN --stdio"` to wire the
# subprocess. Per mcp-cli's docs the exact flag names vary by minor
# version; the harness invocation below targets v0.x.

declare -a TRANSPORT=(--transport stdio --server-cmd "$BIN --stdio")

# --- tools/list ---
"$MCP_CLI" "${TRANSPORT[@]}" --method tools/list > "${OUT}.tools.json" || {
  echo "FATAL: tools/list failed" >&2
  exit 2
}
# Sanity: the canonical 5-tool surface must be present.
EXPECTED_TOOLS=(record_transaction synthesize_policy simulate_policy export_policy verify_install)
for t in "${EXPECTED_TOOLS[@]}"; do
  if ! grep -q "\"name\":\"$t\"" "${OUT}.tools.json"; then
    echo "FATAL: tools/list missing $t" >&2
    exit 3
  fi
done

# --- resources/list ---
"$MCP_CLI" "${TRANSPORT[@]}" --method resources/list > "${OUT}.resources.json" || {
  echo "FATAL: resources/list failed" >&2
  exit 2
}

# --- prompts/list ---
"$MCP_CLI" "${TRANSPORT[@]}" --method prompts/list > "${OUT}.prompts.json" || {
  echo "FATAL: prompts/list failed" >&2
  exit 2
}

# --- tools/call record_transaction (touches Stellar testnet RPC) ---
"$MCP_CLI" "${TRANSPORT[@]}" --method tools/call \
  --params "{\"name\":\"record_transaction\",\"arguments\":{\"network\":\"testnet\",\"hash\":\"$BLEND_HASH\",\"rpc_url\":\"https://soroban-testnet.stellar.org\"}}" \
  > "${OUT}.record.json" || {
    echo "FATAL: record_transaction call failed" >&2
    exit 2
  }

# Concatenate the per-call transcripts into a single JSON document for
# the CI matrix comparator.
jq -s '{ tools: .[0], resources: .[1], prompts: .[2], record: .[3] }' \
  "${OUT}.tools.json" \
  "${OUT}.resources.json" \
  "${OUT}.prompts.json" \
  "${OUT}.record.json" \
  > "$OUT"

echo "OK: transcript written to $OUT"
