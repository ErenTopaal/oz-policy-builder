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

## Per-script preflight behaviour

The Track-B `function_allowlist` policy is deployed on testnet (see
`walkthroughs/phase7-testnet-install/deployed-addresses.json`), so example
01 (Blend) runs through preflight successfully. The Track-A primitives
(`simple_threshold`, `weighted_threshold`, `spending_limit`) are **not**
yet deployed on testnet, so example 02 (SEP-41 subscription)
short-circuits at preflight with:

```
Error: E_INSTALL_PREFLIGHT_FAILED('primitive_address_unknown spending_limit on Test SDF Network ; September 2015')
```

The example scripts surface this verbatim as `status: "preflight_failed"`;
they do **not** suppress, retry, or paper over it. Deploying the OZ
Track-A primitive WASMs on testnet and registering their addresses is a
Phase 9 follow-up — see
`crates/oz-policy-installer/src/registry.rs` for the rationale.

## Scripts

| Script                                | Walkthrough                                | Status notes |
|---------------------------------------|--------------------------------------------|--------------|
| `01-blend-yield-headless.ts`          | `walkthroughs/01-blend-yield/`             | Real recording frozen; `function_allowlist` registry hit ready (testnet install verified 2026-05-18). |
| `02-sep41-subscription-headless.ts`   | `walkthroughs/02-sep41-subscription/`      | Real recording frozen; preflight returns `E_INSTALL_PREFLIGHT_FAILED` (spending_limit not in registry yet). |
| `03-soroswap-bounded-headless.ts`     | `walkthroughs/03-soroswap-bounded/`        | **Placeholder script** — corpus is frozen (Phase 8); the script still exits `status: "placeholder"` pending the same wiring as examples 01/02. |

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
