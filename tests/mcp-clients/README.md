# Cross-client MCP configurations (Phase 5 Stream D documentation)

This directory holds reference MCP-client configs that point at the
`oz-policy-mcp` server. They are **not** consumed by any automated unit
test — the in-repo conformance comes from `crates/oz-policy-mcp/tests/`
(`stdio_smoke.rs` and `http_smoke.rs`). These files exist so:

1. **Manual smoke runs** — copy a snippet into your client's settings
   to validate the wire surface in a real environment.
2. **CI matrix runs** — Phase 5 Stream D (per `plan.md`) wires a
   GitHub-Actions matrix job that drives the same scripted JSON-RPC
   session through each client and diffs the resulting transcripts.
   The job sources the config files in this directory verbatim.

## Required environment

All clients must have access to a built `oz-policy-mcp` binary. The
canonical paths used by the configs below:

* `${OZ_POLICY_MCP_BIN}` — absolute path to the binary
  (default: `target/release/oz-policy-mcp` from the repo root).
* `${OZ_POLICY_MCP_TOKEN}` — bearer token for HTTP transport
  (REQUIRED for `--http`; ignored under `--stdio`).
* `${OZ_POLICY_MCP_DATA_DIR}` — optional persistence path
  (default: `$XDG_DATA_HOME/oz-policy-mcp` if that dir exists, else
  memory-only).

The configs use the STDIO transport by default — every supported
client subprocesses MCP servers under STDIO. Switch to HTTP by pointing
the client at `http://<host>:<port>/mcp` (per its own docs) and
including `Authorization: Bearer ${OZ_POLICY_MCP_TOKEN}` per request.

## Files

| File                                  | Client                                                                     |
|---------------------------------------|----------------------------------------------------------------------------|
| `claude_desktop_mcp_servers.json`     | Claude Desktop (`mcpServers` block in `~/Library/Application Support/Claude/claude_desktop_config.json`) |
| `cursor_mcp.json`                     | Cursor (`~/.cursor/mcp.json`)                                              |
| `cline_settings.json`                 | Cline (`mcpServers` in the Cline VSCode extension settings JSON)           |
| `continue.json`                       | Continue (`~/.continue/config.json`)                                       |
| `mcp_cli_script.sh`                   | `mcp-cli` (the Anthropic MCP reference CLI; drives a scripted session)     |

Per `plan.md` Phase 5 Stream D, **all five clients must produce
byte-equal transcripts** when driven against the same fresh server.
The matrix job's pass condition is `diff <client> <reference> == empty`
across the five.
