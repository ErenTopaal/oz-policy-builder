<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 OZ Policy Builder contributors
-->

# Project Completion Report — OZ Accounts Policy Builder

**Date:** 2026-06-03
**Branch:** `phase-1-foundations`
**HEAD:** `84ab19c`
**Total commits:** 150
**License:** Apache-2.0

---

## 1. What this project is

An Apache-2.0 toolkit that **records a Stellar transaction and synthesizes the smallest possible OpenZeppelin smart-account policy that would permit exactly that transaction and nothing more**. The output is real, compilable Soroban policy code that can be installed on a Stellar smart account so an AI agent (or dapp, or service) can repeat the recorded flow under tight bounds — without ever receiving full account keys.

The toolkit ships as four interfaces over one Rust core:

- **CLI** (`oz-policy-cli`) — terminal commands for `record / synthesize / codegen / simulate / prepare-install`
- **MCP server** (`oz-policy-mcp`) — Model Context Protocol server exposing the same surface to AI clients (Claude Desktop, Cursor, Cline, Continue, mcp-cli) over STDIO or Streamable HTTP
- **Agent skill** (`skills/oz-policy-builder/SKILL.md`) — Anthropic Agent Skills package wrapping the MCP tools with a conversational workflow + clarification prompts + flat-file twin for non-Claude frameworks
- **Wallet adapter** (`@oz-policy-builder/wallet-adapter`) — TypeScript package implementing SEP-43 for Freighter + passkey-kit, plus `installPolicy` / `verifyInstall` orchestration and the OZ smart-account AuthPayload encoder

---

## 2. The end-to-end flow

The toolkit is built around a single linear pipeline. Each stage has a public command and a typed JSON output:

```
┌────────────┐    ┌──────────────┐    ┌──────────┐    ┌────────────┐    ┌────────────┐    ┌──────────────┐
│   record   │ -> │  synthesize  │ -> │ codegen  │ -> │  simulate  │ -> │  prepare-  │ -> │   install    │
│            │    │              │    │  (Track-B │    │            │    │  install   │    │   (wallet)   │
│ RPC + XDR  │    │ PolicySpec   │    │   only)   │    │ permit +   │    │            │    │              │
│ -> Recording│   │  IR         │    │ Rust+WASM│    │ deny       │    │ envelope   │    │  + verify    │
└────────────┘    └──────────────┘    └──────────┘    └────────────┘    └────────────┘    └──────────────┘
       │                  │                │                │                  │                  │
   public            "compose vs.       audit-bounded   soroban-env-host    wallet-signable   on-chain
   testnet tx       generate" decision  askama templates  in-process VM    XDR (no submit)  context rule
   or simulation                                                                              installed
```

**Demonstrated end-to-end on Stellar testnet** with three frozen walkthroughs (Blend yield-claim, SEP-41 subscription, Soroswap bounded trading) plus a live install + verify roundtrip at tx `038583fa4c95654c9a26323702b86729e084357d47ab169fa22a77d821ce90bb` (ledger 2617998, context_rule_id 4, `verifyInstall.matches=true`).

---

## 3. Architecture

### Component overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                          CONSUMER LAYER                             │
│   Claude / Cursor / Cline / Continue  ←──→  agent skill (SKILL.md)  │
│   Browser dapp / Node CLI             ←──→  wallet-adapter (TS)     │
│   Direct user                         ←──→  oz-policy-cli           │
└────────────────────────────────┬────────────────────────────────────┘
                                 │ MCP (STDIO or Streamable HTTP)
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│  oz-policy-mcp                                                      │
│  5 tools: record_transaction, synthesize_policy, simulate_policy,   │
│           export_policy, verify_install                             │
│  3 resource URI families, 3 prompt templates                        │
│  STDIO + Streamable HTTP, bearer auth, /healthz                     │
└─────┬─────────────┬────────────┬────────────┬────────────┬──────────┘
      │             │            │            │            │
      ▼             ▼            ▼            ▼            ▼
┌───────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐
│ recorder  │ │   core   │ │ codegen  │ │ simhost  │ │ installer  │
│           │ │          │ │          │ │          │ │            │
│ RPC + XDR │ │PolicySpec│ │ askama   │ │ soroban- │ │  envelope  │
│ decode    │ │decision  │ │ templates│ │ env-host │ │ + preflight│
│           │ │ tree     │ │ + sandbox│ │ + proptest│ │ + registry │
└───────────┘ └──────────┘ └──────────┘ └──────────┘ └────────────┘
```

### Rust workspace (7 crates)

| Crate | Responsibility | Public surface |
|---|---|---|
| `oz-policy-core` | `PolicySpec` IR, decision tree, SEP-41 detection, `ArgValue`, `Recording` IR, `Error` type | `synthesize()`, `is_sep41_transfer()`, types |
| `oz-policy-recorder` | Soroban RPC client + XDR decoder | `record_by_hash()`, `record_by_simulation()` |
| `oz-policy-codegen` | askama templates + sandbox compile + audit lints | `render_contract()`, `synthesize_track_b()`, 7 constraint templates |
| `oz-policy-simhost` | In-process `soroban-env-host` harness | `TestHost`, `replay_recording()`, `generate_deny_vectors()`, `run_full_suite()` |
| `oz-policy-installer` | Install envelope builder + preflight + address registry | `build_install_envelope()`, `AccountRevision` |
| `oz-policy-mcp` | rmcp server (STDIO + HTTP), tool handlers, store, on-chain readback | 5 MCP tools, real `get_context_rule` readback |
| `oz-policy-cli` | Thin CLI wrapping all crates | `record / synthesize / codegen / simulate / prepare-install / verify-install` |

### TypeScript package

| Package | Responsibility |
|---|---|
| `@oz-policy-builder/wallet-adapter` | SEP-43 types + Freighter adapter + passkey-kit adapter + `installPolicy` + `verifyInstall` + OZ AuthPayload encoder + 3 headless example scripts |

### Templates (`templates/`)

| File | Constraint primitive |
|---|---|
| `base.rs.jinja` | Soroban policy contract skeleton (`#[contract]` + install/enforce/uninstall) |
| `constraints/function_allowlist.rs.jinja` | Function name whitelist (Symbol match) |
| `constraints/argument_pattern.rs.jinja` | Typed slot match (Address / i128 / u32 / u64 / Bytes) |
| `constraints/amount_range.rs.jinja` | i128 clamp on a named arg |
| `constraints/asset_allowlist.rs.jinja` | C-address whitelist |
| `constraints/time_window.rs.jinja` | Ledger-sequence window |
| `constraints/call_frequency.rs.jinja` | N-per-window stateful rate limiter |
| `constraints/sequence_ordering.rs.jinja` | Phase-ordered state machine |

Every generated contract is gated by 5 audit lint rules before sandbox compile (`require_auth_first`, `storage_keyed_by_pair`, `no_unsafe`, `panic_uses_policy_error`, `no_floats_on_amounts`). Found and fixed one real `.unwrap()` bug in `call_frequency` during Phase 9.

---

## 4. RFP deliverable scorecard

| # | RFP Deliverable | Status | Evidence |
|---|---|---|---|
| 1 | MCP server (open source) | ✅ Delivered | `crates/oz-policy-mcp/`; 5 tools, both transports, bearer + healthz |
| 2 | Claude skill / agent integration | ✅ Delivered | `skills/oz-policy-builder/SKILL.md` + scripts + 3 evals + flat-file twin |
| 3 | Policy synthesizer library (Rust) | ✅ Delivered | `oz-policy-core` + `oz-policy-codegen`; produces real compilable Soroban WASM |
| 4 | Simulation / dry-run harness | ✅ Delivered | `oz-policy-simhost`; `soroban-env-host` + proptest deny generator |
| 5 | Reference wallet integration end-to-end | ✅ Delivered | `wallet-adapter/`; full pipeline closed on testnet at tx `038583fa…`, `verifyInstall.matches=true` |
| 6 | Three documented walkthroughs | ✅ Delivered | `walkthroughs/01-blend-yield/`, `02-sep41-subscription/`, `03-soroswap-bounded/` — all frozen with corpus + READMEs |
| 7 | Developer documentation | ✅ Delivered | 30+ markdown files: `docs/`, `audits/`, walkthrough READMEs, top-level READMEs |
| 8 | Test suite | ✅ Delivered | **363 tests** (285 Rust + 78 TS), 10 ignored network/sandbox tests, all gates green |
| 9 | Security audit + remediation | ❌ Deferred | Audit-prep done (`audits/THREAT_MODEL.md`, `SCOPE.md`, `READY.md`, `handoff-package/`); external engagement deferred by user |
| 10 | Production release + versioned endpoint | ❌ Human-required | `.github/workflows/release.yml` ready; `infra/fly/` blueprint ready; needs GPG key + GHA secrets + cloud account + mainnet XLM |

**Net: 8 of 10 delivered, 1 deferred, 1 human-required.**

---

## 5. What was built — phase-by-phase

The implementation followed a 10-phase plan (`plan.md`, ~95KB). Each phase ended with a binary completion gate.

### Phase 1 — Foundations + recorder
- Rust workspace (7 crates, virtual workspace), CI scaffolding, deny policy, LICENSE
- Recorder: `getTransaction` / `simulateTransaction` → typed `Recording` (22-variant `ArgValue` enum covering all `ScVal` shapes; I128 serialized as JSON string for precision)
- 30s timeout on RPC awaits, network-passphrase cross-check, tracing instrumentation
- Frozen Blend testnet `claim` fixture (tx `5a0ccffe…`) and byte-equal roundtrip test
- **Gate:** `cargo nextest run -- --include-ignored recorder::integration::blend_claim_roundtrip` passes byte-equal

### Phase 2 — PolicySpec IR + Track A synthesizer + installer
- `PolicySpec` IR (`oz-policy-builder/v1` schema) with full schemars derives
- Decision tree synthesizer composing the 3 OZ primitives (`simple_threshold`, `weighted_threshold`, `spending_limit`)
- Install envelope builder (calls `simulate_transaction_envelope`, assembles, returns base64 XDR — does not submit)
- Preflight: PR-#655 + PR-#649 enforcement; MAX_POLICIES/SIGNERS/NAME_SIZE gates
- Frozen SEP-41 USDC subscription corpus + Phase 2 completion gate
- **Real OZ source verification** in `docs/oz-internal-shapes.md` (verbatim struct extracts from `stellar-accounts@v0.7.1`)

### Phase 3 — Track B codegen + sandbox
- 7 askama templates + base skeleton; conditional rendering per used constraints
- Sandbox compile via `cargo build --target wasm32-unknown-unknown` + `stellar contract optimize` (wasm-opt 0.116.1)
- macOS `sandbox-exec` profile; Linux `bwrap` fallback
- Byte-deterministic WASM hashes pinned in `walkthroughs/phase3-codegen-fixture/expected/slot_0/wasm_hash.txt` (`cb2a8736…`)
- Audit lints (Phase 9) integrated as pre-compile gate

### Phase 4 — Simulation harness
- `TestHost` wrapping real `soroban-env-host = 25.0.1`
- Permit replay via direct `enforce` invocation (Phase 7 wallet integration closed the `__check_auth` wrap gap)
- proptest-driven deny-vector generator: per-primitive boundary mutations matched to real OZ error codes (3220-3227) and template-emitted codes (1010-1070)
- Deterministic for fixed seed; report serializes as JSON
- **Gate:** non-ignored Phase 4 test exercises spec → render → install → permit + 1+ deny per primitive

### Phase 5 — MCP server
- `rmcp = 1.7.0` on MCP spec **2025-11-25** (STDIO + Streamable HTTP)
- 5 tools: `record_transaction`, `synthesize_policy`, `simulate_policy`, `export_policy`, `verify_install`
- 3 resource URI families: `recording://`, `spec://`, `artifact://`
- 3 prompt templates: `record_and_explain`, `synthesize_subscription`, `synthesize_delegated_trading`
- Bearer-token auth for HTTP; `/healthz` endpoint
- Cross-client conformance: configs for Claude Desktop / Cursor / Cline / Continue / mcp-cli; STDIO + HTTP smoke tests
- `McpStore` in-memory + optional disk persistence
- Real on-chain readback (`verify_chain.rs`, 1082 lines) added in deliverable-5 closure

### Phase 6 — Agent skill
- `skills/oz-policy-builder/SKILL.md` with progressive disclosure + workflow
- Python scripts: `summarize_recording.py`, `propose_clarifications.py`
- 3 walkthrough evals in YAML
- Flat-file twin (`prompt.md` + `tools.json`) for non-Claude frameworks; `tools.json` generated from real `schemars::schema_for!` output

### Phase 7 — Wallet adapter + AuthPayload encoder + testnet install
- TypeScript pnpm package; SEP-43 types + Freighter adapter + passkey-kit adapter
- `installPolicy` (sign + submit + poll + extract context_rule_id) + `verifyInstall` (MCP subprocess wrapper)
- **OZ AuthPayload encoder** (`oz_smart_account_auth.ts`, ~400 LOC): `encodeAuthPayload`, `computeAuthDigest`, `buildOzAuthEntry` + verified SHA-256 against fixed inputs
- Real testnet deployment: SA `CAQGYWVE…3A` + policy contract `CDBE67MN…AR`
- **Closed deliverable #5** at tx `038583fa…ce90bb` on 2026-05-18

### Phase 8 — Walkthrough corpora
- Three corpora frozen end-to-end:
  - **Blend yield-claim** (tx `5a0ccffe…`): Track-B `function_allowlist=["claim"]` on pool `CCEBVDYM…`
  - **SEP-41 subscription** (tx `52b86b53…`): Track-A `spending_limit` composition on USDC-style SAC
  - **Soroswap bounded trading** (tx `7475b169…`, submitted by us): Track-B function + asset allowlist on `swap_exact_tokens_for_tokens`
- CI workflow `.github/workflows/walkthroughs.yml` re-derives spec + sim-report byte-equally on every PR
- Per-walkthrough `README.md` narratives + corpus directories with `source.json`, `expected-recording.json`, `expected-spec-auto.json`, `wasm/`, `expected-sim-report.json`, `expected-install-envelope.xdr`

### Phase 9 — Security hardening
- 2 cargo-fuzz harnesses (`oz-policy-codegen/fuzz/spec_to_wasm_panic_free`, `oz-policy-recorder/fuzz/recording_decode_panic_free`) + nightly CI
- 5 audit lint rules over generated contract source (`audit_lints.rs`); pre-compile gate
- Reproducible-build script (`scripts/reproducible-build.sh`) + Dockerfile + GHA workflow; re-derives all 3 pinned WASMs byte-equally
- `SECURITY.md` + `audits/THREAT_MODEL.md` (10 threats with mitigation + test cross-refs) + `audits/SCOPE.md` + `audits/READY.md` + `audits/handoff-package/`
- Found and remediated a real template bug during this phase: `call_frequency.rs.jinja` had a bare `.unwrap()` inside a `for` loop; replaced with `unwrap_or_else(|| panic_with_error!(env, PolicyError::Default))`

### Phase 10 — Release engineering + docs
- 11-doc cookbook in `docs/`
- Top-level: `README.md`, `STATUS.md`, `HANDOFF.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `NOTICE`
- `.github/workflows/release.yml` (tag-triggered, secrets-gated, SHA-pinned actions): linux-amd64 + linux-arm64 + darwin-amd64 + darwin-arm64 binaries, signed `SHA256SUMS`, crates.io publish in dependency order, npm publish
- Fly.io IaC blueprint (`infra/fly/{fly.toml, Dockerfile.runtime, deploy.sh}`)
- `docs/mainnet-readiness.md` runbook for the canary
- `HANDOFF.md`: 7-step checklist of human-required actions to reach v1.0.0

---

## 6. Test coverage breakdown

**Total: 363 tests passing** (12 skipped / network-gated)

### Rust workspace (285 passing, 10 ignored)

Counted via `cargo nextest run --workspace`:

- `oz-policy-core`: ~30 tests (decision tree, sep41 detection, PolicySpec round-trip, ArgValue serde, errors, schema export)
- `oz-policy-recorder`: ~10 tests + 1 ignored integration (`blend_claim_roundtrip`) + 2 XDR-decode fixture tests
- `oz-policy-codegen`: ~57 tests (7 golden render tests + 8 lint rules + render context + sandbox driver + composition + determinism)
- `oz-policy-simhost`: ~46 tests (host wrapper + permit replay + deny-vector generator with per-primitive coverage + run_full_suite determinism)
- `oz-policy-installer`: ~15 tests + 1 ignored testnet integration (`envelope_against_testnet`) + Phase 2 completion gate
- `oz-policy-mcp`: ~87 tests + 2 ignored smoke tests (`stdio_smoke_full_session`, `http_smoke_full_session`) covering the 5 tools + store + resources + prompts + transports + auth + verify_chain
- `oz-policy-cli`: ~10 tests (clap parser shapes + exit-code mapping)
- Phase completion gates: `blend_claim_roundtrip`, `phase2_completion`, `phase3_render_byte_equal`, `phase4_simulate_emits_passing_report`

### TypeScript wallet-adapter (78 passing, 2 skipped)

Counted via `pnpm test`:

- `sep43.test.ts`: 4 tests
- `adapters/freighter.test.ts`: 16 tests (mocked freighter-api)
- `adapters/passkey.test.ts`: 11 + 1 skipped (real stellar-sdk signing, no mocks)
- `install.test.ts`: 16 tests (mocked stellar-sdk + adapter)
- `verify.test.ts`: 13 tests (mocked MCP subprocess)
- `oz_smart_account_auth.test.ts`: 17 tests (encoder + digest)
- `phase7_integration.test.ts`: 1 (INTEGRATION-gated, runs against testnet)

### Fuzz harnesses (nightly CI)

- `spec_to_wasm_panic_free.rs` — arbitrary PolicySpec → render → assert no panic
- `recording_decode_panic_free.rs` — arbitrary bytes → recorder XDR decoder → assert no panic

### Verification gates (must stay green on every commit)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo deny check
cargo nextest run -- --include-ignored recorder::integration::blend_claim_roundtrip
cargo nextest run --workspace phase2_completion
cargo nextest run -p oz-policy-codegen phase3_render_byte_equal
cargo nextest run --workspace phase4_simulate_emits_passing_report
cd wallet-adapter && pnpm test
./scripts/reproducible-build.sh test-run    # 3/3 WASMs re-derive byte-equally
```

All 10 gates exit 0 at HEAD `84ab19c`.

---

## 7. What is honestly NOT done

Two categories — both are tracked, neither is hidden.

### Deferred (your call — `audits/READY.md` documents the prereqs)

**RFP #9: External security audit.** Audit-prep is complete (`audits/THREAT_MODEL.md` enumerates 10 threats with mitigation + test cross-references; `audits/SCOPE.md` lists in-scope vs out-of-scope; `audits/handoff-package/README.md` describes the bundle structure). External engagement is paid work with OtterSec or a fallback firm (Veridise, Runtime Verification, CoinFabrik, QuarksLab, Coinspect — all SDF-blessed). Estimated $10-30k, 1-2 week cycle.

### Human-required (your accounts/keys/funds — `HANDOFF.md` is the 7-step checklist)

**RFP #10: Production release + hosted endpoint + mainnet canary.** All the infrastructure is written and tested locally:

| Item | Needs |
|---|---|
| Push branch to GitHub remote | Your GitHub org/repo + `git push` |
| GHA secrets | Your `CARGO_REGISTRY_TOKEN`, `NPM_TOKEN`, `RELEASE_GPG_KEY`, `OZ_POLICY_MCP_TOKEN` |
| Hosted MCP endpoint | Your Fly.io account + DNS (Fly.io blueprint in `infra/fly/` is ready to `./deploy.sh`) |
| Mainnet canary | Real mainnet XLM (~5-10 XLM), mainnet keypair, runbook in `docs/mainnet-readiness.md` |
| GPG signing key | Your `gpg --gen-key` (currently a placeholder in SECURITY.md) |
| v1.0.0 tag push | After all above; release workflow runs automatically |

Placeholder values currently in the worktree:
- Org name: substituted to `oz-policy-builder` (your call, change if needed)
- Security email: `security@example.com` (RFC-2606 reserved DUMMY, marked TODO)
- Conduct email: `conduct@example.com` (same)
- GPG fingerprint: literal `<placeholder>` (can't dummy a real fingerprint)

### Stashed work (decision pending)

One git stash (`stash@{0}: rejected-polish-agent-audit-lints-additions`) — 627 lines + 13 tests adding 2 additional audit lint rules (`no_recursive_calls`, `ttl_bump_on_persistent_write`). Was a polish dispatch you rejected mid-flight; preserved in case you want to keep it. `git stash pop` to apply, `git stash drop` to discard.

### Minor known gaps (non-blocking, documented in code)

- `wallet-adapter/examples/03-soroswap-bounded-headless.ts` has the corpus referenced but the script body is an UNWIRED placeholder (header comment explicitly notes this; wiring would mirror `01`/`02-headless.ts`)
- The 3rd fuzz harness (`enforce_arbitrary_ctx`) was skipped in Phase 9 because `soroban-env-host` instantiation is too slow for libFuzzer feedback (~100ms per iter); documented in `audits/READY.md` as a Phase 9 follow-up
- `wallet-adapter/install.ts` carries a stellar-sdk 12.3.0 V4-meta fallback (raw-RPC + hand-rolled ScVal scanner) because the SDK throws "Bad union switch: 4" on Protocol-23 result-meta XDR; closes when stellar-sdk fixes upstream

---

## 8. How to use it locally

Three paths, all working from `.worktrees/phase-1-foundations/`. No external accounts needed.

### Path A — CLI (fastest, no AI client needed)

```bash
# (1) Synthesize a policy from the frozen Blend recording
cargo run -p oz-policy-cli -- synthesize \
  walkthroughs/01-blend-yield/expected-recording.json \
  --mode auto --tightness exact --lifetime 432000 \
  --rule-name "blend-claim" > /tmp/spec.json
cat /tmp/spec.json | jq .

# (2) Generate the Soroban contract WASM
cargo run -p oz-policy-cli -- codegen /tmp/spec.json --out /tmp/out
cat /tmp/out/slot_0/wasm_hash.txt

# (3) Simulate permit + deny vectors
cargo run -p oz-policy-cli -- simulate \
  /tmp/spec.json \
  walkthroughs/01-blend-yield/expected-recording.json \
  --wasm-dir /tmp/out --out /tmp/sim.json
cat /tmp/sim.json | jq '{permit, deny: [.deny_results[] | {name, passed}]}'
```

### Path B — MCP via Claude Desktop

Pre-build for fast startup:
```bash
cargo build --release -p oz-policy-mcp
```

Edit `~/Library/Application Support/Claude/claude_desktop_config.json`:
```json
{
  "mcpServers": {
    "oz-policy-builder": {
      "command": "/Users/mert/Projects/oz-account-policy-builder/.worktrees/phase-1-foundations/target/release/oz-policy-mcp",
      "args": ["--stdio"]
    }
  }
}
```

Restart Claude Desktop. Try the prompt:
> Record this Stellar testnet transaction and synthesize a policy: `5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e`. Use the testnet RPC. Then simulate it.

Claude will call `record_transaction` → `synthesize_policy` → `simulate_policy` in sequence, showing each result inline.

Configs for Cursor / Cline / Continue / mcp-cli are in `tests/mcp-clients/`.

### Path C — Programmatic via wallet-adapter

```bash
cd wallet-adapter
pnpm test                                        # mocked: 78 passed
export PHASE7_SA_OWNER_SECRET=$(stellar keys show sa-owner-p7r2 --network testnet)
INTEGRATION=1 pnpm test phase7_integration       # real testnet install
```

Real worked example in `wallet-adapter/src/phase7_integration.test.ts`: build envelope → sign with passkey-kit → submit → poll → verifyInstall.

---

## 9. File map (where to find what)

```
.worktrees/phase-1-foundations/
├── README.md                       Project overview, 1-page elevator pitch
├── STATUS.md                       Project status snapshot (this is the source of truth)
├── HANDOFF.md                      7-step human-required checklist for v1.0.0
├── completion.md                   THIS FILE
├── plan.md                         Original 10-phase implementation plan (95KB, audit trail)
├── CHANGELOG.md                    Per-release log (currently [Unreleased])
├── SECURITY.md                     Disclosure policy
├── CONTRIBUTING.md                 PR workflow + 6 required CI gates + DCO
├── CODE_OF_CONDUCT.md              Contributor Covenant 2.1 by reference
├── LICENSE-APACHE                  Apache 2.0 verbatim
├── NOTICE                          MIT upstream acknowledgments (OZ, pollywallet, passkey-kit)
│
├── Cargo.toml                      Workspace + pinned versions + overflow-checks profile
├── rust-toolchain.toml             Rust 1.89.0 stable
├── deny.toml                       cargo-deny config (licenses + advisories)
├── Cargo.lock                      Committed for reproducibility
│
├── crates/
│   ├── oz-policy-core/             PolicySpec IR + decision tree + sep41 + Error + ArgValue + Recording
│   ├── oz-policy-recorder/         Soroban RPC + XDR decoder + tests + fuzz
│   ├── oz-policy-codegen/          askama templates + sandbox + audit_lints + fuzz
│   ├── oz-policy-simhost/          soroban-env-host harness + permit + deny + run + vendored minimal SA WASM
│   ├── oz-policy-installer/        envelope builder + preflight + registry + Phase 2 completion gate
│   ├── oz-policy-mcp/              rmcp server + 5 tools + store + resources + prompts + verify_chain (1082 LOC on-chain readback)
│   └── oz-policy-cli/              Thin CLI: record / synthesize / codegen / simulate / prepare-install / verify-install
│
├── templates/                      askama .rs.jinja files (base + 7 constraint primitives)
│
├── wallet-adapter/                 TypeScript pnpm package
│   ├── src/
│   │   ├── sep43.ts                SEP-43 types + WalletError
│   │   ├── adapters/{freighter,passkey}.ts
│   │   ├── install.ts              installPolicy with ozAuthPayloadEncoder hook
│   │   ├── verify.ts               verifyInstall via MCP subprocess
│   │   ├── oz_smart_account_auth.ts AuthPayload encoder (closed Phase 7 BLOCKER)
│   │   └── phase7_integration.test.ts  Live testnet end-to-end
│   └── examples/                   3 headless example scripts (01/02 wired, 03 unwired placeholder)
│
├── skills/oz-policy-builder/       Anthropic Agent Skills package
│   ├── SKILL.md                    Frontmatter + progressive disclosure workflow
│   ├── references/                 Cheatsheet + walkthrough refs + error codes
│   ├── scripts/                    Python clarification + summary scripts
│   ├── evals/                      3 walkthrough YAML evals
│   └── flat/                       prompt.md + tools.json (non-Claude clients)
│
├── walkthroughs/
│   ├── 01-blend-yield/             Phase 1 frozen corpus + Phase 8 spec/WASM/sim/envelope
│   ├── 02-sep41-subscription/      Phase 2 frozen corpus + Phase 8 spec/sim/envelope (track-A, no WASM)
│   ├── 03-soroswap-bounded/        Phase 8 frozen corpus (real testnet swap captured)
│   ├── phase3-codegen-fixture/     Phase 3 minimal fixture for codegen completion gate
│   ├── phase7-testnet-install/     Deployed addresses + install-result + RESOLVED BLOCKER
│   └── canonicalize-sim-report.sh
│
├── docs/                           11-file cookbook + 4 decision logs
│   ├── concepts.md, install.md, operations.md, security.md, wallets.md, mcp-clients.md, upstream.md, reproducible-build.md, mainnet-readiness.md
│   ├── walkthroughs/{01,02,03}.md  Long-form walkthrough narratives
│   └── (decision logs) oz-internal-shapes.md, codegen-dependency-mode.md, mcp-sdk-decision.md, rpc-retention-decision.md, simhost-smart-account-source.md
│
├── audits/                         Audit-prep package
│   ├── SCOPE.md, THREAT_MODEL.md, READY.md, index.md
│   └── handoff-package/README.md
│
├── infra/                          Hosting blueprint (human deploys)
│   ├── README.md
│   └── fly/{fly.toml, Dockerfile.runtime, deploy.sh}
│
├── ci/Dockerfile                   Reproducible-build base image (rust:1.89.0 + stellar-cli 25.1.0)
│
├── scripts/
│   ├── reproducible-build.sh       Verifies 3 pinned WASM hashes byte-equally
│   └── sandbox-profile-macos.sb    Apple Seatbelt profile for codegen sandbox
│
├── tests/mcp-clients/              Example configs for Claude Desktop / Cursor / Cline / Continue / mcp-cli
│
└── .github/workflows/
    ├── ci.yml                      fmt + clippy + nextest + deny on every PR
    ├── walkthroughs.yml            Re-derives 3 walkthrough corpora byte-equally
    ├── reproducible-build.yml      On release tag; manifest emission
    ├── fuzz-nightly.yml            cargo-fuzz spec + recording targets
    ├── release.yml                 Tag-triggered; binaries + crates.io + npm + signed SHA256SUMS
    └── skills-lint.yml             SKILL.md frontmatter + eval YAML validator
```

---

## 10. Pinned versions (verified)

### Languages / toolchain

| Component | Version | Source verified |
|---|---|---|
| Rust | 1.89.0 stable | `rust-toolchain.toml` |
| Node.js | 22.x | `wallet-adapter/package.json` engines |
| pnpm | 10.x | corepack |
| TypeScript | 5.6.3 | `wallet-adapter/package.json` |

### Stellar / Soroban

| Crate | Pin |
|---|---|
| `stellar-accounts` | `=0.7.1` (license MIT) |
| `soroban-sdk` | `=25.3.0` |
| `soroban-env-host` | `=25.0.1` |
| `stellar-rpc-client` | `=25.1.0` |
| `stellar-xdr` | `=25.0.0` |
| `stellar-cli` (CI) | `v25.1.0` (wasm-opt 0.116.1 embedded) |
| `@stellar/stellar-sdk` (TS) | `12.3.0` |
| `@stellar/freighter-api` | `=6.0.1` (Apache-2.0) |
| `passkey-kit` | `=0.12.0` (MIT) |

### Toolchain

| Crate | Pin |
|---|---|
| `askama` | `=0.16.0` |
| `rmcp` | `=1.7.0` (MCP spec 2025-11-25) |
| `schemars` | `=1.0` |
| `proptest` | `=1.11.0` |
| `cargo-nextest` | `=0.9.128` (MSRV-compatible with rustc 1.89.0) |
| `cargo-deny` | `=0.19.6` |
| `cargo-fuzz` | `=0.13.1` |
| `cargo-audit` | latest stable |

---

## 11. License + attribution

This project is **Apache-2.0**. See `LICENSE-APACHE` for verbatim text.

### MIT upstream acknowledgments (`NOTICE`)

- **OpenZeppelin `stellar-contracts`** (MIT) — the smart-account framework we build atop. `docs/oz-internal-shapes.md` quotes verbatim from `v0.7.1`.
- **`kalepail/pollywallet`** (Apache-2.0) — strategic prior art for the record→synthesize approach. We adopted the deterministic-schema-as-contract pattern, kept the sandboxed `cargo build` shape, and replaced the LLM-only synthesis path with deterministic decision-tree + audit-bounded templates.
- **`passkey-kit`** (MIT) — programmatic Stellar smart-wallet SDK by kalepail; powers the headless wallet path.

Dual-license interaction documented in `NOTICE`.

### Decision logs (audit trail)

Every non-obvious technical decision has a written record:
- `docs/codegen-dependency-mode.md` — why we link `stellar-accounts` as a library vs the pollywallet "copy the trait" approach
- `docs/mcp-sdk-decision.md` — why `rmcp 1.7.0`
- `docs/rpc-retention-decision.md` — why 24h public RPC + 12h CI cadence vs private RPC
- `docs/simhost-smart-account-source.md` — why a minimal vendored SA WASM vs upstream example
- `docs/oz-internal-shapes.md` — verbatim OZ source extracts with v0.7.1 line refs

---

## 12. What's next

Roughly in order of unblock dependency:

1. **You substitute final org/email/GPG values** (Step 1 of `HANDOFF.md`) — replace the marked dummies (`example.com`, `<placeholder>`) with real values. 5 minutes.
2. **Push to GitHub** (`git push origin phase-1-foundations`). Unblocks: CI workflows actually run, audit firms can reference a fixed commit SHA.
3. **Deploy hosted MCP endpoint** via `infra/fly/deploy.sh` — needs your Fly.io account.
4. **External audit engagement** with OtterSec (or fallback). Engage at `audits/handoff-package/`. Estimated $10-30k, 1-2 weeks.
5. **Mainnet canary** per `docs/mainnet-readiness.md`. ~5-10 mainnet XLM.
6. **v1.0.0 release** — tag `v1.0.0` and push. The GHA workflow handles binaries + crates.io publish + npm publish + signed SHA256SUMS.

Optional polish work I can do autonomously if you authorize it:
- Wire `wallet-adapter/examples/03-soroswap-bounded-headless.ts` body (mirror 01/02)
- Pop the stashed `no_recursive_calls` + `ttl_bump_on_persistent_write` audit lints (`stash@{0}`)
- Refresh `docs/oz-internal-shapes.md` line refs against a fresh `stellar-contracts@v0.7.1` checkout
- Add structural tests for the V4-meta fallback path in `wallet-adapter/install.ts`

---

## 13. One-line summary

**A working, tested, documented Stellar smart-account policy synthesizer with a real end-to-end install demonstrated on testnet (tx `038583fa4c95654c9a26323702b86729e084357d47ab169fa22a77d821ce90bb`), 363 passing tests, 30+ documentation files, audit-ready package, release pipeline gated on your accounts — 8 of 10 RFP deliverables complete, 1 deferred by you, 1 awaiting your tokens and keys.**

---

*Generated 2026-06-03 from worktree HEAD `84ab19c` on branch `phase-1-foundations`. Source of truth for current state is `git log` + `STATUS.md`; this document is a synthesis. Apache-2.0 licensed.*
