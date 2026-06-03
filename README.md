# OZ Accounts Policy Builder

**Synthesize the minimum OpenZeppelin smart-account context rule + policies
that authorise exactly a recorded Stellar/Soroban operation — nothing more.
Hand the install envelope to the user's wallet to sign. Never auto-deploy.**

Built for the SCF #44 "OZ Accounts Policy Builder" RFP. This is an alpha
release — pre-`v1.0.0`, audit pending. See [Status](#status) before using
on mainnet.

---

## What it does

```
┌──────────┐    ┌────────────┐    ┌──────────┐    ┌────────┐    ┌────────┐    ┌─────────┐
│ Recording│───▶│ Synthesize │───▶│ Simulate │───▶│ Export │───▶│ Wallet │───▶│ Install │
│ (hash or │    │ PolicySpec │    │ permit + │    │ envelope│    │ signs  │    │ on-chain│
│  envelope)│   │ (Track A/B)│    │ deny     │    │ XDR    │    │        │    │         │
└──────────┘    └────────────┘    └──────────┘    └────────┘    └────────┘    └─────────┘
   record           synthesize       simulate       export        sign         submit
   (CLI / MCP)      (decision tree)  (simhost)      (installer)   (SEP-43)     (Soroban RPC)
```

Each arrow is a deterministic pure function (except `record` which hits a
Soroban RPC and freezes the result, and `submit` which the user explicitly
triggers in their own wallet). The pipeline is a regression suite over
three frozen walkthrough corpora:

- **Walkthrough 01** — [Blend yield-claim](docs/walkthroughs/01-blend-yield.md)
  (Track B `function_allowlist`).
- **Walkthrough 02** — [SEP-41 subscription](docs/walkthroughs/02-sep41-subscription.md)
  (Track A `spending_limit`).
- **Walkthrough 03** — [Soroswap bounded delegated trading](docs/walkthroughs/03-soroswap-bounded.md)
  (Track B `function_allowlist` + `asset_allowlist`).

---

## Quick start

Three commands from a fresh checkout (prerequisites: Rust 1.89.0 +
`stellar` CLI 25.1.0; see [`docs/install.md`](docs/install.md)):

```bash
# 1. Build the CLI.
cargo install --path crates/oz-policy-cli

# 2. Re-derive a generated policy from the Phase 3 fixture (offline).
oz-policy-cli codegen \
  walkthroughs/phase3-codegen-fixture/spec.json \
  --out /tmp/sanity-codegen

# 3. Confirm every committed walkthrough WASM hash byte-matches.
./scripts/reproducible-build.sh
```

For the full developer loop (record → synthesize → simulate → export → sign),
see [`docs/concepts.md`](docs/concepts.md) and the per-walkthrough docs.

---

## Documentation

| Topic                                     | Doc                                                    |
|-------------------------------------------|--------------------------------------------------------|
| Install the CLI, MCP server, wallet adapter | [`docs/install.md`](docs/install.md)                  |
| Concepts: context rules, policies, agent flow | [`docs/concepts.md`](docs/concepts.md)            |
| Walkthrough 01 — Blend yield-claim        | [`docs/walkthroughs/01-blend-yield.md`](docs/walkthroughs/01-blend-yield.md) |
| Walkthrough 02 — SEP-41 subscription      | [`docs/walkthroughs/02-sep41-subscription.md`](docs/walkthroughs/02-sep41-subscription.md) |
| Walkthrough 03 — Soroswap bounded         | [`docs/walkthroughs/03-soroswap-bounded.md`](docs/walkthroughs/03-soroswap-bounded.md) |
| MCP client configs (Claude Desktop / Cursor / Cline / Continue / mcp-cli) | [`docs/mcp-clients.md`](docs/mcp-clients.md) |
| Wallet setup: Freighter + passkey-kit     | [`docs/wallets.md`](docs/wallets.md)                   |
| Running a self-hosted MCP server          | [`docs/operations.md`](docs/operations.md)             |
| Security: disclosure, scope, threat model | [`docs/security.md`](docs/security.md), [`SECURITY.md`](SECURITY.md) |
| Proposed upstream OZ primitives           | [`docs/upstream.md`](docs/upstream.md)                 |
| Internal OZ shapes reference              | [`docs/oz-internal-shapes.md`](docs/oz-internal-shapes.md) |
| Reproducible build                        | [`docs/reproducible-build.md`](docs/reproducible-build.md) |
| The plan                                  | [`plan.md`](plan.md)                                   |

---

## Status

**Alpha.** Pre-`v1.0.0`. Used on Stellar testnet only.

| Area                              | Status                                                                                                                            |
|-----------------------------------|-----------------------------------------------------------------------------------------------------------------------------------|
| Phases 1–9 binary completion      | Green. All 285 workspace tests + 78 wallet-adapter tests + 9 gates pass.                                                          |
| Phase 7 testnet install           | **Resolved 2026-05-18.** Full record → install → verify flow lands a SUCCESS tx on testnet (`038583fa…ce90bb`, ledger 2617998, `context_rule_id=4`, `verifyInstall.matches=true`). See [`walkthroughs/phase7-testnet-install/install-result.json`](walkthroughs/phase7-testnet-install/install-result.json). |
| Phase 10 hosted MCP endpoint      | **TBD.** Stream B; see `infra/README.md` once it lands.                                                                            |
| Phase 10 mainnet canary           | **TBD.** Stream D; see `docs/mainnet-readiness.md` once it lands.                                                                  |
| Phase 10 release (`v1.0.0`)       | **TBD.** Stream C; tag + GitHub release + crates.io + npm publish.                                                                 |
| External audit                    | **Pending.** Handoff package at [`audits/handoff-package/`](audits/handoff-package). Self-audit (lint suite + fuzz) in place.    |

Do **not** run any flow that submits to mainnet until the Phase 10 mainnet
canary lands and the audit completes.

---

## License

Apache-2.0. See [`LICENSE-APACHE`](LICENSE-APACHE) for the full text.

This project bundles MIT-licensed upstream code (OpenZeppelin
`stellar-accounts`, kalepail's `pollywallet`, `passkey-kit`). The MIT
copyright lines are reproduced in [`NOTICE`](NOTICE) per the Apache-2.0
dual-license interaction. Re-distributions must keep `NOTICE` alongside
`LICENSE-APACHE`.

---

## Building on existing work

This project would not exist without:

- **OpenZeppelin `stellar-contracts`** — MIT
  (<https://github.com/OpenZeppelin/stellar-contracts>). The
  `MinimalSmartAccount`, the `Policy` trait, the `simple_threshold` /
  `weighted_threshold` / `spending_limit` primitives. Vendored under
  `crates/oz-policy-simhost/vendor/`. See
  [`docs/oz-internal-shapes.md`](docs/oz-internal-shapes.md) for the
  shapes we depend on.
- **`kalepail/pollywallet`** — MIT
  (<https://github.com/kalepail/pollywallet>). The SEP-43 implementation
  pattern that informs `wallet-adapter/src/sep43.ts`. Per
  [`plan.md`](plan.md) §"Cross-Phase Invariants → 5. Pollywallet
  engagement is explicit", every adoption is named in commit messages
  with its disposition (Adopt / Extend / Replace).
- **`passkey-kit`** — MIT
  (<https://github.com/kalepail/passkey-kit>). Backs the wallet
  adapter's passkey signing path.

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
