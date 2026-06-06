#!/usr/bin/env bash
# drive an mcp session through the `mcp-cli` reference tool, write transcript.
# env: OZ_POLICY_MCP_BIN (default ./target/release/oz-policy-mcp), MCP_CLI.
# exit codes: 0 ok, 1 prereq missing, 2 call failed, 3 tool-list mismatch.

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

# each call spawns a fresh mcp-cli; in-memory store doesn't persist between calls.
# byte-equality matrix only checks the first four requests; tool/call validated
# end-to-end by the rust integration test.

declare -a TRANSPORT=(--transport stdio --server-cmd "$BIN --stdio")

# tools/list
"$MCP_CLI" "${TRANSPORT[@]}" --method tools/list > "${OUT}.tools.json" || {
  echo "FATAL: tools/list failed" >&2
  exit 2
}
# sanity: canonical 5-tool surface must be present.
EXPECTED_TOOLS=(record_transaction synthesize_policy simulate_policy export_policy verify_install)
for t in "${EXPECTED_TOOLS[@]}"; do
  if ! grep -q "\"name\":\"$t\"" "${OUT}.tools.json"; then
    echo "FATAL: tools/list missing $t" >&2
    exit 3
  fi
done

# resources/list
"$MCP_CLI" "${TRANSPORT[@]}" --method resources/list > "${OUT}.resources.json" || {
  echo "FATAL: resources/list failed" >&2
  exit 2
}

# prompts/list
"$MCP_CLI" "${TRANSPORT[@]}" --method prompts/list > "${OUT}.prompts.json" || {
  echo "FATAL: prompts/list failed" >&2
  exit 2
}

# tools/call record_transaction (hits stellar testnet rpc)
"$MCP_CLI" "${TRANSPORT[@]}" --method tools/call \
  --params "{\"name\":\"record_transaction\",\"arguments\":{\"network\":\"testnet\",\"hash\":\"$BLEND_HASH\",\"rpc_url\":\"https://soroban-testnet.stellar.org\"}}" \
  > "${OUT}.record.json" || {
    echo "FATAL: record_transaction call failed" >&2
    exit 2
  }

# concat per-call transcripts into a single doc for the matrix comparator.
jq -s '{ tools: .[0], resources: .[1], prompts: .[2], record: .[3] }' \
  "${OUT}.tools.json" \
  "${OUT}.resources.json" \
  "${OUT}.prompts.json" \
  "${OUT}.record.json" \
  > "$OUT"

echo "OK: transcript written to $OUT"
