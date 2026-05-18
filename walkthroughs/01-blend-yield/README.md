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

| File                       | Purpose                                                                                     |
|----------------------------|---------------------------------------------------------------------------------------------|
| `source.json`              | Minimal descriptor: hash + RPC + passphrase. Test harness reads this to drive the recorder. |
| `expected-recording.json`  | Frozen pretty-printed `Recording` for the hash above. Byte-equal to live recorder output.   |
| `README.md`                | This file.                                                                                  |

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
