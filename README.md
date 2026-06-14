# oz-policy-builder

records a stellar transaction and synthesizes the minimum openzeppelin
smart-account policy that would permit exactly that transaction.

ships as a rust cli + mcp server + typescript wallet adapter + a hosted
playground.

## live

- landing — <https://policy.erentopal.xyz>
- playground — <https://policy.erentopal.xyz/playground>
- mcp endpoint — <https://mcp.erentopal.xyz/mcp>

## build

```bash
cargo build --release
```

requires rust 1.89, stellar-cli 25. for the wallet adapter and frontend: node 22 + pnpm 10.

## cli

```bash
# record a testnet tx
cargo run -p oz-policy-cli -- record \
  --hash <tx-hash> \
  --rpc https://soroban-testnet.stellar.org \
  --network "Test SDF Network ; September 2015" \
  > recording.json

# synthesize the minimum policy
cargo run -p oz-policy-cli -- synthesize recording.json \
  --mode auto --tightness exact --lifetime 432000 > spec.json

# generate the soroban contract source + wasm
cargo run -p oz-policy-cli -- codegen spec.json --out ./out

# simulate permit + deny
cargo run -p oz-policy-cli -- simulate spec.json recording.json \
  --wasm-dir ./out --out report.json
```

## mcp server

9 tools (`record_transaction`, `synthesize_policy`, `simulate_policy`,
`export_policy`, `verify_install`, `get_policy_artifacts`,
`simulate_custom_source`, `create_snapshot`, `get_snapshot`).

```bash
./target/release/oz-policy-mcp --stdio                          # local
./target/release/oz-policy-mcp --http 8080 --token "$TOKEN"     # hosted
```

## playground

interactive `/playground` route — record → synthesize → inspect generated
rust → edit → re-simulate → share as a stable url.

## layout

```
crates/
  oz-policy-core/        policy ir, decision tree, errors
  oz-policy-recorder/    soroban rpc + xdr decoder
  oz-policy-codegen/     askama templates + sandbox build
  oz-policy-simhost/     in-process soroban-env-host harness
  oz-policy-installer/   install envelope builder
  oz-policy-mcp/         rmcp server (stdio + http)
  oz-policy-cli/         thin cli over the above
wallet-adapter/          typescript sep-43 adapter
frontend/                vite + react landing + /playground
skills/oz-policy-builder/  agent skill
```

## license

apache-2.0
