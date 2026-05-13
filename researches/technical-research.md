# OZ Accounts Policy Builder — Internal Technical Research Report

**Document type:** Internal technical research (will inform — but is not — a future grant proposal).
**Subject:** AI-assisted toolkit (MCP server + agent skill + developer tooling) that synthesizes OpenZeppelin smart-account context rules and policies from observed or simulated Stellar transactions.
**License posture:** Apache 2.0.

---

## TL;DR

- The proposed "record-and-generate" workflow is technically feasible on top of the published OpenZeppelin `stellar-accounts` library (crates.io pin `=0.7.1`, repo HEAD `v0.7.0-rc.1` at commit 239a2a7). The current `Policy` trait — `install` / `enforce` / `uninstall`, with `can_enforce` removed in RC v0.7 — gives us a clean, deterministic surface to target with both primitive composition and codegen.
- `kalepail/pollywallet` (Apache-2.0; 48 commits; TypeScript 81.9% / JavaScript 14.2% / Rust 3.0%) already documents a six-phase Policy Builder pipeline in `PLAN.md` — transaction analysis → deterministic JSON schema (`pollywallet-policy/v0`, 8 rule types) → AI codegen via Cloudflare Workers AI `@cf/moonshotai/kimi-k2.5` → sandboxed `cargo build` → optimize+deploy → manage. The Policy Builder is **planned, not yet shipping**: today's repo only ships passkey-backed wallet creation, deterministic contract derivation, Friendbot funding, and XLM sends via OpenZeppelin Channels relayer. The schema, decision-tree pattern, and sandbox concept are excellent inputs to adopt; we replace its Cloudflare-only/LLM-only runtime and add an MCP-first integration layer.
- The right shape for the new project is a Rust workspace shipping (a) a recorder consuming Soroban RPC `getTransaction` + `simulateTransaction`; (b) a two-track synthesizer (Track A: parameterize `simple_threshold` / `weighted_threshold` / `spending_limit`; Track B: `askama` template-driven Rust codegen for net-new policy contracts); (c) an MCP server using the official Rust `rmcp` SDK over both STDIO and Streamable HTTP (MCP spec 2025-11-25); (d) an Anthropic Agent Skills `SKILL.md` portable across MCP clients; (e) a local `soroban-env-host` simulation harness with `proptest`-based deny-vector generation; (f) a SEP-43 wallet adapter targeting Freighter as primary and passkey-kit for programmatic flows. Deployment is always a separate, wallet-signed step — never auto-deployed.

---

## Key Findings

1. **Policy trait API is locked in and minimal.** Verbatim from the official policies doc page:

   ```rust
   pub trait Policy {
       type AccountParams: FromVal<Env, Val>;
       fn enforce(e: &Env, context: Context, authenticated_signers: Vec<Signer>,
                  context_rule: ContextRule, smart_account: Address);
       fn install(e: &Env, install_params: Self::AccountParams,
                  context_rule: ContextRule, smart_account: Address);
       fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address);
   }
   ```
   Up to **5** policies per context rule, executed in insertion order; any panic reverts. Stateful policies must `smart_account.require_auth()` in every state-mutating hook and segregate persistent storage by `(smart_account, context_rule_id)`.

2. **Existing primitives cover a meaningful slice of recordings but not all.** `simple_threshold` (single `u32` threshold), `weighted_threshold` (signer→weight map + threshold), `spending_limit` (rolling window tracked in persistent state per `(smart_account, context_rule_id)`). The OZ RC v0.7.0 differential audit confirms `spending_limit` decrements the remaining limit on `enforce` when the target fn is `transfer` with ≥3 args; the audit also disclosed and PR-#649 fixed a multi-token confusion when `spending_limit` is used under `Default` context rules — install now rejects `Default`.

3. **Critical OZ audit caveats the synthesizer must respect.**
   - `ContextRuleType::CallContract(Address)` matches only on target address — *no* fn-name or arg filtering at the rule level. Function-allowlist is a policy responsibility.
   - Signer-set additions/removals do not auto-update threshold policies' stored thresholds; the synthesizer must surface this and emit joint-update instructions.
   - Sponsor-side `context_rule_ids` substitution was fixable only by binding rule IDs into the signed payload (OZ PR #655). Generated policies do not need additional mitigation for this; the synthesizer should refuse to install onto smart accounts that predate this fix.

4. **PollyWallet PLAN.md is the strongest prior-art roadmap to adopt.** Its 8 rule types — `threshold`, `spending_limit`, `allowlist`, `blocklist`, `time_lock`, `function_whitelist`, `max_single_transfer`, `daily_tx_count` — are a sound starting taxonomy. Its decision to make a **deterministic JSON schema** the bridge between UI and codegen (so the same schema always produces functionally equivalent code) is the right architectural choice. **Adopt** the schema concept, phase decomposition, and sandboxed-`cargo build` pattern. **Extend** with explicit `synthesis_mode` (`compose_existing` vs `generate_new` vs `auto`), sequence-ordering and argument-pattern primitives, and proptest-based deny-vector generation. **Replace** Cloudflare-only runtime, browser `localStorage` wallet state, deterministic deployer seed, hard-coded testnet, and single-model lock-in (Kimi K2.5 priced at $0.60/M input, $0.10/M cached, $3.00/M output per Cloudflare Workers AI pricing; deprecated May 30 2026 per Cloudflare changelog).

5. **MCP is mature enough to standardize on.** Per the *Exploring the Future of MCP Transports* post by Kurtis Van Gent and Shaun Smith (Transport WG Maintainers, blog.modelcontextprotocol.io, 19 Dec 2025): "To ensure a minimum compatibility baseline across the ecosystem, MCP will continue to support only two official transports: STDIO for local deployments and Streamable HTTP for remote deployments." The deprecated HTTP+SSE transport (2024-11-05) is superseded. The Rust `rmcp` SDK is the right choice for a Rust synthesizer.

6. **`getTransaction` retention shapes ingest design.** Per Stellar Docs: "The stellar-rpc system maintains a restricted history of recently processed transactions, with the default retention window set at 24 hours...we do not recommend values longer than 7 days." We must therefore support both hash-ingest (recent) and envelope-ingest (`simulateTransaction`) paths, and recommend Hubble BigQuery or extended-retention private RPC for older flows.

7. **Soroban runtime constraints are well-understood and within budget.** Three storage tiers (`temporary`, `persistent`, `instance`); OZ library manages TTL for temporary/persistent but not instance. Multidimensional fee model: instructions, ledger reads/writes, IO bytes, tx size, events. Per Stellar's Protocol 23 mainnet vote announcement (3 Sept 2025), CAP-0065 (reusable module cache) and CAP-0066 (in-memory state) cut Soroban invocation cost broadly — "User fees will be lower due to the elimination of parsing, validation, and translation costs" and "This removes disk reads entirely from smart contract invocations, significantly improving throughput and reducing fees." This makes 5-policy composition realistic.

8. **The audit story has named, qualified counterparties.** Per SDF's *Soroban Audit Bank* blog post: "six top-tier audit firms — Ottersec, Veridise, Runtime Verification, CoinFabrik, QuarksLab, and Coinspect." OtterSec has published the Soroswap core audit. OpenZeppelin's own team performed the stellar-contracts RC v0.7.0 audit. Certora handles formal verification work on OZ stellar-contracts.

---

## Details

### 1. Problem statement and motivation

Authoring an OZ smart-account policy correctly requires the author to (a) implement the three-method `Policy` trait; (b) segregate persistent storage by `(smart_account, context_rule_id)`; (c) `require_auth` from the smart account in every state-mutating hook; (d) live inside Soroban's instruction budgets, storage tiers, and TTL rules; (e) compose with up to 5 policies, knowing any panic reverts the whole tx; (f) defend against the audit-class issues OZ already disclosed (CallContract scope, signer divergence, sponsor `context_rule_ids` substitution, multi-token confusion under `Default`).

This is the wrong abstraction for end users and AI agents. The right inversion is *record-and-generate*: a user executes the representative flow once; the tool infers the minimum policy set permitting exactly that flow. The agent then receives a tightly-scoped C-address signer authority instead of full account keys.

### 2. OZ background (deep)

Three composable elements per `docs.openzeppelin.com/stellar-contracts/accounts/smart-account`: **Context Rules** (routing entries binding context type, lifetime via `valid_until: Option<u32>` ledger, signers, policies — example: `Subscription: dapp pubkey can withdraw 100 USDC every month for one year`); **Signers** (delegated = any Soroban address; external = raw keys via `Verifier` contracts for ed25519, secp256r1/WebAuthn, BLS); **Policies** (external contracts, ≤5 per rule).

`__check_auth` at v0.7+ takes `signatures: AuthPayload` (renamed from `Signatures` in v0.6) and `auth_contexts: Vec<Context>`. `AuthPayload` carries `context_rule_ids` — every client now explicitly selects which rule to validate against; the older newest-first auto-iteration path was removed.

Three policy primitives ship in `packages/accounts/src/policies/`:

- **`simple_threshold`** — `AccountParams = { threshold: u32 }`. `enforce` panics if `authenticated_signers.len() < threshold`. Storage: u32 threshold per `(smart_account, context_rule_id)`. Audit-caveat: threshold doesn't auto-track signer-set changes.
- **`weighted_threshold`** — signer→weight map + total threshold. `enforce` sums authenticated signers' weights, panics below threshold. Exact field names verified by direct repo inspection in Phase 1. Audit-caveat: weights drift from `ContextRule.signers` if not jointly updated.
- **`spending_limit`** — per OZ RC v0.7.0 audit verbatim: "The remaining limit is decreased on each policy enforce call where the target function name equals `transfer` and that function has at least 3 arguments." Persistent state per `(smart_account, context_rule_id)` holds at least `{ limit: i128, period, remaining, window_start }`. The docs page calls the period "Duration in seconds" but Soroban convention favors ledger counts — **flagged TBD** for direct source verification. Audit fix in PR #649: `install` now rejects `Default` context rules to prevent multi-token confusion.

### 3. PollyWallet teardown

Repo `github.com/kalepail/pollywallet` (Apache 2.0; 0 stars / 0 forks; 48 commits; single-author, Tyler van der Hoeven, SDF). Stack: TanStack Start + React 19, Cloudflare Vite plugin / Wrangler, `@stellar/stellar-sdk`, OpenZeppelin Channels relayer, SimpleWebAuthn, Tailwind 4, Vitest. `stellar-contracts` included as submodule pinned to commit `187ad25`.

PLAN.md (verbatim 536 lines, 21.6 KB) describes a six-phase pipeline: (1) `tx-analyzer.ts` calling Soroban RPC and decoding XDR via `@stellar/stellar-sdk` to extract `{contractAddress, functionName, args, signers, amounts}`; (2) deterministic JSON schema `pollywallet-policy/v0` with the 8 rule types listed in Key Finding 4; (3) AI codegen via `env.AI.run("@cf/moonshotai/kimi-k2.5", {messages, stream: true})` with `x-session-affinity` for prompt-caching and `queueRequest: true` for batch; (4) sandbox testing via `@cloudflare/sandbox` on `standard-2` (1 vCPU, 6 GiB, 12 GB disk) with custom Dockerfile preloading Rust + stellar-cli, using WebSocket transport to dodge the 1000-subrequest cap; (5) `stellar contract build` → `stellar contract optimize` → upload → deploy → `add_policy()` via Channels relayer; (6) list/view/update/remove policies.

Shipping today (verbatim README "What It Does"): passkey-backed wallet creation in browser; deterministic contract address derivation from `(deployer pubkey, network passphrase, credential-ID salt)`; wallet metadata in `localStorage` under key `pollywallet:wallet`; Friendbot funding via temporary Stellar account; XLM sends using passkey-signed AuthPayload and Channels relayer.

**Adopt:** the deterministic JSON-schema-as-contract-between-UI-and-codegen pattern; phase decomposition; sandboxed `cargo build` pattern; pnpm-workspace + generated TS bindings layout. **Extend:** add `synthesis_mode` flag; add sequence-ordering and argument-pattern primitives; add proptest deny-vector generation; treat LLM as a clarification-surface tool only, not a codegen primary path. **Replace:** Cloudflare-only runtime → multi-runtime including local; `localStorage` wallet state → wallet-owned; deterministic deployer seed → user wallet signing; testnet-only → testnet+mainnet with guardrails; Kimi K2.5 lock-in → model-agnostic prompting (especially given Kimi K2.5's scheduled May 30 2026 deprecation per Cloudflare changelog).

Demo video (https://youtu.be/vmFnCtkqQJA): not directly accessible by the research tooling — **TBD: view at implementation time**. Based on PLAN.md and README, it almost certainly shows passkey creation → smart-account deployment via Channels → Friendbot funding → outgoing transfer, *not* end-to-end Policy Builder (which is unbuilt).

### 4. Architecture proposal

```mermaid
flowchart TB
  Client[MCP Client: Claude Desktop, Cursor, Cline, Continue, generic mcp-cli]
    --> Server[MCP Server, Rust rmcp, STDIO + Streamable HTTP]
  Server --> Recorder[Recorder: stellar-rpc-client + stellar-xdr]
  Server --> Synth[Synthesizer: decision tree → PolicySpec]
  Synth --> Track A[Track A: compose simple_threshold/weighted_threshold/spending_limit]
  Synth --> Track B[Track B: askama codegen → scratch crate]
  Track B --> Build[Sandboxed cargo build wasm32 + stellar contract optimize]
  Build --> Sim[Simulation: soroban-env-host + proptest]
  Sim --> Export[export_policy: Rust + WASM + install envelope]
  Export --> Wallet[SEP-43 wallet: Freighter primary, passkey-kit secondary]
```

Component languages: synthesizer + recorder + MCP server + simulation in Rust (single static binary, deterministic, native XDR via `stellar-xdr`, shares types with `soroban-sdk`); codegen via `askama` templates (compile-time-checked, byte-deterministic, reviewer-readable output matching reference impls — accepted trade-off: not type-checked-by-construction like `syn`/`quote`, compensated by always compiling in sandbox); agent skill as Anthropic Agent Skills `SKILL.md` (portable open standard); wallet adapter in TypeScript (SEP-43 lives in the browser).

Data flow: input (hash on Stellar / envelope XDR) → record (RPC `getTransaction` or `simulateTransaction` → decode XDR → normalize to `Recording`) → synthesize (decision tree → `PolicySpec` IR) → render (Track A param structs OR Track B codegen) → compile (cargo + wasm-opt) → simulate (permit + deny vectors via `soroban-env-host`) → review (return spec + Rust + WASM hash + sim results) → wallet signs install. **No auto-deployment in any tool.**

Decision rule: if recorded constraints decompose into ≤5 slices each expressible by one OZ primitive, compose; else fall back to codegen for the inexpressible slices and compose with primitives for the rest (additive, not exclusive).

### 5. Recording layer

`getTransaction(hash)` returns `status`, `ledger`, `envelopeXdr`, `resultXdr`, `resultMetaXdr`, `events.contractEvents` (nested `Vec<Vec<ContractEvent>>` per operation), `events.transactionEvents`, `events.diagnosticEvents` (only when RPC has `ENABLE_SOROBAN_DIAGNOSTIC_EVENTS`). `simulateTransaction(envelope, resourceConfig?)` returns `transactionData`, `minResourceFee`, `events`, `results[0].auth`, `results[0].retval`. "Forked state" in Soroban means replaying captured `LedgerEntry`s inside `soroban-env-host` rather than via RPC — true off-chain forks are a `stellar contract invoke` / quickstart pattern.

Extracted: contract IDs from every `Context::Contract`; fn names from `Symbol fn_name`; ScVal args converted to typed `ArgValue` enum (`Address`, `I128`, `U32`, `U64`, `Bytes`, `Symbol`, `Vec`, `Map`); `SorobanAuthorizationEntry { credentials, root_invocation }` walked for nested auth tree; state changes from `resultMetaXdr.v3.operations[].changes`; SEP-41 token movements from `transfer/mint/burn/clawback` event topics.

Library choice: **`stellar-rpc-client` + `stellar-xdr` (Rust)** for the recorder — auto-generated from canonical XDR definitions, stays in lockstep with protocol upgrades, avoids JSON round-trip. JS SDK reserved for the wallet adapter only.

### 6. Synthesizer — both tracks (equal depth)

**Track A — configure existing primitives.** Each primitive's `AccountParams` shape, storage layout, and enforce panic conditions are documented in §2 above. Composition rules: two thresholds on one rule are redundant; threshold + spending_limit is the canonical pair; rule signers and threshold/weights must stay in sync via joint-update scripts the synthesizer emits. Parameter derivation: for `spending_limit`, observed amount A → default cap = A (exact) with `(A, A·1.5, A·2)` slider; default period = 7-day ledger window (≈ `17280·7` ledgers) with daily/monthly alternatives. For thresholds, default = strict signer count with `count − 1` slider.

**Track B — generate new policy contracts.** Codegen via `askama` (chosen for readability, byte-determinism, compile-time template syntax checking; trade-off accepted vs `syn`/`quote`). Generated contract skeleton implements the full Policy trait, has `#[contracttype] InstallParams` parameterized by constraint primitives present in the spec, a `StorageKey` enum keyed by `(Address, u32)` for segregation, `#[contracterror]` with one variant per constraint primitive, and `#[contractevent]` for `PolicyInstalled` / `PolicyEnforced` / `PolicyUninstalled`. `install` and `uninstall` set/remove persistent entries under `(smart_account, context_rule_id)`; `enforce` performs `smart_account.require_auth()` then runs only the constraint branches the spec requires (omitted branches don't bloat WASM).

Supported constraint primitives: function allowlist (`Vec<Symbol>` exact match on `fn_name`); argument-pattern matching (typed slot match — e.g., `args[1] == merchant_addr`, or amount range on `args[2]`); amount ranges (min/max on i128); asset allowlist (`Vec<Address>` of token contracts); time windows (ledger-sequence based); call-frequency limits (max N per `window_ledgers`); sequence ordering (state-machine phases stored per `(smart_account, context_rule_id)`).

Storage layout: all stateful primitives key by `(smart_account, context_rule_id)` in persistent storage; TTL bumped on every `enforce` to the context rule's `valid_until` ceiling, or a documented re-bump cadence if `valid_until = None`. Temporary tier used only for short-lived nonces (none in current primitives).

Compilation pipeline: synthesizer writes scratch crate referencing pinned `soroban-sdk` + `stellar-accounts =0.7.1`; `cargo build --release --target wasm32-unknown-unknown`; `stellar contract optimize` (wasm-opt under the hood, version pinned for reproducibility).

Verification: `cargo clippy -- -D warnings`; `cargo deny check`; auto-generated test crate; `proptest` strategies derived from the spec; spec-level lint requiring every constraint to produce at least one panic branch.

### 7. MCP server (framework-agnostic)

Spec revision: 2025-11-25. Transports: STDIO (subprocess for IDE clients) and Streamable HTTP (long-running service for remote/CI). The deprecated 2024-11-05 HTTP+SSE transport is not implemented. SDK: Rust `rmcp` (modelcontextprotocol/rust-sdk). Justification: single-binary distribution, co-located with synthesizer, native protocol support; trade-off: thinner example corpus than TS SDK.

Tools exposed (with JSON schemas in full report):
- `record_transaction` — by hash + network OR envelope XDR + rpc_url + instruction_leeway.
- `synthesize_policy` — `recording_id`, `tightness ∈ {exact, small_margin, loose}`, `lifetime_ledgers`, optional `delegated_signer`, `mode ∈ {auto, compose_only, codegen_only}`.
- `simulate_policy` — `spec_id`, optional `extra_deny_vectors`.
- `export_policy` — `spec_id`, `smart_account`, `format ∈ {rust_source, wasm, install_envelope, all}`.
- `verify_install` — `smart_account`, `context_rule_id`, optional `spec_id` for diff-style validation.

Resources: `recording://<id>`, `spec://<id>`, `artifact://<id>/source.rs`, `artifact://<id>/policy.wasm`, `artifact://<id>/install_envelope.xdr`. Prompt templates: `record_and_explain`, `synthesize_subscription`, `synthesize_delegated_trading`.

Determinism: every tool's output is a pure function of its inputs. Codegen via `askama` is byte-deterministic; build determinism via pinned `rust-toolchain.toml`, `Cargo.lock`, pinned `wasm-opt`.

Error codes: `E_RECORDER_HASH_NOT_FOUND`, `E_RECORDER_SIM_FAILED`, `E_SYNTH_NOT_EXPRESSIBLE`, `E_CODEGEN_COMPILE_FAILED`, `E_SIM_PERMIT_DENIED`, `E_SIM_DENY_PASSED`, `E_VERIFY_DRIFT`.

Comparison with Cloudflare Agent Setup: adopt the `createMcpHandler`-style abstraction and bearer-token-over-TLS auth posture; do *not* adopt Worker-AI-only inference or implicit per-Worker sticky sessions (the MCP Transport WG roadmap by Kurtis Van Gent and Shaun Smith — *Exploring the Future of MCP Transports*, blog.modelcontextprotocol.io 19 Dec 2025 — explicitly de-emphasizes transport-level sessions in favor of application-layer session semantics).

Auth/sandboxing/secrets: local stdio = parent-process trust; remote HTTP = bearer-token over TLS, request-independent validation; cargo builds in `bubblewrap`/container with no network except a cached crates.io mirror; secrets passed as env/headers and held in-memory only.

Framework-agnostic posture: zero client-specific assumptions; CI matrix tests against Claude Desktop, Cursor, Cline, Continue, and `mcp-cli`.

### 8. Agent skill design

Format: Anthropic Agent Skills open standard (`SKILL.md` with YAML frontmatter `name` + `description` required; optional `references/`, `scripts/`, `assets/`, `evals/`). Progressive disclosure: only frontmatter loads at startup. Sketch frontmatter:

```yaml
name: oz-policy-builder
description: Records a Stellar transaction (by hash or simulation) and generates the
  minimum OpenZeppelin smart-account context rule + policies that would permit exactly
  that flow. Use whenever a user wants to authorize a third party (human or AI agent)
  to repeat a specific Stellar/Soroban operation under tight bounds — e.g., "let this
  agent claim my Blend yield weekly", "authorize this dapp up to 20 USDC monthly",
  "give my trading bot a 100-USDC-per-day Soroswap budget".
```

Workflow: ask mode (hash vs. simulate) → `record_transaction` → summarize → confirm constraint shape with clarifications → `synthesize_policy` → `simulate_policy` (never skip) → `export_policy` → hand to wallet for signature.

Clarification triggers: single observed amount → "cap this at observed, or allow a weekly total?"; delegated signer present → "same address or new agent key?"; Soroswap router invocation → slippage cap (default: observed slippage + 2%); `Default` context rule used → warn and rewrite to `CallContract`.

Portability: ships in two parallel formats — `skill/SKILL.md` (Anthropic standard) and `skill/prompt.md` + `skill/tools.json` (flat-file for non-SKILL.md frameworks).

### 9. Simulation harness

Approach: **local `soroban-env-host` execution**, not RPC `simulateTransaction`, for deterministic reproducibility, custom ledger-state injection, and exact VM-semantic match. RPC simulate is for ingest only.

Permit case: replay recording against candidate WASM; assert `enforce` returns Ok.

Deny-case generator: per constraint primitive, the harness emits boundary mutations — different asset (→ `AssetNotAllowed`), `2×`/`100×` amount (→ `AmountExceedsCap`), `approve` instead of `transfer` (→ `FunctionNotAllowed`), ledger past `window_start + window_ledgers + 1`, swapped sequence ordering (→ `SequenceViolation`), N+1 calls in window (→ `CallFrequencyExceeded`).

Property tests: `proptest` chosen over `quickcheck` because `stellar-contracts` test utilities already integrate `proptest` and its strategy DSL is better suited to typed ScVal generation. Strategies derived from `PolicySpec` so they stay in sync with the constraint set.

User-extensible test vectors: `simulate_policy` accepts `extra_deny_vectors: [{context, expected_error}]` JSON.

### 10. Wallet integration

Survey: **Freighter** (SDF flagship browser extension, open source at `github.com/stellar/freighter`, Soroban signing + `signAuthEntry` + dapp integration, SEP-43 implementation); **Lobstr** (`github.com/Lobstrco/lobstr-browser-extension`, multisig, browser+mobile); **passkey-kit by kalepail** (TypeScript SDK creating smart-account contract wallets with passkeys — not a wallet UI but the most relevant programmatic signer for MCP/agent flows).

SEP-43 (Standard Web Wallet API, Draft v1.2.1 by Piyal Basu, Leigh McCulloch, George Kudrayvtsev, Enrique Arrieta, Orbit Lens): defines `getAddress`, `signTransaction`, `signAuthEntry`, `signMessage`. Error codes: `-1` internal, `-2` external service, `-3` invalid request, `-4` user rejected.

Integration surface: build envelope with `add_context_rule`/`add_policy` → wallet `signTransaction` → submit. Read-only `get_context_rules` for listing. `remove_policy`/`remove_context_rule` for revocation.

**Primary target: Freighter.** Justifications: open source, SDF-stewarded, ships SEP-43, documented dapp API at `developers.stellar.org/docs/build/guides/freighter`. **Secondary: passkey-kit** for the C-address passkey flow and for headless/CI use.

### 11. Walkthroughs (concrete)

**Walkthrough 1 — Blend yield-claim → USDC.** Recorded: `Context::Contract(BlendPool, "claim", […])` then `Context::Contract(CometDex, "swap_exact_tokens_for_tokens", [amount_in, amount_out_min, path=[BLND, USDC], to, deadline])`. Per docs.blend.capital, Blend is "a universal liquidity protocol primitive that enables the permissionless creation of lending pools" (`github.com/blend-capital/blend-contracts` and v2). Synthesized: one `Default` context rule (two contracts involved), `valid_until = now + 1y`, delegated agent signer; a generated policy with function-allowlist `{claim, swap_exact_tokens_for_tokens}`, asset-allowlist `{BLND, USDC, BlendPool, CometDex}`, amount cap `≤ 1.1× observed`, path must equal `[BLND, USDC]`, max 1 claim+swap per 7 days. Simulation covers: 5 deny vectors plus permit replay.

**Walkthrough 2 — SEP-41 subscription billing.** Recorded: `transfer(user_smart_account, merchant, 5_000_000)` on the USDC SAC. Synthesized: rule `CallContract(USDC_SAC)`, `valid_until = now + 12mo`, merchant signer; policy 1 = existing `spending_limit` with `limit=5_000_000`, `period_ledgers≈432_000` (≈30 days) — safe specifically because the rule is `CallContract`, not `Default` (PR-#649 fix); policy 2 = generated, function-allowlist `{transfer}`, recipient must equal merchant addr (prevents drain to attacker).

**Walkthrough 3 — Bounded Soroswap delegated trading.** Per docs.soroswap.finance, the router exposes `swap_exact_tokens_for_tokens(e: Env, amount_in: i128, amount_out_min: i128, path: Vec<Address>, to: Address, deadline: u64) -> Vec<i128>`. Recorded: 100 USDC → XLM swap. Synthesized: rule `CallContract(SoroswapRouter)`, 30-day lifetime, bot signer; generated policy with function-allowlist `{swap_exact_tokens_for_tokens}`, amount_in cap, path ⊂ `{USDC, XLM, BLND}`, `to == user_smart_account`, slippage cap derived as install-time floor on `amount_out_min` (oracle-free), max 5 swaps/day. Deny vectors test each boundary.

### 12. Security / audit

Audit scope = synthesizer logic *itself*, not sample outputs. Reasoning: synthesizer bugs recur across every generated policy; auditing a synthesizer + coverage matrix catches the class, not the instance.

Audit firms: per SDF's *Soroban Audit Bank* blog post — "six top-tier audit firms — Ottersec, Veridise, Runtime Verification, CoinFabrik, QuarksLab, and Coinspect." **Choose OtterSec as primary** for Soroban-specific synthesizer audit (they have published the Soroswap core audit). **Consider Certora** for formal verification of the decision tree as a state machine (they already do formal-verification work on OZ stellar-contracts).

Threat model — synthesizer: spec underspecification (mitigation: clarification prompts + plain-English replay + explicit approval); codegen template bug (mitigation: per-constraint property tests, cross-check against sim deny cases, audit of templates); reproducibility failure (mitigation: pinned toolchain + `wasm-opt` + Cargo.lock); LLM-in-loop non-determinism (mitigation: deterministic template path is primary; LLM is restricted to clarification/summarization surfaces only).

Threat model — generated policies: cross-rule replay (mitigation: `(smart_account, context_rule_id)` storage segregation); i128 overflow (mitigation: `overflow-checks = true` in Cargo profile, matching Blend's guidance verbatim: *"Under no circumstances should the overflow-checks flag be removed otherwise contract math will become unsafe"*); unauthorized state mutation (mitigation: template always emits `smart_account.require_auth()` in mutating hooks); TTL exhaustion (mitigation: enforce-time TTL bumps); sponsor `context_rule_ids` substitution (mitigation: OZ PR #655 fixes at the smart-account layer; synthesizer refuses to install on pre-fix versions).

Fuzz testing: `cargo-fuzz` with libFuzzer harness over `enforce(ctx, signers, rule, smart_account)` with structured ScVal fuzzing, continuous in CI.

Reproducible WASM: `rust-toolchain.toml` pinned; `soroban-sdk` + `stellar-accounts =0.7.1` pinned; `wasm-opt` vendored; `Cargo.lock` checked in; `wasm-hash` published in spec; CI rebuild verifies hash.

### 13. Implementation stack — concrete

| Layer | Choice | Pinned version (research-time) |
|---|---|---|
| Synthesizer / recorder / MCP server / sim | Rust 1.83+ | per `soroban-sdk` minimum |
| Codegen | `askama` | latest 0.x, pinned at impl time |
| MCP SDK | `rmcp` (modelcontextprotocol/rust-sdk) | MCP spec 2025-11-25 |
| Soroban SDK | `soroban-sdk` | matches `stellar-accounts =0.7.1` |
| OZ accounts | `stellar-accounts` | `=0.7.1` (crates.io README pin) |
| Stellar CLI | `stellar-cli` | latest stable, CI matrix |
| Soroban RPC | Stellar RPC (Protocol 23) | current |
| Sim host | `soroban-env-host` | aligned with `soroban-sdk` |
| Property tests | `proptest` | 1.x |
| Fuzz | `cargo-fuzz` | 0.12+ |
| CI | GitHub Actions + cargo-deny + clippy + nextest | — |
| License | Apache 2.0 | matches OZ + PollyWallet |

Pre-implementation verification items: confirm `stellar-accounts 0.7.1` is the latest crates.io publish (README pins it but GitHub release tag is `v0.7.0-rc.1`); confirm `soroban-sdk` minor compatibility; confirm `rmcp` supports MCP spec 2025-11-25.

### 14. RFP-deliverables checklist

| RFP requirement | Component(s) | Decision |
|---|---|---|
| Transaction recording layer | `record_transaction` MCP tool, Rust recorder with `stellar-rpc-client` + `stellar-xdr` | §5 |
| Synthesizer biased toward minimum permissions | Decision tree §6; default `tightness=exact`; function+asset allowlists always emitted | §6 |
| Generated policy code in Rust | `askama` templates, full Policy-trait skeleton | §6 Track B |
| MCP server | Rust `rmcp` SDK, STDIO + Streamable HTTP, five tools | §7 |
| Agent skill | Anthropic Agent Skills `SKILL.md`, flat-file twin for portability | §8 |
| Simulation / dry-run harness | `soroban-env-host` + `proptest`, permit + deny vectors | §9 |
| Wallet integration | SEP-43 adapter → Freighter primary, passkey-kit secondary | §10 |
| Three documented walkthroughs | Blend yield, SEP-41 subscription, Soroswap bounded trading | §11 |
| Configurable composition / generation mode | `synthesize_policy` `mode ∈ {auto, compose_only, codegen_only}` | §7 |
| Code-first, deploy-second | `export_policy` returns artifacts; install always wallet-signed; no auto-deploy | §4 |
| Open source, permissive license | Apache 2.0 | top of doc |

### 15. Dependencies on Stellar infrastructure

OZ `stellar-accounts =0.7.1` pinned; track promotion from `v0.7.0-rc.1` GitHub-release tag to stable. OZ security team is the natural reviewer of synthesizer correctness against policy/context-rule semantics. Protocol 23 already deployed and contains the cost reductions described above (CAP-0065 reusable module cache, CAP-0066 in-memory state). CAP-71 (authentication delegation for custom accounts) — tracking, not a blocker. Coordinate with Freighter team (SDF), passkey-kit maintainer (kalepail), and the C-Address Tooling cohort for shared policy-spec rendering conventions. `stellar-cli` and `soroban-cli` versions pinned in CI matrix; allow user-supplied RPC URL.

### 16. Feasibility, risks, open questions

**Known to work today:** `getTransaction` + `simulateTransaction` semantics; XDR decoding; OZ Policy trait pattern at v0.7.1; `soroban-env-host` in-process execution; `askama` + `cargo build --target wasm32-unknown-unknown`; `stellar contract optimize` with pinned wasm-opt; MCP STDIO + Streamable HTTP.

**Needs prototyping:** decision-tree expressibility predicate (requires test corpus of recorded flows); proptest strategy derivation from `PolicySpec`; cross-version regression matrix for generated WASM vs. `stellar-accounts` minor bumps; Freighter `add_context_rule` builder helper.

**TBDs requiring direct source inspection before implementation:** exact `AccountParams` struct field names for all three primitives; exact `spending_limit` period semantics (`period_ledgers: u32` vs `period_seconds: u64`); whether `spending_limit::AccountParams` includes `token: Address`; exact error-enum variants per primitive; `rmcp` SDK protocol-revision support; current published `stellar-accounts` version; demo video content (https://youtu.be/vmFnCtkqQJA).

### 17. Phased implementation plan (no time/effort estimates)

- **Phase 1 — Foundations.** Clone OZ repo; resolve §16 TBDs; stand up Rust workspace; implement `record_transaction`. **Gate:** round-trip hash → JSON Recording → correctly re-derive contract addresses + ScVal arg types.
- **Phase 2 — Track A synthesizer.** Decision tree for compose-only; PolicySpec for the three OZ primitives; install-envelope builder. **Gate:** Walkthrough 2 produces a valid install envelope.
- **Phase 3 — Track B codegen.** Template library covering all constraint primitives; sandboxed `cargo build` + `stellar contract optimize`. **Gate:** Walkthrough 3 produces reproducible WASM hash.
- **Phase 4 — Simulation harness.** Permit replay; deny generator; proptest integration. **Gate:** Walkthrough 1 passes permit + 6 generated deny cases.
- **Phase 5 — Full MCP surface.** All 5 tools; resources + prompts; both transports; determinism + error-code conformance. **Gate:** identical outputs across Claude Desktop, Cursor, Cline, Continue, `mcp-cli`.
- **Phase 6 — Agent skill + clarification.** `SKILL.md` with references/scripts; few-shot examples for each walkthrough. **Gate:** correct triggering in Claude.ai paid plans without operator prompting.
- **Phase 7 — Wallet integration.** SEP-43 adapter for Freighter; passkey-kit programmatic signer. **Gate:** end-to-end Walkthrough 2 → on-chain install → `verify_install` returns "matches".
- **Phase 8 — Security hardening.** Internal review of synthesizer + templates; external audit (OtterSec primary; Certora optional for decision-tree FV); reproducible-build CI; continuous fuzz. **Gate:** findings remediated or accepted-with-rationale.
- **Phase 9 — Docs + release.** Cookbook of three walkthroughs; Apache 2.0 LICENSE / CONTRIBUTING / SECURITY.md; testnet reference deployments.

### 18. References (primary + officially attributed)

Stellar / Soroban: `developers.stellar.org` hub; `developers.stellar.org/docs/build/smart-contracts`; getTransaction (`developers.stellar.org/docs/data/apis/rpc/api-reference/methods/getTransaction` — verbatim "default retention window set at 24 hours…we do not recommend values longer than 7 days"); simulateTransaction (`developers.stellar.org/network/soroban-rpc/methods/simulateTransaction`); advanced contract account patterns (`developers.stellar.org/docs/build/guides/contract-accounts/advanced-patterns`); smart wallets (`developers.stellar.org/docs/build/apps/smart-wallets`); resource limits / fees; SAC and SEP-41 token interface.

SEPs: SEP-43 (`github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0043.md`, Draft v1.2.1, authors Piyal Basu / Leigh McCulloch / George Kudrayvtsev / Enrique Arrieta / Orbit Lens).

OpenZeppelin: `github.com/OpenZeppelin/stellar-contracts`; accounts package; Smart Accounts doc (`docs.openzeppelin.com/stellar-contracts/accounts/smart-account`); Policies doc; Context Rules doc; crates.io `stellar-accounts`; RC v0.7.0 differential audit (`openzeppelin.com/news/stellar-contracts-rc-v0.7.0-audit`); Contracts Wizard; OZ Relayer Stellar integration.

Protocol discussions: CAP-71 (`github.com/orgs/stellar/discussions/1784`); WebAuthn smart wallet contract interface CAP-51 (`github.com/orgs/stellar/discussions/1499`); Protocol 23 mainnet vote announcement by Bri Wylde, stellar.org, 3 Sept 2025 (CAP-0065 reusable module cache, CAP-0066 in-memory state).

PollyWallet (prior art): repo `github.com/kalepail/pollywallet`; `PLAN.md`; `CLAUDE.md`; demo video `youtu.be/vmFnCtkqQJA`; `passkey-kit` at `github.com/kalepail/passkey-kit`.

Model Context Protocol: spec home `modelcontextprotocol.io`; Transports 2025-11-25 (`modelcontextprotocol.io/specification/2025-11-25/basic/transports`); Transport WG roadmap *Exploring the Future of MCP Transports* by Kurtis Van Gent and Shaun Smith (blog.modelcontextprotocol.io, 19 Dec 2025); OpenTelemetry MCP semantic conventions.

Agent Skills: `github.com/anthropics/skills`; Anthropic engineering blog *Equipping agents for the real world with Agent Skills*; Claude API Skills overview; open standard at `agentskills.io`; Claude Code skills doc.

Cloudflare: Agent Setup (`developers.cloudflare.com/agent-setup/`); Cloudflare Agents MCP transport doc; Workers AI pricing page (`developers.cloudflare.com/workers-ai/platform/pricing/`) — Kimi K2.5 verbatim `$0.600/M input, $0.100/M cached input, $3.000/M output`; Cloudflare changelog — Kimi K2.5 deprecation scheduled May 30 2026.

Walkthrough protocols: `docs.blend.capital`; `github.com/blend-capital/blend-contracts` and v2; Blend Vault tutorial; `docs.soroswap.finance`; `github.com/soroswap/core`. OtterSec Soroswap core audit referenced via Soroswap repo.

Audit firms: SDF blog post *The Soroban Audit Bank: Fostering a Secure Smart Contract Ecosystem* (stellar.org) — verbatim "six top-tier audit firms — Ottersec, Veridise, Runtime Verification, CoinFabrik, QuarksLab, and Coinspect."

Misc (labeled): JamesBachini Stellar Smart Accounts blog (third-party; carries older 4-method Policy trait shape, superseded by current crates.io docs); Stellar x402 facilitator (context for agentic payments).

---

## Recommendations

**Immediate next steps (Phase 1 in §17):**

1. **Clone OZ stellar-contracts at the pinned `=0.7.1` tag and resolve every §16 TBD by direct source inspection.** The web-tooling layer this research used could not fetch raw `.rs` files; the implementation team must. Without verbatim `AccountParams` struct field names, error-enum variants, and the `spending_limit` period unit, the synthesizer's Track A path cannot be wired up correctly. **Threshold to escalate:** if `spending_limit::AccountParams` does not include `token: Address`, reconsider whether composing with `spending_limit` is safe under any rule type other than `CallContract(<token>)` and document the constraint explicitly.

2. **Stand up the Rust workspace with `rmcp` MCP server skeleton plus `record_transaction` against Stellar testnet RPC.** Gate proceeds to Phase 2 only after a round-trip recording successfully decodes a Blend testnet transaction and re-emits a JSON Recording whose contract addresses + ScVal arg types match the on-chain record byte-for-byte.

3. **Mirror PollyWallet's deterministic JSON schema as a starting point**, but rename to `oz-policy-builder/v1` and add the `synthesis_mode` field at the top level on day one — retrofitting that field later requires breaking schema changes.

**Mid-term:**

4. **Choose OtterSec early for the synthesizer audit** rather than waiting for Phase 8. Engage them at Phase 3 (after codegen produces reproducible WASM) so the audit team sees the template language, not the generated artifacts. **Threshold to swap:** if scheduling slips beyond a quarter, fall back to one of the other five SDF-blessed firms (Veridise, Runtime Verification, CoinFabrik, QuarksLab, Coinspect).

5. **Decide LLM posture in writing before Phase 6.** The synthesizer must be a deterministic function of inputs; LLMs are clarification/summarization surfaces only. **Threshold to revisit:** only if a real user-study shows the agent fails to elicit the right constraints in conversational mode — and even then, restrict the LLM's effect to the *spec*, never the *codegen*.

**Long-term:**

6. **Plan for `stellar-accounts` minor-version bumps as breaking events.** Maintain a compatibility matrix and CI regression suite per generated walkthrough. **Threshold to break:** if a minor bump changes the Policy trait or `SmartAccount::add_policy` signature, gate releases until templates are updated and re-audited.

7. **Coordinate with Freighter and passkey-kit maintainers on shared policy-rendering conventions** so users see consistent plain-English explanations of the same policy across wallets. **Threshold to expand:** if Lobstr / Hot / Beans add C-address support, add them as additional integration targets in Phase 7-bis without delaying the Freighter cutover.

---

## Caveats

- **Source-level TBDs (§16):** All three OZ policy primitives' exact `AccountParams` struct field names, the `spending_limit` period unit (ledgers vs seconds), the per-token field presence in `spending_limit`, and exact error enum variants remain to be verified by direct source inspection. The web-fetch tooling used in this research could not retrieve raw `.rs` files from `github.com/blob/...` or `raw.githubusercontent.com` URLs. Mitigation: Phase 1 first task.
- **PollyWallet demo video (https://youtu.be/vmFnCtkqQJA) not viewed.** Its content is *inferred* from `README.md`, `PLAN.md`, and `CLAUDE.md`. Almost certainly demos passkey+wallet+transfer flows, not full Policy Builder. **Verify by direct viewing at implementation time.**
- **Discrepancy between `stellar-accounts` GitHub release tag (`v0.7.0-rc.1`, commit 239a2a7) and crates.io README pin (`=0.7.1`).** Either crates.io has a post-RC publish without a corresponding tagged release, or the README is ahead of the tag. Verify with `cargo search stellar-accounts` at impl time.
- **`rmcp` Rust MCP SDK protocol-revision parity.** TS and Python SDKs ship the 2025-11-25 spec first; the Rust SDK may lag. Verify minimum-supported MCP revision before pinning. Fallback: use the TypeScript SDK for the MCP server and FFI to the Rust synthesizer — slower to ship and adds a runtime boundary, but viable.
- **Doc/source discrepancy on `spending_limit` time window.** The OZ docs page calls the period "Duration in seconds"; the audit text and Soroban convention favor ledger counts. We assume ledger counts in this report and flag for verification.
- **CAP-71 (delegated-signer auth-context forwarding)** is not yet in protocol; the report's "AI agent can hold a delegated signer" model works today but will gain meaningful UX/cost improvements when CAP-71 lands.
- **PollyWallet's `stellar-contracts` submodule is pinned to commit `187ad25`**, which predates the RC v0.7.0 (commit 239a2a7). Any code reused from PollyWallet must be re-targeted against `=0.7.1` and re-validated against the v0.7 trait shape (notably the `can_enforce` removal).
- **Audit-class issues from RC v0.7.0** (CallContract scope, signer-set divergence, sponsor `context_rule_ids` substitution, `Default`+`spending_limit` confusion) are partially fixed in OZ PRs #649, #655, and others. The synthesizer must check for these fixes at install time and refuse pre-fix deployments — assume this gate is added in Phase 7.
- **Kimi K2.5 deprecation 30 May 2026 per Cloudflare changelog** — irrelevant to our deterministic codegen path, but a confirmation point that single-model lock-in (as in PollyWallet's plan) is brittle.
