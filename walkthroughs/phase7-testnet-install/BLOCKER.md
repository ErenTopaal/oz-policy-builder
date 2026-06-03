# Phase 7 Round 2 — Honest BLOCKER report

## RESOLVED 2026-05-18 — RFP deliverable #5 closed

The AuthPayload `Void` trap documented below has been closed. The full
record → generate → simulate → install → use flow now lands a SUCCESS
transaction on Stellar testnet end-to-end.

**Closure artifacts:**
* `walkthroughs/phase7-testnet-install/install-result.json` — frozen
  install evidence: tx hash `038583fa4c95654c9a26323702b86729e084357d47ab169fa22a77d821ce90bb`,
  status `SUCCESS`, ledger `2617998`, `context_rule_id = 4`,
  `verifyInstall: { matches: true, drift: [] }`.
* `wallet-adapter/src/oz_smart_account_auth.ts` — `makeOzSmartAccountAuthEncoder`
  (Phase 8 Stream B). Implements the OZ-SA-specific AuthPayload encoder
  + nested `require_auth_for_args` entries + footprint refresh.
* `wallet-adapter/src/install.ts` — wired the encoder into `installPolicy`
  via the `ozAuthPayloadEncoder` hook (three-step refresh:
  clear → re-simulate → encode).
* `crates/oz-policy-mcp/src/verify_chain.rs` — real on-chain `ContextRule`
  readback via `simulateTransaction(SA.get_context_rule(rule_id))` +
  typed field-for-field drift comparator. Replaces the Phase-5 synthetic
  placeholder.
* `wallet-adapter/src/phase7_integration.test.ts` — integration test now
  asserts SUCCESS (rather than the previous documented FAILED shape).
  Verified with `INTEGRATION=1 pnpm test phase7_integration` (live
  testnet, real ed25519 signing, real RPC round-trip; 11.6 s wall
  clock).

**Transaction on explorer:**
https://stellar.expert/explorer/testnet/tx/038583fa4c95654c9a26323702b86729e084357d47ab169fa22a77d821ce90bb

The remainder of this document describes the pre-closure state and the
diagnostic that drove the fix. It is preserved verbatim for historical
context — do NOT delete it.

---

Captured 2026-05-16 during Phase 7 Round 2 work.

## TL;DR

`oz-policy-installer::build_install_envelope` builds a structurally correct
install envelope that simulates successfully against testnet (resource
footprint + nonce + read/write entries all correct), but **on submission the
on-chain transaction fails with `Error(Auth, InvalidAction)` because the
smart account's `__check_auth` traps on a `Void` `AuthPayload` signature**.

The fix is not a one-liner. It requires shipping an OZ-SA-specific
auth-tree-signer (a TypeScript helper in `wallet-adapter/`, or a new Rust
sub-crate) that knows how to encode the `AuthPayload {signers: Map<Signer,
Bytes>, context_rule_ids: Vec<u32>}` shape and stitch it into a
`SorobanAuthorizationEntry::credentials.address.signature`. Phase 2's v1
envelope builder deliberately delegates auth-tree construction to the
wallet adapter; this round established that **no wallet-adapter signer
today knows the OZ-SA shape** — they only know the SAC-style outer-envelope
sign.

## Detailed diagnostic

### What works (verified on testnet, captured 2026-05-16)

1. Both WASMs uploaded successfully:
   * smart-account: tx `942cfa84ccbcc902ad6d999d419dd8e535416e1561eefcfa352ed9daa817cebb`, hash `4b855eb5…`
   * function_allowlist policy: tx `c4b25d3db81d024f5903e19532a719b0d4367c6a844c6ce4f4bbb26f086b4f97`, hash `cb2a8736…` (byte-equal to `walkthroughs/phase3-codegen-fixture/expected/slot_0/wasm_hash.txt`)

2. Both contracts deployed:
   * smart-account: `CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A` (tx `2838989b…`)
   * policy:        `CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR` (tx `89ebf13d…`)

3. SA initialised via `init` (uses library `add_context_rule`, no auth):
   * tx `8d5a205ae87a86aceaa8e61e3c70bbff4c469fdef4f59a134c9667bc4a00ecb8`
   * registers `Signer::Delegated(G-sa-owner)` with id 0
   * creates `ContextRule { id: 0, context_type: Default, signers: [Delegated], policies: [] }`

4. `oz-policy-cli prepare-install` builds an envelope that:
   * passes preflight (post-PR-655 account revision recognised)
   * resolves the policy address via the new `project_deployed_policy_address` registry hit
   * runs `simulateTransaction` and gets back a proper footprint:
     ```
     read_only: [SignerLookup(SA), policy instance, SA wasm code, policy wasm code]
     read_write: [ContextRuleData(1), PolicyData(0), PolicyLookup, SignerData(0), SA instance, nonce, Installed(SA, 1)]
     instructions: 4_490_213
     resource_fee: 305_827 stroops
     ```
   * returns base64 XDR (2120 bytes) ready for signing

5. `stellar tx sign --sign-with-key sa-owner-p7r2` produces a real ED25519
   outer-envelope signature.

### What fails

6. `stellar tx send` returns:
   ```
   TxFailed([ OpInner(InvokeHostFunction(Trapped)) ])
   ```

   With the corresponding `simulateTransaction` diagnostic events:
   ```
   [Diagnostic Event] error: VM call trapped: UnreachableCodeReached, __check_auth
   [Diagnostic Event] failed account authentication with error,
                      CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A,
                      Error(WasmVm, InvalidAction)
   [Diagnostic Event] escalating error to VM trap from failed host function call: require_auth
   [Diagnostic Event] fn_call SA __check_auth, data:[Bytes(e1eea57b...), Void,
                      [[Contract, { args: [Default, "p7-rule", null,
                                            [[Delegated, G-sa-owner]],
                                            { C-policy: { _marker: 0 } }],
                                    contract: SA, fn_name: add_context_rule }]]]
   ```

   The second positional argument to `__check_auth` is `Void`. The OZ
   smart account's `__check_auth` reads the second arg as `AuthPayload`
   via `soroban_sdk::TryFromVal::try_from_val`, which traps with
   `UnreachableCodeReached` when the host returns `Void` instead of a map.

### Root cause

The OZ-SA contract's `Signature` associated type is `AuthPayload`:

```rust
// vendor-src/minimal-smart-account/src/lib.rs:68
impl CustomAccountInterface for MinimalSmartAccount {
    type Error = SmartAccountError;
    type Signature = AuthPayload;
    // ...
}

// stellar-accounts v0.7.1, packages/accounts/src/smart_account/storage.rs:
#[contracttype]
pub struct AuthPayload {
    pub signers: Map<Signer, Bytes>,
    pub context_rule_ids: Vec<u32>,
}
```

When the auth tree's `SorobanCredentials::Address.signature` is the canonical
record-mode placeholder (`ScVal::Void`), the host hands `Void` to `__check_auth`
as the second positional arg. Soroban's `AuthPayload::try_from_val(Void)`
returns an error, which the auto-generated `#[contractimpl]` shim escalates
to a panic — surfacing as `UnreachableCodeReached`.

`build_install_envelope` v1 has no code path that constructs an
`AuthPayload` ScVal. It relies on `simulateTransaction`'s `record_signature_payload`
auth mode to fill in the signature, but record mode emits `Void` (the
post-record signing step is the wallet's job — and our wallet adapter
today only signs the outer envelope, not custom auth-payload signatures).

The signed `auth_digest` for each AuthPayload entry is:
```
auth_digest = sha256( signature_payload_bytes || xdr(context_rule_ids) )
```

For a `Signer::Delegated(addr)` entry, the OZ SA calls
`addr.require_auth_for_args((auth_digest,))` — which itself requires a
*nested* Soroban authorisation entry. So a full install transaction tree
looks like:

```
[InvokeHostFunction: SA.add_context_rule(...) ]
└── SorobanAuthorizationEntry
    credentials: Address(SA)
    signature: ScVal::Map(AuthPayload {
        signers: { Delegated(G-owner): Bytes::empty() }   ← we need to emit this
        context_rule_ids: [0]                              ← bootstrap rule
    })
    root_invocation: ContractFn(SA, "add_context_rule", [...])
    sub_invocations: [
        ContractFn(G-owner, "require_auth_for_args", [auth_digest])  ← and this
    ]
```

Both the outer `Map` shape and the nested `require_auth_for_args` invocation
need to be assembled client-side. The simulator can identify the resource
footprint but cannot synthesise the application-specific auth shape.

## Remediation path (Phase 8 work)

### Option A — TypeScript auth-payload helper (recommended, in `wallet-adapter/`)

Add `wallet-adapter/src/oz-auth-payload.ts` exporting:

```ts
export function signOzInstallEnvelope(opts: {
  envelopeXdrBase64: string;          // from build_install_envelope
  smartAccountAddress: string;        // C…
  signerKeypair: Keypair;             // the SA's Delegated signer (S…)
  contextRuleId: number;              // e.g. 0 for bootstrap rule
  networkPassphrase: string;
  signatureExpirationLedger: number;
}): Promise<{ signedTxXdr: string }>;
```

Internally:
1. Decode envelope, locate the `SorobanAuthorizationEntry` whose credentials
   address is the SA.
2. Compute `signature_payload = sha256(envelopeContentsHash)` and
   `auth_digest = sha256(signature_payload || xdr(context_rule_ids))`.
3. Sign `auth_digest` with `signerKeypair` to produce a Soroban auth-entry
   signature for the nested `Delegated`-address `require_auth_for_args` call.
4. Append a sub-invocation to the SA's auth entry that targets
   `G-owner.require_auth_for_args((auth_digest,))` with the
   keypair-produced signature.
5. Encode the outer signature as `AuthPayload { signers: { Delegated(G-owner): Bytes() }, context_rule_ids: [0] }`
   and place it in the SA entry's `signature` slot.
6. Sign the outer envelope with `signerKeypair`.
7. Return the signed XDR.

This is ~150 lines of TypeScript and is the cleanest place for it because
the rest of the wallet-adapter already has the `stellar-sdk` import path.

### Option B — Rust auth-payload helper in `oz-policy-installer`

Same shape, but in Rust. Lets the CLI's `prepare-install` emit a signed
envelope directly. More work because we'd have to wrap `stellar-strkey` +
`ed25519-dalek` + `stellar-xdr` for a path that's already covered by
stellar-sdk in JS. Defer unless we drop the wallet-adapter as a separate
package.

### Option C — Use OZ's published "smart account SDK" if/when it ships

OpenZeppelin has not released a JS SDK that wraps `AuthPayload` construction
as of `stellar-contracts@0.7.1` (verified 2026-05-16). When/if such an SDK
ships, prefer that over our own.

## Why not block Phase 7 entirely?

Everything Phase 7 was supposed to deliver *as wallet-adapter v1* is in
place:

* SEP-43 type surface — done (Stream A, freighter + sep43 modules).
* Freighter adapter — done (signs outer envelopes against testnet).
* passkey-kit/headless-keypair adapter — done (signs outer envelopes).
* `installPolicy` orchestration (sign → submit → poll → extract id) — done.
* `verifyInstall` MCP subprocess round-trip — done.
* `prepare-install` CLI emitting submittable envelopes for primitives whose
  address registry has entries — works (this round added the first such
  entry: `function_allowlist` on testnet).
* Real testnet deployments — done (this round).
* End-to-end on-chain install for SA-authorised calls — **BLOCKED** on the
  AuthPayload-shape helper described above.

The remaining gap is one specific code path (custom `__check_auth` payload
encoding) that lives at a layer above the SEP-43 surface. Phase 7's binary
completion criterion is amended to reflect that — see `plan.md` Phase 7
*Verification / Test / Validation*.

## What the integration test does today

`wallet-adapter/src/phase7_integration.test.ts` (gated by `INTEGRATION=1`):

1. Loads `walkthroughs/phase7-testnet-install/deployed-addresses.json`.
2. Builds the install envelope via the `prepare-install` CLI binary
   (real Phase-2 path; real testnet `simulateTransaction` call).
3. Asserts the envelope is well-formed base64 XDR that round-trips through
   `TransactionBuilder.fromXDR`.
4. Signs the outer envelope with the SA-owner keypair (real ED25519).
5. Submits the signed envelope to testnet RPC.
6. Asserts the submission lands as `TxFailed(InvokeHostFunction(Trapped))`
   with the canonical `Error(Auth, InvalidAction)` diagnostic — i.e. the
   exact failure mode this BLOCKER describes. **If the failure ever
   changes, the test fails loudly, forcing a re-read of this document.**
7. Calls `verifyInstall` against the bootstrap rule id 0 (which exists
   on-chain — installed by `init`) and captures the literal MCP report.
   The current MCP `verify_install` is a placeholder (`matches: false`,
   one synthetic drift item explaining the rpc-readback is not yet wired);
   the test pins that shape so the future swap to a real readback surfaces
   as a test failure.
