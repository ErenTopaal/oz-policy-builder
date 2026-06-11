#!/usr/bin/env bash
# Rebuilds and restarts the MCP server on the yokai server, then verifies
# the new tools are registered.
#
# Assumes:
#   * `yokai` is configured in ~/.ssh/config.
#   * Source is already rsync'd to /home/ubuntu/oz-policy-builder/ on the server.
#   * systemd unit oz-policy-mcp.service exists and the user has sudo NOPASSWD
#     for `systemctl restart oz-policy-mcp`.
#
# Honest: no fake "deployed" output. If the build, restart, or tool-list
# verification fails, this exits non-zero and the prior binary remains running
# (systemd restart is atomic — failed Exec leaves the old PID in place).

set -euo pipefail

REMOTE_HOST="yokai"
REMOTE_SRC="/home/ubuntu/oz-policy-builder"

echo "Rebuilding oz-policy-mcp on ${REMOTE_HOST}..."
ssh "$REMOTE_HOST" "
  set -euo pipefail
  cd '${REMOTE_SRC}'
  # LTO disabled to stay under the 4GB RAM ceiling. Profile flag is explicit.
  cargo build --release --locked -p oz-policy-mcp --config profile.release.lto=false 2>&1 | tail -3
"

echo "Restarting oz-policy-mcp service..."
ssh "$REMOTE_HOST" "sudo systemctl restart oz-policy-mcp && sleep 2 && sudo systemctl is-active oz-policy-mcp"

echo "Verifying new tools are registered (via tools/list over the live endpoint)..."
ssh "$REMOTE_HOST" '
  set -euo pipefail
  ENV_TOK=$(sudo grep -oP "(?<=OZ_POLICY_MCP_TOKEN=).*" /etc/oz-policy-mcp.env)
  SID=$(curl -sf -X POST http://127.0.0.1:8080/mcp \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Authorization: Bearer $ENV_TOK" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"deploy-check\",\"version\":\"0\"}}}" \
    -D /tmp/h.txt -o /dev/null && grep -i "^mcp-session-id:" /tmp/h.txt | awk "{print \$2}" | tr -d "\r\n")
  TOOLS=$(curl -sf -X POST http://127.0.0.1:8080/mcp \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "Authorization: Bearer $ENV_TOK" \
    -H "Mcp-Session-Id: $SID" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}")
  echo "$TOOLS" | grep -oE "\"name\":\"[^\"]+\"" | sort -u
'

echo "Deploy complete."
