# Wallets

The `@oz-policy-builder/wallet-adapter` TypeScript package implements
SEP-43 for two signing surfaces and provides the **OZ smart-account
`AuthPayload` encoder** that closed the historical Phase 7 BLOCKER
(resolved 2026-05-18 — see
[`walkthroughs/phase7-testnet-install/install-result.json`](../walkthroughs/phase7-testnet-install/install-result.json)
for the frozen testnet SUCCESS evidence;
[`BLOCKER.md`](../walkthroughs/phase7-testnet-install/BLOCKER.md) for
the diagnostic).

Sections:

1. [Freighter (browser extension)](#freighter-browser-extension)
2. [passkey-kit (headless + browser ceremony)](#passkey-kit-headless--browser-ceremony)
3. [The OZ `AuthPayload` encoder](#the-oz-authpayload-encoder)
4. [End-to-end install + verify](#end-to-end-install--verify)

The full package README:
[`wallet-adapter/README.md`](../wallet-adapter/README.md). The headless
example scripts that this doc cross-references:
[`wallet-adapter/examples/`](../wallet-adapter/examples/).

---

## Freighter (browser extension)

**Use case:** web apps where the user holds keys in the Freighter
extension. The user explicitly approves every signature in the extension
popup.

### Setup

1. Install Freighter from <https://freighter.app/>.
2. In the extension, switch the network to **testnet** for development.
3. Fund the testnet G-address via Friendbot (the extension exposes a
   button, or hit
   `https://friendbot.stellar.org/?addr=<G-addr>` directly).

### Code example

Mirrors the worked snippet in
[`wallet-adapter/README.md`](../wallet-adapter/README.md) "Install a policy
from a browser".

```ts
import {
  FreighterWallet,
  installPolicy,
  verifyInstall,
} from "@oz-policy-builder/wallet-adapter";

const wallet = new FreighterWallet();
if (!(await wallet.isAvailable())) {
  throw new Error("Freighter extension not installed");
}

const { txHash, contextRuleId, ledger } = await installPolicy({
  adapter: wallet,
  envelopeXdrBase64: envelopeXdr,    // from `export_policy` / `oz-policy-installer`
  rpcUrl: "https://soroban-testnet.stellar.org",
  network: "testnet",
  networkPassphrase: "Test SDF Network ; September 2015",
});

const report = await verifyInstall({
  smartAccount: "C...your-smart-account-address...",
  contextRuleId,
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  expectedSpec: policySpec,
});
if (!report.matches) {
  console.error("drift detected:", report.drift);
}
```

The adapter wraps `@stellar/freighter-api` v6.0.1 (pinned in
[`wallet-adapter/package.json`](../wallet-adapter/package.json) under
`dependencies`). Source:
[`wallet-adapter/src/adapters/freighter.ts`](../wallet-adapter/src/adapters/freighter.ts).

Error mapping is by SEP-43 numeric code (`-1` Internal, `-2`
ExternalService, `-3` InvalidRequest, `-4` UserRejected). A user-rejected
signature surfaces from `installPolicy` as
`WalletInstallError(code: "E_WALLET_REJECTED")`.

---

## passkey-kit (headless + browser ceremony)

**Two paths**, selected at construction time. See
[`wallet-adapter/src/adapters/passkey.ts`](../wallet-adapter/src/adapters/passkey.ts)
for the full implementation.

### Path 1 — Headless keypair (CI / scripts)

For CI tests, walkthrough corpus generation, and any non-browser context.
Signs the transaction envelope with
`Keypair.fromSecret(secret).sign(hash)` from `@stellar/stellar-sdk`. This
is **not a mock** — the returned `signedTxXdr` is a real, submittable
Stellar envelope.

```ts
import {
  PasskeyWallet,
  installPolicy,
} from "@oz-policy-builder/wallet-adapter";

const wallet = new PasskeyWallet({
  signerSecretKey: "S...testnet-only-secret...",   // NEVER pass a mainnet secret here
});

await installPolicy({
  adapter: wallet,
  envelopeXdrBase64: envelopeXdr,
  rpcUrl: "https://soroban-testnet.stellar.org",
  network: "testnet",
  networkPassphrase: "Test SDF Network ; September 2015",
});
```

This is the path the headless example scripts use
([`wallet-adapter/examples/01-blend-yield-headless.ts`](../wallet-adapter/examples/01-blend-yield-headless.ts)
and friends).

### Path 2 — Passkey credential (browser ceremony)

For real WebAuthn-backed signing of Soroban auth entries. Requires a
browser + authenticator. Delegates to `passkey-kit`'s `PasskeyKit.sign`.

```ts
const wallet = new PasskeyWallet({
  passkeyCredentialId: "<base64-passkey-credential-id>",
});
```

This path is not covered by mocked unit tests — it is exercised by Phase 7
manual browser tests and a Playwright + virtual-authenticator suite is
planned for a Phase 7 Round 2 follow-up.

The constructor accepts both options simultaneously; if both are set the
adapter prefers `signerSecretKey` (matching `passkey-kit`'s own
keypair-override semantics).

---

## The OZ `AuthPayload` encoder

The OpenZeppelin `MinimalSmartAccount`'s `__check_auth` reads its second
positional argument as `AuthPayload { signers: Map<Signer, Bytes>,
context_rule_ids: Vec<u32> }` (verbatim from
`stellar-accounts 0.7.1`, transcribed in
[`docs/oz-internal-shapes.md`](oz-internal-shapes.md) §10). The
`record_signature_payload` simulator mode emits `Void` in that slot,
which traps the SA with `UnreachableCodeReached` — the historical Phase 7
BLOCKER.

The encoder
[`wallet-adapter/src/oz_smart_account_auth.ts`](../wallet-adapter/src/oz_smart_account_auth.ts)
is the client-side post-processor that converts a Void-signature auth
entry into a properly encoded `AuthPayload` ScVal plus computes the
post-PR-#655 auth digest each signer must actually sign. It is the
fix that closed the historical BLOCKER above.

The exported surface
([`wallet-adapter/src/oz_smart_account_auth.ts`](../wallet-adapter/src/oz_smart_account_auth.ts)):

- `encodeSignerScVal(signer: OzSigner): xdr.ScVal`
- `encodeContextRuleIdsScVal(ids: number[]): xdr.ScVal`
- `encodeAuthPayload(payload: OzAuthPayload): xdr.ScVal`
- `computeAuthDigest(...)`
- `computeSignaturePayload(params)`
- `makeOzSmartAccountAuthEncoder(args)` — convenience factory
- `buildOzAuthEntry(params)` — full `SorobanAuthorizationEntry` builder

The auth digest is computed as (per `storage.rs:493-495` of
`stellar-accounts 0.7.1`):

```
auth_digest = sha256(signature_payload || xdr(context_rule_ids_scval))
```

Signers sign `auth_digest`, **not** the raw `signature_payload`.

### Wiring into `installPolicy`

`installPolicy` exposes an `ozAuthPayloadEncoder` hook
(commit `bd60009`). When supplied, it runs **after** the wallet signs the
outer envelope and **before** submission, letting callers inject the
properly encoded `AuthPayload` ScVal into any
`SorobanCredentials::Address(<SA>)` auth entry. Pseudocode:

```ts
import {
  installPolicy,
  PasskeyWallet,
  makeOzSmartAccountAuthEncoder,
} from "@oz-policy-builder/wallet-adapter";

const wallet = new PasskeyWallet({ signerSecretKey: SA_OWNER_SECRET });

const ozAuthPayloadEncoder = makeOzSmartAccountAuthEncoder({
  smartAccountAddress: "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A",
  signerKeypair: Keypair.fromSecret(SA_OWNER_SECRET),
  contextRuleIds: [0],
  networkPassphrase: "Test SDF Network ; September 2015",
  // ... plus expiration ledger, etc.
});

await installPolicy({
  adapter: wallet,
  envelopeXdrBase64: envelopeXdr,
  rpcUrl: "https://soroban-testnet.stellar.org",
  network: "testnet",
  networkPassphrase: "Test SDF Network ; September 2015",
  ozAuthPayloadEncoder,
});
```

---

## End-to-end install + verify

The `installPolicy → verifyInstall` orchestration:

```
prepare_install  →  signTransaction  →  sendTransaction  →  poll  →  verify_install
                    │                   │                  │       │
                    │                   │                  │       └─ MCP subprocess round-trip
                    │                   │                  └────────  Soroban RPC `getTransaction`
                    │                   └───────────────────────────  Soroban RPC `sendTransaction`
                    └───────────────────────────────────────────────  SEP-43 adapter (user consent)
```

Diagram source:
[`wallet-adapter/README.md`](../wallet-adapter/README.md) "Adapters".

### Mainnet safety

`installPolicy` **refuses** to submit a mainnet envelope unless the caller
sets `confirmMainnet: true` on `InstallPolicyParams`. Without that flag,
the function throws `WalletInstallError(code: "E_MAINNET_REQUIRES_CONSENT")`
**before** any wallet or RPC call. This guard is intentionally loud — see
[`plan.md`](../plan.md) §"Cross-Phase Invariants → 1. No auto-deployment,
ever".

The package itself **never** auto-submits. Every signature requires an
explicit user gesture (Freighter popup) or a pre-provisioned passkey
credential. Calling code is responsible for any in-app consent UX on top.

### `verifyInstall` subprocess contract

`verifyInstall` spawns the `oz-policy-mcp` server as a subprocess and
drives a single JSON-RPC session over STDIO (`initialize` → `tools/call
verify_install`). The default command is `cargo run -p oz-policy-mcp --
--stdio`. For CI environments without cargo on `PATH`, override with a
precompiled binary:

```ts
await verifyInstall({
  /* ... */
  mcpServerCmd: ["./target/release/oz-policy-mcp", "--stdio"],
});
```

A browser-targeted helper that calls the MCP server's HTTP transport
(`POST /mcp` with bearer auth) is a deliberate v1.1 follow-up — the
subprocess-only API keeps the v1 surface tight.

### Error model

All orchestration helpers throw typed errors with a string `code` field
(verbatim from
[`wallet-adapter/README.md`](../wallet-adapter/README.md) "Error model"):

| Error class            | Codes                                                                                                            |
|------------------------|------------------------------------------------------------------------------------------------------------------|
| `WalletError`          | numeric: -1..-4 (Internal / ExternalService / InvalidRequest / UserRejected)                                     |
| `WalletInstallError`   | `E_WALLET_REJECTED`, `E_INSTALL_SUBMIT_FAILED`, `E_INSTALL_POLL_TIMEOUT`, `E_INSTALL_RESULT_DECODE_FAILED`, `E_MAINNET_REQUIRES_CONSENT` |
| `VerifyInstallError`   | `E_VERIFY_SUBPROCESS_SPAWN_FAILED`, `E_VERIFY_SUBPROCESS_TIMEOUT`, `E_VERIFY_SUBPROCESS_CRASHED`, `E_VERIFY_PROTOCOL_ERROR`, `E_VERIFY_TOOL_ERROR` |

A wallet user rejection always surfaces as
`WalletInstallError(E_WALLET_REJECTED)`. All other wallet errors map to
`E_INSTALL_SUBMIT_FAILED` since they happen pre-submit.

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
