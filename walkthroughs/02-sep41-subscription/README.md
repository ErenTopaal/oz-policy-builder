# walkthrough 02 — SEP-41 USDC-style transfer (Track-A spending_limit)

This walkthrough freezes a real Stellar testnet recording of a SEP-41
`transfer` invocation and the corresponding Track-A `PolicySpec` produced by
the Phase 2 decision tree. It is the Phase 2 binary completion fixture for
the OZ Accounts Policy Builder (see `plan.md` § "Phase 2 — Policy IR & Track
A synthesizer" *Verification / Test / Validation*).

The fixture exercises the cleanest possible decision-tree branch:

* Exactly one `Context::Contract` target in the recording.
* That target's `ContractRecord` is a SEP-41 `transfer(Address, Address,
  i128)` invocation, gated by `oz_policy_core::sep41::is_sep41_transfer`.
* Single auth entry with `Credentials::SourceAccount` (no soroban-auth
  payload to walk; the source account stands in for the signature).

Per `decision_tree.rs` the expected output is a single `PolicySlot::Existing
{ primitive: SpendingLimit, ... }` with `context_type = CallContract { CDG7…
}` per OZ PR-#649 — and no `SimpleThreshold` slot, because `Credentials::
SourceAccount` does not produce any `SignerSpec` entries (`build_signers` is
explicit: only `Credentials::Address` becomes a `SignerSpec`). See
`expected-spec-track-a.json` for the byte-frozen output.

## Source transaction

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
| Captured at           | `2026-05-15`                                                                                   |

Explorer links:

- StellarExpert: <https://stellar.expert/explorer/testnet/tx/52b86b5393b9ee936aa7b62638fb9d40fdbbed93ea6ac685e925205f52d50fcf>
- Horizon: <https://horizon-testnet.stellar.org/transactions/52b86b5393b9ee936aa7b62638fb9d40fdbbed93ea6ac685e925205f52d50fcf>

This is the same hash that backs the recorder's `simple_transfer.*.xdr.base64`
P1-T3 fixture (see `crates/oz-policy-recorder/tests/fixtures/README.md` for
the hex-level details); we reuse the on-chain transaction here because
Phase 2 inherits the recorder's exact decoding contract, and freezing the
walkthrough on a known-good hash keeps the round-trip auditable.

## Files

| File                          | Purpose                                                                                                   |
|-------------------------------|-----------------------------------------------------------------------------------------------------------|
| `source.json`                 | Minimal descriptor: hash + RPC + passphrase + on-chain context. The test harness reads this to drive the recorder. |
| `recording.json`              | Frozen pretty-printed `Recording` for the hash above. Byte-equal to live recorder output (verified by re-running). |
| `expected-spec-track-a.json`  | Frozen pretty-printed `PolicySpec` produced by `synthesize` under `mode=compose-only, tightness=exact, lifetime=432000, rule-name="sep41-subscription"`. |
| `README.md`                   | This file.                                                                                                |

## Shape of the recording

- `schema`: `oz-policy-builder/recording/v1`
- `contracts`: 1 — the SAC `transfer` invocation
- `auth_tree.roots`: 1 — `source_account` credentials, root invocation is the
  same `transfer` on the SAC, no sub-invocations
- `state_changes`: 4 — two ledger-entry change pairs (`Account`, `Ttl`) plus
  before/after `Balance` deltas on `from` and `to`
- `events`: 1 — the SAC's `transfer` event (topics `[Symbol("transfer"),
  Address(from), Address(to)]`, data `I128(51613347)`)

## Shape of the expected spec (`expected-spec-track-a.json`)

The decision tree compiles the recording above into:

- `synthesis_mode = "compose_only"` (frozen by the CLI flag)
- `context_rule.name = "sep41-subscription"`
- `context_rule.context_type = { kind: "call_contract", address: "CDG7..." }` —
  forced by OZ PR-#649 (`spending_limit` rejects `Default`)
- `signers = []` — `Credentials::SourceAccount` does not produce a signer,
  so under `compose_only` no `SimpleThreshold` slot is emitted either
  (decision tree §2d only adds `SimpleThreshold` when `signers` is non-empty)
- `policies = [PolicySlot::Existing { primitive: "spending_limit", params:
  { kind: "spending_limit", period_ledgers: 432000, limit_stroops_string:
  "51613347" } }]` — exactly one slot, observed `i128` amount carried through
  at `Tightness::Exact`
- `lifetime_ledgers = 432000`
- `recording_ref.hash = "52b86b53…d50fcf"`,
  `recording_ref.schema = "oz-policy-builder/recording/v1"`

This shape is what the Phase 2 binary completion gate
(`tests/phase2_completion.rs` in `oz-policy-installer`) asserts byte-equal.

## How this fixture was produced

1. Located the existing SEP-41 transfer hash in
   `crates/oz-policy-recorder/tests/fixtures/README.md` (the
   `simple_transfer.*.xdr.base64` fixture for `tests/decode_simple_transfer.rs`).
2. Verified the testnet RPC still returns it via
   ```
   curl -X POST -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction",
                     "params":{"hash":"52b86b53…"}}' \
     https://soroban-testnet.stellar.org
   ```
   (`status: SUCCESS`, within retention as of 2026-05-15).
3. Captured the recording:
   ```
   oz-policy-cli record \
     --hash 52b86b5393b9ee936aa7b62638fb9d40fdbbed93ea6ac685e925205f52d50fcf \
     --rpc https://soroban-testnet.stellar.org \
     --network "Test SDF Network ; September 2015" \
     > walkthroughs/02-sep41-subscription/recording.json
   ```
4. Synthesised the Track-A spec from that recording:
   ```
   oz-policy-cli synthesize walkthroughs/02-sep41-subscription/recording.json \
     --mode compose-only --tightness exact --lifetime 432000 \
     --rule-name "sep41-subscription" \
     > walkthroughs/02-sep41-subscription/expected-spec-track-a.json
   ```
5. Verified determinism: ran step 4 twice; `diff` returned 0 bytes.

## Stability contract — append-only

This fixture is **append-only**, same discipline as `walkthroughs/01-blend-yield/`.
Once frozen, the `hash`, the `recording.json`, the `expected-spec-track-a.json`,
and this README are not edited. The Stellar testnet RPC has a ~24 h retention
window for `getTransaction`, which means any test that re-records from the
network (e.g. `recorder::integration::blend_claim_roundtrip` against the
Phase 1 hash) will eventually fail with `E_RECORDER_HASH_NOT_FOUND` when the
source ledger ages out. That is expected and intentional — those tests are
`#[ignore]`-gated.

The Phase 2 binary completion gate (`oz-policy-installer/tests/phase2_completion.rs`)
deliberately reads the **frozen** `recording.json` and the **frozen**
`expected-spec-track-a.json` directly off disk, so it runs offline with no
network dependency and remains stable forever.

Rotating the hash (or any field in `source.json`) requires:

1. A new `walkthrough/0N-…/` directory beside this one.
2. An explicit decision in the plan / PR description that the old fixture is
   stale (don't silently overwrite).
3. Re-running the determinism check and committing the new
   `recording.json` + `expected-spec-track-a.json` alongside the new
   `source.json`.

The old directory stays in the tree as a historical reference. Append-only.
