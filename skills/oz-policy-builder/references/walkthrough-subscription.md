# Walkthrough — SEP-41 subscription (Track-A `spending_limit`)

Use this reference when the user asks "authorize Vouchify to take $5 USDC from
my smart account monthly for 12 months" or any close variant. Everything below
is quoted verbatim from `walkthroughs/02-sep41-subscription/README.md`; do not
invent details that contradict that file. The walkthrough is **append-only**.

---

## Frozen source transaction

| Field                 | Value                                                                                          |
|-----------------------|------------------------------------------------------------------------------------------------|
| Network               | Stellar testnet (`Test SDF Network ; September 2015`)                                          |
| Transaction hash      | `52b86b5393b9ee936aa7b62638fb9d40fdbbed93ea6ac685e925205f52d50fcf`                              |
| Ledger                | `2566000`                                                                                      |
| Source account        | `GDTE7FQIUPKN6NMXK2T37GEJZRLQEQGYC55OQCZM7SASBWY36C4WCPZ4`                                      |
| Invoked contract      | `CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH` (SEP-41 SAC on testnet)              |
| Function              | `transfer(Address from, Address to, i128 amount) -> ()`                                        |
| `from`                | `GDTE7FQIUPKN6NMXK2T37GEJZRLQEQGYC55OQCZM7SASBWY36C4WCPZ4` (source = `from`)                    |
| `to`                  | `CADQECDLVSOZUVANZWNNV2BO4U2APZYNXEYL34B2WFLETJI5OKMIOCQZ` (destination contract)               |
| `amount`              | `51_613_347` stroops                                                                            |
| On-chain status       | `SUCCESS`                                                                                      |

---

## Why this fixture exercises the cleanest decision-tree branch

Quoted from `walkthroughs/02-sep41-subscription/README.md`:

> The fixture exercises the cleanest possible decision-tree branch:
>
> * Exactly one `Context::Contract` target in the recording.
> * That target's `ContractRecord` is a SEP-41 `transfer(Address, Address,
>   i128)` invocation, gated by `oz_policy_core::sep41::is_sep41_transfer`.
> * Single auth entry with `Credentials::SourceAccount` (no soroban-auth
>   payload to walk; the source account stands in for the signature).
>
> Per `decision_tree.rs` the expected output is a single `PolicySlot::Existing
> { primitive: SpendingLimit, ... }` with `context_type = CallContract { CDG7…
> }` per OZ PR-#649 — and no `SimpleThreshold` slot, because `Credentials::
> SourceAccount` does not produce any `SignerSpec` entries (`build_signers` is
> explicit: only `Credentials::Address` becomes a `SignerSpec`).

---

## Expected spec (`compose_only`, frozen)

Quoted from `walkthroughs/02-sep41-subscription/README.md`:

> The decision tree compiles the recording above into:
>
> - `synthesis_mode = "compose_only"` (frozen by the CLI flag)
> - `context_rule.name = "sep41-subscription"`
> - `context_rule.context_type = { kind: "call_contract", address: "CDG7..." }` —
>   forced by OZ PR-#649 (`spending_limit` rejects `Default`)
> - `signers = []` — `Credentials::SourceAccount` does not produce a signer,
>   so under `compose_only` no `SimpleThreshold` slot is emitted either
>   (decision tree §2d only adds `SimpleThreshold` when `signers` is non-empty)
> - `policies = [PolicySlot::Existing { primitive: "spending_limit", params:
>   { kind: "spending_limit", period_ledgers: 432000, limit_stroops_string:
>   "51613347" } }]` — exactly one slot, observed `i128` amount carried through
>   at `Tightness::Exact`
> - `lifetime_ledgers = 432000`
> - `recording_ref.hash = "52b86b53…d50fcf"`,
>   `recording_ref.schema = "oz-policy-builder/recording/v1"`

The byte-frozen output is in
`walkthroughs/02-sep41-subscription/expected-spec-track-a.json`.

---

## How to drive the skill against this corpus

1. **Step 1.** Ingest by `hash`.
2. **Step 2 (`record_transaction`).** `{network: "testnet", hash: "52b8…0fcf"}`.
3. **Step 3 (summary).** Expected summary is roughly:
   > This transaction on Stellar testnet was recorded from on-chain hash
   > `52b86b5393b9ee93…`. It invokes
   > `transfer(GDTE7F…CPZ4, CADQEC…OCQZ, 51,613,347 stroops)` on contract
   > `CDG7N5…HKIH`. It is signed by the source account itself (no delegated
   > signer). …
4. **Step 4 (clarifications).** `propose_clarifications.py` returns **one**
   question:
   > "The recording contains a single observed amount of 51613347 stroops.
   > Should the policy cap **each call** at that amount, or accept up to that
   > amount as a **weekly/monthly total** across many calls?"
   For a "monthly subscription" intent, the answer is "weekly/monthly total".
   Map that to `tightness: small_margin` and pick `lifetime_ledgers` per the
   user's stated period:
   - 7 days → `120_960`
   - 30 days → `518_400`
   - 12 months @ 30d each → `12 × 518_400 = 6_220_800` (then re-issue annually)
5. **Step 5 (`synthesize_policy`).** Recommended:
   - `mode: compose_only` — SEP-41 transfer is the canonical Track-A shape.
   - `tightness: small_margin` (per above).
   - `lifetime_ledgers` per the user's intent.
   - `rule_name: "sep41-subscription"` (or shorter — clamped at 20 bytes).
6. **Step 6 (`simulate_policy`).** Must show `permit.passed = true` plus
   deny vectors for: wrong recipient, over-limit amount, wrong token, expired
   rule.
7. **Step 7 (`export_policy`).** `format: install_envelope` is sufficient (no
   Track-B WASM is generated for compose_only). The envelope is what the user
   signs.
8. **Step 8.** Hand off. The user submits via Freighter / passkey-kit.

---

## Common follow-ups

- **"How do I revoke?"** → ask the user to call `remove_context_rule` from
  their wallet, or to wait for `valid_until` to lapse. The synthesizer does
  not handle revocation envelopes in v1.
- **"Can I pay any recipient?"** → no. `spending_limit` does not encode a
  recipient match; the synthesizer fences the rule to the token contract via
  `CallContract(<token>)`, so the agent can transfer to any address as long
  as the rolling total stays under the cap. If recipient pinning is
  required, ask the user — that's a Track-B request the synthesizer can
  encode as a `recipient_allowlist` slot.
- **"Can I use this for XLM (native)?"** → only via the native Stellar Asset
  Contract (SAC), not via classic payment ops. The recording must be of a
  Soroban `transfer` on the SAC contract address.
