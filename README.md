# oz-policy-builder

Records a Stellar transaction and synthesizes the minimum OpenZeppelin
smart-account policy that would permit exactly that transaction.

Ships as a Rust CLI, an MCP server, a TypeScript wallet adapter, and a
hosted playground.

## Live

- Landing — <https://policy.erentopal.xyz>
- Playground — <https://policy.erentopal.xyz/playground>
- MCP endpoint — <https://mcp.erentopal.xyz/mcp>

## Build

```bash
cargo build --release
```

Requires Rust 1.89, stellar-cli 25. For the wallet adapter and frontend:
Node 22 and pnpm 10.

## CLI

```bash
# Record a testnet tx
cargo run -p oz-policy-cli -- record \
  --hash <tx-hash> \
  --rpc https://soroban-testnet.stellar.org \
  --network "Test SDF Network ; September 2015" \
  > recording.json

# Synthesize the minimum policy
cargo run -p oz-policy-cli -- synthesize recording.json \
  --mode auto --tightness exact --lifetime 432000 > spec.json

# Generate the Soroban contract source and wasm
cargo run -p oz-policy-cli -- codegen spec.json --out ./out

# Simulate permit and deny
cargo run -p oz-policy-cli -- simulate spec.json recording.json \
  --wasm-dir ./out --out report.json
```

## MCP server

Nine tools (`record_transaction`, `synthesize_policy`, `simulate_policy`,
`export_policy`, `verify_install`, `get_policy_artifacts`,
`simulate_custom_source`, `create_snapshot`, `get_snapshot`).

```bash
./target/release/oz-policy-mcp --stdio                          # local
./target/release/oz-policy-mcp --http 8080 --token "$TOKEN"     # hosted
```

## Playground

Interactive `/playground` route. Record, synthesize, inspect the generated
Rust, edit, re-simulate, and share as a stable URL.

## Layout

```
crates/
  oz-policy-core/        policy IR, decision tree, errors
  oz-policy-recorder/    Soroban RPC and XDR decoder
  oz-policy-codegen/     askama templates and sandbox build
  oz-policy-simhost/     in-process soroban-env-host harness
  oz-policy-installer/   install envelope builder
  oz-policy-mcp/         rmcp server (stdio and http)
  oz-policy-cli/         thin CLI over the above
wallet-adapter/          TypeScript SEP-43 adapter
frontend/                Vite + React landing and /playground
skills/oz-policy-builder/  agent skill
```

## License

Apache-2.0
