# walkthrough 03 — Soroswap bounded delegated trading

This walkthrough freezes a real Stellar testnet recording of a Soroswap v1
router `swap_exact_tokens_for_tokens` invocation, plus the synthesized
Track-B `PolicySpec` produced by the Phase 2 decision tree, the compiled
policy WASM, the simulator's report, and the prepare-install envelope built
against the Phase 7 deployed smart account.

This is the canonical Phase 8 Walkthrough 3 fixture per `plan.md` § "Phase 8 —
Three end-to-end walkthroughs": a real DEX swap whose synthesized policy
pins the function name AND the targets (router + first-leg token) so a
compromised delegated signer cannot drain funds to an arbitrary token via
the same router.

## Source transaction

| Field                 | Value                                                                                              |
|-----------------------|----------------------------------------------------------------------------------------------------|
| Network               | Stellar testnet (`Test SDF Network ; September 2015`)                                              |
| Transaction hash      | `7475b1690d155f114129e193503fef8a529e6c492f65c835a3a49a0242abf382`                                 |
| Ledger                | `2575524`                                                                                          |
| Source account        | `GB6L7TKI77ZTTYAMQ2Z2YVSNLSADRFWNKTL5CPPNE7LDDZEKD6KI5C5K` (`p8-soroswap`, Friendbot-funded)       |
| Router contract       | `CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD` (Soroswap testnet router)               |
| Function              | `swap_exact_tokens_for_tokens(amount_in, amount_out_min, path, to, deadline) -> Vec<i128>`         |
| `amount_in`           | `100_000_000` stroops XLM (= 10 XLM)                                                               |
| `amount_out_min`      | `1` (intentionally loose to guarantee on-chain fill regardless of pool depth)                      |
| `path`                | `[CDLZFC3S… (XLM SAC), CB3TLW74… (testnet USDC SAC)]`                                              |
| `to`                  | `GB6L7TKI77ZTTYAMQ2Z2YVSNLSADRFWNKTL5CPPNE7LDDZEKD6KI5C5K` (source = self)                         |
| `deadline`            | `1_778_890_165` (~1 hour after submission)                                                         |
| On-chain return       | `[100000000, 1610680]` (10 XLM → 1.61 USDC base units at the on-chain rate)                        |
| On-chain status       | `SUCCESS`                                                                                          |
| Captured at           | `2026-05-16`                                                                                       |

Explorer links:

- StellarExpert: <https://stellar.expert/explorer/testnet/tx/7475b1690d155f114129e193503fef8a529e6c492f65c835a3a49a0242abf382>
- Horizon: <https://horizon-testnet.stellar.org/transactions/7475b1690d155f114129e193503fef8a529e6c492f65c835a3a49a0242abf382>

The router address was taken from the upstream
<https://raw.githubusercontent.com/soroswap/core/main/public/testnet.contracts.json>
(`ids.router`). The XLM and USDC SAC addresses were taken from
<https://raw.githubusercontent.com/soroswap/core/main/public/tokens.json>.

## How this fixture was produced (Option B — composed from scratch)

The Soroswap router's recent on-chain activity at the time of capture
(2026-05-16) is dominated by *sub-invocations* from third-party aggregators
and oracle contracts — root-level invocations of `swap_exact_tokens_for_tokens`
from EOAs are sparse. Rather than risk picking a fixture that aged out of
testnet RPC retention, the fixture was *composed*:

1. Generated a fresh testnet keypair:
   `stellar keys generate p8-soroswap --network testnet --fund --overwrite`.
2. Confirmed Friendbot funded it with 10000 XLM.
3. Resolved the router + token addresses (see "Source transaction" above).
4. Submitted the swap:
   ```
   stellar contract invoke --network testnet --source-account p8-soroswap --send=yes \
     --id CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD \
     -- swap_exact_tokens_for_tokens \
     --amount_in 100000000 \
     --amount_out_min 1 \
     --path '["CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC", "CB3TLW74NBIOT3BUWOZ3TUM6RFDF6A4GVIRUQRQZABG5KPOUL4JJOV2F"]' \
     --to GB6L7TKI77ZTTYAMQ2Z2YVSNLSADRFWNKTL5CPPNE7LDDZEKD6KI5C5K \
     --deadline 1778890165
   ```
   Returned `["100000000","1610680"]`. The diagnostic events surfaced a
   `SoroswapRouter::swap` event plus a `SoroswapPair::swap` sync — proving
   the call landed via the real router (no mock).
5. Recorded:
   ```
   oz-policy-cli record \
     --hash 7475b1690d155f114129e193503fef8a529e6c492f65c835a3a49a0242abf382 \
     --rpc https://soroban-testnet.stellar.org \
     --network "Test SDF Network ; September 2015" \
     > walkthroughs/03-soroswap-bounded/recording.json
   ```
   Verified determinism: re-ran the same command; `diff` returned 0 bytes.

## Files

| File                                  | Purpose                                                                                                          |
|---------------------------------------|------------------------------------------------------------------------------------------------------------------|
| `source.json`                         | Minimal descriptor: hash + RPC + passphrase + on-chain context.                                                  |
| `recording.json`                      | Frozen pretty-printed `Recording` for the hash above. Byte-equal to live recorder output (verified by re-running). |
| `expected-spec-auto.json`             | Phase 2 synthesizer output, `--mode auto --tightness small-margin --lifetime 432000 --rule-name "soroswap-bounded"`. |
| `wasm/slot_0/source.rs`               | Phase 3 codegen-rendered Rust source for the Generated slot (function + asset allowlist).                        |
| `wasm/slot_0/policy.wasm`             | Phase 3 optimised WASM bytes for slot 0.                                                                         |
| `wasm/slot_0/wasm_hash.txt`           | Lowercase-hex SHA-256 of `policy.wasm` (= `4e488f545daf1efd951bfbb787bbbee167f0d83b2e9c5b09ca06b8d4ace35f75`).   |
| `expected-sim-report.json`            | Phase 4 simhost `SimReport`. Permit replays cleanly; 2 deny vectors (1 passes, 1 fails open — see Known gap).    |
| `expected-install-envelope.xdr`       | Phase 2 install envelope XDR built via `prepare-install` against the Phase 7 testnet SA.                         |
| `README.md`                           | This file.                                                                                                       |

## Synthesized policy

The decision tree under `--mode auto` emits a single Track-B Generated
slot with the `function_allowlist` template family carrying two
constraints:

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

Note the `--tightness small-margin` flag was set per Phase 8 brief
(Soroswap swaps need slippage tolerance), but the synthesizer's
current Phase-2 emission for swap traces doesn't yet bind `amount_in` /
`amount_out_min` into an `amount_range` constraint — that's a follow-up
tracked as Phase 9 work (richer DEX-aware extractors). The committed
spec is the actual output for the recording shape today.

### What this policy permits

- Calls to `CCJUD55…ROUTER.swap_exact_tokens_for_tokens(...)` with ANY
  argument values, as long as the call's target contract is the router
  itself OR the first-leg XLM SAC (no other contracts can be invoked).
- Any pre-authorised signer can invoke; the signer set is install-time
  -resolved.
- Valid for the next ~30 days of ledger sequence (`lifetime_ledgers: 432000`).

### What this policy denies

The Phase 4 deny generator emits two regression vectors:

| Name                                            | Expected panic               | Outcome on this corpus              |
|-------------------------------------------------|------------------------------|-------------------------------------|
| `slot0_c0_function_allowlist_wrong_function`    | `FunctionNotAllowed (1010)`  | **PASS** (panics with 1010)         |
| `slot0_c1_asset_allowlist_wrong_asset`          | `AssetNotAllowed (1040)`     | FAIL open (panics with host error)  |

**Known gap (committed honestly).** The `asset_allowlist` deny vector
generator (`crates/oz-policy-simhost/src/deny.rs:452`) builds a "wrong
asset" payload by substituting an invalid C-StrKey placeholder
(`"CDISALLOWEDXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"`). The
generated policy's `Address::from_string(allowlist_entry)` succeeds,
then comparison against the invalid placeholder traps inside the host
(strkey parse error) before the explicit `panic_with_error!(e,
AssetNotAllowed)` branch executes — so the vector trips with no
`PolicyPanic` code surfaced (`actual_error_code: null`). Switching the
generator to use a *valid* C-StrKey not on the allowlist will close this
gap; it's tracked as a Phase 9 deny-generator hardening item (and
explicitly NOT patched here, to keep this corpus a faithful snapshot of
the simulator's current observable behaviour).

### Install outcome

`prepare-install` against the Phase 7 testnet smart-account succeeds at
the build stage:

```
oz-policy-cli prepare-install walkthroughs/03-soroswap-bounded/expected-spec-auto.json \
  --smart-account CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A \
  --source GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ \
  --rpc https://soroban-testnet.stellar.org \
  --network "Test SDF Network ; September 2015" \
  --account-revision post-pr-655
```

- `host_function_count`: `1`
- `min_resource_fee`: `248138` stroops
- `envelope_xdr_base64` saved to `expected-install-envelope.xdr` (1672 bytes
  base64).

The envelope resolves the policy contract address via the registry hit
for `TemplateFamily::FunctionAllowlist` on testnet
(`CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR`). That deployed
contract was uploaded with the Phase 3 fixture's WASM
(hash `cb2a8736…`) — **NOT** the WASM this walkthrough's spec compiles to
(hash `4e488f54…`). The installer's v1 design treats one deployed contract
per template family as servicing all slots; the per-slot constraint set is
re-interpreted at install time through the `InstallParams` payload. Whether
that design holds for a Generated slot whose constraint set differs from
the deployed instance's compiled set is **a known open question**: tracked
as Phase 9 work (per-spec WASM hash verification, or per-deployment
constraint pinning).

**On-chain submission is BLOCKED by the same Phase 7 issue documented in
`walkthroughs/phase7-testnet-install/BLOCKER.md`** — the SA's
`__check_auth` traps on a `Void` `AuthPayload`. The envelope here is
correct shape-wise; landing it requires the AuthPayload-encoder helper
that Stream B of Phase 8 is shipping in `wallet-adapter/`. No
`expected-install-envelope-error.txt` exists because the *build*
succeeded; the failure mode is described once, centrally, in the Phase 7
BLOCKER doc.

## Stability contract — append-only

This fixture follows the same append-only discipline as `01-blend-yield/`
and `02-sep41-subscription/`. Once frozen, the `hash`, the `recording.json`,
the `expected-spec-auto.json`, the WASM artifacts, the sim report, the
envelope XDR, and this README are not edited. The Stellar testnet RPC has
a ~24 h retention window for `getTransaction`, so any re-recording from
the network will eventually fail with `E_RECORDER_HASH_NOT_FOUND` once
the source ledger ages out — that's expected and intentional.

Rotating the hash (or any field in `source.json`) requires a new
`walkthrough/0N-…/` directory beside this one plus an explicit decision
in the plan / PR description that the old fixture is stale. The old
directory stays in the tree as a historical reference.
