# MCP client configuration

Drop-in configs for the five MCP clients we cross-test:

1. [Claude Desktop](#claude-desktop)
2. [Cursor](#cursor)
3. [Cline (VSCode extension)](#cline-vscode-extension)
4. [Continue](#continue)
5. [`mcp-cli`](#mcp-cli)

Each section quotes the canonical snippet verbatim from
[`tests/mcp-clients/`](../tests/mcp-clients/). These same files are
consumed by the Phase 5 Stream D matrix job, which asserts byte-equal
transcripts across all five clients.

---

## Required environment

| Variable                 | Required for | Notes                                                                                                  |
|--------------------------|--------------|--------------------------------------------------------------------------------------------------------|
| `OZ_POLICY_MCP_BIN`      | All          | Absolute path to the compiled `oz-policy-mcp` binary. Default: `target/release/oz-policy-mcp`.         |
| `OZ_POLICY_MCP_TOKEN`    | HTTP only    | Bearer token for `Authorization: Bearer …`. Refused at startup if missing under `--http`.              |
| `OZ_POLICY_MCP_DATA_DIR` | Optional     | Persistence dir. Default: `$XDG_DATA_HOME/oz-policy-mcp` if that exists, else memory-only.             |

See [`tests/mcp-clients/README.md`](../tests/mcp-clients/README.md) for the
full envvar contract.

---

## Claude Desktop

Source: [`tests/mcp-clients/claude_desktop_mcp_servers.json`](../tests/mcp-clients/claude_desktop_mcp_servers.json).

Drop the `mcpServers.oz-policy-builder` entry into your local Claude Desktop
config — `~/Library/Application Support/Claude/claude_desktop_config.json`
on macOS, `%APPDATA%/Claude/claude_desktop_config.json` on Windows. Restart
Claude Desktop to pick up the change. The STDIO transport spawns the binary
as a subprocess; no token is required for STDIO.

```json
{
  "mcpServers": {
    "oz-policy-builder": {
      "command": "/absolute/path/to/oz-policy-mcp",
      "args": ["--stdio"],
      "env": {
        "_OZ_POLICY_MCP_DATA_DIR": "Optional. Uncomment + set to a writable directory to persist recordings/specs/artifacts across Claude Desktop restarts. Without this, every Claude Desktop launch starts with an empty store.",
        "RUST_LOG": "info"
      }
    }
  }
}
```

Docs: <https://modelcontextprotocol.io/docs/quickstart/user>.

---

## Cursor

Source: [`tests/mcp-clients/cursor_mcp.json`](../tests/mcp-clients/cursor_mcp.json).

Cursor reads `~/.cursor/mcp.json` (or `<repo>/.cursor/mcp.json` for project
scope).

```json
{
  "mcpServers": {
    "oz-policy-builder": {
      "command": "/absolute/path/to/oz-policy-mcp",
      "args": ["--stdio"],
      "env": {
        "_OZ_POLICY_MCP_DATA_DIR": "Optional persistence dir; see README.md for semantics.",
        "RUST_LOG": "info"
      }
    },

    "_oz_policy_builder_http_example": {
      "url": "http://127.0.0.1:8080/mcp",
      "headers": {
        "Authorization": "Bearer ${OZ_POLICY_MCP_TOKEN}"
      }
    }
  }
}
```

The second entry is a worked example of HTTP wiring; Cursor speaks SSE /
Streamable HTTP transports. The bearer token MUST be supplied — the
server's auth middleware refuses requests without it.

Docs: <https://docs.cursor.com/context/model-context-protocol>.

---

## Cline (VSCode extension)

Source: [`tests/mcp-clients/cline_settings.json`](../tests/mcp-clients/cline_settings.json).

Open VSCode's settings JSON (Command Palette → "Preferences: Open User
Settings (JSON)") and add:

```json
{
  "cline.mcpServers": {
    "oz-policy-builder": {
      "command": "/absolute/path/to/oz-policy-mcp",
      "args": ["--stdio"],
      "env": {
        "_OZ_POLICY_MCP_DATA_DIR": "Optional persistence dir; see README.md.",
        "RUST_LOG": "info"
      },
      "disabled": false,
      "autoApprove": []
    }
  }
}
```

**Leave `autoApprove` empty.** Per [`plan.md`](../plan.md) Phase 5 Stream D,
every `record_transaction` / `synthesize_policy` / `simulate_policy` /
`export_policy` / `verify_install` invocation should surface to the human
operator for review before running. Auto-approving tool calls defeats the
purpose of the policy-builder safety story.

Docs: <https://docs.cline.bot/mcp/configuring-mcp-servers>.

---

## Continue

Source: [`tests/mcp-clients/continue.json`](../tests/mcp-clients/continue.json).

Continue reads `~/.continue/config.json` (or `.continue/config.json` for
project scope).

```json
{
  "experimental": {
    "modelContextProtocolServer": {
      "transport": {
        "type": "stdio",
        "command": "/absolute/path/to/oz-policy-mcp",
        "args": ["--stdio"]
      },
      "env": {
        "_OZ_POLICY_MCP_DATA_DIR": "Optional persistence dir; see README.md.",
        "RUST_LOG": "info"
      }
    }
  }
}
```

Continue's STDIO contract matches Claude Desktop and Cursor; only the
configuration shape differs.

Docs: <https://docs.continue.dev/customization/mcp-servers>.

---

## `mcp-cli`

Source: [`tests/mcp-clients/mcp_cli_script.sh`](../tests/mcp-clients/mcp_cli_script.sh).

The Anthropic `mcp-cli` reference tool drives a scripted JSON-RPC session.
The committed script runs `initialize`, `tools/list`, `resources/list`,
`prompts/list`, and a single `tools/call record_transaction` against the
Phase 1 Blend hash. From the script header:

```bash
BIN="${OZ_POLICY_MCP_BIN:-$(pwd)/target/release/oz-policy-mcp}"
MCP_CLI="${MCP_CLI:-$(command -v mcp-cli 2>/dev/null || true)}"
OUT="${OUT:-oz-policy-mcp-mcp-cli-transcript.json}"
BLEND_HASH="5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e"
```

Install `mcp-cli` (`pip install mcp-cli`), build the server in release
mode, then:

```bash
cargo build --release -p oz-policy-mcp
tests/mcp-clients/mcp_cli_script.sh
```

The exit codes are pinned:

- `0` — transcript written.
- `1` — missing binary / unmet prereq.
- `2` — `mcp-cli` call failed.
- `3` — assertion mismatch on the canonical 5-tool surface
  (`record_transaction`, `synthesize_policy`, `simulate_policy`,
  `export_policy`, `verify_install`).

The script writes per-call JSON files and merges them into one transcript
under `$OUT`, which the Stream-D matrix job compares byte-for-byte against
the other four clients' transcripts.

---

## HTTP transport

To use the Streamable HTTP transport instead of STDIO:

1. Start the server with `--http <port>`:

   ```bash
   OZ_POLICY_MCP_TOKEN=$(openssl rand -hex 32) \
     oz-policy-mcp --http 8080
   ```

2. Point the client at `http://<host>:<port>/mcp` with header
   `Authorization: Bearer <OZ_POLICY_MCP_TOKEN>`.
3. Health-check via `GET /healthz` — this endpoint sits **outside** the
   auth layer (load balancers don't have the secret). See
   [`crates/oz-policy-mcp/src/main.rs`](../crates/oz-policy-mcp/src/main.rs)
   for the route wiring.

The MCP server speaks MCP spec revision `2025-11-25` per
[`docs/mcp-sdk-decision.md`](mcp-sdk-decision.md).

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
