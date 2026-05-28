# `@oz-policy-builder/wallet-adapter` — headless examples

End-to-end programmatic install flows that mirror the Phase 1 / Phase 2
walkthrough corpora. Each script uses the package's `PasskeyWallet` adapter
in **headless-keypair mode** (`signerSecretKey`) to sign and submit a real
testnet transaction.

## Prerequisites

| Requirement     | Notes                                                                     |
|-----------------|---------------------------------------------------------------------------|
| Node.js         | `>= 22.11.0` (LTS "Jod"). `pnpm tsx` is the runner.                       |
| pnpm            | `>= 10.x`. Used for `pnpm install` and `pnpm tsx ...`.                    |
| Network         | Public Stellar testnet RPC (`https://soroban-testnet.stellar.org`).       |
| Friendbot       | `https://friendbot.stellar.org` reachable.                                |
| Built CLI       | `cargo build -p oz-policy-cli` — the scripts shell out to it.             |
| Walkthrough corpus | Phase 1 frozen walkthroughs at `walkthroughs/0{1,2}-*/`.               |

## What each script does

All scripts follow the same shape:

1. `Keypair.random()` — fresh testnet secret, never persisted.
2. Fund the new G-address via Friendbot.
3. `oz-policy-cli synthesize` against the frozen recording → `PolicySpec` JSON.
4. `oz-policy-cli prepare-install` → install envelope XDR (calls Soroban RPC).
5. `PasskeyWallet.signTransaction(...)` — real `Keypair.sign()` via stellar-sdk.
6. Soroban RPC `sendTransaction`.
7. Print a single JSON report to stdout, exit 0 on `submitted` else non-zero.

The JSON report shape (stable, parseable by CI):

```json
{
  "walkthrough": "01-blend-yield",
  "network": "testnet",
  "status": "submitted" | "preflight_failed" | "rejected" | "network_error" | "placeholder",
  "txHash": "...",            // present iff submitted
  "contextRuleId": 0,         // present iff submitted AND retrievable
  "error": "...",             // present iff non-success
  "signerAddress": "G..."
}
```

## Network-dependent — by design

These scripts are **NOT unit tests**. They hit the public testnet and require
working network access in both directions (egress to Friendbot + RPC; the RPC
endpoint's own egress to the Stellar network). When the network is unreachable
the script reports `status: "network_error"` and exits non-zero. CI can opt in
to running these under an `INTEGRATION=1` (or similar) gate; default `pnpm test`
**does not** run them — see `vitest.config.ts`.

## Known preflight blocker: `primitive_address_unknown`

Until Phase 7 ships a per-network deployment registry of the OpenZeppelin
account-policy primitive contracts (`simple_threshold`, `weighted_threshold`,
`spending_limit`), `oz-policy-cli prepare-install` will return:

```
Error: E_INSTALL_PREFLIGHT_FAILED('primitive_address_unknown spending_limit on Test SDF Network ; September 2015')
```

This is honest, expected behavior — see
`crates/oz-policy-installer/src/registry.rs` for the rationale. The example
scripts surface this verbatim as `status: "preflight_failed"`. They do **not**
suppress, retry, or paper over it.

Until that registry lands the examples are useful for:

- Validating that the headless signing path produces a structurally valid
  envelope (signing succeeds even if preflight fails, because the failure is
  before signing).
- Smoke-testing Friendbot funding + RPC reachability in CI.
- Exercising the JSON report contract that downstream tools depend on.

When the registry lands, the same scripts will start reporting `submitted`
without code changes.

## Scripts

| Script                                | Walkthrough                                | Status notes |
|---------------------------------------|--------------------------------------------|--------------|
| `01-blend-yield-headless.ts`          | `walkthroughs/01-blend-yield/`             | Real recording frozen; preflight blocked. |
| `02-sep41-subscription-headless.ts`   | `walkthroughs/02-sep41-subscription/`      | Real recording frozen; preflight blocked. |
| `03-soroswap-bounded-headless.ts`     | `walkthroughs/03-soroswap-bounded/` (TBD)  | **Placeholder.** Phase 8 must freeze the corpus first. |

## Running

From the workspace root:

```bash
cargo build -p oz-policy-cli        # builds target/debug/oz-policy-cli
cd wallet-adapter
pnpm install                        # if not already done
pnpm tsx examples/01-blend-yield-headless.ts
pnpm tsx examples/02-sep41-subscription-headless.ts
pnpm tsx examples/03-soroswap-bounded-headless.ts   # placeholder
```

Each script prints a single JSON object to stdout and exits with:

- `0` — `submitted`
- `1` — `preflight_failed` / `rejected` / `placeholder`
- `2` — unhandled top-level rejection

## Security notes

- Testnet only. The scripts never reach mainnet.
- The keypair is freshly generated per run via `Keypair.random()` and never
  persisted. Friendbot funds it; after the run, any leftover XLM is
  unrecoverable but trivial (testnet has no value).
- Never edit these scripts to read a secret from disk or env unless you are
  certain that secret is testnet-only. The headless `signerSecretKey` path
  in `PasskeyWallet` is for development and CI, NOT for production custody.
