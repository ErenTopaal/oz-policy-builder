# Walkthrough 02 — SEP-41 subscription (Track A `spending_limit`)

> Cross-reference: raw artefacts under
> [`walkthroughs/02-sep41-subscription/`](../../walkthroughs/02-sep41-subscription/) —
> `source.json`, `recording.json`, `expected-spec-track-a.json`,
> `expected-spec-auto.json`, `expected-sim-report.json`, and
> `expected-install-envelope-error.txt`.

---

## User story

> *"Authorize this app to pull up to 51 613 347 stroops (~5.16 USDC) per
> 30-day window from my smart account. Same SEP-41 contract every time."*

This is the canonical **Existing** (Track A) shape: a SEP-41 `transfer`
with a single observable `i128` amount fits the OpenZeppelin
`spending_limit` primitive verbatim. The decision tree composes
`spending_limit` and emits no Track-B WASM.

---

## Recorded transaction

| Field           | Value                                                                                          |
|-----------------|------------------------------------------------------------------------------------------------|
| Network         | Stellar testnet                                                                                |
| Hash            | `52b86b5393b9ee936aa7b62638fb9d40fdbbed93ea6ac685e925205f52d50fcf`                              |
| Ledger          | `2566000`                                                                                      |
| SAC contract    | `CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH` (SEP-41 SAC on testnet)             |
| Function        | `transfer(Address from, Address to, i128 amount)`                                              |
| `from`          | `GDTE7FQIUPKN6NMXK2T37GEJZRLQEQGYC55OQCZM7SASBWY36C4WCPZ4` (source = `from`)                   |
| `to`            | `CADQECDLVSOZUVANZWNNV2BO4U2APZYNXEYL34B2WFLETJI5OKMIOCQZ`                                      |
| `amount`        | `51 613 347` stroops                                                                            |
| Status          | `SUCCESS`                                                                                      |

Explorer:

- StellarExpert — <https://stellar.expert/explorer/testnet/tx/52b86b5393b9ee936aa7b62638fb9d40fdbbed93ea6ac685e925205f52d50fcf>
- Horizon — <https://horizon-testnet.stellar.org/transactions/52b86b5393b9ee936aa7b62638fb9d40fdbbed93ea6ac685e925205f52d50fcf>

---

## Synthesized `PolicySpec`

`synthesize --mode compose-only --tightness exact --lifetime 432000
--rule-name sep41-subscription` composes the upstream `spending_limit`
primitive. In plain English:

> *"Allow `transfer` on this SEP-41 SAC up to a rolling 51 613 347-stroop
> budget per 432 000 ledgers (~30 days). Reject any call exceeding the
> budget, any call to a different function, or any call to a different
> SAC."*

Frozen spec excerpt (full file in
[`walkthroughs/02-sep41-subscription/expected-spec-track-a.json`](../../walkthroughs/02-sep41-subscription/expected-spec-track-a.json)):

| Field                      | Value                                                                                              |
|----------------------------|----------------------------------------------------------------------------------------------------|
| `synthesis_mode`           | `compose_only` (auto-mode emits the same shape with `synthesis_mode: "auto"`)                      |
| `context_rule.name`        | `sep41-subscription`                                                                               |
| `context_rule.context_type`| `call_contract` → `CDG7N5LG…HKIH` (forced by OZ PR-#649 — `spending_limit` rejects `Default`)      |
| `signers`                  | `[]` — `Credentials::SourceAccount` produces no `SignerSpec`                                       |
| `policies[0]`              | `Existing(spending_limit)` with `period_ledgers: 432000`, `limit_stroops_string: "51613347"`       |
| `lifetime_ledgers`         | `432000`                                                                                           |

No Track-B WASM is produced; the policy address is resolved from the
[installer registry](../../crates/oz-policy-installer/src/registry.rs) at
install time.

---

## Simulation report

From
[`walkthroughs/02-sep41-subscription/expected-sim-report.json`](../../walkthroughs/02-sep41-subscription/expected-sim-report.json):

- **Permit** — `passed: true`. The simhost's `replay_recording` is a
  no-op when no Generated policies are installed for the contract under
  test — the Track-A `spending_limit` WASM is not vendored into
  `crates/oz-policy-simhost/vendor/`, so there's nothing to invoke.
- **Deny vectors** — three `slot0_spending_limit_*` perturbations
  (`amount_2x_cap`, `amount_just_over_cap`, `wrong_function`). All three
  expect `SpendingLimitExceeded (3221)` / `NotAllowed (3223)` panics. All
  three currently fail open (`actual_error_code: null, passed: false`)
  because the SpendingLimit WASM is not installed in the test host.

This is a **known gap, committed honestly** — closing it requires vendoring
the OZ `spending_limit.wasm` from `stellar-accounts 0.7.1` and teaching
`run::run_full_suite` to auto-install per-primitive WASMs. Tracked in
[`walkthroughs/02-sep41-subscription/README.md`](../../walkthroughs/02-sep41-subscription/README.md)
"Known gap (committed honestly)" as Phase 9 follow-up work.

---

## Install envelope

The build path **does not complete** for this walkthrough. From
[`walkthroughs/02-sep41-subscription/expected-install-envelope-error.txt`](../../walkthroughs/02-sep41-subscription/expected-install-envelope-error.txt):

```
E_INSTALL_PREFLIGHT_FAILED: primitive_address_unknown for SpendingLimit on Test SDF Network ; September 2015
```

Phase 7 Round 2 deployed only the `function_allowlist` Generated policy to
testnet (see
[`walkthroughs/phase7-testnet-install/deployed-addresses.json`](../../walkthroughs/phase7-testnet-install/deployed-addresses.json)).
`spending_limit` has no deployed address in the installer's registry, so
`prepare-install` cannot resolve the policy contract address and refuses
to build the envelope.

This is **NOT** the Phase 7 `__check_auth` BLOCKER — the build itself
short-circuits before auth payload encoding ever matters. Deploying
`spending_limit.wasm` on testnet and registering its address is a Phase 9
follow-up.

---

## Expected outcome on testnet

When the Phase 9 follow-up lands and `spending_limit` is registered, the
install envelope will build, the wallet adapter's `AuthPayload` encoder
([`wallet-adapter/src/oz_smart_account_auth.ts`](../../wallet-adapter/src/oz_smart_account_auth.ts))
will sign it, and the rule will land on testnet — same submission shape
as walkthrough 01.

Until then this walkthrough exercises:

- The Track-A decision-tree branch byte-for-byte (asserted by
  `oz-policy-installer/tests/phase2_completion.rs`).
- The honest preflight failure mode at the installer boundary.
- The downstream tooling's surfacing of `E_INSTALL_PREFLIGHT_FAILED` —
  the wallet-adapter examples
  ([`wallet-adapter/examples/README.md`](../../wallet-adapter/examples/README.md))
  report this verbatim as `status: "preflight_failed"`.

A Phase-10 mainnet canary using a $0.10 USDC `spending_limit` subscription
is TBD — track in `docs/mainnet-readiness.md` once Stream D begins.

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
