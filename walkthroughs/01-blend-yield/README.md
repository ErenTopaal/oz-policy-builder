# walkthrough 01 — Blend yield-claim

This walkthrough freezes a real Stellar testnet recording of a Blend v2 pool
`claim` invocation. It is the Phase 1 binary completion fixture for the OZ
Accounts Policy Builder (see `plan.md` § "Phase 1 — Foundations", P1-T4) and
is the primary input the Phase 2 synthesizer will be developed against.

## Source transaction

| Field                 | Value                                                                                          |
|-----------------------|------------------------------------------------------------------------------------------------|
| Network               | Stellar testnet (`Test SDF Network ; September 2015`)                                          |
| Transaction hash      | `5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e`                              |
| Ledger                | `2572326`                                                                                      |
| Source account        | `GATJIJRQXBCGP25K4NLG532UMO4PC4FE7O64P4XSHOKFPBQF6TDTGAJN`                                      |
| Invoked contract      | `CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF` (Blend `TestnetV2` pool)             |
| Function              | `claim(from, reserve_token_ids: Vec<u32>, to) -> i128`                                         |
| Reserve token IDs     | `[0, 1, 2, 3, 4, 5, 6, 7]` — all reserve b/dToken indices for the pool's 4 reserves            |
| On-chain status       | `SUCCESS`                                                                                      |
| BLND claimed          | `0` (source has no accrued emissions — the call is a successful no-op)                         |
| Captured at           | `2026-05-15`                                                                                   |

Explorer links:

- StellarExpert: <https://stellar.expert/explorer/testnet/tx/5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e>
- Horizon: <https://horizon-testnet.stellar.org/transactions/5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e>

The pool contract address was taken from the upstream Blend `blend-utils`
`testnet.contracts.json` (key `TestnetV2`). Other entries from that file the
recording also references via auth/state changes include the `BLND` reward
token (`CB22KRA3YZVCNCQI64JQ5WE7UY2VAV7WFLK6A2JN3HEX56T2EDAFO7QF`) and the
`backstopV2` contract (`CBDVWXT433PRVTUNM56C3JREF3HIZHRBA64NB2C3B2UNCKIS65ZYCLZA`).

## Files

| File                                 | Purpose                                                                                              |
|--------------------------------------|------------------------------------------------------------------------------------------------------|
| `source.json`                        | Minimal descriptor: hash + RPC + passphrase. Test harness reads this to drive the recorder.          |
| `expected-recording.json`            | Frozen pretty-printed `Recording` for the hash above. Byte-equal to live recorder output.            |
| `expected-spec-auto.json`            | Phase 2 synthesizer output, `--mode auto --tightness exact --lifetime 432000`. Track-B (Generated).  |
| `wasm/slot_0/source.rs`              | Phase 3 codegen-rendered Rust source for the Generated `function_allowlist` slot.                    |
| `wasm/slot_0/policy.wasm`            | Phase 3 optimised WASM bytes for slot 0.                                                             |
| `wasm/slot_0/wasm_hash.txt`          | Lowercase-hex SHA-256 of `policy.wasm`.                                                              |
| `expected-sim-report.json`           | Phase 4 simhost `SimReport` — permit replays the recording; 1 generated deny vector flips `claim`.   |
| `expected-install-envelope.xdr`      | Phase 2 install envelope XDR built via `prepare-install` against the Phase 7 testnet deployment.     |
| `README.md`                          | This file.                                                                                           |

## Shape of the recording

- `schema`: `oz-policy-builder/recording/v1`
- `contracts`: 1 — the pool `claim` invocation
- `auth_tree.roots`: 1 — `source_account` credentials, `Contract` invocation function
- `state_changes`: 17 — reserve b/dToken accounting + emissions config entries the host
  touched during the no-op claim
- `events`: 1 — the pool's `claim` event (topics: `[Symbol("claim"), Address(from)]`)

## How this fixture was produced

1. Generated a fresh testnet keypair: `stellar keys generate p1t4-blend --overwrite --network testnet`.
2. Funded it via Friendbot: `curl https://friendbot.stellar.org/?addr=<G_addr>`.
3. Sourced the Blend testnet pool address from
   <https://github.com/blend-capital/blend-utils/blob/main/testnet.contracts.json>
   (key `TestnetV2`).
4. Read the pool's reserve list via
   `stellar contract invoke ... -- get_reserve_list` (4 reserves: XLM, wETH, wBTC, USDC →
   reserve_token_ids `0..=7` for the b/dToken pairs).
5. Submitted the `claim` transaction:
   ```
   stellar contract invoke --network testnet --source-account p1t4-blend \
     --id CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF -- claim \
     --from GATJIJRQXBCGP25K4NLG532UMO4PC4FE7O64P4XSHOKFPBQF6TDTGAJN \
     --reserve_token_ids '[0,1,2,3,4,5,6,7]' \
     --to GATJIJRQXBCGP25K4NLG532UMO4PC4FE7O64P4XSHOKFPBQF6TDTGAJN
   ```
6. Captured the resulting hash and froze the recorder output:
   ```
   oz-policy-cli record \
     --hash 5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e \
     --rpc https://soroban-testnet.stellar.org \
     --network "Test SDF Network ; September 2015" \
     > walkthroughs/01-blend-yield/expected-recording.json
   ```
7. Verified determinism: ran the same command twice; `diff` returned 0 bytes.

## Stability contract — append-only

This fixture is **append-only**: once frozen, the `hash`, the
`expected-recording.json`, and this README are not edited. The Stellar testnet
RPC has a ~24 h retention window for `getTransaction`, which means the test
`recorder::integration::blend_claim_roundtrip` will eventually fail with
`E_RECORDER_HASH_NOT_FOUND` when the source ledger ages out. That is expected
and intentional: the test is `#[ignore]`-gated for exactly this reason — it
runs only on demand against a live testnet, and a new `walkthrough/0N-…/`
directory must be added when a fresh fixture is needed rather than rotating
the hash in this directory.

Rotating the hash (or any field in `source.json`) requires:

1. A new `walkthrough/0N-…/` directory beside this one.
2. An explicit decision in the plan / PR description that the old fixture is
   stale (don't silently overwrite).
3. Re-running the determinism check and committing the new
   `expected-recording.json` alongside the new `source.json`.

The old directory stays in the tree as a historical reference. Append-only.

## Phase 8 — synthesized policy

The Blend `claim` function isn't a SEP-41 `transfer`, so it doesn't fit any
Track-A (Existing) primitive. The synthesizer's `auto` mode falls through to
Track-B (Generated) and emits a single `function_allowlist` slot pinning the
allowed function name set to `{"claim"}`. The full spec:

| Field                      | Value                                                                                  |
|----------------------------|----------------------------------------------------------------------------------------|
| `context_rule.name`        | `blend-claim`                                                                          |
| `context_rule.context_type`| `call_contract` → `CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF` (pool)    |
| `signers`                  | `[]` — install-time-resolved (the SA's pre-installed signer set authorises)            |
| `policies[0]`              | Generated `function_allowlist` with `functions: ["claim"]`                             |
| `lifetime_ledgers`         | `432000` (~30 days)                                                                    |
| `policies[0]` WASM hash    | `c9b915b11beeece4c7439f4a81452c72550c3d40b788f82d97e0eef955b700b7` (committed)         |

### What this policy permits

- Calls to `CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF.claim(...)`
  with any argument shape, by any pre-authorised signer, for the next ~30 days
  of ledger sequence.

### What this policy denies

The Phase 4 deny generator emits one regression vector:

- `slot0_c0_function_allowlist_wrong_function` — flips the recording's
  `claim` call to a different function name (`xfer`); the generated policy
  must panic with `E_POLICY_VIOLATION` (code `1010`).

The committed `expected-sim-report.json` shows `permit.passed=true` plus
this single deny vector passing — confirming the policy is exactly as tight
as the spec advertises and as loose as the recording requires.

### Install outcome

`prepare-install` succeeds and emits a 1776-byte base64 XDR envelope
(committed as `expected-install-envelope.xdr`):

```
oz-policy-cli prepare-install walkthroughs/01-blend-yield/expected-spec-auto.json \
  --smart-account CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A \
  --source GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ \
  --rpc https://soroban-testnet.stellar.org \
  --network "Test SDF Network ; September 2015" \
  --account-revision post-pr-655
```

- `host_function_count`: `1`
- `min_resource_fee`: `253455` stroops

The build path passes preflight (post-PR-655 SA recognised, policy address
resolved via the `oz-policy-installer` registry hit for the
`function_allowlist` testnet deployment at
`CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR`).

**On-chain submission is verified end-to-end as of 2026-05-18.** The
historical `__check_auth` trap (see
`walkthroughs/phase7-testnet-install/BLOCKER.md`) is closed by the
AuthPayload-encoder helper at `wallet-adapter/src/oz_smart_account_auth.ts`
plus the `installPolicy` `ozAuthPayloadEncoder` hook (commit `bd60009`).
The frozen testnet SUCCESS evidence lives at
`walkthroughs/phase7-testnet-install/install-result.json`.
