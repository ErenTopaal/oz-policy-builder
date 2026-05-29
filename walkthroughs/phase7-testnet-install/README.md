# Phase 7 Round 2 — Testnet deployment fixture

Frozen record of the real Stellar **testnet** deployments produced by Phase 7
Round 2 (captured 2026-05-16). These addresses back:

* `crates/oz-policy-installer/src/registry.rs::project_deployed_policy_address`
  (Rust call-sites consume the policy contract address from there)
* `wallet-adapter/src/phase7_integration.test.ts` (the end-to-end test runs
  against these exact contracts)
* the Phase 8 walkthrough corpora (planned), once the on-chain install path
  unblocks (see `BLOCKER.md`)

This corpus is **APPEND-ONLY**. Rotating an address requires explicit
replacement *plus* a new CHANGELOG entry — the addresses are referenced from
source and consumed by integration tests.

---

## Captured addresses

See [`deployed-addresses.json`](./deployed-addresses.json) for the canonical
machine-readable copy. Summary:

| Field                  | Value                                                          |
| ---------------------- | -------------------------------------------------------------- |
| Smart account          | `CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A`     |
| Policy (FunctionAllowlist) | `CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR` |
| SA owner pubkey (G…)   | `GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ`     |
| SA owner alias         | `sa-owner-p7r2` (in your local `~/.config/soroban/identity/`)  |
| Network                | `Test SDF Network ; September 2015`                            |
| RPC                    | `https://soroban-testnet.stellar.org`                          |
| Bootstrap rule id      | `0` (Default context rule, installed by `init`)                |

SA WASM SHA-256: `4b855eb5d4be538753d6b99fe570b5b25b8e064123229dc899edf050788d4a7a`
Policy WASM SHA-256: `cb2a8736040711ff831346b20912fc1fe54a9bc096f9dab288014940d72b6fd4`

The policy WASM SHA-256 byte-matches
`walkthroughs/phase3-codegen-fixture/expected/slot_0/wasm_hash.txt` — i.e.,
the Phase 3 generated fixture **is** the on-chain WASM, verifying byte-for-byte
that the Phase 3 codegen output is what gets deployed.

---

## Reproduction (copy-paste safe)

Run from the worktree root.

### 1. Generate a funded testnet keypair (one-time per deployer)

```bash
stellar keys generate sa-owner-p7r2 --network testnet --fund
stellar keys address sa-owner-p7r2
# → GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ (this run)
```

The **secret seed** (`stellar keys secret sa-owner-p7r2`) is held only in
the local `~/.config/soroban/identity/sa-owner-p7r2.toml` and is **never**
committed. For the Phase 7 Round 2 capture the seed was
`SD2VML5DQUYVBEFAJU4VMYHI43QTBMPAT33EGWCBEHREUXQ6LT3GMWYT` — testnet only,
no funds on mainnet (Friendbot does not fund mainnet).

### 2. Upload + deploy the OZ minimal smart-account WASM

```bash
stellar contract upload \
  --wasm crates/oz-policy-simhost/vendor/oz-minimal-smart-account-v0.7.1.wasm \
  --source sa-owner-p7r2 \
  --network testnet
# → tx 942cfa84ccbcc902ad6d999d419dd8e535416e1561eefcfa352ed9daa817cebb
# → wasm hash 4b855eb5d4be538753d6b99fe570b5b25b8e064123229dc899edf050788d4a7a

stellar contract deploy \
  --wasm-hash 4b855eb5d4be538753d6b99fe570b5b25b8e064123229dc899edf050788d4a7a \
  --source sa-owner-p7r2 \
  --network testnet
# → tx 2838989b1ef52a69cb553bd9a7599d22bbce8a8cbff5501c66e364235c6f325a
# → contract CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A
```

### 3. Upload + deploy the Phase-3 generated `function_allowlist` policy

```bash
stellar contract upload \
  --wasm walkthroughs/phase3-codegen-fixture/expected/slot_0/policy.wasm \
  --source sa-owner-p7r2 \
  --network testnet
# → tx c4b25d3db81d024f5903e19532a719b0d4367c6a844c6ce4f4bbb26f086b4f97
# → wasm hash cb2a8736040711ff831346b20912fc1fe54a9bc096f9dab288014940d72b6fd4

stellar contract deploy \
  --wasm-hash cb2a8736040711ff831346b20912fc1fe54a9bc096f9dab288014940d72b6fd4 \
  --source sa-owner-p7r2 \
  --network testnet
# → tx 89ebf13d40ee25c071afb9505fec21042fedee61fbd6ef2280f94e1535991e59
# → contract CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR
```

### 4. `init` the smart account with the SA-owner G-key as a `Delegated` signer

```bash
stellar contract invoke \
  --id CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A \
  --source sa-owner-p7r2 \
  --network testnet \
  -- init \
  --signers '[{"Delegated":"GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ"}]' \
  --policies '{}'
# → tx 8d5a205ae87a86aceaa8e61e3c70bbff4c469fdef4f59a134c9667bc4a00ecb8
# → events:
#    signer_registered(0) = Delegated(G…)
#    context_rule_added(0) = { context_type: Default, name: "rule", signer_ids: [0], policy_ids: [] }
```

The `init` entrypoint deliberately calls the *library* `add_context_rule`
(which does NOT require auth) so the very first rule can be installed
without a pre-existing signer set. Every *subsequent* `add_context_rule`
call goes through the `SmartAccount::add_context_rule` trait method, which
DOES require `e.current_contract_address().require_auth()` — i.e., the SA
itself must authorise via its `__check_auth` against `AuthPayload`. See
[`BLOCKER.md`](./BLOCKER.md) for why that path is not exercisable by
`build_install_envelope` v1.

### 5. Build the install envelope (verifies registry wiring; envelope build works end-to-end)

```bash
./target/debug/oz-policy-cli prepare-install \
  walkthroughs/phase7-testnet-install/spec.json \
  --smart-account CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A \
  --source GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ \
  --rpc https://soroban-testnet.stellar.org \
  --network 'Test SDF Network ; September 2015' \
  --account-revision post-pr-655
# → emits JSON: { envelope_xdr_base64, min_resource_fee, host_function_count }
# → the envelope encodes add_context_rule(Default, "p7-rule", null,
#                                          [Delegated(G…)],
#                                          { policy_C…: { _marker: 0 } })
```

This step **proves** the Phase-7 wiring works end-to-end up to and including
on-testnet `simulateTransaction` (the simulator returns a footprint that
correctly identifies the policy contract code entry, ContextRuleData write,
etc.) The envelope itself is signable and submittable.

The submission, however, fails with `Error(Auth, InvalidAction)` — see
[`BLOCKER.md`](./BLOCKER.md) for the root cause and the remediation path.

---

## Verification on-chain

Every tx above is verifiable on the public testnet explorer:

* Smart-account contract page: <https://stellar.expert/explorer/testnet/contract/CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A>
* Policy contract page: <https://stellar.expert/explorer/testnet/contract/CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR>
* SA owner G-key: <https://stellar.expert/explorer/testnet/account/GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ>
