# oz-policy-builder

records a stellar transaction and synthesizes the minimum openzeppelin smart-account
policy that would permit exactly that transaction. ships as a rust cli + mcp server
+ typescript wallet adapter.

## requirements

- rust 1.89
- stellar-cli 25
- node 22, pnpm 10 (only for the wallet adapter)

## build

```bash
cargo build --release
```

## use (cli)

```bash
# record a testnet tx
cargo run -p oz-policy-cli -- record \
  --hash <tx-hash> \
  --rpc https://soroban-testnet.stellar.org \
  --network "Test SDF Network ; September 2015" \
  > recording.json

# synthesize the minimum policy
cargo run -p oz-policy-cli -- synthesize recording.json \
  --mode auto --tightness exact --lifetime 432000 \
  --rule-name "my-rule" > spec.json

# generate the soroban contract source + wasm
cargo run -p oz-policy-cli -- codegen spec.json --out ./out

# simulate permit + deny vectors
cargo run -p oz-policy-cli -- simulate spec.json recording.json \
  --wasm-dir ./out --out report.json

# build a wallet-signable install envelope (does not submit)
cargo run -p oz-policy-cli -- prepare-install spec.json \
  --smart-account <c-addr> --source <g-addr> \
  --rpc https://soroban-testnet.stellar.org \
  --network "Test SDF Network ; September 2015" \
  --account-revision post-pr-655
```

## use (mcp server)

5 tools (`record_transaction`, `synthesize_policy`, `simulate_policy`, `export_policy`,
`verify_install`) over stdio or streamable http.

```bash
cargo build --release -p oz-policy-mcp
./target/release/oz-policy-mcp --stdio          # subprocess transport
./target/release/oz-policy-mcp --http 8080 --token "$TOKEN"   # http transport
```

wire the binary path into your mcp client's config.

## use (wallet adapter)

```bash
cd wallet-adapter
pnpm install
pnpm test
```

real example in `wallet-adapter/src/integration.test.ts` — runs the full
sign + submit + verify flow against testnet.

## crate layout

- `oz-policy-core` — policy ir, decision tree, sep-41 detection, error types
- `oz-policy-recorder` — soroban rpc + xdr decoder
- `oz-policy-codegen` — askama templates + sandbox build
- `oz-policy-simhost` — in-process `soroban-env-host` harness
- `oz-policy-installer` — install envelope builder + preflight
- `oz-policy-mcp` — rmcp server
- `oz-policy-cli` — thin cli over the others
- `wallet-adapter` — typescript sep-43 adapter (freighter + passkey-kit)

## license

apache-2.0 (see `LICENSE-APACHE`)
