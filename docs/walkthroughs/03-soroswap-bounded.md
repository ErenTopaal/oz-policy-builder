# Walkthrough 03 — Soroswap bounded delegated trading

> Cross-reference: raw artefacts under
> [`walkthroughs/03-soroswap-bounded/`](../../walkthroughs/03-soroswap-bounded/) —
> `source.json`, `recording.json`, `expected-spec-auto.json`,
> `expected-sim-report.json`, `expected-install-envelope.xdr`, and the
> compiled WASM under `wasm/slot_0/`.

---

## User story

> *"Let my trading bot call `swap_exact_tokens_for_tokens` on the Soroswap
> router — XLM → USDC only. It must not call any other function and must
> not touch any other token contract."*

This is the canonical **bounded DEX delegation** shape: a function
allowlist plus an asset allowlist pinning both the router and the first-leg
token. A compromised delegated signer cannot drain funds to an arbitrary
token via the same router.

---

## Recorded transaction

| Field             | Value                                                                                       |
|-------------------|---------------------------------------------------------------------------------------------|
| Network           | Stellar testnet                                                                             |
| Hash              | `7475b1690d155f114129e193503fef8a529e6c492f65c835a3a49a0242abf382`                          |
| Ledger            | `2575524`                                                                                   |
| Router contract   | `CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD` (Soroswap testnet router)         |
| Function          | `swap_exact_tokens_for_tokens(amount_in, amount_out_min, path, to, deadline) -> Vec<i128>`  |
| `amount_in`       | `100_000_000` stroops (10 XLM)                                                              |
| `amount_out_min`  | `1`                                                                                         |
| `path`            | `[CDLZFC3S… (XLM SAC), CB3TLW74… (testnet USDC SAC)]`                                       |
| On-chain return   | `[100000000, 1610680]` (10 XLM → 1.61 USDC base units)                                      |
| Status            | `SUCCESS`                                                                                   |

Explorer:

- StellarExpert — <https://stellar.expert/explorer/testnet/tx/7475b1690d155f114129e193503fef8a529e6c492f65c835a3a49a0242abf382>
- Horizon — <https://horizon-testnet.stellar.org/transactions/7475b1690d155f114129e193503fef8a529e6c492f65c835a3a49a0242abf382>

Router + token addresses were sourced from
[`soroswap/core` `testnet.contracts.json`](https://raw.githubusercontent.com/soroswap/core/main/public/testnet.contracts.json)
and
[`tokens.json`](https://raw.githubusercontent.com/soroswap/core/main/public/tokens.json).

---

## Synthesized `PolicySpec`

`synthesize --mode auto --tightness small-margin --lifetime 432000
--rule-name soroswap-bounded` emits a single Track-B Generated slot with
two constraints. In plain English:

> *"Allow calls to the Soroswap router's `swap_exact_tokens_for_tokens`
> function only. The call may target only the router itself or the
> XLM SAC. Reject any call to a different function on the router, or to
> any other contract."*

Spec excerpt (full file in
[`walkthroughs/03-soroswap-bounded/expected-spec-auto.json`](../../walkthroughs/03-soroswap-bounded/expected-spec-auto.json)):

```json
{
  "kind": "generated",
  "template_family": "function_allowlist",
  "constraints": [
    { "kind": "function_allowlist", "functions": ["swap_exact_tokens_for_tokens"] },
    { "kind": "asset_allowlist",     "assets":    ["CCJUD55…ROUTER", "CDLZFC3S…XLM SAC"] }
  ]
}
```

Compiled WASM hash:
`4e488f545daf1efd951bfbb787bbbee167f0d83b2e9c5b09ca06b8d4ace35f75` (under
[`walkthroughs/03-soroswap-bounded/wasm/slot_0/`](../../walkthroughs/03-soroswap-bounded/wasm/slot_0/)).

**Known synthesizer gap.** The `--tightness small-margin` flag was set
because Soroswap swaps need slippage tolerance, but the Phase 2 emission
for swap traces does not yet bind `amount_in` / `amount_out_min` into an
`amount_range` constraint. Phase 9 follow-up: richer DEX-aware extractors.
The committed spec is the actual output for the recording shape today —
see
[`walkthroughs/03-soroswap-bounded/README.md`](../../walkthroughs/03-soroswap-bounded/README.md)
"Synthesized policy".

---

## Simulation report

From
[`walkthroughs/03-soroswap-bounded/expected-sim-report.json`](../../walkthroughs/03-soroswap-bounded/expected-sim-report.json):

- **Permit** — `passed: true`. The recording replays through the
  installed Track-B WASM without panicking.
- **Deny vector 1** — `slot0_c0_function_allowlist_wrong_function`.
  Expects `FunctionNotAllowed (1010)`. **Passes** (1010 / 1010).
- **Deny vector 2** — `slot0_c1_asset_allowlist_wrong_asset`. Expects
  `AssetNotAllowed (1040)`. **Fails open** —
  `actual_error_code: null, passed: false`.

**Known deny-generator gap.** The `asset_allowlist` deny vector
([`crates/oz-policy-simhost/src/deny.rs:452`](../../crates/oz-policy-simhost/src/deny.rs))
substitutes an invalid C-StrKey placeholder
(`"CDISALLOWEDXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"`). The
generated policy's `Address::from_string(allowlist_entry)` traps inside
the host (strkey parse error) before the explicit `panic_with_error!(
AssetNotAllowed)` branch runs, so the vector trips with no `PolicyPanic`
code surfaced. Switching to a valid-but-not-allowlisted C-StrKey will
close the gap — Phase 9 follow-up.

`total_vectors: 2, passed: 1`.

---

## Install envelope

`prepare-install` succeeds at the build stage:

```bash
oz-policy-cli prepare-install walkthroughs/03-soroswap-bounded/expected-spec-auto.json \
  --smart-account CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A \
  --source GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ \
  --rpc https://soroban-testnet.stellar.org \
  --network "Test SDF Network ; September 2015" \
  --account-revision post-pr-655
```

- `host_function_count`: `1`
- `min_resource_fee`: `248138` stroops
- Base64 envelope: 1672 bytes; frozen at
  [`walkthroughs/03-soroswap-bounded/expected-install-envelope.xdr`](../../walkthroughs/03-soroswap-bounded/expected-install-envelope.xdr)

The envelope resolves the policy contract via the registry hit for
`TemplateFamily::FunctionAllowlist` on testnet
(`CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR`).

**Per-spec WASM hash question.** That deployed contract was uploaded with
the Phase 3 fixture's WASM (hash `cb2a8736…`) — **not** the WASM this
walkthrough's spec compiles to (hash `4e488f54…`). The installer's v1
design treats one deployed contract per template family as servicing all
slots; the per-slot constraint set is re-interpreted at install time
through the `InstallParams` payload. Whether that design holds for
Generated slots whose constraint set differs from the deployed instance's
compiled set is a **known open question**, tracked as Phase 9 work
(per-spec WASM hash verification, or per-deployment constraint pinning).
See walkthrough README "Install outcome".

---

## Outcome on testnet

The build path works end-to-end through `simulateTransaction`. The
on-chain `add_context_rule` happy path is closed for the function-allowlist
template family on testnet (2026-05-18, see walkthrough 01's frozen
evidence at
[`walkthroughs/phase7-testnet-install/install-result.json`](../../walkthroughs/phase7-testnet-install/install-result.json));
this walkthrough's envelope uses the same registry hit and the same
wallet-adapter encoder
([`wallet-adapter/src/oz_smart_account_auth.ts`](../../wallet-adapter/src/oz_smart_account_auth.ts),
`installPolicy` hook in commit `bd60009`).

The Phase-10 hosted endpoint and the mainnet canary that exercises a
real bounded-swap policy are TBD — track in [Operations](../operations.md)
and the Stream-D `docs/mainnet-readiness.md` doc.

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
