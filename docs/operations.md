# Operations

Running a self-hosted `oz-policy-mcp` server. For client-side
configuration see [MCP clients](mcp-clients.md). For the build-time
prerequisites see [Install](install.md).

---

## Transports + bind surface

`oz-policy-mcp` supports two transports
([`crates/oz-policy-mcp/src/main.rs`](../crates/oz-policy-mcp/src/main.rs)):

| Transport         | Flag(s)              | When to use                                                                                |
|-------------------|----------------------|--------------------------------------------------------------------------------------------|
| STDIO             | `--stdio`            | Editor / desktop clients that subprocess-spawn the server (Claude Desktop, Cursor, etc.).  |
| Streamable HTTP   | `--http <port>`      | Hosted multi-user endpoints. Binds `0.0.0.0:<port>`.                                       |

Under `--http` the server exposes:

- `POST /mcp` — MCP spec `2025-11-25` Streamable HTTP transport, gated by
  `Authorization: Bearer <token>`.
- `GET /healthz` — JSON `{ ok: true, version: "<pkg-version>" }`. Sits
  **outside** the bearer-auth layer so load balancers and k8s probes
  don't need the secret.

`--stdio` and `--http` are mutually exclusive. STDIO is the default.

---

## Environment variables

| Variable                  | Required?                  | Effect                                                                                                                                |
|---------------------------|----------------------------|---------------------------------------------------------------------------------------------------------------------------------------|
| `OZ_POLICY_MCP_TOKEN`     | **Yes** under `--http`     | Bearer secret. The HTTP transport refuses to start if neither this env var nor `--token <TOKEN>` supplies a value.                    |
| `OZ_POLICY_MCP_DATA_DIR`  | Optional                   | Persistence directory for `McpStore`. If unset, falls back to `$XDG_DATA_HOME/oz-policy-mcp` (if the dir exists) else memory-only.    |
| `RUST_LOG`                | Optional                   | tracing-subscriber filter. Defaults to `info`. Example: `RUST_LOG=info,oz_policy_mcp=debug`.                                          |

The `--token` flag (or `OZ_POLICY_MCP_TOKEN`) and the `--data-dir` flag
(or `OZ_POLICY_MCP_DATA_DIR`) are interchangeable; the flag wins per CLI
convention. See
[`crates/oz-policy-mcp/src/main.rs`](../crates/oz-policy-mcp/src/main.rs)
for the exact precedence rules.

---

## Persistence

`McpStore` holds recordings, specs, and artefact bundles produced by
in-flight sessions. Two modes:

1. **Memory-only** (default when `OZ_POLICY_MCP_DATA_DIR` is unset and
   `$XDG_DATA_HOME/oz-policy-mcp` does not exist). Every server restart
   loses all state. Fine for STDIO clients which spawn a fresh server per
   session.
2. **On-disk** (`--data-dir <PATH>` or `OZ_POLICY_MCP_DATA_DIR=<PATH>`).
   `McpStore` writes JSON files under `<PATH>/`. Surviving state is read
   on next startup. **Recommended for hosted endpoints.**

There is **no shared store backend** today — every server replica has its
own `McpStore`. This is fine for STDIO and for single-replica HTTP
deployments. For horizontal scaling under HTTP, see
[Scaling notes](#scaling-notes).

---

## Logging

**All logs go to stderr.** This is non-negotiable for STDIO mode (stdout
is the JSON-RPC frame channel — any stray write corrupts the protocol).
The HTTP transport uses the same stderr writer for consistency.

`tracing_subscriber::fmt` is initialised with:

- writer pinned to `std::io::stderr`
- ANSI disabled (`with_ansi(false)`) — friendlier for log aggregators
- filter from `RUST_LOG`, defaulting to `info`

Example dev invocation:

```bash
RUST_LOG=info,oz_policy_mcp=debug \
  oz-policy-mcp --http 8080 --token "$OZ_POLICY_MCP_TOKEN"
```

---

## Graceful shutdown

The HTTP server installs a SIGINT/SIGTERM handler that gives in-flight
connections a `GRACEFUL_SHUTDOWN_GRACE = 10s` window to drain (long
enough for SSE streams to deliver a final priming event; short enough
that container orchestrators don't escalate to SIGKILL). See
[`crates/oz-policy-mcp/src/main.rs`](../crates/oz-policy-mcp/src/main.rs).

---

## TLS termination

The server speaks **plain HTTP**. TLS termination is the operator's job
— terminate at your reverse proxy (Caddy, nginx, Cloudflare, Fly.io's
edge, etc.). Two reasons:

1. The hosted-MCP target uses a managed cert from the provider (per
   [`plan.md`](../plan.md) Phase 10 Stream B's hosting note).
2. Reverse-proxy TLS is independently testable / rotatable / monitorable
   from the app.

Direct internet exposure with `--http` and no proxy is unsupported.

---

## Scaling notes

The MCP server is **stateless beyond `McpStore`**. Beyond that store
every tool handler is a pure function of (input, store snapshot), so
horizontal scaling is possible if the store is shared.

Today `McpStore` is in-memory or local-disk only — there is **no shared
backend** (no Redis, no SQL, no S3). For a single-replica deployment
that is fine. For multi-replica HTTP scaling you would need to either:

- pin a session to a replica (sticky session by `Mcp-Session-Id` header),
  *or*
- swap `McpStore` for a backend that shares state across replicas. This
  is **TBD** — track in `infra/README.md` once Stream B starts.

---

## Observability

Currently the server emits **stderr-formatted tracing events only**. The
plan ([`plan.md`](../plan.md) Phase 10 Stream B) is to expose metrics via
an OpenTelemetry exporter using the MCP semantic conventions. That work
is **TBD** — track in `infra/<provider>/observability/` once Stream B
starts.

---

## Phase 7 BLOCKER status (resolved 2026-05-18)

The `__check_auth` trap on `Void` `AuthPayload` signatures is closed.
The full record → install → verify flow lands a SUCCESS transaction on
testnet — frozen evidence in
[`walkthroughs/phase7-testnet-install/install-result.json`](../walkthroughs/phase7-testnet-install/install-result.json)
(tx `038583fa…ce90bb`, `context_rule_id=4`, `verifyInstall.matches=true`).

The fix has two pieces, both client-side from the MCP server's
perspective:

1. **Write path** — the AuthPayload encoder
   ([`wallet-adapter/src/oz_smart_account_auth.ts`](../wallet-adapter/src/oz_smart_account_auth.ts))
   computes the post-PR-#655 digest and injects the encoded payload via
   the `installPolicy` `ozAuthPayloadEncoder` hook (commit `bd60009`).
2. **Read path** — `verify_install` now performs a real on-chain
   readback via `simulateTransaction(SA.get_context_rule(rule_id))` and
   diffs the decoded `ContextRule` against the supplied `PolicySpec`
   (commit `2606f84`, `crates/oz-policy-mcp/src/verify_chain.rs`).

Operators do not need to do anything special — the encoder runs
client-side in the wallet adapter, transparently. See
[`walkthroughs/phase7-testnet-install/BLOCKER.md`](../walkthroughs/phase7-testnet-install/BLOCKER.md)
for the historical diagnostic.

---

## Phase 10 TBDs

These items are explicitly out of scope for this dispatch (Phase 10
Stream A is docs-only; Streams B / C / D are running in parallel or
sequentially):

| Item                                          | Owner          | Pointer                                                          |
|-----------------------------------------------|----------------|------------------------------------------------------------------|
| Hosted MCP endpoint URL + IaC                 | Stream B       | `infra/README.md` (TBD), `docs/hosting-decision.md` (TBD)        |
| Production RPC provider choice                | Stream B       | `docs/rpc-mainnet-decision.md` (TBD)                             |
| OpenTelemetry exporter + dashboards           | Stream B       | `infra/<provider>/observability/` (TBD)                          |
| Shared-store backend for horizontal scaling   | Stream B       | Per-deployment decision; tracked in `infra/README.md` once it lands |
| Mainnet canary tx + evidence                  | Stream D       | `docs/mainnet-readiness.md` (TBD), `docs/canary/` (TBD)          |
| `v1.0.0` release artefact bundle + signing    | Stream C       | `.github/workflows/release.yml` (in flight)                       |

Until these land, treat any URL or runbook in this document as a contract
for what the hosted endpoint **will** expose, not an endpoint you can hit
today.

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
