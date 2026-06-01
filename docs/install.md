# Install

How to get the three pieces of the **OZ Accounts Policy Builder** onto your
machine: the **CLI** (`oz-policy-cli`), the **MCP server** (`oz-policy-mcp`),
and the **wallet adapter** TypeScript package
(`@oz-policy-builder/wallet-adapter`).

This is the alpha-channel install guide. Pre-built release binaries and an
npm-published wallet-adapter are gated on `v1.0.0` (see
[Release status](#release-status)).

---

## Prerequisites

| Tool                  | Version   | Why                                                                  |
|-----------------------|-----------|----------------------------------------------------------------------|
| Rust toolchain        | `1.89.0`  | Pinned by [`rust-toolchain.toml`](../rust-toolchain.toml). Includes `rustfmt`, `clippy`, `wasm32-unknown-unknown` target. |
| `stellar` CLI         | `25.1.0`  | Used by `cargo build`-time codegen optimisation and by reproducible build (see [`docs/reproducible-build.md`](reproducible-build.md)). |
| Node.js               | `22.x`    | Wallet-adapter LTS pin — verified in [`wallet-adapter/examples/README.md`](../wallet-adapter/examples/README.md). |
| pnpm                  | `>= 10.x` | Workspace package manager for the wallet-adapter.                    |

Network access requirements (only for live operations — every component is
offline-first for synthesis and codegen):

- **Soroban testnet RPC** at `https://soroban-testnet.stellar.org` — needed
  by `record` (hash mode), `prepare-install`, and any wallet-adapter example
  that actually submits.
- **Friendbot** at `https://friendbot.stellar.org` — funds testnet keypairs.

---

## Install the CLI (`oz-policy-cli`)

The CLI mirrors the MCP surface: `record`, `synthesize`, `simulate`,
`codegen`, `prepare-install`. See
[`crates/oz-policy-cli/src/main.rs`](../crates/oz-policy-cli/src/main.rs) for
the full subcommand list and the deterministic exit-code mapping.

### From source (today)

```bash
git clone <repo> oz-accounts-policy-builder
cd oz-accounts-policy-builder
cargo install --path crates/oz-policy-cli
```

Verify:

```bash
oz-policy-cli --help
```

### Pre-built binaries (after v1.0.0 — TBD)

Per [`plan.md`](../plan.md) Phase 10 Stream C, the `v1.0.0` GitHub Release
will attach `oz-policy-mcp-{linux,darwin}-{amd64,arm64}` binaries together
with `SHA256SUMS` and a signed `SHA256SUMS.asc`. The CLI ships in the same
release bundle. **Not yet available** — track Stream C in `.github/workflows/release.yml`.

---

## Install the MCP server (`oz-policy-mcp`)

The MCP server exposes the same five tools the CLI mirrors
(`record_transaction`, `synthesize_policy`, `simulate_policy`,
`export_policy`, `verify_install`) over either of two transports:

- **STDIO** — for editor / desktop clients that subprocess MCP servers
  (Claude Desktop, Cursor, Cline, Continue). See
  [MCP clients](mcp-clients.md).
- **Streamable HTTP** — `POST /mcp` with `Authorization: Bearer <token>`,
  per MCP spec revision `2025-11-25`. See [Operations](operations.md).

### From source

```bash
cargo install --path crates/oz-policy-mcp
```

Verify:

```bash
oz-policy-mcp --help
oz-policy-mcp --stdio          # listens on STDIN/STDOUT
oz-policy-mcp --http 8080 --token "$OZ_POLICY_MCP_TOKEN"
```

The HTTP transport refuses to start if neither `--token` nor the
`OZ_POLICY_MCP_TOKEN` env var supplies a bearer secret — see
[`crates/oz-policy-mcp/src/main.rs`](../crates/oz-policy-mcp/src/main.rs).

### As a managed service

For Claude Desktop / Cursor / Cline / Continue / `mcp-cli`, copy a snippet
from [`tests/mcp-clients/`](../tests/mcp-clients/) into your client's
config file. The [MCP clients](mcp-clients.md) doc walks through each.

---

## Install the wallet adapter (`@oz-policy-builder/wallet-adapter`)

The TypeScript package implementing SEP-43 (Freighter, passkey-kit) plus the
post-sign `AuthPayload` encoder helper required by OZ smart-account installs.

### From local workspace (today)

```bash
cd wallet-adapter
pnpm install
pnpm build
pnpm test                       # mocked
INTEGRATION=1 pnpm test         # real testnet (requires built CLI)
```

The package's public exports (entry point, Freighter, passkey) are pinned in
[`wallet-adapter/package.json`](../wallet-adapter/package.json) under
`exports`.

### From npm (after v1.0.0 — TBD)

```bash
pnpm add @oz-policy-builder/wallet-adapter @stellar/stellar-sdk
```

npm publish is **not yet done** — Phase 10 Stream C ships it alongside the
v1.0.0 release. The package currently lives in this monorepo only.

---

## Quick sanity check

Once the CLI is installed, run a Phase 3 codegen against a frozen Phase 1
fixture without touching the network:

```bash
oz-policy-cli codegen \
  walkthroughs/phase3-codegen-fixture/spec.json \
  --out /tmp/sanity-codegen
ls /tmp/sanity-codegen/slot_0/
# Expect: policy.wasm, source.rs, wasm_hash.txt
```

The committed WASM hash for that fixture is asserted byte-equal by the
reproducible-build script:

```bash
./scripts/reproducible-build.sh
```

See [`docs/reproducible-build.md`](reproducible-build.md) for the toolchain
pin and the manifest emitted at the end.

---

## Release status

| Channel               | Status                                                                       |
|-----------------------|------------------------------------------------------------------------------|
| Source builds         | Supported. All Phase 1–9 binary completion criteria green.                   |
| `cargo install`       | Supported (from a local checkout).                                           |
| GitHub Releases       | **TBD** — gated on Phase 10 Stream C (`.github/workflows/release.yml`).      |
| crates.io publish     | **TBD** — Phase 10 Stream C.                                                 |
| npm publish           | **TBD** — Phase 10 Stream C.                                                 |
| Hosted MCP endpoint   | **TBD** — Phase 10 Stream B. Track in `infra/README.md` once Stream C lands. |
| Mainnet canary        | **TBD** — Phase 10 Stream D. Track in `docs/mainnet-readiness.md`.           |
| External audit        | Pending — see [`audits/READY.md`](../audits/READY.md).                       |

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
