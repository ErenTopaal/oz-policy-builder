# Walkthrough 01 — Blend yield-claim

> Cross-reference: raw artefacts under
> [`walkthroughs/01-blend-yield/`](../../walkthroughs/01-blend-yield/) —
> `source.json`, `expected-recording.json`, `expected-spec-auto.json`,
> `expected-sim-report.json`, `expected-install-envelope.xdr`, and the
> compiled WASM under `wasm/slot_0/`.

---

## User story

> *"Let this agent claim my Blend yield weekly. It should be able to call
> `claim` on the pool, nothing else."*

This is the canonical **Generated** (Track B) shape: Blend's pool `claim`
function is not a SEP-41 transfer and does not match any OpenZeppelin
primitive's install-param shape, so the decision tree falls through to
Track B and emits a single `function_allowlist` slot pinning the allowed
function name to `{"claim"}`.

---

## Recorded transaction

| Field             | Value                                                                                                   |
|-------------------|---------------------------------------------------------------------------------------------------------|
| Network           | Stellar testnet (`Test SDF Network ; September 2015`)                                                   |
| Hash              | `5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e`                                      |
| Ledger            | `2572326`                                                                                               |
| Pool contract     | `CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF` (Blend `TestnetV2`)                          |
| Function          | `claim(from, reserve_token_ids: Vec<u32>, to) -> i128`                                                  |
| Reserve token IDs | `[0, 1, 2, 3, 4, 5, 6, 7]`                                                                              |
| BLND claimed      | `0` (source has no accrued emissions — successful no-op)                                                |
| Status            | `SUCCESS`                                                                                               |

Explorer:

- StellarExpert — <https://stellar.expert/explorer/testnet/tx/5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e>
- Horizon — <https://horizon-testnet.stellar.org/transactions/5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e>

The pool address came from the upstream
[`blend-utils` `testnet.contracts.json`](https://github.com/blend-capital/blend-utils/blob/main/testnet.contracts.json)
key `TestnetV2`. See
[`walkthroughs/01-blend-yield/README.md`](../../walkthroughs/01-blend-yield/README.md)
"How this fixture was produced" for the exact `stellar contract invoke`
incantation.

---

## Synthesized `PolicySpec`

`synthesize --mode auto --tightness exact --lifetime 432000 --rule-name blend-claim`
emits a Track-B Generated slot. In plain English:

> *"Allow any call to the Blend `TestnetV2` pool's `claim` function, by
> any signer in this rule's signer set, for the next 432 000 ledgers
> (~30 days). Reject any call to a different function on this pool."*

Frozen spec excerpt (full file in
[`walkthroughs/01-blend-yield/expected-spec-auto.json`](../../walkthroughs/01-blend-yield/expected-spec-auto.json)):

| Field                      | Value                                                                                  |
|----------------------------|----------------------------------------------------------------------------------------|
| `synthesis_mode`           | `auto`                                                                                 |
| `context_rule.name`        | `blend-claim`                                                                          |
| `context_rule.context_type`| `call_contract` → `CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF` (pool)    |
| `signers`                  | `[]` — install-time-resolved (the SA's pre-installed signer set authorises)            |
| `policies[0]`              | Generated `function_allowlist` with `functions: ["claim"]`                             |
| `lifetime_ledgers`         | `432000` (~30 days)                                                                    |
| Compiled WASM hash         | `c9b915b11beeece4c7439f4a81452c72550c3d40b788f82d97e0eef955b700b7`                     |

---

## Simulation report

The simhost replays the recording and runs the deny generator. From
[`walkthroughs/01-blend-yield/expected-sim-report.json`](../../walkthroughs/01-blend-yield/expected-sim-report.json):

- **Permit** — `passed: true`. The recording's `claim` call runs through
  the installed generated policy and does not panic.
- **Deny vector 1** — `slot0_c0_function_allowlist_wrong_function`. Flips
  the function name from `claim` to `xfer`; the policy must panic with
  `FunctionNotAllowed (1010)`. **Passes** —
  `expected_error_code: 1010, actual_error_code: 1010`.

`total_vectors: 1, passed: 1`. The deny suite is intentionally minimal
here because there's only one Generated constraint to perturb.

---

## Install envelope

`oz-policy-cli prepare-install` builds the install envelope from the spec
against the Phase 7 testnet smart account. Build path succeeds:

```bash
oz-policy-cli prepare-install walkthroughs/01-blend-yield/expected-spec-auto.json \
  --smart-account CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A \
  --source GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ \
  --rpc https://soroban-testnet.stellar.org \
  --network "Test SDF Network ; September 2015" \
  --account-revision post-pr-655
```

- `host_function_count`: `1`
- `min_resource_fee`: `253455` stroops
- Base64 envelope: 1776 bytes; frozen at
  [`walkthroughs/01-blend-yield/expected-install-envelope.xdr`](../../walkthroughs/01-blend-yield/expected-install-envelope.xdr)

The envelope resolves the deployed `function_allowlist` policy contract via
the [registry hit](../../crates/oz-policy-installer/src/registry.rs) for
testnet (`CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR`,
captured in
[`walkthroughs/phase7-testnet-install/deployed-addresses.json`](../../walkthroughs/phase7-testnet-install/deployed-addresses.json)).

---

## Outcome on testnet

The build path is verified end-to-end through testnet
`simulateTransaction`, and on-chain submission of an `add_context_rule`
transaction lands a SUCCESS on testnet — the Phase 7 happy path is
closed as of 2026-05-18 (see
[`walkthroughs/phase7-testnet-install/install-result.json`](../../walkthroughs/phase7-testnet-install/install-result.json)
for the frozen evidence, and
[`BLOCKER.md`](../../walkthroughs/phase7-testnet-install/BLOCKER.md)
for the historical diagnostic that drove the fix). The wallet-adapter
`AuthPayload` encoder
([`wallet-adapter/src/oz_smart_account_auth.ts`](../../wallet-adapter/src/oz_smart_account_auth.ts))
and the `installPolicy` integration hook (commit `bd60009`) are the
load-bearing pieces of the unblock.

A Phase-10 hosted endpoint and a recurring mainnet canary that exercises
this exact walkthrough are TBD — tracked in [Operations](../operations.md).

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
