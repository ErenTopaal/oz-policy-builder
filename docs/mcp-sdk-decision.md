# `rmcp` (Rust MCP SDK) — Phase 1 Decision

## Pinned crate version

**`rmcp = 1.7.0`** (latest stable, released 2026-05-13).

License: Apache-2.0 (workspace).
Edition: `2024`. MSRV: requires Rust >= 1.85 (edition 2024 baseline). Compatible with our 1.89.0 toolchain pin. The rmcp upstream repo's own `rust-toolchain.toml` pins 1.92, but that only applies to building the rmcp repo itself; consumers of the published crate can build on 1.85+.

Source URLs verified:
- crates.io: `https://crates.io/crates/rmcp/1.7.0`
- GitHub release: `https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v1.7.0`
- CHANGELOG: `https://github.com/modelcontextprotocol/rust-sdk/blob/main/crates/rmcp/CHANGELOG.md`

## MCP spec revisions supported

`rmcp 1.7.0` supports both currently-deployed MCP spec revisions:

- **`2025-06-18`** — added in earlier releases, fully supported in 1.7.0.
- **`2025-11-25`** — added in PR #802 (`feat: add 2025-11-25 protocol version support`), merged 2026-04-10, first published in `rmcp-v1.5.0` on 2026-04-16.

Evidence in source:
- `crates/rmcp/src/model.rs`: `pub const V_2025_11_25: Self = Self(Cow::Borrowed("2025-11-25"));`
- `crates/rmcp/src/model.rs`: `"2025-11-25" => return Ok(ProtocolVersion::V_2025_11_25),`
- `crates/rmcp/src/transport/streamable_http_server/tower.rs`: doc-link to `https://modelcontextprotocol.io/specification/2025-11-25/basic/transports#streamable-http`
- `conformance/results/2026-02-25-rust-sdk-assessment.md`: conformance tests reference both `2025-06-18` and `2025-11-25` and pass for the core surface we need (tools/call, server initialize, prompts, resources).
- `crates/rmcp/README.md`: *"The official Rust SDK for the [Model Context Protocol](https://modelcontextprotocol.io/specification/2025-11-25)."*

## Transports

The plan requires **STDIO** (subprocess for IDEs) and **Streamable HTTP** (remote/CI). Both are supported in `rmcp 1.7.0`:

- **STDIO** — `transport-async-rw`, `transport-child-process`, `transport-io` features (all default-feature-adjacent). Documented in `crates/rmcp/src/transport/async_rw.rs` with spec link to 2025-06-18 (the digest format applies identically under 2025-11-25). Tests in `crates/rmcp/tests/test_streamable_http_priming.rs` and `test_server_initialization.rs` exercise stdio + `protocolVersion: "2025-11-25"` priming end-to-end.
- **Streamable HTTP** — `transport-streamable-http-server`, `transport-streamable-http-client` features, with explicit `tower`-based service in `streamable_http_server/tower.rs`. Spec-aligned to 2025-11-25 (the deprecated SSE-only transport from 2024-11-05 is NOT implemented, matching the plan's exclusion).

## Decision: **GREEN LIGHT**

Proceed with the Rust SDK as the primary MCP server stack for Phase 5. No TypeScript-SDK FFI fallback is required. The `rmcp 1.7.0` pin satisfies:

- spec revision `2025-11-25` (mandatory per plan)
- both required transports (STDIO + Streamable HTTP)
- Apache-2.0 license alignment with the toolkit
- MSRV compatibility with our pinned Rust 1.89.0 stable

## Feature flags to enable in `oz-policy-mcp/Cargo.toml`

```toml
[dependencies]
rmcp = { version = "=1.7.0", default-features = false, features = [
    "macros",
    "server",
    "transport-async-rw",          # stdio framing
    "transport-io",                # stdio wiring
    "transport-streamable-http-server",  # http transport
    "schemars",                    # JSON Schema emission for tools
] }
```

Avoid the `auth` / `oauth2` features unless we adopt OAuth in Phase 5 (the plan's MCP-auth posture is still open per research §16).

## Risks / notes

- `rmcp` introduced edition-2024 at v1.5.0+; this implicitly raises MSRV to 1.85. Our 1.89.0 stable pin is above this.
- `rmcp 1.4.0` added a workspace `rust-toolchain.toml` bump to 1.92. This is for the rmcp dev-loop only; downstream consumers (us) are not constrained by it.
- The plan's research §16 flag "rmcp parity as an open risk" is now closed for v1; we re-evaluate at Phase 5 kickoff in case the next MCP spec revision drops before then.
- If a 2026-xx-xx spec revision is published before Phase 5 begins, re-check this doc before pinning a newer rmcp. The crate ships protocol-revision constants per release, so the upgrade path is: bump rmcp version, verify the new `V_xxxx_xx_xx` constant exists, run conformance.
