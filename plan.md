# OZ Accounts Policy Builder — Implementation Plan

> **For agentic workers:** Each phase below is a self-contained unit of work. When dispatched, locate the requested phase, verify every dependency listed in its `Depends on:` field is `Status: Complete`, read the *Tech Stack & Versions* section and use only the pinned versions listed there, follow its *Agent Orchestration* strategy to spawn sub-agents in parallel where indicated, execute *Search / Research* and *Implementation* through those agents, run *Verification / Test / Validation* and confirm the *Completion Criterion* (a binary check) before flipping `Status:` to `Complete`. Do not skim research files — this plan contains everything needed.

**Goal:** Ship an Apache-2.0 toolkit that records a Stellar transaction (by hash or simulation) and synthesizes the minimum OpenZeppelin smart-account context rule + policies that would permit exactly that flow — delivered as a Rust synthesizer library, an MCP server (STDIO + Streamable HTTP), and an Anthropic Agent Skills `SKILL.md` portable across MCP clients, with a SEP-43 wallet adapter (Freighter primary, passkey-kit secondary) and a `soroban-env-host`-based simulation harness with proptest-driven deny-vector generation.

**Architecture:** Rust workspace produces (a) the synthesizer engine, (b) an `rmcp` MCP server wrapping it, (c) an `askama` codegen pipeline emitting Soroban policy contract source for net-new policies, (d) a `soroban-env-host` simulation harness, plus a TypeScript wallet-adapter package targeting SEP-43. The synthesizer is deterministic; LLMs are restricted to clarification/summarization surfaces inside the agent skill, never to codegen. Composition over `simple_threshold` / `weighted_threshold` / `spending_limit` is preferred where the constraint shape matches; codegen via audited template families covers the rest. Deployment is always a separate, wallet-signed step.

**Source of truth:** `researches/analysis.md` (strategic decomposition of the RFP) and `researches/technical-research.md` (technical architecture, OZ deep-dive, pollywallet teardown, walkthrough specs, and §17 phased plan). This plan is derived from both and should be the only artifact the executor reads at run time.

---

## Tech Stack & Versions

All phases reference this section. Do not introduce versions not listed here without updating this section first. Versions marked **TBD** must be resolved (by reading the indicated source) inside Phase 1 before downstream phases run; until then they must not be pinned in any `Cargo.toml`, `package.json`, or `rust-toolchain.toml`.

### Languages & Toolchains
| Component | Version | Source / Reason |
|---|---|---|
| Rust toolchain | `1.89.0` stable, channel = `stable` (pinned via `rust-toolchain.toml`) | Local installed stable in our dev environment; satisfies `soroban-sdk 25.3.0` MSRV (1.84.0), `stellar-xdr 25.0.0` MSRV (1.84.0), `stellar-rpc-client 25.1.0` MSRV (1.85.0), and `rmcp 1.7.0` edition-2024 baseline (1.85+). Latest stable as of writing is 1.93+ but 1.89.0 is the frozen pin for this project. **Cargo-nextest 0.9.128** is the latest nextest compatible with rustc 1.89.0 (nextest 0.9.129 bumped MSRV to rustc 1.91, per https://nexte.st/changelog/ entry dated 2026-02-22: *"The MSRV for building nextest has been updated to Rust 1.91."*). |
| Cargo profile | `release` with `overflow-checks = true` | Blend security guidance verbatim per research §12; required for generated policies to safely manipulate `i128` amounts |
| Node.js | `22.11.0` LTS ("Jod") | Latest LTS line in May 2026; required only by the wallet-adapter TypeScript package |
| pnpm | `9.x` (use whatever `corepack enable` selects) | Matches pollywallet's package manager choice; isolates store, deterministic locks |
| TypeScript | `5.6.x` | Latest stable minor at time of pinning; needed for SEP-43 type definitions and `@stellar/stellar-sdk` 12.x compatibility |

### Stellar / Soroban Ecosystem
| Crate / Tool | Version | Source / Reason |
|---|---|---|
| `stellar-accounts` | `=0.7.1` | Phase 1 confirmed: crates.io publish `0.7.1` (license: MIT) matches GitHub tag `v0.7.1` (released 2026-04-10). The earlier research-time discrepancy with `v0.7.0-rc.1` no longer applies — both v0.7.0 and v0.7.1 stable tags are now published. Workspace `Cargo.toml` at this tag pins `soroban-sdk = "25.3.0"`. **License: MIT** (NOT Apache-2.0 as previously assumed — see `docs/oz-internal-shapes.md` §Discrepancies). |
| `soroban-sdk` | `=25.3.0` | Direct dependency of `stellar-accounts 0.7.1` (verified in `/tmp/stellar-contracts-clone/Cargo.toml` workspace block). Apache-2.0. MSRV 1.84.0. |
| `soroban-env-host` | `=25.0.1` | Pinned in `soroban-sdk 25.3.0`'s workspace `Cargo.toml` (`https://github.com/stellar/rs-soroban-sdk/blob/v25.3.0/Cargo.toml`); the 25.x line tops out at 25.2.1 stable but the sdk pins exactly 25.0.1. Apache-2.0. Required for the in-process simulation harness in Phase 4. |
| `stellar-rpc-client` (Rust) | `=25.1.0` | Latest stable on crates.io that builds on Rust 1.89.0. (`26.0.0` requires Rust 1.93.0 — incompatible with our toolchain pin; `26.0.0-rc.x` are pre-releases.) Supports Protocol 23 event structure. Apache-2.0. Source: `https://crates.io/crates/stellar-rpc-client/25.1.0`. |
| `stellar-xdr` (Rust) | `=25.0.0` | Pinned by `soroban-sdk 25.3.0`'s workspace; aligning with the SDK avoids type-mismatch errors at the recorder<->codegen boundary. Apache-2.0. Protocol 23 ships post-Sept-2025; 25.0.0 carries the post-Protocol-23 XDR set. Local `stellar` CLI 25.1.0 reports `stellar-xdr 25.0.0`, confirming consistency. Source: `https://crates.io/crates/stellar-xdr/25.0.0`. NOTE: latest crates.io publish is `26.0.1` but adopting it would diverge from `soroban-sdk 25.3.0`'s pin. |
| `stellar-cli` | `v25.1.0` (CI matrix pin) | Local install version; the same tag will be used in CI. Embeds `wasm-opt = 0.116.1` Rust crate (Binaryen v116) via the `additional-libs` feature. Verified in `cmd/soroban-cli/Cargo.toml` of `stellar-cli` at tag `v25.1.0`. Source: `https://github.com/stellar/stellar-cli/releases/tag/v25.1.0`. NOTE: latest release is `v26.0.0` (2026-04-13) but pinning to local-install version maintains parity between dev and CI. |
| `@stellar/stellar-sdk` (JS) | `12.x` latest at Phase 7 | Wallet adapter only; do not use from Rust paths. |
| `@stellar/freighter-api` | `6.0.1` | Latest on npm (`npm view @stellar/freighter-api version`), Apache-2.0 (matches our toolkit). SEP-43 implementation entry point for Freighter. Source: `https://www.npmjs.com/package/@stellar/freighter-api`. |
| `passkey-kit` | `0.12.0` (latest), **MIT-licensed** | Latest on npm (`npm view passkey-kit version`). License is **MIT**, NOT Apache-2.0 as the row originally implied. MIT is downstream-compatible with this toolkit's Apache-2.0 distribution (standard Rust dual-license pattern), but the discrepancy must be acknowledged in the LICENSE NOTICE for any released bundle that vendors passkey-kit. Source: `https://github.com/kalepail/passkey-kit` (verified `LICENSE` via GitHub API: SPDX MIT). |

### Codegen & MCP
| Crate | Version | Source / Reason |
|---|---|---|
| `askama` | `=0.16.0` | Latest stable on crates.io as of 2026-05 (the 0.13.x line is superseded — 0.13 -> 0.14 -> 0.15 -> 0.16, all published 2026). Compile-time-checked, byte-deterministic templating per research §6. Reasoning: `syn`/`quote` AST construction loses reviewer-readability; `askama`'s readable Rust output compensates with a sandbox compile gate. Apache-2.0 OR MIT. Source: `https://crates.io/crates/askama/0.16.0`. |
| `rmcp` (modelcontextprotocol/rust-sdk) | `=1.7.0` | Resolved in Phase 1: supports MCP spec revisions `2025-06-18` AND `2025-11-25` (PR #802 merged 2026-04-10, first published in `rmcp-v1.5.0`). STDIO and Streamable HTTP transports both supported. Apache-2.0. Edition 2024 (MSRV >= 1.85, satisfied by our 1.89.0 pin). The TypeScript-SDK-FFI fallback is NOT needed — see `docs/mcp-sdk-decision.md`. Source: `https://crates.io/crates/rmcp/1.7.0`. |
| `serde` / `serde_json` | latest stable 1.x | Standard. |
| `schemars` | `=1.0` (current latest is `1.2.1`; pin major to match rmcp's dep) | Emit JSON Schema for the policy IR and for MCP tool input/output schemas. `rmcp 1.7.0` declares `schemars = "1.0"` so we follow the same major to avoid version-resolution conflicts. The plan's earlier `0.8.x` is stale (schemars hit 1.0 stable in 2026). MIT OR Apache-2.0. |

### Testing & Security
| Tool | Version | Source / Reason |
|---|---|---|
| `proptest` | `=1.11.0` | Latest stable on crates.io (`cargo search proptest`). MIT OR Apache-2.0. Property-based deny-vector generation per research §9. Chosen over `quickcheck` because `stellar-contracts` test utilities integrate `proptest` and its strategy DSL fits typed `ScVal` generation. |
| `cargo-nextest` | `=0.9.128` | Pinned: latest nextest compatible with rustc 1.89.0. Nextest **0.9.129** (released 2026-02-22) bumped MSRV to rustc >= 1.91, which exceeds our toolchain pin — every 0.9.129+ release inherits this MSRV. Source: https://nexte.st/changelog/ (entry `cargo-nextest 0.9.129`, 2026-02-22, "Changed" section: *"The MSRV for building nextest has been updated to Rust 1.91."*). Faster test runs, deterministic isolation; used in CI. Apache-2.0 OR MIT. **CI must install via `cargo install cargo-nextest --version =0.9.128 --locked` — never `cargo install cargo-nextest` without `--version` (would silently land outside the pin window and break the build on rustc 1.89.0).** |
| `cargo-fuzz` | `=0.13.1` | Latest stable on crates.io. MIT OR Apache-2.0. libFuzzer harness over `enforce(ctx, signers, rule, smart_account)` per research §12. |
| `cargo-deny` | `=0.19.6` | Latest stable on crates.io (`cargo search cargo-deny`). MIT OR Apache-2.0. Supply-chain / advisory checks gated in CI. |
| `cargo-audit` | `=0.22.1` | Latest stable on crates.io (`cargo search cargo-audit`). Apache-2.0 OR MIT. Continuous CVE checks against Cargo.lock. |
| `clippy` | bundled with pinned Rust toolchain; CI runs `cargo clippy -- -D warnings` | Required gate. |
| `bubblewrap` (Linux) / `sandbox-exec` (macOS) | OS-provided | Sandbox the build worker; no network except cached crates mirror per research §7. |

### MCP Surface
- MCP spec revision: **2025-11-25** (research §7). Transports: **STDIO** (subprocess for IDEs) and **Streamable HTTP** (long-running service for remote/CI). The deprecated 2024-11-05 HTTP+SSE transport is not implemented.

### License
- **Apache-2.0** for this toolkit's own code, packages, and released bundles — per the RFP Section 3 requirement.
- **Upstream license notes (Phase 1 verified, 2026-05-15):**
  - OpenZeppelin `stellar-contracts` (incl. `stellar-accounts 0.7.1`) is **MIT** (not Apache-2.0 as research §13 assumed). MIT is permissively downstream-compatible with Apache-2.0 distribution.
  - `kalepail/passkey-kit` (npm `passkey-kit`) is **MIT**.
  - `@stellar/freighter-api` (npm) is Apache-2.0 (matches us).
  - `rmcp` (Rust MCP SDK) is Apache-2.0.
  - `soroban-sdk`, `soroban-env-host`, `stellar-xdr`, `stellar-rpc-client`, `stellar-cli` are all Apache-2.0.
- **NOTICE file requirement:** because we vendor or re-distribute MIT-licensed upstreams (`stellar-accounts`, `passkey-kit`), the released bundle's `NOTICE` must reproduce their MIT copyright lines as required by the Apache-2.0 dual-license interaction. Phase 9 (audits) and Phase 10 (release) must check this.

### Workspace Layout (canonical paths — referenced by every phase)
```
repo-root/
├── Cargo.toml                             # virtual workspace
├── rust-toolchain.toml                    # pins stable, MSRV, components
├── deny.toml                              # cargo-deny config
├── crates/
│   ├── oz-policy-core/                    # PolicySpec IR, decision tree, schema, errors
│   ├── oz-policy-recorder/                # tx-analyzer (RPC + XDR → Recording)
│   ├── oz-policy-codegen/                 # askama templates → Rust + cargo+wasm-opt driver
│   ├── oz-policy-simhost/                 # soroban-env-host harness + deny generator
│   ├── oz-policy-installer/               # builds install envelope XDR (no submit)
│   ├── oz-policy-mcp/                     # rmcp server binary
│   └── oz-policy-cli/                     # thin CLI mirror of MCP tools (debugging)
├── templates/                             # askama .rs.jinja files (Phase 3)
├── skills/                                # Anthropic Agent Skills (Phase 6)
│   └── oz-policy-builder/
│       ├── SKILL.md
│       ├── references/
│       ├── scripts/
│       └── evals/
├── wallet-adapter/                        # TypeScript pnpm package (Phase 7)
│   ├── package.json
│   └── src/
├── walkthroughs/                          # End-to-end recordings + expected specs (Phase 8)
│   ├── 01-blend-yield/
│   ├── 02-sep41-subscription/
│   └── 03-soroswap-bounded/
├── audits/                                # Synthesizer audit artifacts (Phase 9)
├── docs/                                  # Cookbook + reference (Phase 10)
└── .github/workflows/                     # CI (matrix across MCP clients)
```

### Naming Conventions (used throughout the plan)
- Policy IR root type: `PolicySpec` (versioned). Schema URI: `oz-policy-builder/v1`.
- Recording IR root type: `Recording`. Schema URI: `oz-policy-builder/recording/v1`.
- Tool error code prefix: `E_`. Full list: `E_RECORDER_HASH_NOT_FOUND`, `E_RECORDER_SIM_FAILED`, `E_RECORDER_XDR_DECODE_FAILED`, `E_SYNTH_NOT_EXPRESSIBLE`, `E_CODEGEN_COMPILE_FAILED`, `E_SIM_PERMIT_DENIED`, `E_SIM_DENY_PASSED`, `E_VERIFY_DRIFT`, `E_WALLET_REJECTED`, `E_INSTALL_PREFLIGHT_FAILED`. (`E_RECORDER_XDR_DECODE_FAILED` added in P1-T3 for the recorder's XDR decode path; distinct from `E_RECORDER_SIM_FAILED` which signals the RPC call itself reported failure.)
- Storage key convention in generated contracts (from research §5.2.1): `max_{arg_name}` / `min_{arg_name}` for ranges, `threshold`, `allowed_{arg_name}` for allowlists.
- All synthesizer outputs are pure functions of inputs; codegen byte-determinism enforced by pinned toolchain + `Cargo.lock` + pinned `wasm-opt`.

---

## Phase Index

| Phase | Title | Depends on | Status |
|---|---|---|---|
| 1 | Foundations: workspace, TBD resolution, recorder | — | Pending |
| 2 | Policy IR & Track A synthesizer (compose existing primitives) | 1 | Pending |
| 3 | Track B codegen: askama templates + sandbox build | 1, 2 | Pending |
| 4 | Simulation harness + proptest deny-vector generator | 1, 2, 3 | Pending |
| 5 | Full MCP server surface (5 tools, resources, prompts, both transports) | 1, 2, 3, 4 | Pending |
| 6 | Agent skill (`SKILL.md` + flat-file twin) with clarification logic | 5 | Pending |
| 7 | Wallet integration (SEP-43 adapter, Freighter primary, passkey-kit secondary) | 5 | Pending |
| 8 | Three end-to-end walkthroughs (Blend, SEP-41 subscription, Soroswap) | 2, 3, 4, 5, 7 | Pending |
| 9 | Security hardening, fuzzing, external audit, reproducible builds | 1–8 | Pending |
| 10 | Docs, release, mainnet readiness, hosted MCP endpoint | 1–9 | Pending |

---

## Phase 1 — Foundations: workspace, TBD resolution, recorder

**Status:** Pending
**Depends on:** —
**Deliverable:** A Rust workspace at the canonical layout above, every TBD in *Tech Stack & Versions* resolved with the verified version recorded back into this plan, and a working `oz-policy-recorder` crate that ingests a Stellar transaction (by hash or by envelope-simulate) and emits a deterministic `Recording` JSON document.

### Search / Research
- **Resolve every TBD in *Tech Stack & Versions*.** For each row marked TBD: `cargo search <crate>` and read the crate's GitHub releases page; pin the latest stable version compatible with `soroban-sdk = 25.3.0`; write the resolved version back into the *Tech Stack & Versions* table in this plan (edit `plan.md` in place).
- **Reconcile `stellar-accounts` version reality.** Research §16 flags a discrepancy between crates.io README pin `=0.7.1` and GitHub release tag `v0.7.0-rc.1`. Run `cargo search stellar-accounts` and visit `https://crates.io/crates/stellar-accounts`; pin the actual newest stable publish and amend the *Tech Stack & Versions* table accordingly.
- **Resolve every §16 TBD from `researches/technical-research.md` by direct source inspection.** Clone `https://github.com/OpenZeppelin/stellar-contracts` at the pinned tag, read `packages/accounts/src/policies/` source directly, and capture:
  - **Type-name glossary:** the *associated type* on the `Policy` trait is `AccountParams`; the actual impl types are `SimpleThresholdAccountParams`, `WeightedThresholdAccountParams`, `SpendingLimitAccountParams`. `docs/oz-internal-shapes.md` is authoritative for both. Subsequent short-form references in this plan (e.g., `simple_threshold::AccountParams`) are shorthand for the corresponding long-form impl type.
  - Exact `AccountParams` struct field names for `simple_threshold`, `weighted_threshold`, `spending_limit`.
  - Exact `spending_limit` period unit (ledgers vs seconds — research §2 flags doc/source disagreement).
  - Whether `spending_limit::AccountParams` includes a `token: Address` field (escalation threshold per research §17 Recommendation 1: if absent, `spending_limit` is only safe under `CallContract(<token>)` rules and the synthesizer must enforce that).
  - Exact error enum variants per primitive (used for our `E_SYNTH_NOT_EXPRESSIBLE` mapping and for sim-harness deny-vector error matching).
  - Exact `SmartAccount::add_context_rule` / `add_policy` signatures (Phase 7 install-envelope builder depends on this).
  Write the verified shapes into `docs/oz-internal-shapes.md` (this file is the authority for downstream phases — Phases 2 and 3 read it; do not duplicate elsewhere).
- **Confirm `rmcp` MCP spec revision support.** Open `https://github.com/modelcontextprotocol/rust-sdk` releases and the `CHANGELOG.md`. Verify the crate version supports MCP spec **2025-11-25** transports (STDIO + Streamable HTTP). If it lags, choose between (a) waiting and pinning the latest revision that ships in the engagement window, or (b) the TypeScript-SDK + FFI fallback (research §16). Record the decision in `docs/mcp-sdk-decision.md`.
- **Verify Soroban RPC `getTransaction` retention and Hubble fallback.** Research §6/§5 states default retention is 24h with hard recommendation ≤ 7d. Confirm current Stellar testnet and mainnet retention values by reading the public RPC docs `https://developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getTransaction`. If retention is shorter than expected for our walkthrough corpus (Phase 8), provision either an extended-retention private RPC endpoint or Hubble BigQuery access — pick one and document in `docs/rpc-retention-decision.md`.
- **Determine the wasm-opt version shipped by the pinned `stellar-cli`.** Run `stellar contract optimize --help` after installing the pinned `stellar-cli` and record the embedded wasm-opt version. This becomes part of the reproducible-build manifest (Phase 9).

### Implementation
- Initialize a virtual workspace `Cargo.toml` at repo root with `resolver = "2"` and the crate list under `[workspace.members]` matching *Workspace Layout*.
- Write `rust-toolchain.toml` pinning `channel = "stable"`, `components = ["rustfmt", "clippy"]`, `targets = ["wasm32-unknown-unknown"]`. Record the exact `stable` minor version in a sibling comment.
- `crates/oz-policy-core`: create skeleton with `lib.rs` declaring `pub mod spec; pub mod errors;` and an empty `PolicySpec` placeholder (filled in Phase 2). Add `#[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]` to anything public from day one. Define the `Error` enum populated with the full set of `E_*` codes from *Naming Conventions*.
- `crates/oz-policy-recorder`: implement the recorder per research §5. Public surface:
  - `pub async fn record_by_hash(rpc_url: &str, network_passphrase: &str, hash: &str) -> Result<Recording, Error>` — calls `stellar-rpc-client::getTransaction(hash)`, decodes `envelopeXdr` + `resultMetaXdr` via `stellar-xdr`.
  - `pub async fn record_by_simulation(rpc_url: &str, envelope_xdr: &str, instruction_leeway: Option<u64>) -> Result<Recording, Error>` — calls `simulateTransaction(envelope)`, extracts `results[0].auth`, `events`, `transactionData`.
  - `Recording` type (in `recorder::types`): contains `contracts: Vec<ContractRecord>` (each with `address: stellar_strkey::Contract`, `function: Symbol`, `args: Vec<ArgValue>`), `auth_tree: AuthTree`, `state_changes: Vec<StateDelta>`, `events: Vec<TypedEvent>`. Versioned with schema URI `oz-policy-builder/recording/v1`.
  - `ArgValue` enum variants per research §5: `Address`, `I128`, `U32`, `U64`, `Bytes`, `Symbol`, `Vec`, `Map`. Each variant carries the decoded value; nested structures fully decoded, not opaque XDR.
- `crates/oz-policy-cli`: provide `record` subcommand that wraps the recorder and prints the `Recording` JSON to stdout. This is the only Phase-1 user-facing surface.
- Set up CI workflow `.github/workflows/ci.yml` running `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo nextest run`, `cargo deny check` on every PR. Pin `actions/checkout` and `dtolnay/rust-toolchain@stable` to specific commits.
- Add `deny.toml` with `[advisories] vulnerability = "deny"`, `[licenses] allow = ["Apache-2.0", "MIT", "BSD-3-Clause", "BSL-1.0", "ISC", "Unicode-3.0", "CC0-1.0"]`, and `[bans] multiple-versions = "warn"`.
- Author `docs/oz-internal-shapes.md`, `docs/mcp-sdk-decision.md`, `docs/rpc-retention-decision.md` with the resolved findings from *Search / Research*.

### Agent Orchestration
Spawn the following streams in **parallel** — they share no inputs and write to disjoint files:

- **Stream A — TBD resolution.** Runs `cargo search` lookups, reads GitHub releases, edits *Tech Stack & Versions* table in `plan.md`, authors `docs/oz-internal-shapes.md` from direct OZ source inspection, `docs/mcp-sdk-decision.md`, `docs/rpc-retention-decision.md`.
- **Stream B — Workspace bootstrap.** Creates root `Cargo.toml`, `rust-toolchain.toml`, `deny.toml`, all crate skeletons with `lib.rs` placeholders, the CI workflow file, and the Phase-1 unit-test scaffolding (`#[cfg(test)]` modules emitting placeholder tests that pass).
- **Stream C — Recorder implementation.** Implements `oz-policy-recorder` fully, writes `oz-policy-cli`'s `record` subcommand, writes recorder integration tests against a pinned testnet transaction hash.

After all three streams return, the lead session runs `cargo nextest run --workspace` and the recorder integration test described under *Verification*, then merges the streams' outputs and amends `plan.md`'s *Tech Stack & Versions* row values.

**Sequential after parallel:** none — Phase 1 completion is a single merge step.

### Verification / Test / Validation
- `cargo fmt --check` and `cargo clippy --workspace -- -D warnings` exit 0.
- `cargo nextest run --workspace` is green.
- Run `cargo run -p oz-policy-cli -- record --hash <PINNED_TESTNET_HASH> --rpc https://soroban-testnet.stellar.org --network "Test SDF Network ; September 2015"` (the pinned hash must be a Phase-1-frozen Blend testnet `claim` tx; store it in `walkthroughs/01-blend-yield/source.json` for reuse in Phase 8). Compare stdout against `walkthroughs/01-blend-yield/expected-recording.json` using `diff`; the run is correct iff `diff` returns no output.
- Confirm every TBD row in *Tech Stack & Versions* has been replaced with a verified version string (no row contains the literal token "TBD" except for items intentionally deferred to a later phase with a written justification).
- `docs/oz-internal-shapes.md` exists and contains verbatim `AccountParams` struct definitions for the three primitives.

**Completion Criterion (binary):** Running `cargo nextest run --workspace -- --include-ignored recorder::integration::blend_claim_roundtrip` produces a green test that reads `walkthroughs/01-blend-yield/source.json`, calls the recorder, and asserts the produced `Recording` is byte-equal to `walkthroughs/01-blend-yield/expected-recording.json`. **AND** `grep -c "TBD: verify" plan.md` returns `0` for the *Tech Stack & Versions* section.

---

## Phase 2 — Policy IR & Track A synthesizer (compose existing primitives)

**Status:** Pending
**Depends on:** Phase 1
**Deliverable:** A versioned `PolicySpec` schema (`oz-policy-builder/v1`) with full JSON Schema export, a deterministic decision tree that compiles a `Recording` into a `PolicySpec`, a Track-A synthesizer that parameterizes the three OZ primitives (`simple_threshold`, `weighted_threshold`, `spending_limit`) when the constraint shape matches, and an install-envelope builder that emits a wallet-signable Soroban transaction XDR calling `SmartAccount::add_context_rule` / `add_policy` — never auto-submitting.

### Search / Research
- Read `docs/oz-internal-shapes.md` (produced in Phase 1) for the exact `AccountParams` field names and types of all three primitives, plus the exact `SmartAccount::add_context_rule(context_type, name, valid_until, signers, policies: Map<Address, Val>)` signature.
- Read research §2 (OZ deep), §3 (caveats), §6 Track A (composition rules), §5.1 of `analysis.md` for hard limits: `MAX_POLICIES = 5`, `MAX_SIGNERS = 15`, `MAX_NAME_SIZE = 20`, `MAX_EXTERNAL_KEY_SIZE = 256`, 15 context rules per account.
- Validate the audit-class issues to be enforced at synthesis time per research §3 and §12:
  - `ContextRuleType::CallContract(Address)` matches only target address — function-allowlist is a *policy* responsibility, never a rule-level filter.
  - Signer-set divergence: any spec mutating signers must emit a joint threshold-update instruction.
  - Sponsor `context_rule_ids` substitution: synthesizer must refuse to install onto smart accounts older than PR-#655. Per `docs/oz-internal-shapes.md` §8, no on-chain marker exists for distinguishing pre/post-PR-#655 smart accounts. Implement the install preflight using strategy 3 (user-asserted `--account-revision` flag) per the §8 recommendation; queue strategy 1 (WASM-hash whitelist) for v1.1 once a curated post-#655 WASM-hash corpus is built.
  - `spending_limit` under `Default` is rejected by `install` post-#649 — synthesizer must default to `CallContract(<target>)` whenever `spending_limit` is part of the spec.
- Verify pollywallet's schema field-by-field by reading `kalepail/pollywallet/src/lib/policy-schema.ts` (line counts in research §5.2 confirm 452 LOC). Identify which constructs to keep verbatim, which to rename, and which to extend. The result is the `oz-policy-builder/v1` schema; do not rename for cosmetic reasons.

### Implementation
- `crates/oz-policy-core/src/spec.rs` — define `PolicySpec` exhaustively:
  - Top-level fields: `schema: String` (constant `"oz-policy-builder/v1"`), `synthesis_mode: SynthesisMode` (`Auto | ComposeOnly | CodegenOnly`), `context_rule: ContextRuleSpec`, `signers: Vec<SignerSpec>`, `policies: Vec<PolicySlot>` (max 5), `lifetime_ledgers: Option<u32>`, `recording_ref: RecordingRef` (links back to the source `Recording`).
  - `ContextRuleSpec`: `name: String` (≤ 20 chars), `context_type: ContextType` (`Default` or `CallContract(Address)`), `valid_until: Option<u32>`.
  - `SignerSpec`: discriminated union of `ExternalEd25519`, `ExternalWebAuthn`, `Delegated(Address)`.
  - `PolicySlot`: either `Existing { primitive: ExistingPrimitive, params: ExistingPrimitiveParams }` (Track A) or `Generated { template_family: TemplateFamily, constraints: Vec<Constraint> }` (Track B, filled in Phase 3).
  - `ExistingPrimitive`: `SimpleThreshold | WeightedThreshold | SpendingLimit`.
  - `ExistingPrimitiveParams`: matching struct per primitive, fields named per `docs/oz-internal-shapes.md`.
  - `Constraint`: enum of `FunctionAllowlist(Vec<Symbol>)`, `ArgumentPattern { fn_name, arg_index, matcher }`, `AmountRange { min: Option<i128>, max: Option<i128> }`, `AssetAllowlist(Vec<Address>)`, `TimeWindow { start_ledger, end_ledger }`, `CallFrequency { max_calls, window_ledgers }`, `SequenceOrdering(Vec<Phase>)`.
  - Derive `Serialize`, `Deserialize`, `JsonSchema` everywhere.
- `crates/oz-policy-core/src/decision_tree.rs` — implement `synthesize(recording: &Recording, opts: SynthesisOptions) -> Result<PolicySpec, Error>`:
  - Walk `recording.auth_tree`, identify distinct `Context::Contract` targets.
  - If exactly one target and the `fn_name` is `transfer` with `args.len() >= 3` and target appears to be a SEP-41 SAC: candidate for `spending_limit` composition (gated by `args[2]: i128 = observed amount`).
  - Compute observed signer count → propose `simple_threshold` with threshold = observed count.
  - For any constraint the recording requires that does not match a primitive (e.g., function whitelist with multiple fn names, slippage cap, recipient lock, time window), emit a `Generated` policy slot — Track B fills this in Phase 3.
  - `tightness ∈ { exact, small_margin, loose }` (per research §7 MCP signature) scales numeric constraints: `exact` → constraint = observed value; `small_margin` → 1.1× observed; `loose` → 2× observed.
  - Enforce hard limits at end of decision: total `policies.len() <= 5`, `signers.len() <= 15`, `context_rule.name.len() <= 20`. If any limit exceeded, return `Error::E_SYNTH_NOT_EXPRESSIBLE` with the violated field named.
  - When `spending_limit` is selected, force `context_type = CallContract(<target SAC>)` to honor PR-#649.
- `crates/oz-policy-installer/src/envelope.rs` — implement `build_install_envelope(spec: &PolicySpec, smart_account: Address, source_account: Address, network: Network, rpc_url: &str) -> Result<Bytes, Error>`:
  - Construct a Soroban transaction with `add_context_rule` then `add_policy` invocations per `PolicySpec`.
  - Run `simulateTransaction` to fetch resource fees and the auth tree; assemble the transaction via the canonical `assembleTransaction` pattern (research §5.6).
  - Run install-time preflight: refuse if the target `SmartAccount` predates OZ PR-#655 per `docs/oz-internal-shapes.md`. Return `Error::E_INSTALL_PREFLIGHT_FAILED` with the version mismatch detail.
  - Return the assembled `TransactionEnvelope` XDR ready for wallet `signTransaction`. **Do not submit.**
- Update `oz-policy-cli` to expose `synthesize <recording-file> --mode auto --tightness exact --lifetime <ledgers>` and `prepare-install <spec-file> --smart-account <addr> --source <addr> --rpc <url>`.

### Agent Orchestration
Two streams that can run in **parallel** once Phase 1 is complete; they share `oz-policy-core`'s `PolicySpec` placeholder, which must be authored first:

- **Sequential prerequisite (single agent):** Author the full `PolicySpec` type in `crates/oz-policy-core/src/spec.rs` with all sub-types and derives. Commit and move on. This is the literal input both following streams need.
- **Stream A — Decision tree.** Implements `decision_tree.rs`, the SEP-41 SAC detection helper (`crates/oz-policy-core/src/sep41.rs`), and unit tests covering every branch in the tree (one test per `ExistingPrimitive` selection, one negative test per hard-limit violation).
- **Stream B — Installer.** Implements `oz-policy-installer/src/envelope.rs`, the install-time preflight, integration tests that simulate (not submit) a `simple_threshold` install against a Phase-1-recorded smart account on testnet.

After both streams return, lead session reconciles type signatures (one stream may have inferred shapes the other refines), runs the full test suite, and updates `oz-policy-cli`.

### Verification / Test / Validation
- `cargo nextest run -p oz-policy-core -p oz-policy-installer` green, with at least one test per `ExistingPrimitive`, per `Constraint` variant, per `synthesis_mode`, and per hard-limit error path.
- `cargo run -p oz-policy-cli -- synthesize walkthroughs/02-sep41-subscription/recording.json --mode compose_only --tightness exact --lifetime 432000` emits a `PolicySpec` JSON whose only `PolicySlot` is `Existing { primitive: SpendingLimit, params: ... }` with `context_type = CallContract(<USDC_SAC>)`. Diff against `walkthroughs/02-sep41-subscription/expected-spec-track-a.json` is empty.
- `cargo run -p oz-policy-cli -- prepare-install <that spec> --smart-account <testnet smart account> --source <funded source> --rpc <testnet RPC>` returns a base64 XDR envelope; decoding the envelope via `stellar-xdr` shows exactly two host-function invocations (`add_context_rule`, `add_policy`) in correct order with the expected `AccountParams` payload.
- JSON Schema export: `cargo run -p oz-policy-core --features schema-export -- emit-schema > target/policy-spec.schema.json` produces a non-empty file that round-trips: every example in `crates/oz-policy-core/tests/examples/` parses against the schema and serializes back byte-equal.

**Completion Criterion (binary):** `cargo nextest run -p oz-policy-core -p oz-policy-installer -p oz-policy-cli` is green AND the SEP-41 subscription Track-A spec/envelope round-trip described above passes the byte-equal diff.

---

## Phase 3 — Track B codegen: askama templates + sandbox build

**Status:** Pending
**Depends on:** Phase 1, Phase 2
**Deliverable:** A template library of audit-bounded `askama` `.rs.jinja` templates covering the seven constraint primitives, a sandboxed `cargo build --target wasm32-unknown-unknown` driver that produces a reproducible-hash WASM artifact, and a Track-B codegen pipeline that turns a `PolicySpec` with `Generated` policy slots into compilable Rust source emitting a single Soroban policy contract per generated slot.

### Search / Research
- Read research §6 Track B and the pollywallet codegen footgun list in research §5.2.1 verbatim (`policy-codegen.ts` system prompt insights):
  - `symbol_short!()` only accepts 9 ASCII chars; longer names need `Symbol::new(env, "...")` plus equality comparison.
  - Default rules see an `execute()` wrapper (`args[0] = target Address`, `args[1] = inner fn Symbol`, `args[2] = inner args Vec`); `CallContract` rules see direct call args. Templates must handle both.
  - StrKey addresses are base32 — never hardcode hex.
  - Default-reject everywhere: unrecognized fn_name / contract address → panic.
  - `Context` doesn't implement `Debug` or `PartialEq` — never log or compare it.
  - `Address`, `Symbol`, `String`, `Vec`, `Bytes` don't implement `Copy` — `.clone()` is required.
  - Install params must accept both `Val::VOID` (no config → defaults) and `Map<Val, Val>` shapes with `unwrap_or` defaults.
  - Storage keys: `max_{arg_name}`, `min_{arg_name}`, `threshold`, `allowed_{arg_name}`.
- Read `docs/oz-internal-shapes.md` for the exact `Policy` trait signature (research §1 reproduces it but Phase 1 has the authoritative verbatim copy).
- Determine the exact `wasm-opt` arguments that `stellar contract optimize` uses (Phase 1 captured the wasm-opt version; here we need the args). Read the pinned `stellar-cli` source `cmd/contract/optimize.rs` or run `stellar contract optimize --help`.
- Verify that the `stellar-accounts` crate at `=0.7.1` is publishable as a `lib` dependency to a generated `cdylib` crate that imports `Policy`, or whether (as pollywallet's `e2e-policy-test/src/lib.rs` did — research §5.2) generated contracts must re-implement the trait pattern from scratch without depending on `stellar-accounts` as a library. Record the answer in `docs/codegen-dependency-mode.md`. Choose accordingly.

### Implementation
- `crates/oz-policy-codegen/templates/` (paths resolved by askama via `templates/` workspace-level dir referenced in `oz-policy-codegen/build.rs` or `#[template(path = "...")]` attributes — pick the simpler `templates = "../templates"` Cargo config):
  - `base.rs.jinja` — full skeleton: `#![no_std]` (or `cdylib` Cargo profile), `use soroban_sdk::*;`, `#[contracttype] InstallParams`, `#[contracttype] StorageKey` (always keyed by `(Address, u32)` for `(smart_account, context_rule_id)`), `#[contracterror] PolicyError`, `#[contractevent] PolicyInstalled` / `PolicyEnforced` / `PolicyUninstalled`, `#[contract] pub struct Policy;`, full `impl Policy { fn install ... fn enforce ... fn uninstall ... }` Soroban-style. Conditional Jinja blocks include each constraint primitive's enforce branch only when the spec uses it (omitted branches don't bloat WASM).
  - `constraints/function_allowlist.rs.jinja` — render-time iterates over `Vec<Symbol>`, produces a match arm against `fn_name` using either `symbol_short!` (≤9 chars) or `Symbol::new(env, "…")` (longer).
  - `constraints/argument_pattern.rs.jinja` — typed slot match: addresses (`.try_into::<Address>().unwrap_or_panic()`), `i128` ranges, `u32` exact, `u64` exact, `Bytes` exact length + `==`.
  - `constraints/amount_range.rs.jinja` — clamp `args[<index>]: i128` between `min` and `max`.
  - `constraints/asset_allowlist.rs.jinja` — same pattern for `Address` lists; encodes addresses as `Address::from_string(&String::from_str(env, "C..."))`.
  - `constraints/time_window.rs.jinja` — reads `env.ledger().sequence()` and compares against `start_ledger`/`end_ledger`.
  - `constraints/call_frequency.rs.jinja` — stateful: stores `Vec<u32>` of recent enforce ledgers under storage key `freq_{rule}`; on enforce, evicts entries older than `current - window_ledgers`, panics if remaining length ≥ `max_calls`, else pushes and persists. TTL extended each write.
  - `constraints/sequence_ordering.rs.jinja` — stateful: stores current `Phase` index under storage key `phase`; on enforce, checks `fn_name` matches `phases[current]`, advances or wraps.
  - All stateful templates emit `smart_account.require_auth()` as the *first* line of `enforce` (security-critical per research §12).
- `crates/oz-policy-codegen/src/render.rs` — `pub fn render_contract(spec: &PolicySpec, slot_index: usize) -> Result<RenderedCrate, Error>`:
  - Selects the `Generated` slot at `slot_index`.
  - Builds an askama context struct with all variables referenced by `base.rs.jinja`.
  - Renders to a `RenderedCrate` value containing `src/lib.rs` source and a `Cargo.toml` template materialized with the codegen mode chosen in `docs/codegen-dependency-mode.md` (either depends on `stellar-accounts = 0.7.1` or stands alone).
- `crates/oz-policy-codegen/src/sandbox.rs` — `pub async fn compile(crate_dir: &Path, network_passphrase: &str) -> Result<CompiledArtifact, Error>`:
  - Materializes the `RenderedCrate` to a tempdir under `target/oz-sandbox/<sha256-of-render-input>`. Tempdir naming is the cache key — repeated identical renders skip compilation.
  - Runs `cargo build --release --target wasm32-unknown-unknown --locked` under `bubblewrap` (Linux) or `sandbox-exec` (macOS) with no network access except a read-only mount of the local cargo registry. The sandbox profile is committed at `scripts/sandbox-profile-{linux.sh,macos.sb}`.
  - Runs `stellar contract optimize --wasm <built.wasm> --output <optimized.wasm>`.
  - Computes the SHA-256 of the optimized WASM and returns `CompiledArtifact { wasm: Vec<u8>, wasm_hash: [u8; 32], source: String }`.
  - On compile failure, surfaces stderr to the caller as `Error::E_CODEGEN_COMPILE_FAILED` with the failing source line(s).
- `crates/oz-policy-codegen/src/lib.rs` — high-level `pub async fn synthesize_track_b(spec: &PolicySpec) -> Result<Vec<CompiledArtifact>, Error>` orchestrates per-`Generated`-slot rendering and compilation.
- Add `codegen` subcommand to `oz-policy-cli`: `oz-policy-cli codegen <spec-file> --out <dir>` writes `source.rs`, `policy.wasm`, `wasm_hash.txt` per generated slot.

### Agent Orchestration
- **Sequential prerequisite:** Author `base.rs.jinja` and the askama context struct first — every constraint template assumes the same surrounding skeleton.
- After the base template is committed, spawn **parallel** streams (one per constraint primitive: function_allowlist, argument_pattern, amount_range, asset_allowlist, time_window, call_frequency, sequence_ordering). Each stream owns: its `.rs.jinja` template, its render-context fields in `render.rs`, golden-output tests asserting byte-deterministic render, and an integration test that compiles a minimal spec using only that primitive and asserts the WASM compiles successfully under the sandbox driver.
- **Final sequential merge:** Lead session implements the sandbox driver `sandbox.rs` (it's not parallelizable — it depends on at least one renderable spec from any of the streams). After merge, run the cross-primitive composition test: a spec with 3 primitives composed into a single contract must render once and compile once.

### Verification / Test / Validation
- `cargo nextest run -p oz-policy-codegen` green. Required tests:
  - One golden-output test per constraint primitive: render fixture spec → assert generated `src/lib.rs` is byte-equal to checked-in `tests/golden/<primitive>.rs`.
  - One end-to-end test per constraint primitive: render → sandbox compile → assert WASM hash matches checked-in `tests/golden/<primitive>.wasm.sha256`.
  - One composition test combining function_allowlist + amount_range + call_frequency into a single contract; assert the generated source contains exactly one `pub struct Policy` and three enforce branches; assert WASM compiles and a recorded SHA matches checked-in golden.
  - One determinism test: render the same spec 100× with `proptest::sample::Index` perturbing irrelevant ordering details (Vec insertion order, etc.) → all 100 outputs byte-equal. The contract is: `synthesize_track_b` must be a pure function.
- Sandbox isolation test: attempt to `cargo build` a generated crate after killing the local cargo registry mount → the build must fail (proving no network egress). This is a CI-only test marked `#[ignore]` for local runs.
- `oz-policy-cli codegen walkthroughs/03-soroswap-bounded/spec.json --out target/walkthrough-3-out` produces `policy.wasm`, `source.rs`, `wasm_hash.txt`; the WASM hash matches the value pinned in `walkthroughs/03-soroswap-bounded/expected-wasm-hash.txt`.

**Completion Criterion (binary):** `cargo nextest run -p oz-policy-codegen` green AND the Soroswap-walkthrough codegen produces a WASM whose SHA-256 matches the pinned expected hash.

---

## Phase 4 — Simulation harness + proptest deny-vector generator

**Status:** Pending
**Depends on:** Phase 1, Phase 2, Phase 3
**Deliverable:** An in-process `soroban-env-host` simulation harness that, given a compiled policy WASM and a `Recording`, (1) replays the recording against the policy and asserts `enforce` returns Ok (permit case), (2) generates deny vectors per constraint primitive via `proptest` strategies derived from the `PolicySpec`, and (3) installs the candidate policy onto an in-memory smart account, simulates each deny vector against `__check_auth`, and asserts every vector triggers a panic with the expected error variant.

### Search / Research
- Read research §9 (simulation harness) verbatim. Note specifically: simulation runs locally via `soroban-env-host`, **not** via Soroban RPC `simulateTransaction`. RPC simulate is only used in the recorder (Phase 1) for envelope ingest.
- Read research §10's note about the OZ caveat that `simulateTransaction` does not return the delegated signer's `__check_auth` auth entry (CAP-71 pending). Confirm: since our deny-suite uses `soroban-env-host` end-to-end, this gap does not affect deny vectors directly — but the harness must still construct two auth entries manually (one for the `AuthPayload`, one for the nested delegated-signer invocation) when reproducing the delegated-signer auth path. Verify this in `docs/oz-internal-shapes.md`.
- Read research §12 threat model: cross-rule replay (mitigated by `(smart_account, context_rule_id)` storage segregation), `i128` overflow (`overflow-checks = true`), unauthorized state mutation (`smart_account.require_auth()` first thing in mutating hooks), TTL exhaustion (re-bump in `enforce`).
- Resolve which `soroban-env-host` API is the canonical entry point for installing a contract WASM and invoking `__check_auth` directly in-process (e.g., `Host::register_contract_wasm`, `Host::call`). Record the chosen API surface in `docs/simhost-api.md`.

### Implementation
- `crates/oz-policy-simhost/src/host.rs` — wrap `soroban-env-host`:
  - `pub fn new_test_host(ledger_seq: u32, network_passphrase: &str) -> TestHost` — builds a `Host` with a fresh in-memory ledger, sets `LedgerInfo { sequence_number: ledger_seq, ... }`, mounts an empty storage map.
  - `pub fn install_policy(host: &TestHost, wasm: &[u8], smart_account: Address, context_rule_id: u32, install_params: ScVal) -> Result<Address, Error>` — registers the WASM, invokes its `install` entry point, returns the deployed policy contract address.
  - `pub fn install_smart_account(host: &TestHost, owner_signer_pubkey: ScVal) -> Result<Address, Error>` — installs the `stellar-accounts` smart-account contract from a vendored WASM blob (committed under `crates/oz-policy-simhost/vendor/stellar-accounts-v0.7.1.wasm`), seeds an initial signer; this is the address policies are installed onto.
  - `pub fn invoke_check_auth(host: &TestHost, smart_account: Address, payload: AuthPayload, contexts: Vec<Context>) -> Result<(), HostError>` — invokes the SA's `__check_auth` entry, returns `Ok(())` if enforcement passed, `Err(HostError)` with the panic error code otherwise.
- `crates/oz-policy-simhost/src/permit.rs` — `pub fn replay_recording(host: &TestHost, recording: &Recording, smart_account: Address, context_rule_id: u32) -> Result<(), Error>`:
  - Translates each `recording.contracts[i]` invocation into a `Context::Contract { contract: addr, fn_name, args }`.
  - Constructs an `AuthPayload` matching the recording's signer composition.
  - Calls `invoke_check_auth`; asserts `Ok(())`. Returns `Error::E_SIM_PERMIT_DENIED` with the host error code if it panics.
- `crates/oz-policy-simhost/src/deny.rs` — `pub fn generate_deny_vectors(spec: &PolicySpec, recording: &Recording, rng_seed: u64) -> Vec<DenyVector>`:
  - One `proptest::strategy::Strategy` per constraint primitive (per research §9): different asset → `AssetNotAllowed`; 2× / 100× amount → `AmountExceedsCap`; `approve` instead of `transfer` → `FunctionNotAllowed`; `ledger_seq > window_start + window_ledgers + 1` → `TimeWindowExpired` / `WindowExceeded`; swapped sequence ordering → `SequenceViolation`; N+1 calls in window → `CallFrequencyExceeded`.
  - Each strategy emits a `DenyVector { name: String, payload: AuthPayload, contexts: Vec<Context>, expected_error: PolicyErrorCode }`.
  - Total: minimum 1 vector per constraint primitive in the spec; the harness invokes each in turn and asserts a matching panic.
- `crates/oz-policy-simhost/src/run.rs` — high-level `pub fn run_full_suite(spec: &PolicySpec, recording: &Recording, wasm_per_slot: &[CompiledArtifact], extra_deny: Vec<DenyVector>) -> SimReport`:
  - Creates a test host, installs the SA, installs each policy slot in order.
  - Runs permit case → records pass/fail.
  - Generates deny vectors per spec + appends `extra_deny`.
  - Runs each deny vector → records pass/fail per vector. A vector **passes** if `__check_auth` panics with the expected error variant; **fails** otherwise (including: panicked but with wrong error → `E_SIM_DENY_PASSED` is the error for "should have denied but allowed", `Mismatch` for "denied with wrong code").
  - Returns `SimReport { permit: PermitResult, deny_results: Vec<DenyResult>, total_vectors: usize, passed: usize }`.
- Add `simulate` subcommand to `oz-policy-cli`: `oz-policy-cli simulate <spec-file> <recording-file> --wasm-dir <dir> [--extra-deny <json>] --out report.json`.

### Agent Orchestration
- **Sequential prerequisite:** Implement `host.rs` first — `permit.rs`, `deny.rs`, `run.rs` all depend on its types.
- After `host.rs`, spawn **parallel** streams:
  - **Stream A:** `permit.rs` + permit-case unit tests against a fixed compiled WASM blob (built once and committed under `crates/oz-policy-simhost/tests/fixtures/` so the test doesn't recompile on every run).
  - **Stream B:** `deny.rs` proptest strategies + per-primitive deny-vector unit tests, each asserting the strategy generates valid `Context` shapes (typed correctness, not yet end-to-end through the host).
- **Sequential after parallel:** `run.rs` glues the two streams; final integration test runs the full suite for the SEP-41 subscription walkthrough (using the Phase 2 spec and the Phase 3 compiled WASM if Track B is involved; pure Track A walkthroughs need only the vendored OZ smart-account WASM).

### Verification / Test / Validation
- `cargo nextest run -p oz-policy-simhost` green.
- Permit replay test: against the SEP-41 subscription walkthrough's spec + WASM, the harness's `replay_recording` returns `Ok(())`.
- Per-primitive deny tests: for a contrived spec containing each of the seven constraint primitives, the deny generator emits ≥1 vector per primitive and every vector causes `__check_auth` to panic with the expected error variant.
- Composition deny test: for a spec with three primitives (function_allowlist + amount_range + call_frequency), the deny suite contains ≥3 vectors that pass.
- Negative meta-test: feed a deliberately-broken WASM (one without `smart_account.require_auth()` in `enforce`) to the harness; the harness must produce a `SimReport` flagging a permit-pass-with-warning (the recording was permitted, but the audit lint fired — for now, just verify the audit lint is in place as a Phase-9 stub).

**Completion Criterion (binary):** `cargo nextest run -p oz-policy-simhost` green AND `oz-policy-cli simulate walkthroughs/02-sep41-subscription/spec.json walkthroughs/02-sep41-subscription/recording.json --wasm-dir walkthroughs/02-sep41-subscription/wasm --out target/sim-report.json` produces a report with `permit.passed = true` and `deny_results.iter().all(|r| r.passed) = true`.

---

## Phase 5 — Full MCP server surface (5 tools, resources, prompts, both transports)

**Status:** Pending
**Depends on:** Phase 1, Phase 2, Phase 3, Phase 4
**Deliverable:** A single binary `oz-policy-mcp` built from `crates/oz-policy-mcp` that serves the MCP spec **2025-11-25** over both STDIO (subprocess) and Streamable HTTP (long-running service). Exposes 5 tools, 3 resource types, and 3 prompt templates as listed in research §7. Determinism, structured error codes, and conformance to the MCP spec are all gated tests.

### Search / Research
- Read research §7 verbatim. Tool signatures, resource URIs, prompt names, error codes, transport revision are all specified there.
- Read `docs/mcp-sdk-decision.md` (Phase 1) for the resolved `rmcp` crate version. If `rmcp` doesn't yet ship MCP spec 2025-11-25, fall back to the TypeScript-SDK + FFI path documented there.
- Verify how `rmcp` wants tool input/output schemas declared (likely via `#[derive(JsonSchema)]` on the input/output struct + a `#[tool]` attribute on the handler fn). Match the canonical pattern from the SDK's examples.
- Decide bearer-token format for HTTP auth: minimal viable is a static `Authorization: Bearer <token>` header validated against an env var; document this in `docs/mcp-auth.md` and plan TLS termination at the reverse-proxy layer in Phase 10 (no in-process TLS).

### Implementation
- `crates/oz-policy-mcp/src/tools.rs` — implement each tool as a typed handler. Tool surface (with full JSON Schema derived from `schemars`):
  - `record_transaction(input: RecordTransactionInput) -> RecordTransactionOutput`. Input: `{ hash?, envelope_xdr?, network: "testnet" | "mainnet", rpc_url?, instruction_leeway? }` (exactly one of `hash` or `envelope_xdr` must be present — schema marks them `oneOf`). Output: `{ recording_id, recording: Recording, retention_warning?: String }`. Errors: `E_RECORDER_HASH_NOT_FOUND`, `E_RECORDER_SIM_FAILED`.
  - `synthesize_policy(input: SynthesizePolicyInput) -> SynthesizePolicyOutput`. Input: `{ recording_id, tightness: "exact" | "small_margin" | "loose", lifetime_ledgers?: u32, delegated_signer?: Address, mode: "auto" | "compose_only" | "codegen_only" }`. Output: `{ spec_id, spec: PolicySpec, generated_count, composed_count }`. Errors: `E_SYNTH_NOT_EXPRESSIBLE`.
  - `simulate_policy(input: SimulatePolicyInput) -> SimulatePolicyOutput`. Input: `{ spec_id, extra_deny_vectors?: Vec<DenyVector> }`. Output: `SimReport`. Errors: `E_SIM_PERMIT_DENIED`, `E_SIM_DENY_PASSED`, `E_CODEGEN_COMPILE_FAILED` (when Track-B WASMs need building on the fly).
  - `export_policy(input: ExportPolicyInput) -> ExportPolicyOutput`. Input: `{ spec_id, smart_account: Address, source_account: Address, format: "rust_source" | "wasm" | "install_envelope" | "all" }`. Output: artifacts inlined as base64 in JSON (small) plus resource URIs (large). Errors: `E_CODEGEN_COMPILE_FAILED`, `E_INSTALL_PREFLIGHT_FAILED`.
  - `verify_install(input: VerifyInstallInput) -> VerifyInstallOutput`. Input: `{ smart_account: Address, context_rule_id: u32, spec_id?: String, network: "testnet" | "mainnet", rpc_url? }`. Output: `{ matches: bool, drift: Vec<DriftItem> }`. Errors: `E_VERIFY_DRIFT`.
- `crates/oz-policy-mcp/src/resources.rs` — implement `resources/list` and `resources/read` for URI families `recording://<id>`, `spec://<id>`, `artifact://<id>/source.rs`, `artifact://<id>/policy.wasm`, `artifact://<id>/install_envelope.xdr`. Backing store: an in-memory `dashmap` keyed by ID, with an optional disk-backing under `${OZ_POLICY_MCP_DATA_DIR:-$XDG_DATA_HOME/oz-policy-mcp}` for persistence across STDIO sessions.
- `crates/oz-policy-mcp/src/prompts.rs` — three prompt templates per research §7:
  - `record_and_explain` — accepts `hash | envelope` and returns a multi-step prompt that the agent uses to walk a user through recording + summarizing.
  - `synthesize_subscription` — wizard for SEP-41 subscription flows (binds to walkthrough 2).
  - `synthesize_delegated_trading` — wizard for Soroswap delegated trading flows (binds to walkthrough 3).
- `crates/oz-policy-mcp/src/main.rs` — bin entrypoint:
  - Detect transport: `--stdio` (default if no port arg) or `--http <port>`. Both transports load identical tool/resource/prompt registries.
  - For HTTP: bind `0.0.0.0:<port>`, require `Authorization: Bearer <token>` matching env `OZ_POLICY_MCP_TOKEN`; reject without it. Implement the Streamable HTTP framing per spec 2025-11-25.
  - For STDIO: read framed JSON-RPC from stdin, write to stdout, logs to stderr.
- Determinism: every tool's output is computed by the same library functions used in `oz-policy-cli`. The `rmcp` handler is a thin wrapper. No tool may invoke an LLM, fetch random state, or read environment-time-dependent values beyond inputs.
- Error mapping: every `E_*` from `oz-policy-core::errors` maps to an MCP `code` field (custom JSON-RPC error code in the application range) plus a `data: { error_code: "E_…", details: {...} }` payload so clients can branch on the literal `E_` string.

### Agent Orchestration
Spawn **parallel** streams once `tools.rs` skeleton is laid out:

- **Sequential prerequisite (single agent):** Author `tools.rs` skeleton with empty handler bodies that compile; declare every input/output type with `schemars::JsonSchema` derives; emit the registered tool list. This locks the public surface.
- **Stream A — Tool bodies.** Fill each handler's body by calling into `oz-policy-recorder`, `oz-policy-core`, `oz-policy-codegen`, `oz-policy-simhost`, `oz-policy-installer`. One agent owns this stream; parallelism within is bounded by shared in-memory store types.
- **Stream B — Resource + prompt surfaces.** Implements `resources.rs` and `prompts.rs` against the same in-memory store.
- **Stream C — Transport wiring.** Implements `main.rs` STDIO + HTTP, the bearer-token middleware, and the conformance smoke tests below.
- **Stream D — MCP client conformance matrix.** Stands up an integration test suite that drives the server from each of Claude Desktop, Cursor, Cline, Continue, and `mcp-cli` via their respective config files (committed under `tests/mcp-clients/`). Each client runs an identical scripted session (call `record_transaction`, then `synthesize_policy`, then `simulate_policy`, then `export_policy`) and asserts the response payloads are byte-equal across clients.

After streams return, lead session validates that the JSON Schema emitted by each tool is identical to the schema produced by `schemars` on the input/output types (no drift), and then runs the determinism gate (same inputs → byte-equal outputs across 100 invocations).

### Verification / Test / Validation
- `cargo nextest run -p oz-policy-mcp` green; every tool has at least one happy-path test, one error-path test per `E_*` code reachable from it, and one schema-validity test (`serde_json::from_value` round-trip).
- STDIO conformance: spawn `oz-policy-mcp --stdio`, drive it via `mcp-cli` with a scripted JSON-RPC session that exercises all 5 tools, all 3 resource URIs, all 3 prompts. Assert: every response is valid JSON-RPC; every tool's output schema validates against its declared schema; the same script run twice produces byte-equal output.
- HTTP conformance: spawn `oz-policy-mcp --http 8080 --token testtoken`, run the same scripted session via `curl`. Assert reject-without-token returns 401; reject-with-wrong-token returns 401; well-formed Bearer is accepted; output is byte-equal to the STDIO run.
- Cross-client matrix: the CI job runs the scripted session against Claude Desktop, Cursor, Cline, Continue, `mcp-cli` configs and diffs all five output transcripts; the matrix passes iff all five are identical.
- Determinism gate: invoke `synthesize_policy` 100× with identical inputs → 100 byte-equal outputs; same for `record_transaction` (against an immutable recorded testnet hash — not against simulate which can drift if ledger advances).

**Completion Criterion (binary):** `cargo nextest run -p oz-policy-mcp` green AND the MCP client conformance matrix CI job is green (all 5 client transcripts byte-equal) AND the 100× determinism gate passes for `synthesize_policy` and `record_transaction`.

---

## Phase 6 — Agent skill (`SKILL.md` + flat-file twin) with clarification logic

**Status:** Pending
**Depends on:** Phase 5
**Deliverable:** An Anthropic Agent Skills package at `skills/oz-policy-builder/` containing `SKILL.md` with progressive-disclosure frontmatter, `references/` with documentation excerpts and walkthrough snippets, `scripts/` with reusable utility scripts the skill calls, and `evals/` with at least one eval per walkthrough. Plus a flat-file twin (`skill/prompt.md` + `skill/tools.json`) for non-`SKILL.md` frameworks. The skill must trigger correctly in Claude.ai without operator prompting on the three walkthrough phrases.

### Search / Research
- Read research §8 verbatim for skill design.
- Verify the current Anthropic Agent Skills frontmatter format: required fields `name`, `description`; optional fields `model`, `tools`, `references`, `scripts`, `assets`, `evals`. Source: `https://github.com/anthropics/skills` and Claude API Skills overview. Document any newly-required fields in `docs/skills-format.md`.
- Confirm Claude.ai paid plans honor `SKILL.md` from a packaged directory and that the MCP-served tools are reachable as `tool_use` calls within the skill's context.

### Implementation
- `skills/oz-policy-builder/SKILL.md` — frontmatter:
  ```
  name: oz-policy-builder
  description: Records a Stellar transaction (by hash or simulation) and generates
    the minimum OpenZeppelin smart-account context rule + policies that would permit
    exactly that flow. Use whenever a user wants to authorize a third party
    (human or AI agent) to repeat a specific Stellar/Soroban operation under tight
    bounds — e.g., "let this agent claim my Blend yield weekly", "authorize this dapp
    up to 20 USDC monthly", "give my trading bot a 100-USDC-per-day Soroswap budget".
  ```
  Body: progressive disclosure. Top-level workflow: (1) ask mode (hash vs simulate); (2) call `record_transaction`; (3) summarize recording in plain English to the user and confirm; (4) detect ambiguity → ask clarifications (see below); (5) call `synthesize_policy` with chosen `tightness` and `mode`; (6) **always** call `simulate_policy` and surface permit + deny results; (7) call `export_policy` and hand artifacts to the wallet for signature. **Never auto-deploy.**
- `skills/oz-policy-builder/references/`:
  - `oz-policies-cheatsheet.md` — the three primitives, when each composes, the SEP-41-transfer-only constraint of `spending_limit`, the signer-set divergence footgun.
  - `walkthrough-blend.md`, `walkthrough-subscription.md`, `walkthrough-soroswap.md` — verbatim spec/recording references the skill can quote when explaining a similar synthesis to the user.
  - `error-codes.md` — every `E_*` code with a one-sentence remediation suggestion.
- `skills/oz-policy-builder/scripts/`:
  - `summarize_recording.py` — small Python helper that the skill invokes via `tool_use` (when permitted) to produce a human-readable summary of a `Recording` JSON; pure formatting, no policy logic.
  - `propose_clarifications.py` — given a `Recording`, returns the list of clarification questions the skill should ask. Triggers per research §8:
    - Single observed amount → "Cap at observed only, or allow a weekly/monthly total?"
    - Delegated signer present → "Reuse the same delegated address, or generate a new agent key?"
    - Soroswap router invocation → "Slippage cap defaults to observed + 2%; override?"
    - `Default` context rule selected by the synthesizer → "Switch to `CallContract(<target>)` for safety?"
- `skills/oz-policy-builder/evals/`:
  - One YAML eval per walkthrough: input transcript fragment ("let me give this agent a 20 USDC weekly budget on Blend"), expected tool-call sequence (record → synthesize → simulate → export), expected error if user data is invalid. Evals run via Anthropic's eval harness in CI.
- `skills/oz-policy-builder/flat/prompt.md` + `flat/tools.json` — the flat-file twin (research §8 "Portability"). `prompt.md` re-flows the same workflow into a single prompt; `tools.json` lists the MCP tool schemas in OpenAI-compatible form. This twin is shipped for non-`SKILL.md` frameworks.
- Wire CI to lint `SKILL.md` against the Anthropic skills schema and to dry-run each eval.

### Agent Orchestration
Parallel-friendly: the skill is a documentation-heavy artifact and most files are independent.

- **Stream A — `SKILL.md` body + workflow logic + clarification triggers** (the conceptual core).
- **Stream B — References** (walkthrough write-ups; one agent can own all three since they share format).
- **Stream C — Scripts** (`summarize_recording.py`, `propose_clarifications.py`; one agent owns both).
- **Stream D — Evals** (one YAML per walkthrough; one agent owns all three).
- **Stream E — Flat-file twin** (re-flows Stream A's output into single-prompt format; depends on Stream A, so this is the **only sequential dependency**).

After streams A–D return, lead session writes the flat-file twin and runs the eval suite.

### Verification / Test / Validation
- Anthropic skills schema validator (CI) passes against `skills/oz-policy-builder/`.
- Each of the three eval YAMLs passes when run against the deployed MCP server (point the eval harness at the local `oz-policy-mcp --stdio` binary).
- Manual eval in Claude.ai paid tier: paste each walkthrough trigger phrase as a user turn → assert the skill self-triggers (it appears in the assistant's tool-call sequence) and produces a tool-call ordering matching the eval YAML. Record screenshots into `evals/manual/` as evidence.
- Flat-file twin sanity check: feed `flat/prompt.md` + `flat/tools.json` into one non-Claude framework (Cursor) and execute walkthrough 2; assert the resulting tool-call sequence matches the YAML eval.

**Completion Criterion (binary):** All three eval YAMLs pass in CI AND the flat-file twin Cursor execution produces a tool-call sequence equivalent to walkthrough 2's eval.

---

## Phase 7 — Wallet integration (SEP-43 adapter, Freighter primary, passkey-kit secondary)

**Status:** Pending
**Depends on:** Phase 5
**Deliverable:** A pnpm TypeScript package `wallet-adapter` exposing a SEP-43-compliant interface that wraps Freighter (browser extension, primary) and passkey-kit (programmatic, secondary) for signing the install envelopes produced by `oz-policy-installer`. A `verify_install` MCP-tool round-trip (Phase 5 tool, used here) confirms the on-chain context rule matches the synthesizer's `PolicySpec`.

### Search / Research
- Read research §10 verbatim. SEP-43 surface: `getAddress`, `signTransaction`, `signAuthEntry`, `signMessage`. Error codes: `-1` internal, `-2` external service, `-3` invalid request, `-4` user rejected.
- Read `https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0043.md` (currently Draft v1.2.1). Confirm method signatures haven't changed since research was written.
- Read `@stellar/freighter-api` README on npm for the canonical browser-extension integration pattern; pin the version in *Tech Stack & Versions*.
- Read `kalepail/passkey-kit` README for the programmatic-signer surface. Verify the license is still Apache-2.0.
- Resolve research §16 verification item: confirm Freighter's `signTransaction` produces a signed transaction envelope compatible with our install envelope structure (which contains Soroban host function invocations). Test against testnet end-to-end before committing.

### Implementation
- `wallet-adapter/package.json`: `name: "@oz-policy-builder/wallet-adapter"`, license `Apache-2.0`, `main: dist/index.js`, build via `tsc`, peer dep on `@stellar/stellar-sdk@^12`, runtime dep on `@stellar/freighter-api` and `passkey-kit` at pinned versions.
- `wallet-adapter/src/sep43.ts` — re-export the canonical SEP-43 types (`SignTransactionParams`, `SignAuthEntryParams`, etc.) so consumers depend on us, not on a specific wallet.
- `wallet-adapter/src/adapters/freighter.ts` — implements `WalletAdapter`:
  - `async isAvailable(): Promise<boolean>` (checks `await isConnected()` from freighter-api).
  - `async getAddress(): Promise<string>`.
  - `async signTransaction(envelopeXdr: string, options: { network: "testnet" | "mainnet", networkPassphrase: string }): Promise<{ signedTxXdr: string, signerAddress: string }>`.
  - `async signAuthEntry(authEntryXdr: string, options): Promise<{ signedAuthEntry: string, signerAddress: string }>`.
- `wallet-adapter/src/adapters/passkey.ts` — same surface, backed by passkey-kit. Used for headless / Node.js / CI flows.
- `wallet-adapter/src/install.ts` — high-level helper: `async installPolicy(adapter: WalletAdapter, envelopeXdr: string, rpcUrl: string, network: "testnet" | "mainnet"): Promise<{ txHash: string, contextRuleId: number }>`:
  - Calls `adapter.signTransaction(envelopeXdr, ...)`.
  - Submits via `@stellar/stellar-sdk`'s `Server.sendTransaction`.
  - Polls `getTransaction(hash)` until status is `SUCCESS` or `FAILED`.
  - On success, extracts the new `context_rule_id` from the transaction's diagnostic events / return value and returns it.
  - On failure, surfaces the error in a typed `WalletInstallError`.
- `wallet-adapter/src/verify.ts` — `async verifyInstall(mcpToolHandle, smartAccount: string, contextRuleId: number, specId: string): Promise<VerifyReport>` — thin wrapper that calls the MCP `verify_install` tool, asserts the on-chain rule equals the spec.
- `wallet-adapter/examples/` — three Node.js example scripts (one per walkthrough) that demonstrate end-to-end usage with the passkey-kit adapter (headless), wired to the testnet RPC.
- `wallet-adapter/tests/` — Vitest tests with mocked freighter-api and a real passkey-kit testnet integration test gated by `INTEGRATION=1`.

### Agent Orchestration
Spawn three streams in **parallel**:

- **Stream A — SEP-43 types + Freighter adapter** (browser-facing path).
- **Stream B — passkey-kit adapter + headless examples** (Node.js-facing path).
- **Stream C — `install.ts` and `verify.ts` glue + Vitest mocked tests** (shared logic that both adapters compose into).

After streams return, lead session runs the testnet end-to-end test with the passkey-kit adapter against a fresh smart account, then runs the manual Freighter test in a browser (record steps under `wallet-adapter/tests/manual-freighter.md`).

### Verification / Test / Validation
- `pnpm test` in `wallet-adapter/` is green with mocked freighter-api.
- `INTEGRATION=1 pnpm test` is green: a fresh testnet smart account is funded via Friendbot, a Phase-2 install envelope is signed by passkey-kit, submitted, and `verify_install` confirms the context rule matches the spec.
- Manual Freighter test: load `wallet-adapter/examples/01-blend-yield-browser.html` in a Chromium browser with the Freighter extension installed and connected to testnet; complete the install flow; capture the resulting tx hash + context rule ID into `wallet-adapter/tests/manual-freighter-evidence.md`.

**Completion Criterion (binary):** `INTEGRATION=1 pnpm test` green AND the manual Freighter test produces a recorded testnet tx hash whose on-chain context rule matches the spec when fed through `verify_install`.

---

## Phase 8 — Three end-to-end walkthroughs (Blend, SEP-41 subscription, Soroswap)

**Status:** Pending
**Depends on:** Phase 2, Phase 3, Phase 4, Phase 5, Phase 7
**Deliverable:** Three frozen, reproducible walkthrough corpora under `walkthroughs/01-blend-yield/`, `walkthroughs/02-sep41-subscription/`, `walkthroughs/03-soroswap-bounded/`. Each corpus contains: a source transaction (`source.json` with a real testnet hash or saved envelope), the expected `Recording`, the expected `PolicySpec` (for each `synthesis_mode`), the compiled WASM(s) and their pinned hashes, an expected `SimReport`, a signed install envelope, an on-chain testnet smart account with the rule installed, and a `verify_install` confirmation. CI re-runs each walkthrough end-to-end on every PR.

### Search / Research
- For walkthrough 1 (Blend yield-claim): read `https://docs.blend.capital`, `https://github.com/blend-capital/blend-contracts`, plus research §11 verbatim. Identify the testnet `BlendPool` and `CometDex` contract addresses; record them in `walkthroughs/01-blend-yield/addresses.json`. The recorded flow: `pool.claim` then `comet.swap_exact_tokens_for_tokens(path=[BLND, USDC], to=user_smart_account)`.
- For walkthrough 2 (SEP-41 subscription): identify the testnet USDC SAC address (Stellar Asset Contract). Record in `walkthroughs/02-sep41-subscription/addresses.json`. Recorded flow: a single `transfer(user_smart_account, merchant, 5_000_000)` on USDC SAC.
- For walkthrough 3 (Soroswap bounded trading): read `https://docs.soroswap.finance` for the router contract address on testnet. The exact router function per research §11: `swap_exact_tokens_for_tokens(e: Env, amount_in: i128, amount_out_min: i128, path: Vec<Address>, to: Address, deadline: u64) -> Vec<i128>`. Record under `walkthroughs/03-soroswap-bounded/addresses.json`. Recorded flow: 100 USDC → XLM swap.
- For each walkthrough, identify the real testnet transaction hash that exercises the flow (these are frozen forever — any subsequent edit must update the expected outputs together). If an existing hash with the right shape can't be found, generate one by composing a transaction and submitting it to testnet through Phase 7's wallet adapter; record the hash and never rotate.

### Implementation
Per walkthrough (identical structure for all three):

- `walkthroughs/<n>-<name>/source.json` — `{ network: "testnet", hash: "...", rpc_url: "...", description: "..." }`.
- `walkthroughs/<n>-<name>/expected-recording.json` — output of Phase 1's recorder against `source.json`.
- `walkthroughs/<n>-<name>/expected-spec-track-a.json` — output of `synthesize_policy` with `mode: "compose_only"`. May be `null` / absent if the walkthrough has no compose-only-expressible path (walkthroughs 1 and 3 likely fall here).
- `walkthroughs/<n>-<name>/expected-spec-auto.json` — output of `synthesize_policy` with `mode: "auto"`. This is the canonical spec for the walkthrough.
- `walkthroughs/<n>-<name>/wasm/<slot_id>.wasm` — compiled WASM per generated policy slot, plus `wasm_hash.txt` per file.
- `walkthroughs/<n>-<name>/expected-sim-report.json` — output of `simulate_policy` against the spec + recording; assert at least 1 permit + N deny vectors per primitive in the spec.
- `walkthroughs/<n>-<name>/expected-install-envelope.xdr` — output of `prepare_install` with a frozen `source_account` + `smart_account` (testnet only, funded via Friendbot at corpus-freeze time).
- `walkthroughs/<n>-<name>/install-result.json` — `{ tx_hash, context_rule_id, verify_install: VerifyReport }` — the result of running Phase 7's `installPolicy` once at corpus-freeze time. Frozen, not re-run on every CI run (the on-chain side is durable until testnet resets; CI's responsibility is to re-derive everything else byte-equal).
- `walkthroughs/<n>-<name>/README.md` — narrative walkthrough for human reviewers: what the user wants, what the agent does, what the spec encodes, why the deny vectors matter.

CI workflow `.github/workflows/walkthroughs.yml`:
- Runs `cargo run -p oz-policy-cli -- record …` for each walkthrough, asserts byte-equal to `expected-recording.json`.
- Runs `synthesize` in both modes, asserts byte-equal to expected specs.
- Runs `codegen`, asserts WASM hashes match.
- Runs `simulate`, asserts the report's permit+deny outcomes match (note: timestamps and IDs are exempt from byte-equality and stripped via a canonicalizer).
- Skips `prepare-install` re-derivation in CI (it depends on dynamic resource fees from `simulateTransaction`) but runs a structural-equivalence test that decodes the envelope XDR and compares the host-function invocations against `expected-install-envelope.xdr` decoded the same way.

### Agent Orchestration
The three walkthroughs are fully independent. Spawn three streams in **parallel**, one per walkthrough. Each stream:
1. Resolves the testnet contract addresses.
2. Either picks a real existing testnet tx hash or composes and submits a new one (using the Phase 7 passkey-kit adapter in headless mode).
3. Runs the full Phase 5 MCP tool sequence locally, captures every output into the walkthrough corpus.
4. Authors the `README.md` narrative.
5. Adds the walkthrough to the CI workflow.

After streams return, lead session does the final corpus freeze: commits hashes, marks the walkthrough corpora read-only in the codeowners file, and adds a CONTRIBUTING note that walkthrough corpora are append-only (rotating a hash requires a deliberate replacement and is an explicit decision).

### Verification / Test / Validation
- The `walkthroughs.yml` CI workflow is green on the PR that introduces the corpora.
- For each walkthrough, manually re-running `oz-policy-cli` against `source.json` reproduces the corpus byte-equally (with the canonicalizer applied to sim-report timestamps).
- For each walkthrough, the `install-result.json`'s `verify_install` field shows `matches: true` (re-running `verify_install` against the on-chain rule confirms drift = none).
- The README.md for each walkthrough contains a plain-English description of what the synthesized policy *does and does not* permit, matched to the spec's constraints.

**Completion Criterion (binary):** The `.github/workflows/walkthroughs.yml` job is green on the merge commit AND for each of the three walkthroughs, `oz-policy-cli verify-install --smart-account <frozen> --context-rule-id <frozen> --spec walkthroughs/<n>/expected-spec-auto.json` returns `matches: true`.

---

## Phase 9 — Security hardening, fuzzing, external audit, reproducible builds

**Status:** Pending
**Depends on:** Phase 1, Phase 2, Phase 3, Phase 4, Phase 5, Phase 6, Phase 7, Phase 8
**Deliverable:** A fuzz harness running in CI, a reproducible-build manifest that any third party can use to re-derive the exact published WASM hashes from source, a `SECURITY.md` with disclosure policy, an audit-ready package handed to the chosen auditor, and the auditor's report committed to `audits/` with every finding either remediated in code (with a commit linked) or accepted-with-rationale (with a written rationale linked).

### Search / Research
- Re-read research §12 threat model in full: synthesizer-side (spec underspecification, codegen template bug, reproducibility failure, LLM non-determinism) and generated-policy-side (cross-rule replay, i128 overflow, unauthorized state mutation, TTL exhaustion, sponsor `context_rule_ids` substitution).
- Confirm OtterSec availability and engagement scope per research §17 Recommendation 4. If unavailable, fall back per research priority: Veridise → Runtime Verification → CoinFabrik → QuarksLab → Coinspect (the six SDF Soroban Audit Bank firms).
- Read OZ PR-#649 and PR-#655 source diffs to identify the exact on-chain version markers the synthesizer must check pre-install; update `oz-policy-installer`'s preflight if Phase 2 stubbed this.

### Implementation
- `crates/oz-policy-codegen/fuzz/` — `cargo-fuzz` setup. Harnesses:
  - `fuzz_targets/enforce_arbitrary_ctx.rs` — feeds arbitrary `ScVal` contexts to a fixed generated WASM's `enforce` and asserts the host doesn't panic with a *different* error than the spec's declared error set. Any unexpected panic is a finding.
  - `fuzz_targets/spec_to_wasm_panic_free.rs` — generates arbitrary `PolicySpec`s via `arbitrary` derive and asserts codegen + sandbox-compile + sandbox-run on a synthetic recording either succeeds or fails with one of the declared `E_*` codes.
  - `fuzz_targets/recording_decode.rs` — feeds arbitrary bytes to the recorder's XDR decoder; assert no panics, only typed errors.
  - Schedule continuous fuzz in CI on a nightly job; persist corpora to a separate branch `fuzz-corpora`.
- `crates/oz-policy-codegen/src/audit_lints.rs` — static lints over generated source before sandbox compile:
  - Every stateful template must emit `smart_account.require_auth()` as the first line of `enforce` (regex check).
  - Every storage write must use a `StorageKey` variant keyed by `(Address, u32)` (no bare key writes).
  - No `core::mem::transmute`, no `unsafe`, no panic without a `PolicyError` variant.
  - Any failure surfaces as `E_CODEGEN_COMPILE_FAILED` with a lint-violation message before compilation runs.
- `scripts/reproducible-build.sh` — produces the build manifest:
  - Records `rust-toolchain.toml`, `Cargo.lock`, `wasm-opt --version`, `stellar-cli --version`, `Cargo.toml` SHA, Dockerfile/sandbox-profile SHA.
  - Runs the full walkthrough corpus end-to-end inside a hermetic Docker container `oz-policy-builder/ci:<tag>` (Dockerfile committed under `ci/Dockerfile`), asserts produced WASM hashes match the pinned values.
  - Output: `reproducible-build-manifest.json` committed alongside each release tag.
- `SECURITY.md` — disclosure policy, scope, contact channel (email + GPG fingerprint), known-issues link, audit history link.
- `audits/` — directory layout: `audits/<auditor>-<date>/` per audit cycle, with `scope.md`, `findings.md`, `remediation-log.md`, and the auditor's signed PDF report.
- Audit-ready package (a single `audits/handoff-package/`):
  - The synthesizer source: `crates/oz-policy-core`, `crates/oz-policy-codegen`, `crates/oz-policy-simhost`, `crates/oz-policy-installer` (the surface the auditor must read end-to-end).
  - All template `.rs.jinja` files plus the golden generated `.rs` output for each constraint primitive.
  - The simulation harness + deny-vector generator source.
  - Three walkthroughs as concrete examples of the synthesizer in action.
  - The fuzz harness + accumulated corpora.
  - A `THREAT_MODEL.md` reproducing research §12 with explicit mappings from threat → mitigation → test that exercises the mitigation.
  - A `SCOPE.md` listing what's in/out of audit scope (synthesizer logic, templates, simhost, installer in; UI, MCP transport, wallet adapter out).

### Agent Orchestration
- **Stream A — Fuzz harness setup + nightly CI job** (independent, owns `fuzz/` and the workflow YAML).
- **Stream B — Audit lints + their unit tests** (owns `audit_lints.rs`).
- **Stream C — Reproducible-build script + Dockerfile** (owns `scripts/reproducible-build.sh`, `ci/Dockerfile`, the per-release manifest).
- **Stream D — Audit-ready package authorship** (`audits/handoff-package/`, `THREAT_MODEL.md`, `SCOPE.md`, `SECURITY.md`).

All four streams can run in **parallel** — they touch disjoint files. After they return, lead session engages OtterSec (or the chosen auditor) and tracks findings to closure. Finding remediation is sequential: each finding is fixed on its own PR, re-tested against the fuzz harness, and signed off by the auditor before the next.

### Verification / Test / Validation
- `cargo fuzz run enforce_arbitrary_ctx -- -max_total_time=60` is green (no crashes) in CI nightly.
- `scripts/reproducible-build.sh --release-tag <tag>` produces a manifest whose WASM hashes exactly match those committed in the walkthrough corpora (Phase 8) and the templates' golden hashes (Phase 3). Running the same script from a freshly-cloned repo on a different machine produces a byte-identical manifest.
- Audit report committed to `audits/<auditor>-<date>/` with every finding having either (a) a "Remediated in #<PR>" entry pointing to a merged PR or (b) an "Accepted with rationale" entry pointing to a written justification in `audits/<auditor>-<date>/accepted-rationales.md`.
- `oz-policy-codegen`'s audit-lints fire on a synthetic broken template (a hand-crafted `.rs.jinja` missing `smart_account.require_auth()` in `enforce`) — the negative test must produce `E_CODEGEN_COMPILE_FAILED`.

**Completion Criterion (binary):** The auditor's report is committed to `audits/<auditor>-<date>/` and every finding has a remediation PR or an accepted-rationale entry; the `reproducible-build.sh` end-to-end check passes from a fresh clone on a second machine; the nightly fuzz CI job has run for at least 7 consecutive nights without a crash.

---

## Phase 10 — Docs, release, mainnet readiness, hosted MCP endpoint

**Status:** Pending
**Depends on:** Phase 1, Phase 2, Phase 3, Phase 4, Phase 5, Phase 6, Phase 7, Phase 8, Phase 9
**Deliverable:** A `docs/` cookbook covering install, configuration, the three walkthroughs, and operational notes; an Apache-2.0 LICENSE plus CONTRIBUTING.md and CODE_OF_CONDUCT.md; a hosted MCP endpoint reachable over HTTPS with mainnet RPC and bearer-token auth; a tagged release on GitHub with attached release artifacts (the WASM blobs, the install-envelope examples, the manifest from Phase 9); and a "mainnet-ready" runbook covering deployment, monitoring, and disclosure response.

### Search / Research
- Identify the hosting target. Two viable options per research §7 and §10 / RFP requirements: (1) Cloudflare Workers + Workers AI optional (but research §3/§5.2 warned about Cloudflare lock-in for the synthesizer; here the synthesizer is a single Rust binary that doesn't fit Workers anyway, so option (1) is rejected); (2) any container-friendly host (Fly.io, Railway, AWS Fargate, GCP Cloud Run, Hetzner). Pick one based on the team's existing infra; document in `docs/hosting-decision.md` with cost + latency + region considerations.
- Read RFP requirements for "production-ready release with versioned MCP server endpoint and packaging for the Agent skill" (research §10.3 and `analysis.md` §10.3). Confirm the endpoint must be discoverable (publish under a stable URL, e.g., `mcp.<your-domain>/oz-policy-builder/v1`) and versioned (the URL contains the major version; MCP spec revision is independent of our endpoint version).
- Confirm mainnet RPC endpoint choice: public SDF RPC `https://soroban-rpc.mainnet.stellar.gateway.fm` (rate-limited) vs. dedicated provider (e.g., NowNodes, QuickNode). Pick one; record in `docs/rpc-mainnet-decision.md`.

### Implementation
- `docs/` cookbook structure:
  - `docs/install.md` — install the CLI, the MCP server, the wallet adapter.
  - `docs/concepts.md` — what a context rule is, what a policy is, what the synthesizer does and doesn't do.
  - `docs/walkthroughs/01-blend-yield.md`, `02-sep41-subscription.md`, `03-soroswap-bounded.md` — long-form walkthroughs.
  - `docs/mcp-clients.md` — config snippets for Claude Desktop, Cursor, Cline, Continue, mcp-cli.
  - `docs/wallets.md` — Freighter and passkey-kit setup, including code samples that mirror `wallet-adapter/examples/`.
  - `docs/operations.md` — running a self-hosted MCP server, environment variables, log format, scaling notes.
  - `docs/security.md` — link to `SECURITY.md`, the audit report, the threat model.
  - `docs/upstream.md` — proposed primitive contributions back to OpenZeppelin `stellar-contracts` (per the RFP's "stretch enhancements"); explicit list of which generated template families would make sense as upstreamed primitives (e.g., `function_allowlist`, `bounded_swap` — both currently codegen-only because no OZ primitive covers them).
- `LICENSE` — Apache-2.0 verbatim with the copyright line matching the upstream pollywallet form.
- `CONTRIBUTING.md` — fork → branch → test → PR; walkthrough corpus append-only rule; signed-off-by required.
- `CODE_OF_CONDUCT.md` — Contributor Covenant v2.1.
- Hosting:
  - `infra/<provider>/` — IaC for the chosen provider (Terraform or platform-native config). Contains: container image build, MCP server deploy spec, env-var schema (`OZ_POLICY_MCP_TOKEN`, `OZ_POLICY_MCP_RPC_URL_TESTNET`, `OZ_POLICY_MCP_RPC_URL_MAINNET`), TLS termination via the provider's managed cert, healthchecks against `GET /healthz` (add this endpoint to the MCP server in Phase 5 if not already present — patch Phase 5 forward via an in-place edit if needed).
  - `infra/<provider>/observability/` — logs to stdout, metrics scraped via OpenTelemetry exporter (using MCP semantic conventions per research §7 "auth/sandboxing/secrets" cross-reference; the conventions exist as referenced in the OTel MCP semantic conventions doc).
- Mainnet readiness checklist `docs/mainnet-readiness.md`:
  - Pre-flight: confirm the synthesizer's install preflight blocks any pre-PR-#655 smart account.
  - Manual canary: run walkthrough 2 (SEP-41 subscription) end-to-end on mainnet with `tightness=exact` and a $0.10 USDC cap; record tx hash; verify on-chain rule.
  - Disclosure rehearsal: simulate a Tier-1 finding (synthesizer emits a permissive constraint) and run the SECURITY.md flow; record evidence under `docs/canary/disclosure-rehearsal.md`.
- Release:
  - Annotated git tag `v1.0.0` after Phase 9 sign-off.
  - GitHub release with attachments: `oz-policy-mcp-{linux-amd64,linux-arm64,darwin-arm64,darwin-amd64}` binaries, the walkthrough-WASM bundle, the reproducible-build manifest, `SHA256SUMS` + signed `SHA256SUMS.asc`.
  - Publish `@oz-policy-builder/wallet-adapter` to npm.
  - Publish each Rust crate to crates.io (workspace crates may need `[workspace.package]` metadata fanned out into each member; do this only at release time to avoid churn).

### Agent Orchestration
- **Stream A — Cookbook authoring** (independent; owns `docs/`).
- **Stream B — Hosted MCP deploy** (owns `infra/<provider>/`, `/healthz` endpoint patch if needed, observability wiring).
- **Stream C — Release engineering** (owns `LICENSE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, the release workflow `.github/workflows/release.yml`, the crates.io + npm publish glue).
- **Stream D — Mainnet canary** (owns `docs/mainnet-readiness.md`, runs the canary against mainnet using Phase 7's passkey-kit adapter, records evidence). This stream is the last to run because it consumes the hosted-MCP URL from Stream B.

Streams A, B, C run in **parallel** as soon as Phase 9 is complete. Stream D runs **sequentially** after Stream B reports the hosted URL.

### Verification / Test / Validation
- The hosted MCP endpoint responds to a scripted session (record → synthesize → simulate → export for walkthrough 1) over Streamable HTTP from a remote machine within 5 seconds end-to-end (with cold cache). Record the timing into `docs/operations.md`.
- The hosted endpoint rejects requests without `Authorization: Bearer <token>` with a 401.
- Mainnet canary: a `0.10 USDC` subscription policy is synthesized, the install envelope is signed with a passkey-kit-managed mainnet smart account, the transaction lands on mainnet, and `verify_install` returns `matches: true`. The tx hash is recorded in `docs/canary/mainnet-canary-evidence.md`.
- `v1.0.0` GitHub release is published with all artifacts attached; `sha256sum -c SHA256SUMS` validates locally; `SHA256SUMS.asc` verifies against the project's signing key.
- npm `@oz-policy-builder/wallet-adapter` is installable: `pnpm add @oz-policy-builder/wallet-adapter` in a fresh project works and the package's TypeScript types resolve.
- crates.io: each workspace member crate is fetchable via `cargo install` (CLI) or `cargo add` (libs).

**Completion Criterion (binary):** The hosted MCP endpoint passes the 5-tool scripted-session smoke test from a remote machine, AND the `v1.0.0` release is published with all artifacts AND `SHA256SUMS.asc` verifies AND the mainnet canary tx hash returns `matches: true` from `verify_install`.

---

## Cross-Phase Invariants

These hold throughout every phase. The executor must enforce them as preconditions before declaring any phase complete.

1. **No auto-deployment, ever.** No tool in any phase submits a transaction without a prior wallet signature. The only on-chain submissions in this plan happen in Phase 7 (testnet installs by passkey-kit in headless tests) and Phase 10 (the mainnet canary). Both involve explicit, deliberate signing steps. Reject any PR that adds an unattended submission path.

2. **Deterministic synthesizer.** Codegen, decision tree, deny-vector generation, install-envelope construction are all pure functions of inputs (with `simulateTransaction` calls in the recorder being the documented exception for ingest — that's why the recorder writes the result into the `Recording` and downstream phases work from the frozen `Recording`). Any test that demonstrates output drift for identical inputs is a P0 bug.

3. **LLM is never in the codegen path.** Per research §5 Recommendations. LLMs appear only in the agent skill's clarification/summarization role (Phase 6). The synthesizer, the codegen, and the simulation harness never call an LLM.

4. **Apache-2.0 propagation.** Every committed file has a clear license. Every dependency surfaced in `deny.toml`'s allow-list is non-copyleft and permissive. Re-check this in Phase 9 as part of `cargo deny check`.

5. **Pollywallet engagement is explicit.** Wherever this plan extends, replaces, or directly adopts something from `kalepail/pollywallet`, the commit message names the source file and the disposition (Adopt / Extend / Replace). The README's "Building on existing work" section maintained by Phase 10 enumerates every such borrowing. Reasoning: research §4 Pillar 1 makes this a structural commitment in the RFP. **For the executor: when adopting any pollywallet construct verbatim, retain the original Apache-2.0 license header.**

6. **The walkthrough corpus is the regression suite.** Phases 2–9 all depend on the corpus produced by Phase 8 staying byte-equal under recompilation. If a refactor needs to change the corpus (e.g., a primitive's storage key naming changes), the refactor PR must re-derive every corpus file in a single commit and ship a CHANGELOG entry explaining the rotation.

---

## Open Questions Carried Forward (not blocking, but track these)

These are unresolved at planning time per research §11 (analysis) and §16 (technical-research). They are *not* required to start Phase 1, but each must be answered before the phase listed under "First Required At":

| Question | First Required At | Source / Owner |
|---|---|---|
| Is the OZ accounts policy builder RFP confirmed for SCF #44? | Phase 10 (release announcement) | SCF team — out of scope for execution but tracked here for completeness |
| Has Tyler/kalepail agreed to a co-submission / partner-with-attribution structure? | Phase 10 (release messaging) | Out of scope for execution |
| Which auditor specifically? OtterSec or fallback? | Phase 9 (handoff) | Resolved in `audits/auditor-selection.md` before Phase 9 starts |
| Final hosted-MCP provider choice? | Phase 10 Stream B | `docs/hosting-decision.md` |
| Mainnet RPC provider for the hosted endpoint? | Phase 10 Stream B | `docs/rpc-mainnet-decision.md` |
| Does `rmcp` ship MCP spec 2025-11-25? | **RESOLVED in Phase 1**: yes, `rmcp 1.7.0` supports `2025-11-25` since v1.5.0 (PR #802). See `docs/mcp-sdk-decision.md`. |
| Exact `stellar-accounts` version on crates.io (`0.7.1` vs `0.7.0-rc.1`)? | **RESOLVED in Phase 1**: `0.7.1` (both crates.io publish AND GitHub tag align). The plan's *Tech Stack & Versions* row is updated. |
| Codegen dependency mode (link `stellar-accounts` or stand alone)? | Phase 3 | `docs/codegen-dependency-mode.md` |
| Does `spending_limit::AccountParams` carry a `token: Address`? | **RESOLVED in Phase 1**: NO. The struct (`SpendingLimitAccountParams`) has only `spending_limit: i128` and `period_ledgers: u32`. The token lives in `ContextRule.context_type::CallContract(Address)` and `install` rejects any other context type with `OnlyCallContractAllowed (3227)`. See `docs/oz-internal-shapes.md` §4.1. |
| `spending_limit` period unit (ledgers vs seconds)? | **RESOLVED in Phase 1**: ledgers (`period_ledgers: u32`). Rolling-window logic uses `e.ledger().sequence()` and `entry.ledger_sequence: u32`. See `docs/oz-internal-shapes.md` §4.2. |
| Pre-PR-#655 smart-account version marker (used by install preflight)? | **PARTIALLY RESOLVED in Phase 1**: no on-chain marker exists in source. Three fallback strategies documented in `docs/oz-internal-shapes.md` §8 (WASM-hash whitelist, behavioral probe, user assertion). Decision deferred to Phase 2 implementation; Phase 9 may file an issue upstream requesting a `SMART_ACCOUNT_AUTH_DIGEST_REV` constant. |

---

## Appendix — Where each piece of the research lands

| Research section | Phase that consumes it |
|---|---|
| `analysis.md` §1–§4 (RFP decomposition) | Read by every phase as orientation; cited explicitly in Phase 10's release messaging |
| `analysis.md` §5.1 (OZ framework hard facts) | Phase 1 (verify in source), Phase 2 (PolicySpec hard limits), Phase 3 (codegen templates), Phase 9 (audit threat model) |
| `analysis.md` §5.2 (pollywallet teardown) | Phase 1 (schema as starting point), Phase 3 (codegen patterns), Cross-Phase Invariant #5 |
| `analysis.md` §5.2.1 (policy-codegen footguns) | Phase 3 askama templates, Phase 9 audit lints |
| `analysis.md` §5.6 (Stellar / Soroban tx model) | Phase 1 recorder, Phase 2 installer |
| `analysis.md` §7 (Architectural decision space) | Architecture is fixed in this plan — research §7 reasoning preserved in commit messages and `docs/concepts.md` |
| `analysis.md` §10 (Risks, red flags) | Phase 9 (threat model), Cross-Phase Invariants |
| `analysis.md` Appendix A (pollywallet code map) | Phase 1 (deciding what to fork vs port), Phase 6 (skill workflows mirror pollywallet phases) |
| `analysis.md` Appendix B (OZ hard facts) | Phase 1 (`docs/oz-internal-shapes.md`), Phase 2 PolicySpec validators |
| `technical-research.md` §1 (Policy trait) | Phase 1 verify, Phase 3 codegen base template |
| `technical-research.md` §2 (primitives detail) | Phase 2 Track A |
| `technical-research.md` §3 (audit caveats) | Phase 2 install preflight, Phase 9 audit lints |
| `technical-research.md` §4 (architecture) | This plan's structure |
| `technical-research.md` §5 (recording layer) | Phase 1 |
| `technical-research.md` §6 (synthesizer both tracks) | Phases 2 + 3 |
| `technical-research.md` §7 (MCP server) | Phase 5 |
| `technical-research.md` §8 (agent skill) | Phase 6 |
| `technical-research.md` §9 (simulation harness) | Phase 4 |
| `technical-research.md` §10 (wallet integration) | Phase 7 |
| `technical-research.md` §11 (walkthroughs) | Phase 8 |
| `technical-research.md` §12 (security / audit) | Phase 9 |
| `technical-research.md` §13 (stack + versions) | *Tech Stack & Versions* section above |
| `technical-research.md` §16 (TBDs) | Phase 1 *Search / Research* — must be zero-ed out before downstream phases run |
| `technical-research.md` §17 (phased plan) | This plan's phase decomposition is derived from it, extended with Agent Orchestration and Verification gates |
