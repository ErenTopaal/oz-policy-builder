<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 OZ Policy Builder contributors
-->

# Project Status Snapshot

**Project:** OZ Accounts Policy Builder
**State:** alpha, pre-v1.0.0
**Date:** 2026-05-18
**Branch:** `phase-1-foundations`
**Total commits:** 149
**Total tests:** **363** (285 Rust workspace + 78 `wallet-adapter` passing; 2 wallet-adapter tests skipped — `INTEGRATION=1` testnet + one passkey-kit upstream remediation). 10 Rust tests are `#[ignore]`-gated on external resources (testnet RPC, sandbox compile).

This is a single-page snapshot of where development sits at the end of
the Phase 1-10 work tracked in [`plan.md`](plan.md). For the
human-runnable steps remaining before a real release, see
[`docs/mainnet-readiness.md`](docs/mainnet-readiness.md).

## Phase-by-phase

| Phase | Description | Status | Binary completion gate | Commit SHA range |
| --- | --- | --- | --- | --- |
| Phase 1 | Foundations: workspace, TBD resolution, recorder | Complete | `cargo nextest run --workspace -- --include-ignored recorder::integration::blend_claim_roundtrip` green; `walkthroughs/01-blend-yield/expected-recording.json` byte-equal | `c05ef85` → `995bd4d` |
| Phase 2 | Policy IR & Track A synthesizer | Complete | `cargo nextest run -p oz-policy-core -p oz-policy-installer -p oz-policy-cli` green; SEP-41 subscription Track-A round-trip byte-equal | `35bccac` → `1048cce` |
| Phase 3 | Track B codegen (askama templates + sandbox build) | Complete | `cargo nextest run -p oz-policy-codegen phase3_render_byte_equal` green; ignored `phase3_compile_hash_pinned` matches `cb2a8736…` | `4df5ca4` → `2733f04` |
| Phase 4 | Simulation harness + proptest deny vectors | Complete | `cargo nextest run --workspace` green incl. `oz-policy-simhost::phase4_completion` | `6f7c529` → `37a0efe` |
| Phase 5 | Full MCP server surface (5 tools, resources, prompts) | Complete | `cargo nextest run -p oz-policy-mcp` green; STDIO + Streamable HTTP transports; bearer auth; `/healthz` | `0a425db` → `e243429` |
| Phase 6 | Agent skill (`SKILL.md` + flat-file twin) | Complete | All three eval YAMLs pass; CI lints SKILL.md frontmatter + eval schemas | `44a0191` → `86093ef` |
| Phase 7 | Wallet integration (SEP-43 adapter; Freighter + passkey-kit) | Complete | `INTEGRATION=1 pnpm test` green; Phase 7 Round 2 OZ-SA `AuthPayload` encoder shipped and on-chain install verified end-to-end on testnet (tx `038583fa…ce90bb`, ledger 2617998, `context_rule_id=4`, `verifyInstall.matches=true`) — see [`walkthroughs/phase7-testnet-install/install-result.json`](walkthroughs/phase7-testnet-install/install-result.json). RFP deliverable #5 closed 2026-05-18. | `f8d2aa1` → `202d28f` |
| Phase 8 | Three end-to-end walkthroughs (Blend, SEP-41, Soroswap) | Complete | `.github/workflows/walkthroughs.yml` re-derives every frozen corpus byte-equally on every PR | `77785d8` → `e3306a0` |
| Phase 9 | Security hardening, fuzzing, audit handoff, reproducible builds | Complete with **human-gated outstanding** | Audit lints (5 rules) green on every committed template; fuzz harness runs nightly; `scripts/reproducible-build.sh` exits 0 from a fresh clone. **External auditor not engaged** — see [`audits/READY.md`](audits/READY.md). | `6a94e11` → `a271b57` |
| Phase 10 | Docs, release engineering, mainnet readiness, hosted MCP | Complete with **human-gated outstanding** | Cookbook + CONTRIBUTING + CHANGELOG + release workflow + Fly.io blueprint shipped. **Hosted endpoint not deployed; v1.0.0 not tagged; mainnet canary not run.** Plan.md Phase 10 completion criterion is split into automatable vs. human-required; the human side is the [`docs/mainnet-readiness.md`](docs/mainnet-readiness.md) runbook. | `175735e` → `b3ae04e` |

## Test inventory

- **Rust workspace:** 285 passing, 10 ignored (per-phase ignored gates that
  exercise external resources — testnet RPC, full compile/wasm-opt of
  generated policies, etc.). Run: `cargo nextest run --workspace`.
- **`wallet-adapter`:** 78 passing + 2 skipped — the
  `phase7_integration.test.ts` suite that requires `INTEGRATION=1` +
  testnet, and one passkey-kit skip whose remediation is upstream. Run:
  `cd wallet-adapter && pnpm test`.
- **Combined:** 363 passing, 10 Rust ignored gates + 2 wallet-adapter skips.

## Walkthrough corpus

The regression corpus that every Phase 1-9 gate is measured against:

- [`walkthroughs/01-blend-yield/`](walkthroughs/01-blend-yield/) — Blend
  protocol claim flow, frozen testnet recording + Track-A spec + sim
  report + install envelope.
- [`walkthroughs/02-sep41-subscription/`](walkthroughs/02-sep41-subscription/)
  — SEP-41 subscription cap, frozen on testnet, used as the Phase 2
  byte-equality target.
- [`walkthroughs/03-soroswap-bounded/`](walkthroughs/03-soroswap-bounded/)
  — Soroswap bounded swap, frozen on testnet.
- [`walkthroughs/phase3-codegen-fixture/`](walkthroughs/phase3-codegen-fixture/)
  — Track-B golden codegen fixture; the `cb2a8736…` policy WASM hash is
  the binary completion gate for Phase 3 and the WASM the Phase 10
  mainnet canary will deploy.
- [`walkthroughs/phase7-testnet-install/`](walkthroughs/phase7-testnet-install/)
  — Real testnet deploy of the function-allowlist policy + an OZ smart
  account. Includes [`install-result.json`](walkthroughs/phase7-testnet-install/install-result.json)
  (frozen SUCCESS evidence, 2026-05-18) and [`BLOCKER.md`](walkthroughs/phase7-testnet-install/BLOCKER.md)
  (historical diagnostic of the OZ `AuthPayload` encoding work, resolved
  by `wallet-adapter/src/oz_smart_account_auth.ts`).

`.github/workflows/walkthroughs.yml` re-derives every corpus file from
inputs on every PR; any drift fails CI loudly.

## Known outstanding

Concrete file-referenced gaps the user (the human operator) must close
before `v1.0.0`. Each item below is honest about what is in the
repository versus what must happen off-repository.

1. **External audit not engaged.** Every prerequisite box in
   [`audits/READY.md`](audits/READY.md) is currently honest about its
   state (some are ticked, some are not — notably "OZ engagement plan in
   place" is unchecked). No auditor has read the handoff package at
   [`audits/handoff-package/`](audits/handoff-package/).
2. **Hosted MCP endpoint not deployed.** The blueprint at
   [`infra/fly/`](infra/fly/) is opt-in IaC; running
   `./infra/fly/deploy.sh` requires the human prerequisites listed in
   [`infra/README.md`](infra/README.md) (cloud account, DNS, TLS,
   bearer-token secret).
3. **`v1.0.0` not tagged.** The release workflow at
   [`.github/workflows/release.yml`](.github/workflows/release.yml) is
   wired and gated on the `CARGO_REGISTRY_TOKEN` / `NPM_TOKEN` /
   `RELEASE_GPG_KEY` secrets; pushing the tag is a human step
   (see [`docs/mainnet-readiness.md`](docs/mainnet-readiness.md) §3).
4. **`SECURITY.md` still has placeholders.** The disclosure email and
   the GPG fingerprint are explicitly tagged `<placeholder>`. The
   runbook references them; the human operator replaces them when
   minting the v1.0.0 release key.
5. **Mainnet canary has not been run.** The `walkthroughs/mainnet-canary/`
   directory does not exist in the repository; it is the human operator's
   job to create it under the procedure in
   [`docs/mainnet-readiness.md`](docs/mainnet-readiness.md) §2. The
   "Completed canaries" table in that file starts empty by design.

RFP deliverable scorecard: **8 of 10 delivered + 1 deferred (mainnet
canary) + 1 human-required (external audit engagement).** Deliverable #5
(testnet install + verify) closed 2026-05-18 with the frozen
[`install-result.json`](walkthroughs/phase7-testnet-install/install-result.json).

## Acknowledgments

The OZ Accounts Policy Builder builds on the following upstream work,
each used under its respective license; see [`NOTICE`](NOTICE) for the
authoritative attribution.

- **OpenZeppelin** — `stellar-accounts` v0.7.1 (MIT). The SA contract
  shape (`add_context_rule`, `AuthPayload`, `Signer`, `ContextRule`) is
  consumed via vendor source verified in
  [`docs/oz-internal-shapes.md`](docs/oz-internal-shapes.md). The
  generated policies target this surface.
- **kalepail / pollywallet** (MIT) — the structural reference for the
  RFP's "build on existing work" pillar; specific files and dispositions
  (Adopt / Extend / Replace) are enumerated in commit messages per
  `plan.md` §Cross-Phase Invariants #5.
- **passkey-kit** by Tyler van der Hoeven (MIT, © 2024). Used at
  runtime by [`wallet-adapter/src/adapters/passkey.ts`](wallet-adapter/src/adapters/passkey.ts).

All third-party license texts are preserved verbatim in the source tree
under each integration point; the workspace-level
[`LICENSE-APACHE`](LICENSE-APACHE) covers OZ Policy Builder's own
sources. The [`deny.toml`](deny.toml) allow-list is non-copyleft and
permissive; `cargo deny check` is part of every CI run.
