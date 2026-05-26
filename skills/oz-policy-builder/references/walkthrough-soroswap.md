# Walkthrough — Soroswap delegated trading (pattern)

Use this reference when the user asks "give my trading bot a 100 USDC daily
Soroswap budget with 2% slippage" or any close variant.

> **Status: Phase 8 will freeze the corpus.** Unlike Blend (walkthrough 01)
> and the SEP-41 subscription (walkthrough 02), the Soroswap corpus does not
> yet have a frozen testnet hash + recording on disk. This file describes the
> **synthesizer pattern** the skill should drive — the prompt template in
> `crates/oz-policy-mcp/src/prompts.rs::synthesize_delegated_trading` is the
> canonical wizard. Once Phase 8 ships the
> `walkthroughs/03-soroswap-bounded/` corpus, replace the speculative shapes
> below with verbatim quotes from that README.

---

## Router function shape (from `docs.soroswap.finance` + plan.md §573–576)

The Soroswap router exposes:

```rust
swap_exact_tokens_for_tokens(
    e: Env,
    amount_in: i128,
    amount_out_min: i128,
    path: Vec<Address>,
    to: Address,
    deadline: u64,
) -> Vec<i128>
```

- `amount_in` — the input asset's i128 stroop amount.
- `amount_out_min` — minimum acceptable output (slippage protection).
- `path` — a vec of token addresses describing the swap route
  (e.g. `[USDC, XLM]` for a direct USDC→XLM swap).
- `to` — the recipient of the output tokens (typically the user's smart
  account).
- `deadline` — UNIX seconds after which the swap reverts.

The function name `swap_exact_tokens_for_tokens` contains the substring
`swap`, which is exactly what `scripts/propose_clarifications.py` triggers
on: it surfaces the slippage clarification automatically.

---

## How the synthesizer encodes this

Per `crates/oz-policy-mcp/src/prompts.rs` (the
`synthesize_delegated_trading` template) the spec is **Track-B (codegen)**.
A single `PolicySlot::Generated` slot encodes four sub-constraints:

1. `function_allowlist` — only Soroswap swap fns (`swap_exact_tokens_for_tokens`
   plus its `*_for_exact_tokens` siblings if the user wants both directions).
2. `asset_allowlist` — the comma-separated list of token addresses the user
   approved.
3. `amount_range` — derived from the per-leg cap plus the slippage cap. With
   slippage `S` bps and per-leg input `A`, the rule enforces
   `amount_out_min ≥ A · (1 - S/10000)`.
4. `call_frequency` — window `= 17_280` ledgers ≈ 1 day, `max_calls`
   derived from the daily budget divided by the per-leg cap.

The parent context rule is `CallContract(<router_address>)` — never
`Default`, because the rule should not permit swaps via other routers.

---

## How to drive the skill

Because there's no frozen hash yet, this walkthrough is **simulation-based**:

1. **Step 1 (ingest mode).** Choose `simulation`. Ask the user for the
   candidate envelope XDR they want to authorise (a base64
   `TransactionEnvelope`).
2. **Step 2 (`record_transaction`).** Call with
   `{network: "testnet", envelope_xdr_base64: "<xdr>"}`.
3. **Step 3 (summary).** `summarize_recording.py` will show:
   > This transaction on Stellar testnet was recorded via local simulation of
   > a caller-supplied envelope. It invokes
   > `swap_exact_tokens_for_tokens(…, 100,000,000 stroops, 95,000,000 stroops,
   > [3 items], …, …)` on contract `CROUTER…`. …
4. **Step 4 (clarifications).** `propose_clarifications.py` should surface:
   - The single observed amount (per-leg cap question — cap each call vs daily
     budget? For "daily budget" intent, answer "weekly_total" / daily total).
   - The slippage clarification (default `observed_plus_200bps`).
   - The delegated-signer clarification **if** the envelope is signed by a
     `Credentials::Address` agent already.
   - The `Default` context rule clarification **will not fire** if exactly one
     contract target (the router) is invoked — which is the expected shape.
5. **Step 5 (`synthesize_policy`).** Use:
   - `mode: codegen_only` — the constraint shape (function-allowlist + asset
     allowlist + amount range + call frequency) is not expressible as Track-A
     primitives.
   - `tightness: small_margin` — the user wants flexibility around the per-leg
     cap.
   - `delegated_signer: <agent_signer>` — the trading bot's public key.
   - `lifetime_ledgers: 518_400` (≈ 30 days) — or whatever rotation cadence
     the user wants.
6. **Step 6 (`simulate_policy`).** Pass these deny vectors via
   `extra_deny_vectors` if the auto-generated set doesn't already cover them:
   - A non-allowlisted token in `path`.
   - `amount_in` exceeding the per-leg cap.
   - `amount_out_min` below the observed slippage floor.
   - More than `max_calls` invocations within the daily window.
7. **Step 7 (`export_policy`).** `format: all`. Surface the generated Rust
   source so the user (or their auditor) can read it. Surface the WASM hash so
   they can pin it.
8. **Step 8.** Wallet signs the envelope. Tell the user the rule auto-expires
   after `lifetime_ledgers`; they can rotate the agent key + re-issue the rule
   to refresh.

---

## When Phase 8 freezes the corpus

Once `walkthroughs/03-soroswap-bounded/` is populated with a frozen testnet
hash and `expected-spec-auto.json`, replace the "How to drive" section above
with the same step-by-step structure used in `walkthrough-blend.md` and
`walkthrough-subscription.md`. Keep the "Router function shape" section as-is
(it's `docs.soroswap.finance` ground truth, not corpus-derived).
