<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 OZ Policy Builder contributors
-->

# Changelog

Format: [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).
Versioning: [SemVer 2.0](https://semver.org/spec/v2.0.0.html). Every
release tag MUST add a section above `[Unreleased]` on the day it is
cut.

The walkthrough corpus is the regression suite: any rotation of a frozen
`wasm_hash.txt`, `expected-spec-*.json`, `expected-sim-report.json`, or
install-envelope XDR MUST appear here with a causal explanation
(see `CONTRIBUTING.md` §"Walkthrough corpus — APPEND-ONLY").

## [Unreleased]

### Added — Phase 1: Foundations

- Workspace skeleton (`Cargo.toml`, seven member crates, pinned
  `rust-toolchain.toml = 1.89.0`).
- TBD resolution log (Soroban / Stellar dependency pins verified against
  crates.io 2026-05).
- `oz-policy-recorder`: ingest a Stellar transaction (by hash or simulation)
  and emit a deterministic Recording JSON document.

### Added — Phase 2: Policy IR & Track A synthesizer

- `oz-policy-core::PolicySpec` IR + `ArgValue` enum + decision-tree model.
- Track A synthesizer (compose existing primitives, no codegen path).
- `oz-policy-installer`: build install-envelope XDR for
  `SmartAccount::add_context_rule` / `add_policy` (never auto-submits).

### Added — Phase 3: Track B codegen

- `oz-policy-codegen`: askama templates + sandboxed `cargo build` driver
  producing reproducible Soroban policy WASM artifacts.
- Audit lints over rendered sources (five rules, line-numbered violations).

### Added — Phase 4: Simulation harness + deny-vector generator

- `oz-policy-simhost`: `soroban-env-host` in-process simulation harness.
- `proptest`-driven boundary-mutation deny-vector generator with a
  determinism gate (`generate_deny_vectors_is_byte_equal_for_same_seed`).

### Added — Phase 5: MCP server surface

- `oz-policy-mcp`: five MCP tools (`record_transaction`, `synthesize_policy`,
  `simulate_policy`, `export_policy`, `verify_install`) over STDIO and
  Streamable HTTP transports.
- Static bearer-token auth on the HTTP path (`OZ_POLICY_MCP_TOKEN`).

### Added — Phase 6: Agent skill

- `SKILL.md` + flat-file twin with clarification logic for the recorder /
  propose / simulate / export flow.

### Added — Phase 7: Wallet integration

- `wallet-adapter/`: TypeScript SEP-43 adapter package
  (`@oz-policy-builder/wallet-adapter`) with Freighter primary and
  passkey-kit secondary, 78 tests passing + 2 skipped (the testnet
  `INTEGRATION=1` suite + one upstream-gated passkey skip).

### Added — Phase 8: End-to-end walkthroughs

- Three frozen walkthroughs under `walkthroughs/`: Blend yield, SEP-41
  subscription, Soroswap bounded swap. Each ships recording + spec +
  per-slot `wasm_hash.txt` + sim report.

### Added — Phase 9: Security hardening + reproducible builds

- `cargo-fuzz` harnesses for recorder decode, codegen synth, simhost.
- `ci/Dockerfile` + `scripts/reproducible-build.sh` + manifest workflow.
- `cargo deny` advisories/bans/sources/licenses gate.

### Added — Phase 10: Docs, release, hosted MCP, mainnet readiness

- Cookbook under `docs/` (install, concepts, walkthroughs, MCP clients,
  wallets, operations, security, upstream notes).
- Top-level `README.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
  `SECURITY.md`, this `CHANGELOG.md`.
- `.github/workflows/release.yml` — tag-driven binary + crates.io + npm
  publish pipeline (all third-party secrets gated).
- `infra/fly/` blueprint for a hosted MCP endpoint (human deploy required).

## [0.0.0] - 2026-05-16

Initial pre-release development snapshot. Workspace version is `0.0.0`
across all member crates and the wallet-adapter package. No published
artifacts; nothing in this snapshot is API-stable.

---

<!-- Release-tag link template. The placeholder org MUST be replaced before
     the first published release. -->
[Unreleased]: https://github.com/oz-policy-builder/oz-policy-builder/compare/v0.0.0...HEAD
[0.0.0]: https://github.com/oz-policy-builder/oz-policy-builder/releases/tag/v0.0.0
