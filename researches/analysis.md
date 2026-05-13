# OZ Accounts Policy Builder — RFP Strategic Analysis (v2)

**RFP source**: Stellar SCF, Q2 2026 RFP cohort (RFP Track, expected SCF #44).
**Submission deadline**: **June 14, 2026** — 30 days from today (May 15, 2026).
**Subject**: An AI-assisted toolkit (MCP server + agent skill + Rust policy synthesizer) that turns observed or simulated Stellar transactions into the minimal OpenZeppelin smart-account context rule and policy set needed to repeat that flow — and nothing more.
**Purpose of this document**: Internal pre-drafting analysis. Not a proposal. Designed to align us on what the RFP is actually asking for, where it sits in the Stellar ecosystem, what the realistic solution space looks like, what wins it, and what we should clarify before drafting.
**Status**: v2 — supersedes v1. Reflects deep inspection of OZ's `stellar-contracts` v0.7.1 source, the full pollywallet codebase (~7,000+ LOC of working policy-builder implementation), the complete OZ accounts framework documentation, Soroban's `simulateTransaction` API, the SCF #44 calendar, the C-Address Tooling RFP, and Tyler/kalepail's broader infrastructure footprint on Stellar.

---

## 1. Executive Summary

The RFP asks for what the Ethereum ecosystem calls a "session key" or "scoped permission" generator — but with one critical extension: the toolkit derives the permission scope *automatically from a real or simulated transaction*. Demonstrate the desired flow once, get back an OpenZeppelin context rule plus a Soroban Rust policy bundle that allows exactly that flow and rejects deviations. Three named delivery surfaces: a Rust synthesizer library, an MCP server, and a Claude (and equivalents) agent skill.

**The most important fact for our decision-making** is that the prior art the RFP names — `kalepail/pollywallet` — is not a "one-week MVP" as the RFP gently characterizes it. It is a roughly 7,000+ line working implementation covering: transaction analysis via Stellar RPC, a deterministic per-argument typed JSON schema, an extremely sophisticated AI codegen system prompt that already encodes the framework's real footguns, a Cloudflare Sandbox compile-and-test harness, a deployed AI-generated policy contract on Stellar testnet, an end-to-end simulation-based verification script with permit-and-deny cases, and a 1,058-line GUI. What is genuinely missing from pollywallet relative to the RFP: an MCP server, a Claude/agent skill, a documented audit story, multi-wallet integration beyond pollywallet's own passkey wallet, and the code-first/deploy-second workflow split.

**The second important fact** is that Tyler (kalepail) is not just the author of pollywallet — he is the author of `passkey-kit` (the de-facto Stellar smart-wallet SDK referenced from the SDF's own developer documentation), `Launchtube` (the gasless transaction submission service that the SDF runs a mainnet instance of), the `KALE` project (the largest stress test of Stellar smart contracts to date), and the original Stellar protocol Discussion #1499 proposing the WebAuthn smart-wallet contract interface. He has been working in this exact problem space — passkey-secured smart accounts with policy signers on Stellar — for two years, and the SDF documents his tools in its official guides. Any proposal that does not engage him directly is competing against the de-facto incumbent infrastructure for this problem; any proposal that does engage him is structurally favored.

The strategic context is unusually favorable. OpenZeppelin's smart-account framework shipped on Stellar in late 2025 / early 2026 (currently v0.7.1, with seven audits in the repo), and was explicitly named by the SDF as the contracts layer for the x402 agentic-payments stack live on Stellar mainnet. Blend at $100M+ TVL and Templar's RWA lending give us real DeFi flows worth scoping down. The C-Address Tooling cohort is still being selected in Q1 2026 RFP review; the wallets in that cohort are not yet publicly announced.

Two pieces of timing news shape this round: **(1) Our specific RFP is not yet published on the SCF handbook RFP page** — only six Q1 2026 RFPs are listed (Prices API, DeFi Positions API, C-Address Tooling, Block Explorer, Soroban Reverse Engineering, Hummingbot integration). The OZ accounts policy builder RFP, marked "Added Q2 2026", appears to be pre-publication. We should confirm with the SCF team that the RFP is on the SCF #44 agenda before drafting. **(2) The submission deadline for SCF #44 is June 14, 2026** — exactly 30 days from this document — and tranche #3 funding requires *mainnet* live deployment, not just testnet readiness. That is a tight calendar for a $150K, four-month, mainnet-ready engagement that includes an audit.

Our biggest pre-drafting decisions are: **whether to engage Tyler as a co-submitter or as a collaborator on a derivative submission**, how to scope the audit credibly within the SCF Audit Bank's capacity, and how to position against the existing OpenZeppelin Contracts MCP at `mcp.openzeppelin.com` (which today covers tokens but not accounts/policies — so we are complementary, not competitive). The Tyler decision dominates everything else and should be resolved first.

---

## 2. RFP Decomposition

### 2.1 What this RFP actually wants

A developer- and end-user-facing toolkit. Three nested compositional layers:

- A **Rust synthesizer library** that reads structured Stellar transaction data, derives a deterministic intermediate representation (schema), and emits compilable Soroban policy contract code plus a context rule specification.
- An **MCP server** wrapping the synthesizer for agent consumption (named structured tools, deterministic outputs, machine-readable errors).
- A **Claude (and equivalents) agent skill** wrapping the MCP for high-level conversational invocation — the RFP names the example "this transaction transferred 50 USDC — should the policy cap at 50, or allow up to 100 over a week?"

The RFP explicitly states this is *not* a new contract primitive, *not* a hosted service that auto-deploys policies, and *not* a one-shot demo. Code is the primary output; deployment is a separate, explicit, user-initiated step.

### 2.2 The core mechanic

"Record-and-generate." A user or agent executes a representative transaction (the RFP gives the example of claiming Blend yield and converting it to USDC). The tool synthesizes the minimum context rule and policy set that would permit exactly that flow, and bundles a simulation harness for verifying the result against the recorded transaction (must permit) and against systematically adjacent transactions (must deny — different asset, different amount, different timing).

### 2.3 The three reference use cases

The RFP names three end-to-end walkthroughs:

1. **Blend yield-claim flows** — the canonical example. Calling Blend's `claim` to get BLND rewards, then swapping through Comet or another DEX to USDC, and depositing the proceeds back. This is a multi-contract, multi-function sequence with real economic value.
2. **Subscription billing on a SEP-41 token** — a recurring transfer from a smart account to a merchant, with frequency and cap policies. Note that Vouchify is competing in SCF #44 with "On-Chain Subscriptions for Merchants" ($135K Open Track) which overlaps this use case but does not appear to be addressing this same RFP.
3. **Delegated trading on Soroswap with bounded slippage** — letting an agent execute trades up to a configured amount within a slippage budget. This is the most policy-heavy case because slippage and bounded swap parameters aren't expressible with any existing OZ primitive (see §5.1).

### 2.4 The protocol team's underlying motivation

Three priorities converge here, in order of weight:

1. **AI / agent readiness of Stellar.** With x402 (HTTP 402-based agentic payments) and MPP (Machine Payments Protocol session pre-authorization) live on Stellar mainnet, the missing piece is *safe delegation*. Agents either hold full keys (unacceptable) or write Rust policies by hand (impractical). The SDF has publicly named OpenZeppelin as the contracts layer for x402's "programmable spending limits and guardrails." This RFP is the tooling that turns that contracts layer into something a non-Rust developer can actually use.
2. **C-address (smart account) adoption.** Authoring complexity is the adoption ceiling. The RFP states the policy-authoring bar is "effectively prohibitive for end users."
3. **Soroban developer experience.** Compose-where-you-can, generate-where-you-must — productize the OZ accounts package's expressiveness.

### 2.5 What is left open to the applicant

The RFP is specific on *what* but leaves significant *how* decisions to the bidder:

- The transaction representation / schema that bridges recording and synthesis (pollywallet has a versioned `pollywallet-policy/v0` schema with per-argument typed constraints — we should retain or extend it).
- The split between template composition and LLM codegen. The RFP requires both modes, but only three OZ primitives exist today (`simple_threshold`, `weighted_threshold`, `spending_limit`), and `spending_limit` is hard-coded to the SEP-41 `transfer(from, to, amount)` shape. Most realistic flows will require fresh policy code — the synthesizer's decision rule for *when* to generate is ours to define.
- Hosting/distribution model for the MCP server (remote SSE vs. local stdio — OZ's existing Contracts MCP supports both).
- Simulation harness implementation strategy (RPC `simulateTransaction` vs. sandbox compile-and-test; pollywallet uses both, with simulation as the deny-test verifier and sandbox as the compile loop).
- The deny-case generation strategy (auto-mutation vs. hand-authored adjacency vs. user-supplied counter-examples).
- The wallet integration partner — the RFP says "a wallet from the C-Address Tooling cohort" but that cohort is still in selection.
- The audit scope and auditor identity.

---

## 3. Hard Requirements vs. Nice-to-Haves

### 3.1 Hard requirements

Section 3 of the RFP is the requirements list. Reading each carefully and quoting where it matters:

| Requirement | Source language | Implementation reality |
|---|---|---|
| Transaction recording / observation layer | "ingest either (a) a real on-chain transaction by hash on mainnet/testnet or (b) a locally simulated transaction" | Pollywallet has `tx-analyzer.ts` (300 LOC) that reads via `stellarRpc.getTransaction(hash)`, decodes XDR, extracts InvocationNode trees and SorobanAuthorizationEntries |
| Context-rule + policy synthesizer | "the smallest set of policies needed to constrain the rule" | Pollywallet has `policy-schema.ts` (452 LOC) with typed per-argument constraints |
| Composable-first generation | "compose existing policies first and only generate net-new policy contracts when the constraint cannot be expressed by combining standard ones" | Only three OZ primitives ship, and `spending_limit` is SEP-41-`transfer`-only. Realistic flows generate. See §5.1 |
| Generated Rust code | "must implement the Policy trait correctly, including proper storage segregation for stateful cases" | The trait has three methods: `install`, `enforce`, `uninstall` — no `can_enforce` despite what the public docs imply. Storage must be keyed by `(smart_account, context_rule_id)` |
| MCP server | "agent-friendly: structured inputs/outputs, deterministic behavior, machine-readable error codes" | Not present in pollywallet — net-new build |
| Agent skill | "Can be for Claude and similar tools" | Not present in pollywallet — net-new build |
| Simulation / dry-run harness | "(a) the original recorded transaction (must permit), (b) a set of adjacent transactions that should be denied" | Pollywallet has `verify-policy.mjs` (549 LOC) and `policy-sandbox.ts` (638 LOC) — combines RPC `simulateTransaction` for deny verification + sandbox compilation |
| Wallet integration | "at least one existing Stellar wallet supporting OZ smart accounts (e.g., a wallet from the C-Address Tooling cohort)" | Pollywallet is itself a passkey-secured wallet; integrating with a separate C-Address cohort wallet depends on the cohort being selected (TBD) |
| Three documented walkthroughs | "Blend yield-claim flows … subscription billing on a SEP-41 token, delegated trading on Soroswap with bounded slippage" | None deployed yet, but the synthesizer's schema can express all three |
| Configurable composition / generation mode | "the user should be able to inspect and modify generated policy code before deployment, not be forced into a fully automatic flow" | Inspectability is a hard line. Pollywallet currently auto-flows; we need a review step |
| Code-first, deploy-second workflow | "Deployment is never automatic" | Direct rejection of pollywallet's Phase 5 "compile-optimize-deploy" chain as it exists today |
| Audit of synthesizer logic | "must commit to an audit of the synthesizer logic itself (not just sample outputs)" | Explicit, weighted commitment |
| Open source, permissive license | "Open source, permissive license" | Pollywallet is Apache 2.0 |

### 3.2 Nice-to-haves (raise the score)

- Prior MCP / agent tooling experience ("a strong differentiator" per the RFP).
- Prior OpenZeppelin accounts experience.
- Concrete OZ engagement plan ("describe how they will engage OZ as a technical reviewer").
- Coordination plan with the C-Address Tooling cohort.
- Coherent integration story into existing developer / agent workflows.
- "Building on existing work": "Submissions should explicitly address what they will adopt, extend, or replace from kalepail/pollywallet. Greenfield rewrites should be justified."

### 3.3 Optional / stretch enhancements

- Upstreaming useful policy primitives into the OZ accounts package itself (the RFP names "aligning on what primitives would be valuable to upstream").
- Multi-agent-framework skill packaging (Cursor, Codex, OpenCode, Windsurf — not just Claude).
- A policy sharing/registry model (pollywallet's PLAN.md flags this as future; the RFP does not require it).
- Mainnet readiness for tranche #3 — implied by SCF #44's funding structure (MVP → Testnet → Mainnet) even though the RFP itself doesn't require mainnet explicitly.

### 3.4 Items the RFP does *not* require

Explicit scoping discipline matters for SCF reviewers:

- A general-purpose Soroban smart-contract IDE.
- A policy marketplace.
- New context-rule primitives.
- New verifier contracts (only Ed25519 and WebAuthn ship today; the synthesizer doesn't change that).
- Indexer / explorer functionality beyond what's needed to fetch one transaction.
- Mainnet deployment infrastructure or a hosted relayer.
- Account creation / passkey wallet itself — pollywallet has this; the RFP does not ask for it.

---

## 4. Evaluation Criteria & Selection Logic

### 4.1 Stated criteria (priority order)

From Section 4 of the RFP:

1. **Technical capability** — Soroban + Rust + ideally OZ accounts experience; MCP / AI tooling experience named as a strong differentiator.
2. **Relevant experience** — Authorization frameworks, account abstraction, codegen tooling; agent-facing tooling weighted heavily.
3. **Security & audit history** — Explicit, weighted heavily. Must commit to auditing the synthesizer logic, not just sample outputs.
4. **Coordination with OpenZeppelin** — Described engagement plan with OZ as technical reviewer.
5. **Ecosystem alignment** — Integration with at least one C-Address Tooling cohort wallet; coordination with the cohort.
6. **Delivery timeline** — "relatively short."
7. **Coherent integration plan** — Story for fitting into existing workflows.
8. **Building on existing work** — Pollywallet engagement is required justification.

### 4.2 Inferred priorities (where the weight actually sits)

**Security is the dominant axis.** The RFP devotes more language to it than to anything else, with unusually specific phrasing ("must commit to an audit of the synthesizer logic itself, not just sample outputs", "must articulate a clear story for verifying generated policies", "this tool generates code that runs as authorization logic on user funds"). The audit story has to be structured, not hand-waved.

**Agent-facing surface is the second axis.** MCP named three times, agent skill twice. The Cloudflare Agent Setup reference points to an expected delivery shape (Skills + MCP catalog). Bidders without MCP/skill experience need to compensate hard.

**Engagement signals (OZ + Tyler + cohort) is the third axis.** Three external parties are explicitly named in coordination criteria. Reviewers will be looking for evidence those conversations have started, not just promises.

**Soroban-specific depth matters more than general blockchain experience.** The OZ accounts package is new and Stellar-specific. Generic ERC-4337 / ERC-7579 experience is conceptually transferable but not implementation-level.

### 4.3 SCF #44 structural constraints

Under SCF 7.0:

- $150K cap in XLM, four months, four-tranche milestone structure.
- Tranches: **Initial → Tranche #1 (MVP) → Tranche #2 (Testnet) → Tranche #3 (Live on Mainnet)**. The final tranche is gated on a *mainnet* live deployment.
- "UX readiness" required for the final tranche — onboarding flows, tested interfaces, usability validation.
- RFP track submissions are evaluated by a Category Delegate Panel (not community vote).
- Reviewers may request minor revisions before funding.
- **June 14, 2026 deadline** for SCF #44.

The mainnet gating point is critical: a four-month engagement that ends in mainnet deployment, with audit results remediated before that point, is an aggressive timeline. The plan needs to put the audit early, not late.

---

## 5. Linked Resources Analysis

### 5.1 OpenZeppelin Stellar smart-accounts framework — concrete reality

Verified against the actual `OpenZeppelin/stellar-contracts` v0.7.1 source (cloned May 15, 2026).

**Audit posture is mature**: seven audit reports in `/audits` covering versions 0.1.0-RC, 0.2.0, 0.3.0-rc.2, 0.5.0 (plus re-audit), 0.6.0, 0.7.0. The contracts package depends on `soroban-sdk = "25.3.0"`. Certora formal verification is in progress per OZ's announcements.

**Framework hard limits** (verified in `smart_account/mod.rs`):
- `MAX_POLICIES = 5` per context rule.
- `MAX_SIGNERS = 15` per context rule.
- `MAX_NAME_SIZE = 20` chars (rules must have short names).
- `MAX_EXTERNAL_KEY_SIZE = 256` bytes.
- Max 15 context rules per smart account (per the docs).

**Policy trait surface** (`policies/mod.rs`):
- Three methods only: `install(env, account_params, context_rule, smart_account)`, `enforce(env, context, authenticated_signers, context_rule, smart_account)`, `uninstall(env, context_rule, smart_account)`.
- **No `can_enforce`**, despite what the authorization-flow docs imply in pseudocode. The actual model is: enforce panics on failure. The OZ docs are stale on this point — anyone reading them will be confused. We should call this out in our proposal as a real footgun the toolkit handles.
- Storage must be segregated by `(smart_account_address, context_rule_id)` for stateful policies. Get this wrong and the policy shares state across accounts dangerously.

**The three shipped policy primitives** — and their hard constraints:

| Primitive | Trigger / Scope | Hard constraint |
|---|---|---|
| `simple_threshold` | Any context | N-of-M check on `authenticated_signers.len() >= threshold`. Stateless logically, but stores threshold per (account, rule). Stores via `extend_ttl` on every read |
| `weighted_threshold` | Any context | Weighted sum of signer weights ≥ threshold. Stores `Map<Signer, u32>` weights |
| `spending_limit` | **`CallContract` only** — panics with `OnlyCallContractAllowed` otherwise. Inside enforce, **only handles `symbol_short!("transfer")`** and reads `amount` at `args.get(2)`. Hard-coded to SEP-41 `transfer(from, to, amount)` shape | Rolling window with `MAX_HISTORY_ENTRIES = 1000`, TTL extension `30 days`, error codes 3220–3227 |

**Critical implication**: `spending_limit` is useful for SEP-41 transfers. It is **not** directly useful for Soroswap swaps (function is `swap_exact_amount_in`), Blend `claim`, deposits, or any non-SEP-41-transfer flow. For our three reference use cases:
- Blend yield-claim → custom policy required (composes a function whitelist on `pool.claim` and `comet.swap_exact_amount_in`).
- SEP-41 subscription → `spending_limit` can compose if the cap is on the transfer amount.
- Bounded Soroswap → custom policy required (slippage bounds on swap args).

So "compose existing policies first" is a real RFP requirement but in practice composing alone covers maybe 1 of the 3 use cases. The synthesizer's value is in fresh codegen, with composition as the special case when available.

**Threshold policies have a documented "signer set divergence" footgun**: signers can be added or removed from a rule without the threshold being updated, silently weakening or breaking auth. The synthesizer must surface this to the user during generation. The OZ source for `simple_threshold.rs` has a 40-line module-level doc comment dedicated entirely to this problem.

**Verifiers**: Only `ed25519` and `webauthn` (P-256/passkeys) ship. The `Verifier` trait has `verify`, `canonicalize_key`, and `batch_canonicalize_key`. The framework dedupes signers across encodings via `canonicalize_key`.

**SmartAccount trait surface** the MCP server will need to wrap when preparing install transactions:
- `add_context_rule(context_type, name, valid_until, signers, policies: Map<Address, Val>)` — primary install
- `add_signer`, `add_policy`, `batch_add_signer`
- `update_context_rule_name`, `update_context_rule_valid_until`
- `remove_context_rule`, `remove_signer`, `remove_policy`
- Read queries: `get_context_rule`, `get_context_rules_count`, `get_signer_id`, `get_policy_id`

**Auth-payload design** (from `signers-and-verifiers` docs + `storage.rs`):
- `AuthPayload { signers: Map<Signer, Bytes>, context_rule_ids: Vec<u32> }` is the signature type passed to `__check_auth`.
- The `context_rule_ids` are bound into the auth digest: `auth_digest = sha256(signature_payload || context_rule_ids.to_xdr())`. This prevents downgrade attacks (someone using a more permissive rule than the signer intended).
- Signers sign the digest, not the raw payload.

**Known protocol-level gotcha — the delegated-signer simulation gap**: `simulateTransaction` does *not* return the auth entry for a delegated signer's `__check_auth` call. Clients must manually construct two auth entries — one with the AuthPayload, one for the delegated signer's nested invocation. CAP-71 aims to streamline this, but is not yet in protocol. Our MCP server preparing install transactions must handle this correctly today. External signers (passkey, ed25519) don't have this problem.

### 5.2 `kalepail/pollywallet` — real footprint

What I previously characterized as a "one-week MVP" is in fact a 7,000+ LOC working implementation. Cloned May 15, 2026, Apache 2.0, single contributor (Tyler).

**`src/lib/` — 4,532 LOC of TypeScript** (line counts):
- `policy-codegen.ts` (1,253 LOC) — the AI prompt assembly, including a system prompt that encodes hard-won learnings about Soroban Rust generation (see §5.2.1)
- `policy-sandbox.ts` (638 LOC) — Cloudflare Sandbox compile-and-test client
- `policy-schema.ts` (452 LOC) — the deterministic intermediate representation
- `policy-deploy.ts` (418 LOC) — deployment client (this is the part the RFP wants split into "prepare" vs. "deploy")
- `tx-analyzer.ts` (300 LOC) — recording layer
- `passkey.ts` (433 LOC), `contract-spec.ts` (431 LOC), `rule-management.ts`, `policy-store.ts`, `relayer.ts`, `context-rules.ts`

**`src/routes/` — 1,404 LOC of policy/rules UI**:
- `policies.tsx` (1,058 LOC) — multi-phase policy builder GUI
- `rules.tsx` (346 LOC) — context-rule management UI

**`e2e-policy-test/` — a complete deployable example**:
- `src/lib.rs` (476 LOC) — a standalone single-file Soroban Rust contract that re-implements `spending_limit` correctly (not depending on `stellar-accounts` as a library, because OZ ships the policies as `lib` not `cdylib`). Includes contract tests for install / enforce-within-limit / enforce-exceeds-limit / uninstall using the `#[should_panic]` pattern.
- `verify-policy.mjs` (549 LOC) — a six-phase end-to-end testnet verification harness: fund ephemeral account → install policy → enforce within limit (success) → enforce over limit (expected simulation failure) → read state → uninstall → enforce after uninstall (expected failure).
- A **deployed AI-generated policy at `CDTV55VTCRIPH3BCX5ZVNOWRB4NPFNS44U6X46ZP7K4GAKNNZQDCB6JI` on Stellar testnet** — Tyler has actually deployed a Kimi K2.5-generated `spending_limit` variant and verified it enforces correctly.

**`sandbox-worker/` — Cloudflare Sandbox compile service**:
- Dockerfile preinstalls Rust + stellar-cli + soroban-sdk crate cache (so cold starts don't pay for 180 crate downloads each time)
- `src/index.ts` (443 LOC) — accepts `cargoToml + libRs + testCode`, runs `stellar contract build`, returns WASM or parsed errors

#### 5.2.1 What the `policy-codegen.ts` system prompt reveals

The 1,253-line `policy-codegen.ts` file is the most interesting single artifact in pollywallet for our purposes. Its system prompt embodies real Soroban codegen experience that any synthesizer will need to replicate. Highlights worth carrying forward:

- **`symbol_short!()` only accepts 9 ASCII chars**. Function names longer than that need `Symbol::new(env, "...")` plus equality comparison. Common LLM mistake.
- **Default rules see an `execute()` wrapper; CallContract rules see the direct call**. The generated `enforce` must handle both patterns. In Pattern 1 (`Default`), `args[0]` is target contract Address, `args[1]` is inner fn Symbol, `args[2]` is inner args Vec. In Pattern 2 (`CallContract`), `args` is the raw function args. Tyler discovered this through trial and error.
- **Stellar StrKey addresses are base32, not hex**. LLMs frequently get this wrong and try to hardcode addresses as byte arrays.
- **Default-reject**: any unrecognized fn_name or contract address must panic. Never silent-allow.
- **Context type doesn't implement Debug or PartialEq**. Events containing Context will fail to compile.
- **Address, Symbol, String, Vec, Bytes don't implement Copy** — `.clone()` required everywhere, no `*` dereference.
- **Install params should be optional**: handle `Val::VOID` (no config, use defaults) and `Map<Val, Val>` shapes alike, with `unwrap_or` defaults.
- **Storage key naming convention**: `max_{arg_name}` for range max, `min_{arg_name}` for range min, `threshold`, `allowed_{arg_name}` for allowlists.

The prompt also embeds the *complete* OZ Policy trait source, the complete `simple_threshold.rs` source, and a complete `spending_limit.rs` reference implementation — leveraging Kimi K2.5's 256K context. This is a working prompt-engineering approach that we should either adopt or improve on.

#### 5.2.2 The deterministic schema's actual shape

Pollywallet's schema is more sophisticated than the PLAN.md described:

- Multiple contracts per policy (`contracts: ContractPermission[]`), not just one `CallContract` scope.
- Per-function permissions (`functions: FunctionPermission[]`), each with per-argument permissions.
- Per-argument typed constraints — `exact / range / allowlist / blocklist / unconstrained` — with type-aware validity (addresses can use `allowlist`; integers can use `range`; bools can use `exact`).
- **Spec-driven**: argument names and types come from the contract's WASM spec, not from heuristic parsing. The `contract-spec.ts` (431 LOC) handles WASM spec ingestion.
- Natural-language `note` fields per function and per argument for behaviors that can't be expressed as constraints (e.g., "rolling window sum over 17280 ledgers").
- Global rules separate from per-contract rules: `threshold`, `weighted_threshold`, `time_lock` — exactly the three OZ primitives plus `time_lock` (which isn't yet in OZ).
- Versioned as `pollywallet-policy/v0`.

#### 5.2.3 What pollywallet does *not* have

Gap analysis vs. the RFP requirements:

1. **No MCP server.** This is the largest net-new build.
2. **No agent skill** (Claude or otherwise).
3. **No code-first/deploy-second split** — current Phase 5 auto-deploys.
4. **Limited deny-case test generation** — the verification harness exercises specific permit/deny cases but doesn't systematically mutate the recorded transaction to derive a deny suite.
5. **Testnet only** — `policy-deploy.ts` and the verification scripts are hard-coded to testnet. Mainnet readiness needs explicit work.
6. **Single wallet** — pollywallet itself. No integration with other wallets in the C-Address cohort.
7. **No documented audit story** for the synthesizer logic.
8. **No three documented walkthroughs** of the kind the RFP requires (Blend, SEP-41 subscription, Soroswap). The existing testnet-deployed policy is a `spending_limit` variant only.
9. **Cloudflare-stack lock-in** — Workers AI (Kimi K2.5) and Cloudflare Sandbox. The synthesizer library should be decoupled from this so it's runnable elsewhere.

### 5.3 Tyler / kalepail — broader context

The RFP names "Tyler (kalepail on GitHub)" twice. He is the author of:

- `pollywallet` — covered above.
- `passkey-kit` — TypeScript SDK for creating and managing Stellar smart wallets. The de-facto SDK for passkey-secured smart accounts on Stellar. Referenced in the SDF's own developer documentation under `developers.stellar.org/docs/build/apps/smart-wallets` and used by their guestbook tutorial.
- `Launchtube` — gasless transaction submission service for Soroban. The SDF runs a mainnet instance and documents it in their developer guides.
- `KALE` — the proof-of-teamwork farming game. Per the SDF's own blog: "the largest stress test for smart contracts on Stellar so far, even uncovering protocol-level issues like Blend invocations being bumped by KALE traffic."
- Stellar Protocol Discussion #1499 — the original proposal for the WebAuthn smart-wallet contract interface that ultimately informed OZ's framework.

He is not "a community member who built an MVP." He is, in practice, the foundational author of smart-wallet infrastructure on Stellar. The SDF documents his tools; the OZ accounts framework was informed by his prior work. Any team submitting to this RFP without engaging him will be perceived (correctly) as either uninformed or competitively positioned against the established infrastructure.

The pollywallet repo has 0 stars / 0 forks at the time of inspection — Tyler hasn't yet attracted external contributors, suggesting he may welcome a partner who can underwrite the audit, the agent-surface build, and the ecosystem coordination work he wouldn't naturally take on alone.

### 5.4 Cloudflare Agent Setup

`developers.cloudflare.com/agent-setup/` is a catalog of agent integrations (Claude Code, Codex, Cursor, GitHub Copilot, OpenCode, Windsurf) framed around two reusable surfaces: **Skills** (reusable prompt packages with slash commands) and **MCP servers** (tool integrations). Every listed agent supports both. The RFP's reference to this pattern signals a design center: skill = user-facing entrypoint, MCP = capability provider, library = underlying engine.

### 5.5 OpenZeppelin Contracts MCP

OZ already runs `mcp.openzeppelin.com` — a Contracts MCP that wraps their Contracts Wizard logic. Stellar coverage today: Fungible Token, NFT, Stablecoin templates. **Crucially, it does not cover accounts or policies.** Two implications:

- We are complementary, not competing. Our framing in the proposal: "OZ's Contracts MCP covers token contracts; this toolkit covers accounts and policies. Two MCPs, same family."
- It gives us a concrete delivery pattern (one-click setup for Cursor / VS Code, manual setup for Claude / Gemini / Windsurf, Wizard logic injection per call). OZ has built the MCP plumbing once; cooperating with their accounts team could conceivably share infrastructure or align brand.

### 5.6 Stellar / Soroban transaction & simulation model

Verified via Stellar docs + JS SDK source:

- `getTransaction(hash)` returns the on-chain transaction envelope. `tx-analyzer.ts` decodes `xdr.TransactionEnvelope` for both regular and fee-bump envelopes, walks `operations()`, filters for `invokeHostFunction → hostFunctionTypeInvokeContract`, extracts `contractAddress`, `functionName`, args, and `SorobanAuthorizationEntry[]`.
- `simulateTransaction` returns: `transactionData`, `minResourceFee`, `results[0].auth[]` (the auth tree), `events[]`, optional `stateChanges` (before/after ledger entries), and error info on failure.
- `assembleTransaction(rawTx, simResult)` is the canonical helper that fuses simulation output into a submittable transaction.
- **Two signing methods**: Method 1 is full transaction signing (G-account source signs the envelope; implies authorization). Method 2 is per-auth-entry signing — **the only option for C-accounts**, as they cannot sign transaction envelopes. Our toolkit operates exclusively in Method 2 territory.
- The deny-test mechanism: simulation that fails is the deny signal. Pollywallet's `verify-policy.mjs` uses `expectSimFailure: true` to assert that an over-limit transfer's simulation rejects.

---

## 6. Ecosystem & Strategic Context

### 6.1 Where this RFP sits in Stellar's 2026 roadmap

Three streams have converged to make this RFP timely:

**Stream 1 — OZ accounts shipped.** The framework launched on Stellar with Protocol 23's cheap cross-contract calls. Currently at v0.7.1 with seven audits and ongoing Certora formal verification. Listed by the SDF as the "C-address" primitive.

**Stream 2 — Agentic payments became infrastructure.** x402 launched on Stellar in March 2026, MPP in April 2026. The SDF blog (April 16, 2026 dev meeting transcript) explicitly names OpenZeppelin as the partner powering the x402 facilitator and providing "audited smart account contracts with programmable spending limits and guardrails." x402 is co-governed by Coinbase, Cloudflare, Google, Visa. MPP was co-developed by Stripe and Tempo. Galaxy Research projects $3–5T in agentic commerce by 2030. Per Stellar Q1 2026: "those DeFi protocols, tokenized assets, and stablecoin rails all become accessible to AI agents through these payment primitives." The "AI agent holds a smart account with scoped permissions" pattern is now the default mental model for agentic spending on Stellar.

**Stream 3 — C-address adoption hit a UX wall.** The Q1 2026 C-Address Tooling & Onboarding RFP was scoped around two named blockers: funding (G-to-C bridge) and a viable production wallet. Both necessary, neither sufficient. Once a user has a smart account, "how do I let an agent or service do this one thing without giving it everything?" is the gap our RFP fills.

### 6.2 SCF #44 calendar

- SCF #43 closed April 26, 2026 — currently in panel review. No open RFPs were on that round (Q2 RFPs were pushed to #44).
- **SCF #44 submission deadline: June 14, 2026.**
- SCF #44 already shows submissions in the public dashboard from teams including Vouchify ("On-Chain Subscriptions for Merchants" $135K), AMP liquidity pools, Theo, and others. These look to be Open/Integration track submissions, not direct RFP-track competition.
- Tranche structure: Initial → MVP → Testnet → Mainnet. **Tranche #3 is gated on live mainnet deployment** of "usable, discoverable, and positioned for adoption."

### 6.3 Where the SCF wants this toolkit to sit

The RFP isn't speculative — it's the missing rung on a clearly-articulated ladder. OZ's accounts framework + x402/MPP agentic payments + C-Address Tooling wallets + this RFP's policy-authoring toolkit = the full agent-spending stack on Stellar. The toolkit is the developer/agent-facing wrapper that makes the underlying contracts usable without Rust skills.

### 6.4 Alignment signals to emphasize in the proposal

- Explicitly tie the toolkit to x402 / MPP use cases (agent pre-authorizing a session with bounded spend; per-request micro-authorization). The RFP doesn't name these but the SDF does, in every recent communication.
- Use Blend yield, SEP-41 subscription, bounded Soroswap as the three walkthroughs — exactly what the RFP suggests. Resist the urge to add custom flavor.
- Frame as the OZ accounts framework's "last mile" — the policy-authoring layer that productizes everything OZ has built.

---

## 7. Architectural Decision Space

### 7.1 The three axes of architectural choice

**Axis 1 — Where the synthesizer engine actually lives.**

Three viable options, ranked roughly by reviewer-comfort:

- *Rust-native synthesizer library, called from a thin TypeScript MCP server.* Auditable on its own, reusable across CLI/MCP/skill, separates concerns. **Primary recommendation.** Note: pollywallet's existing synthesizer logic is in TypeScript today (`policy-schema.ts`, `policy-codegen.ts`), so adopting this position means porting / wrapping that logic. We can also keep the TypeScript implementation as the production synthesizer and audit it directly — but Rust gives stronger guarantees for code that emits contract source.
- *TypeScript synthesizer in a Node/Worker MCP, calling out to a Rust compiler.* Easier to ship initially, harder to audit as a standalone unit. This is closer to pollywallet's current shape.
- *Pure LLM codegen with no native synthesizer layer.* Tempting because it's flexible, but conflicts with the RFP's "compose existing policies first" mandate and makes the audit story incoherent.

A pragmatic middle path: keep the TypeScript synthesizer as the production engine for v1 (audit it), and migrate audit-critical sections (policy template families) into Rust later. The deterministic schema is language-agnostic.

**Axis 2 — How the schema relates to generation.**

- *Schema-first synthesis*: parse the recorded transaction, validate against contract specs, produce the schema, then either parameterize an existing primitive or generate from a template family. The LLM is invoked only when no template matches. Predictable, auditable. Matches pollywallet's intent.
- *LLM-first synthesis with schema as a post-hoc check*: reverses the trust direction, harder to audit. Rejected.

We should keep pollywallet's deterministic JSON schema. Renaming to `oz-policy/v1` or similar would signal a clean break and version bump, while preserving conceptual continuity.

**Axis 3 — Verification approach.**

Four layers, all required:

- *Static schema checks*: contract address validity (must start with C or G), name length ≤ 20, constraint-type compatibility, policy count ≤ 5, signer count ≤ 15.
- *Contract-spec validation*: function names exist on the target contract; argument types match; argument indices are correct.
- *Compilation in a sandbox*: does the generated Rust compile against `soroban-sdk` 25.3? Pollywallet's Cloudflare Sandbox is one approach; a local Docker-based sandbox is another for offline use.
- *Simulation harness with permit + deny cases*: does the compiled and deployed policy accept the recorded transaction and reject mutated variants?

The deny-case generation strategy is the most novel safety contribution and the most reviewer-skeptical area. Options:

- *Hand-authored adjacent variants per primitive*: reliable but doesn't generalize.
- *Automated mutation* (swap asset, scale amount, change recipient, shift time window): general, but mutations must be *meaningfully* adjacent — "what if the asset were SHIB" is silly; "what if amount = 1.1× cap" is informative.
- *User-supplied deny cases*: user records both a permit example and one or more deny examples.

A combined approach — baseline mutations per template family + user permit + optional user deny + parameter-boundary mutations — is the strongest. The synthesizer ships with audited mutation rules per template family.

### 7.2 Delivery surface design

The RFP names three surfaces. Their relationship:

- **Synthesizer engine** — TypeScript today (could migrate parts to Rust). Auditable. Wraps tx-analyzer + schema + template families + codegen + simulation harness.
- **MCP server** — Capability provider. Structured tools: `record_transaction(hash)`, `analyze_transaction(envelope)`, `synthesize_policy(schema)`, `compile_policy(rust_source)`, `simulate_policy(wasm, recorded_tx)`, `generate_deny_cases(schema)`, `prepare_install_transaction(wallet_addr, schema, policy_addr)`. Hosted remote and locally runnable.
- **Agent skill** — Conversational shell. Knows when to invoke which tool; knows how to ask clarifying questions ("you transferred 50 USDC — should the cap be 50, or up to 100 over a week?" — the exact example the RFP gives).
- **Review UI** — Optional but implied. The RFP says "the user can inspect and modify generated policy code before deployment." Pollywallet's 1,058-line policies route is a strong starting point. Reviewers shouldn't read it as our headline deliverable, but it should exist.

### 7.3 Wallet integration design

The RFP requires integration with at least one wallet from the C-Address Tooling cohort. That cohort is currently in selection (Q1 2026 RFP, in panel review). Three postures:

- *Pollywallet itself* — easiest because it's the harness and the wallet. Risk: it's testnet-only and a demo; reviewers may not count it as a "cohort wallet."
- *A separate C-Address cohort wallet, once known* — cleanest narrative, but timing risk (cohort selection finalized before we can integrate). We should ask the SCF team for the cohort timeline.
- *Both* — integrate pollywallet as our internal harness, commit to a cohort wallet integration as a milestone for tranche #2 or #3.

The defensible answer is "both."

### 7.4 Hosting and deployment

The MCP can be local-stdio (npm package), remote-SSE (hosted endpoint), or both. OZ's existing Contracts MCP supports both. The RFP says "versioned MCP server endpoint" in deliverables, which implies remote-hosted at least. We should plan for both, with a public hosted instance for ease of agent connection and a self-hostable package for advanced users.

### 7.5 Decision rule for composition vs. codegen

Given that `spending_limit` is the only OZ primitive with substantive enforcement logic, and it only handles SEP-41 `transfer`, the realistic decomposition is:

1. **Signer-side composition** — `simple_threshold` / `weighted_threshold` parameterizable from the recorded transaction's authentication footprint. Use composition.
2. **SEP-41 spending caps** — directly composable from `spending_limit`. Use composition.
3. **Everything else** — function whitelists, recipient allowlists, time locks, slippage caps, rate limits, multi-contract scoping, generic argument constraints. Generate from audited template families.

A defensible synthesizer-level decision rule:
- For each constraint the schema requires, check whether `simple_threshold` / `weighted_threshold` / `spending_limit` (in its SEP-41-transfer-only form) matches the constraint's shape.
- If yes → parameterize and emit an install_param.
- If no → emit a generated policy from the matching template family.
- Within the 5-policy ceiling, bundle compatible templates into a single generated contract where it reduces install cost or storage segregation complexity.

This shape is auditable: synthesizer logic + a finite set of audited template families, regardless of how many user policies eventually get generated.

### 7.6 Audit-target framing — the structural claim

The proposal's strongest structural claim is **synthesizer-as-audit-target**: rather than auditing every conceivable generated output, we audit:

1. The synthesizer logic (a finite TypeScript or Rust artifact).
2. A fixed set of template families (initially 5–7 — function whitelist, recipient allowlist, time lock, slippage cap, rate limit, multi-contract scope; possibly variable-position spending cap).
3. The simulation harness (including deny-case mutation rules).
4. A documented "standard generated outputs" corpus — sample outputs per template family that demonstrate the audited families work end-to-end.

This is a finite, defensible audit boundary. It's also honest: it doesn't claim we've audited every output, only that we've audited the things that produce outputs, and that we've shipped vetted templates.

### 7.7 What we explicitly do *not* build

Out-of-scope items worth naming in the proposal so reviewers see scope discipline:

- No new context-rule primitives.
- No on-chain policy compiler or verifier.
- No policy registry / sharing system.
- No new cryptographic verifier (Ed25519 and WebAuthn are sufficient).
- No general-purpose Soroban IDE.
- No account creation / passkey wallet beyond what pollywallet already provides.
- No multi-chain output (Solidity, Stylus, etc.).

---

## 8. Winnability Strategy

### 8.1 The narrative framing

A single sentence: *"This toolkit makes the OpenZeppelin smart-account framework safely useful from a transaction example, with code-first review, audit-bounded synthesis, and an agent surface that mirrors how teams already work."*

Five pillars:

**Pillar 1 — Built on, with, not around pollywallet.** Tyler is the author of the prior art and the broader Stellar smart-wallet infrastructure. We're partnering (ideally co-submission; alternatively, partner-with-attribution) and the proposal is explicit about what we keep, what we extend, what we replace.

**Pillar 2 — Audit-bounded synthesis.** The audit target is finite by construction. Synthesizer + N template families + simulation harness, regardless of output count.

**Pillar 3 — Agent-native delivery.** MCP server and Claude (plus equivalents) skill are headline deliverables, not afterthoughts. We mirror the patterns from OZ's existing Contracts MCP and Cloudflare's Agent Setup catalog.

**Pillar 4 — Composition where possible, codegen where required.** Honest about the OZ primitive set (only `simple_threshold`, `weighted_threshold`, `spending_limit`, the last hard-coded to SEP-41 transfer) and the implications: most realistic flows generate. Template-family approach minimizes the surface of generated novel code.

**Pillar 5 — Three real walkthroughs, end-to-end.** Blend yield-claim, SEP-41 subscription, bounded Soroswap delegation. Record → schema → policy → simulate → install → use, on testnet and (by tranche #3) mainnet.

### 8.2 What to emphasize in the proposal

- **The Tyler engagement** — first and most prominent. Either co-submission with him, or a partner-with-attribution structure he has signed off on. Reviewers will check whether this conversation has happened.
- **The audit boundary** — concretely defined: synthesizer, template families (listed), simulation harness. Auditor named, ideally with an exploratory conversation already held. Audit timed in the first half of the engagement, not the last.
- **The composition-first decision rule** — explicit algorithm, not hand-waved.
- **The deny-case generation strategy** — concrete: baseline mutations per template family, user-supplied counter-examples, simulation-verified.
- **The OZ engagement plan** — named OZ contact, monthly cadence, specific review checkpoints (synthesizer design after month 1, template families after month 2, generated code quality after month 3).
- **The C-Address Tooling cohort engagement** — once the cohort is selected, named wallet partner with a concrete integration milestone.

### 8.3 What to de-emphasize

- **Cloudflare-stack specifics.** Pollywallet leans heavily on Workers AI + Cloudflare Sandbox. For reviewers concerned about portability, this can read as lock-in. The synthesizer library should be runnable standalone; the hosted MCP can use Cloudflare.
- **GUI polish.** The RFP doesn't require a GUI; pollywallet has one already; attention spent on screens is attention away from architecture and safety.
- **Mainnet stunts.** Mainnet is the tranche #3 milestone but not the headline. Production-ready is the framing.
- **Speculative future work** (registries, marketplaces, sharing). Acknowledge briefly, move on.

### 8.4 Anticipated competitor archetypes

- **Tyler's own independent submission.** Highly likely if we don't engage him. Differentiator: we underwrite the audit, the agent-surface build, and ecosystem coordination he wouldn't naturally take on alone. **This decision must be made before drafting.**
- **A pure LLM-codegen approach.** "Give us the transaction, the model writes the policy." Differentiate by the audit story and template-family bounded surface.
- **A no-MCP web app.** GUI-first but no agent surface. Differentiate by leading with MCP / skill as headline.
- **OpenZeppelin extending their own Contracts MCP.** Unlikely (the RFP positions them as reviewers, not co-owners) but worth confirming.
- **Ethereum-experienced teams porting their session-key tooling.** Strong on smart-account theory, possibly weak on Soroban specifics. Differentiate by Soroban depth and pollywallet engagement.

### 8.5 Reviewer comfort checklist

Each of the following should have a specific named answer in the proposal:

- Does the team understand OZ's accounts framework at the level of `(smart_account, context_rule_id)` storage segregation, the `simple_threshold` signer-set divergence footgun, the `spending_limit` SEP-41-only constraint, and the delegated-signer simulation gap?
- Is the audit plan concrete (named auditor, scope, timing) or hand-wavy?
- Is the agent surface real (MCP tool list, skill commands) or an afterthought?
- Is the integration with a wallet planned (which wallet, what milestone) or aspirational?
- Is the timeline credible for $150K, four months, mainnet-readiness?
- Has the team engaged with OZ and Tyler/pollywallet, or is "we will coordinate" the only commitment?

---

## 9. Competitive & Comparable Initiatives

### 9.1 Direct prior art on Stellar

**`kalepail/pollywallet`** — covered in §5.2 and §5.3. The only known Stellar-native attempt at record-and-generate. Apache 2.0. ~7,000+ LOC working implementation. One contributor (Tyler). Pre-listed in our RFP as the starting point.

**OpenZeppelin Contracts MCP** — `mcp.openzeppelin.com`. Adjacent, not overlapping. Tokens-only on Stellar. Same brand family. We're complementary.

**OpenZeppelin Stellar accounts package** — `stellar-contracts/packages/accounts`. The contract substrate, not a competing tool. We build on top.

**Stellar Developer Tools and SDF SDKs** — Stellar Plus (Cheesecake Labs), the JS SDK's `assembleTransaction` helper. Used as plumbing, not competition.

**Other Stellar smart-wallet examples** — referenced from the SDF docs: "smart wallet that demonstrates the use of stateful policy signers" and another for multi-sig + policy signers. Most appear to be Tyler's repositories or derivatives.

### 9.2 SCF #44 submission overlap

**Vouchify — On-Chain Subscriptions for Merchants** ($135K, Open Track). Overlaps the SEP-41 subscription walkthrough conceptually. Vouchify is a merchant-facing application, not a policy-authoring tool, so it's adjacent rather than competing. We could plausibly partner: their subscription contracts as a target for our toolkit's "generate subscription policy" walkthrough.

No other SCF #44 submission visible in the public dashboard appears to directly compete with this RFP.

### 9.3 Analogous tooling in other ecosystems

**ERC-7579 SmartSession (Rhinestone × Biconomy).** The closest conceptual analog. Configurable session-key policies on ERC-7579 smart accounts (which contracts, which selectors, value ranges, lifetime). Differs in two ways: (1) policies are configured by user/agent up-front, not synthesized from observation; (2) no Stellar/Soroban port.

**ZeroDev session keys.** Production-grade session-key infrastructure on Kernel V3, 6M+ accounts, marketed explicitly as "great for AI agents." Same scoped-permission philosophy. Same gap (configured manually, not synthesized).

**Safe modules + Safe{Core} 7579 adapter.** Pluggable enforcement contracts on Ethereum smart accounts. No record-and-generate tooling.

**Rhinestone ModuleKit and module registry.** Tooling for writing modules, not generating them. The registry pattern (attested modules via ERC-7512) is interesting precedent for a future stretch but not in our RFP scope.

### 9.4 What we learn from the analogous work

- The scoped-session-key pattern is a battle-tested adoption driver on Ethereum. The pattern is the value proposition; Stellar has the substrate but not the tooling.
- "Synthesize from observed transaction" appears to be novel in the Ethereum ecosystem too. If we ship this well, it becomes interesting beyond Stellar — though multi-chain scope creep is a trap.
- Audit precedent (Cyfrin + Spearbit on SmartSession) sets the reviewer expectation. The SCF Soroban Audit Bank can likely cover ours; worth confirming.
- The attested-modules registry pattern is worth keeping as a stretch-goal pointer, not building.

---

## 10. Risks, Constraints & Red Flags

### 10.1 Technical risks

**The `spending_limit` SEP-41 constraint.** Only matches `symbol_short!("transfer")` with `args[2]` as the amount. For Soroswap (`swap_exact_amount_in`), Blend (`claim`, `submit`), Templar, or any non-SEP-41-transfer flow, the synthesizer must generate. Mitigation: explicit template families for each non-SEP-41 pattern.

**The delegated-signer simulation gap.** `simulateTransaction` doesn't return the delegated signer's auth entry. Clients construct two auth entries manually. CAP-71 pending. Our MCP server must handle this correctly when preparing install transactions for accounts whose signers include delegated addresses. External signers (passkey, ed25519) don't have this problem. Mitigation: document the gap explicitly in the MCP server's error messages; bias users toward external signers when possible.

**The `simple_threshold` / `weighted_threshold` signer-set divergence footgun.** Documented in OZ's own source as a known issue. Adding or removing signers without updating threshold can silently degrade security or cause DoS. Our synthesizer must warn users at install time and at any signer-modification time. Mitigation: emit warnings as part of `prepare_install_transaction`'s output and as part of any `add_signer` / `remove_signer` workflow.

**LLM-generated Rust quality variance.** Even with strong prompting (pollywallet's 1,253-line codegen prompt encodes many footguns), generated contracts can have subtle bugs: incorrect storage keying, missed `require_auth`, wrong default-reject behavior, Context lifetime issues. Mitigation: template families with the LLM filling parameterized slots only, not freeform writing entire contracts. Sandbox compile + simulation gates every output.

**Soroban simulation fidelity for policies that don't yet exist.** `simulateTransaction` simulates against current ledger state. To verify a generated policy, we install it to testnet first, then simulate against it. This is exactly what pollywallet's `verify-policy.mjs` does. Mitigation: budget for testnet install transactions as part of the verification flow; the sandbox compile loop catches everything pre-install.

**Recording-layer coverage gaps.** Some flows (DCA into Soroswap, regular yield claims) are parametric over time. The synthesizer needs to accept multiple recordings as generalization signals, not just one transaction. Mitigation: schema supports multiple recorded transactions; the synthesizer extracts the common pattern.

**Storage segregation correctness.** Generated policies that get `(smart_account, context_rule_id)` keying wrong can share state dangerously. This is a top audit-priority bug. Mitigation: every template family hard-codes the storage pattern; the LLM never writes storage code freeform.

### 10.2 Strategic risks

**Tyler contention.** If we don't engage him and he submits independently, we compete with the de-facto incumbent. If we engage him after he's already committed elsewhere, we lose the advantage. **Resolve before drafting.**

**The unpublished-RFP question.** The OZ accounts policy builder RFP is marked "Added Q2 2026" but isn't on the public SCF handbook RFP page (which lists six Q1 2026 RFPs only). This could mean: (a) the Q2 RFPs aren't yet published publicly even though they're settled internally; (b) the RFP we have is a draft awaiting approval; (c) it has been deprioritized and may not appear in #44. We should confirm with the SCF team that it's on the SCF #44 agenda before investing significant drafting effort.

**C-Address Tooling cohort timing.** The Q1 cohort hasn't been announced — those RFPs are in review. If the cohort isn't named before our submission deadline (June 14), we can't credibly commit to a specific cohort wallet partner. Mitigation: name pollywallet as the harness, commit to a cohort wallet for tranche #2 conditional on cohort announcement.

**OZ engagement realization.** The RFP says OZ is "interested in participating as a technical reviewer." If that engagement doesn't materialize concretely, we lose a major signal. Mitigation: confirm an OZ contact and a review cadence before drafting.

**Synthesizer-as-audit-target framing risk.** Reviewers may push back ("just audit the outputs"). We need to defend why finite synthesizer + N templates is more auditable than 1000s of outputs. The argument: bounded surface area, vetted patterns inherited by every output, mutation-tested deny cases.

**Mainnet timeline pressure.** SCF tranche #3 requires mainnet. Four months minus audit time minus remediation time minus testnet validation = tight. The audit must be planned for months 2–3, not month 4.

### 10.3 Red flags in the RFP itself

- **"At least one Stellar wallet supporting OZ smart accounts."** The set of such wallets is small in May 2026 — primarily pollywallet and forks. The C-Address Tooling cohort is meant to grow this set but hasn't yet.
- **"Versioned MCP server endpoint" in deliverables.** Hosted endpoint expected. Uptime becomes part of the deliverable; budget for hosting.
- **"Must commit to an audit of the synthesizer logic itself (not just sample outputs)."** Specific structural commitment. Our proposal needs to define what "synthesizer logic" includes and who will engage.
- **"Production-ready release with versioned MCP server endpoint and packaging for the Agent skill."** Hosted + packaged.
- **"Coordinate with the C-Address Tooling cohort."** Ongoing relationship commitment, not a one-time integration.

### 10.4 Scope-creep traps

The following are seductive but unfunded:

- A policy registry / marketplace.
- A general-purpose Soroban contract IDE.
- Multi-chain support (Solidity, Stylus output).
- Cross-account policy sharing UX.
- Account creation / passkey / wallet UI beyond what's needed for the integration demo.
- Generic Soroban contract generation (OZ's existing MCP covers this).

Each should be called out as explicitly out-of-scope in the proposal with one sentence each.

---

## 11. Open Questions for Clarification

### 11.1 Questions for the Stellar / SCF team

These should be resolved before drafting:

- **Is the OZ accounts policy builder RFP confirmed for SCF #44?** It is not visible on the public RFP-track page (which lists only Q1 RFPs). We need confirmation that we're proposing against a live, accepted RFP.
- **What is the C-Address Tooling cohort selection timeline?** Cohort wallets aren't yet known. Will they be announced before our June 14 submission?
- **Has OZ committed to a specific review cadence?** Who is the point of contact at OZ for the accounts package? Will they review the synthesizer design, the template families, the audit scope, generated outputs — or all of the above?
- **Is Tyler/kalepail planning to submit independently?** Does the SCF team view co-submission positively? Can the SCF team facilitate an introduction if we haven't connected directly?
- **Is the SCF Soroban Audit Bank available for the synthesizer + template families?** Or does the audit cost come from the $150K cap?
- **What's the SCF team's interpretation of "production-ready release"?** Mainnet-live by tranche #3 (which the funding structure implies), or testnet-with-mainnet-readiness?
- **Is the MCP server expected to be hosted SSE, distributable npm/Cargo, or both?** OZ's Contracts MCP supports both.

### 11.2 Questions we can resolve through further research

- The complete set of policy contracts currently in `OpenZeppelin/stellar-contracts/packages/accounts/src/policies/`. **Confirmed only `simple_threshold`, `weighted_threshold`, `spending_limit`** in v0.7.1.
- Stellar `simulateTransaction` response shape and whether it can model context-rule auth flows for rules that don't yet exist. **Confirmed: simulation works only against installed state; we install to testnet then simulate.**
- Whether OZ accepts policy primitive contributions from external teams. **Worth direct outreach to OZ Stellar accounts maintainers.**
- Realistic Blend / SEP-41 / Soroswap transaction shapes for the three walkthroughs. **Blend yield-claim sequence: pool.claim → BLND received → comet.swap_exact_amount_in to USDC → pool.submit with USDC. Each is a separate auth context.**
- Pollywallet license and Tyler's posture on derivative works. **Apache 2.0 confirmed; Tyler's posture needs direct outreach.**

### 11.3 Internal decisions for our team

- **Tyler approach** (dominant decision): co-submit, partner-with-attribution, or independent-with-credit?
- **Audit team**: which auditor (Cyfrin, Spearbit, OpenZeppelin's own audit team, ChainSafe, NCC, Halborn) do we propose? Has an exploratory conversation happened?
- **Mainnet ambition**: how aggressive on mainnet timeline given the four-month, $150K constraint?
- **Hosting commitments**: are we willing to operate a hosted MCP endpoint past the SCF deliverable window? For how long?
- **Multi-agent skill scope**: Claude only, or also Cursor / Codex / OpenCode / Windsurf?
- **GUI scope**: minimal review UI, or skip in favor of CLI + MCP + skill?
- **Template family inventory**: day-one list (likely function whitelist, recipient allowlist, time lock, slippage cap, rate limit, multi-contract scope, generic argument constraint) — final list with rationale?
- **Team composition**: Soroban depth in-house, or do we need a named Soroban contract engineer in the proposal team?
- **Schema posture**: keep `pollywallet-policy/v0`, version up to `oz-policy/v1`, or rename more aggressively?

---

## 12. Recommended Direction

### 12.1 The shape of the solution

A toolkit that takes the pollywallet codebase as starting point, adds the missing agent surfaces (MCP server + Claude/equivalents skill), tightens the safety story (audit-bounded synthesizer + finite template families + simulation-mutated deny harness), decouples synthesis from deployment (review-and-emit, not auto-deploy), and integrates with at least one C-Address Tooling cohort wallet beyond pollywallet itself. The deterministic schema is the public IR. Composition is preferred when an OZ primitive matches (rare beyond signer-thresholds and SEP-41 spending caps); template-family codegen otherwise.

Three documented walkthroughs end-to-end: Blend yield-claim, SEP-41 subscription, bounded Soroswap delegation. Record → analyze → schema → synthesize → simulate → prepare install transaction → user signs/installs → use. Apache 2.0. Hosted MCP plus distributable package. Library, MCP, and skill in one repo with clear boundaries.

### 12.2 The five strongest selling points

1. **Built on, with, not around pollywallet.** Co-submission or partner-with-attribution structure with Tyler. We adopt his ~7,000+ LOC, keep the deterministic schema, retain the AI codegen prompt (extending it), preserve the tx-analyzer and sandbox harness, replace the auto-deploy phase with code-first review, add MCP + skill + wallet partner + audit story.
2. **Audit-bounded synthesizer.** Finite audit target by construction. Synthesizer + 5–7 template families + simulation harness + standard generated-outputs corpus. Independent of how many user policies eventually get generated.
3. **Composition-first by design.** Concrete decision rule for when to parameterize an existing OZ primitive vs. generate from a template family. Honest about the OZ primitive set's actual coverage (SEP-41 transfers only for `spending_limit`).
4. **Agent-native delivery.** MCP + Claude (and equivalents) skill as primary surfaces. Mirrors the pattern from OZ's existing Contracts MCP and Cloudflare's Agent Setup catalog.
5. **End-to-end on real Stellar DeFi.** Three documented walkthroughs that exercise Blend, an SEP-41 token, and Soroswap — going from record to install to use on testnet through SCF tranche #2 and mainnet through SCF tranche #3.

### 12.3 Hard commitments vs. things to avoid committing to

**Commit to:**
- Synthesizer library, MCP server, Claude skill, simulation harness with deny-case mutation, wallet integration, three walkthroughs, audit, Apache 2.0.
- A specific list of template families with one-line rationale each.
- A named OZ contact and a concrete engagement cadence.
- A defined audit scope and a named auditor.
- Code-first / deploy-second workflow with no automatic deployment.
- At minimum Claude skill packaging; ideally also Cursor and one terminal agent.
- A specific Tyler engagement structure (co-submission or partner-with-attribution).

**Do not commit to:**
- Mainnet deployment of every demo (testnet sufficient through tranche #2; mainnet for tranche #3).
- A policy registry or sharing system.
- Multi-chain output.
- Wallet feature work beyond the integration surface needed for install/use.
- An exhaustive list of every conceivable generated policy.
- New context-rule primitives or verifier types.

### 12.4 Pre-drafting sequence

In priority order, before any proposal text is written:

1. **Tyler conversation.** Reach out via Stellar Discord, X, or GitHub. Determine whether co-submission is on the table. Get an answer in writing. This is the single highest-leverage move.
2. **SCF clarification ping.** Confirm (a) the RFP is on the SCF #44 agenda; (b) the C-Address Tooling cohort timeline; (c) audit bank availability for this specific scope; (d) OZ contact identity.
3. **OZ engagement.** Email the OZ Stellar accounts package maintainers. Share the synthesizer-as-audit-target framing and the template-family inventory. Ask for explicit review willingness and cadence.
4. **Soroban depth review.** Read the remaining OZ source we haven't covered (`smart_account/storage.rs` in detail, the verifier source). Confirm we can speak credibly about install flow construction, the delegated-signer gap, and the auth-digest binding.
5. **Template-family inventory shortlist.** Draft the five-to-seven template families with one-line rationale each. Confirm they cover the three walkthroughs.
6. **Auditor conversation.** Reach out to one or two auditors with Soroban experience (the universe is small — OZ themselves, Cyfrin, Spearbit, ChainSafe, Halborn). Scope cost and timing realistically against the four-month window.
7. **Architecture working session.** Internal session to lock the synthesizer/MCP/skill boundaries, schema versioning, simulation harness design — using this document as input.

If those seven items resolve, drafting is straightforward and every section of the proposal has a specific defensible answer ready. If item 1 (Tyler) returns no or unclear, we should reassess whether to submit at all — the alternative path (independent submission positioning against the established Stellar smart-wallet infrastructure) is materially weaker.

---

## Appendix A — Pollywallet code map (for internal reference)

| Path | LOC | Purpose | Reuse posture |
|---|---|---|---|
| `src/lib/tx-analyzer.ts` | 300 | XDR decoding, pattern extraction | Keep, extend |
| `src/lib/policy-schema.ts` | 452 | Deterministic IR with typed constraints | Keep, version up |
| `src/lib/policy-codegen.ts` | 1,253 | AI prompt assembly, system prompt with footgun encoding | Keep, audit, extend |
| `src/lib/policy-sandbox.ts` | 638 | Cloudflare Sandbox client | Keep, decouple from Cloudflare |
| `src/lib/policy-deploy.ts` | 418 | Deploy flow — currently auto-deploys | Split: keep "prepare install"; remove auto-deploy |
| `src/lib/contract-spec.ts` | 431 | WASM contract spec ingestion | Keep |
| `src/lib/passkey.ts` | 433 | Passkey signer | Keep as wallet integration reference |
| `src/lib/rule-management.ts` | 183 | Rule management logic | Keep |
| `src/lib/policy-store.ts` | 153 | Local policy storage | Keep |
| `src/lib/relayer.ts` | 129 | Channels relayer client | Keep |
| `src/lib/context-rules.ts` | 128 | Context-rule helpers | Keep |
| `src/routes/policies.tsx` | 1,058 | Policy builder GUI | Keep, refactor for review-step |
| `src/routes/rules.tsx` | 346 | Context-rule GUI | Keep |
| `e2e-policy-test/src/lib.rs` | 476 | Standalone deployable spending_limit reference | Reference; superseded by template families |
| `e2e-policy-test/verify-policy.mjs` | 549 | Six-phase testnet verification harness | Keep, generalize |
| `sandbox-worker/src/index.ts` | 443 | Cloudflare Sandbox HTTP service | Keep, allow alternative backends |

**Approximate total**: 7,400 LOC. None of this is throwaway. The proposal's "Building on existing work" answer should be specific to this table.

## Appendix B — OZ accounts framework hard facts (for internal reference)

- Package: `stellar-accounts` v0.7.1
- Dependencies: `soroban-sdk = "25.3.0"`
- Audits: v0.1.0-RC, v0.2.0, v0.3.0-rc.2, v0.5.0 + re-audit, v0.6.0, v0.7.0 in `/audits/`
- Hard limits: 15 context rules per account, 15 signers per rule, 5 policies per rule, 20 chars per rule name, 256 bytes per external key
- Policies that ship: `simple_threshold`, `weighted_threshold`, `spending_limit`
- Verifiers that ship: `ed25519` (32-byte keys, 64-byte sigs), `webauthn` (65-byte uncompressed P-256 keys, complex sig_data with authenticator + clientData)
- Policy trait methods: `install`, `enforce`, `uninstall` (no `can_enforce` despite stale docs)
- Storage segregation: `(smart_account_address, context_rule_id)` for stateful policies
- `spending_limit` constraints: requires `CallContract` context type; only handles `symbol_short!("transfer")`; reads amount at `args.get(2)`; MAX_HISTORY_ENTRIES = 1000; TTL extends 30 days on every read; error codes 3220–3227
- Auth digest: `sha256(signature_payload || context_rule_ids.to_xdr())` — context rule IDs are bound to prevent downgrade attacks
- AuthPayload: `{ signers: Map<Signer, Bytes>, context_rule_ids: Vec<u32> }` serialized as ScVal::Map with Symbol keys
- Known gap: delegated-signer auth entry not returned by `simulateTransaction`; manual two-entry construction required; CAP-71 pending

## Appendix C — Tyler's broader infrastructure (for internal reference)

- `kalepail/pollywallet` — this RFP's prior art
- `kalepail/passkey-kit` — TypeScript SDK for Stellar smart wallets, referenced from SDF docs
- `kalepail/launchtube` — gasless Soroban tx submission service; SDF maintains mainnet instance
- KALE — proof-of-teamwork project; per SDF blog, "the largest stress test for smart contracts on Stellar so far"
- Mercury Zephyr program for indexing smart-wallet events
- Stellar Protocol Discussion #1499 — WebAuthn smart-wallet contract interface proposal
- Active in #passkeys and #launchtube channels on Stellar Developer Discord
