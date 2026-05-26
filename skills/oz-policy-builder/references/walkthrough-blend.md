# Walkthrough — Blend yield-claim

Use this reference when the user asks "let this agent claim my Blend yield
weekly" or any close variant. Everything below is quoted verbatim from
`walkthroughs/01-blend-yield/README.md`; do not invent details that contradict
that file. The walkthrough is **append-only** — the hash and the corpus are
frozen forever.

---

## Frozen source transaction

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

Pool source: `blend-utils` `testnet.contracts.json`, key `TestnetV2`.
Referenced asset: BLND reward token
`CB22KRA3YZVCNCQI64JQ5WE7UY2VAV7WFLK6A2JN3HEX56T2EDAFO7QF`.

---

## Expected recording shape

Quoted from `walkthroughs/01-blend-yield/README.md`:

> - `schema`: `oz-policy-builder/recording/v1`
> - `contracts`: 1 — the pool `claim` invocation
> - `auth_tree.roots`: 1 — `source_account` credentials, `Contract` invocation function
> - `state_changes`: 17 — reserve b/dToken accounting + emissions config entries
>   the host touched during the no-op claim
> - `events`: 1 — the pool's `claim` event (topics: `[Symbol("claim"),
>   Address(from)]`)

The actual recording is at `walkthroughs/01-blend-yield/expected-recording.json`
(byte-frozen).

---

## How to drive the skill against this corpus

1. **Step 1 (ingest mode).** The user has the hash above. Choose `hash` mode.
2. **Step 2 (`record_transaction`).** Call with
   `{network: "testnet", hash: "5a0c…db4e"}`.
3. **Step 3 (summary).** Pipe the recording into
   `scripts/summarize_recording.py`. Expected wording is roughly:
   > This transaction on Stellar testnet was recorded from on-chain hash
   > `5a0ccffed7aa586f…`. It invokes `claim(…, [8 items], …)` on contract
   > `CCEBVD…4HGF`. It is signed by the source account itself (no delegated
   > signer). …
4. **Step 4 (clarifications).** Pipe the recording into
   `scripts/propose_clarifications.py`. **Expected output is `[]`** — Blend's
   `claim` has no observable i128 args, no delegated signer in the credentials,
   no swap function, and exactly one contract target (the pool). Nothing to
   ask. Move on.
5. **Step 5 (`synthesize_policy`).** Recommended params:
   - `mode: auto` — `claim` is not a SEP-41 transfer, so `compose_only` would
     fail; `auto` lets the synthesizer pick Track-B if needed.
   - `tightness: small_margin` — Blend's reserve list can grow when new
     reserves are added.
   - `delegated_signer: <new agent key>` — the user is delegating the future
     `claim` to a fresh agent, so generate a fresh key (or accept the one
     they already have).
   - `lifetime_ledgers: 120960` (≈ 7 days) — matches the user's "weekly"
     phrasing.
6. **Step 6 (`simulate_policy`).** Must report `permit.passed = true` and all
   deny vectors `rejected = true` before moving on. The deny vectors exercise:
   wrong contract target, wrong function, wrong reserve_token_ids shape, missing
   delegated signer.
7. **Step 7 (`export_policy`).** `format: all`, `account_revision:
   post_pr_655`. Surface the WASM hash + envelope URI to the user.
8. **Step 8.** Stop. The user signs the envelope with Freighter or passkey-kit.
   They run `verify_install` later with the resulting `context_rule_id`.

---

## Why this synthesizes to Track-B

Quoted from the synthesizer ground truth (`crates/oz-policy-core/src/decision_tree.rs`):

- `claim(from, reserve_token_ids, to)` is **not** a SEP-41 `transfer`, so it
  cannot compose to `spending_limit`.
- The auth tree uses `Credentials::SourceAccount`, so no signer-derived
  `simple_threshold` slot is emitted under `compose_only`.
- The resulting spec under `auto` mode therefore contains a single Track-B
  `PolicySlot::Generated` slot encoding: function_allowlist (`claim` only),
  call_target match (the pool address), and a call_frequency window
  (≈ 1 week).

The `valid_until` field on the context rule is set to `current_ledger +
lifetime_ledgers` so the rule auto-expires after the chosen window. The user
can re-run the skill to refresh it.
