# `@oz-policy-builder/wallet-adapter`

SEP-43 wallet adapter + install / verify orchestration for the **OZ
Accounts Policy Builder**.

This package turns a `build_install_envelope`-produced XDR (from
`oz-policy-installer`, exposed via the MCP `export_policy` tool) into an
on-chain context rule by walking the canonical sequence:

```
prepare_install  →  signTransaction  →  sendTransaction  →  poll  →  verify_install
                    │                   │                  │       │
                    │                   │                  │       └─ MCP `verify_install` round-trip
                    │                   │                  └────────  Soroban RPC `getTransaction`
                    │                   └───────────────────────────  Soroban RPC `sendTransaction`
                    └───────────────────────────────────────────────  SEP-43 adapter (user consent)
```

License: **Apache-2.0**.

---

## Adapters

| Adapter | Path | Use case |
|---------|------|----------|
| Freighter (browser extension) — primary | `./adapters/freighter` | Web apps where the user holds keys in the Freighter extension. The user explicitly approves every signature in the extension popup. |
| passkey-kit (headless / programmatic) | `./adapters/passkey` | CI tests, walkthroughs, server-side automation. WebAuthn-style keys backed by a passkey provider; suitable for non-interactive flows where the operator has fully provisioned the credential ahead of time. |

Both adapters implement the same SEP-43 contract (`WalletAdapter` from
`./sep43`), so the higher-level `installPolicy` and `verifyInstall`
orchestration helpers work identically with either.

---

## Usage

### Install a policy from a browser

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

// 1. Sign + submit + poll + extract context_rule_id.
const { txHash, contextRuleId, ledger } = await installPolicy({
  adapter: wallet,
  envelopeXdrBase64: envelopeXdr,           // from `export_policy` / `oz-policy-installer`
  rpcUrl: "https://soroban-testnet.stellar.org",
  network: "testnet",
  networkPassphrase: "Test SDF Network ; September 2015",
});

// 2. Confirm the on-chain rule matches the spec (MCP round-trip).
const report = await verifyInstall({
  smartAccount: "C...your-smart-account-address...",
  contextRuleId,
  network: "testnet",
  rpcUrl: "https://soroban-testnet.stellar.org",
  expectedSpec: policySpec,                 // PolicySpec from `synthesize_policy`
});
if (!report.matches) {
  console.error("drift detected:", report.drift);
}
```

### Headless install (CI / walkthroughs)

```ts
import {
  PasskeyWallet,
  installPolicy,
} from "@oz-policy-builder/wallet-adapter";

const wallet = new PasskeyWallet({ /* passkey-kit options */ });
await installPolicy({
  adapter: wallet,
  envelopeXdrBase64: envelopeXdr,
  rpcUrl: "https://soroban-testnet.stellar.org",
  network: "testnet",
  networkPassphrase: "Test SDF Network ; September 2015",
});
```

---

## Mainnet safety

`installPolicy` refuses to submit a mainnet envelope unless the caller
sets `confirmMainnet: true` on `InstallPolicyParams`. Without that flag,
the function throws
`WalletInstallError(code: "E_MAINNET_REQUIRES_CONSENT")` **before** any
wallet or RPC call. This guard is deliberately loud — see `plan.md` §
"Cross-Phase Invariants → No auto-deployment, ever".

This package itself **never** auto-submits. Every signature requires an
explicit user gesture (Freighter popup) or a pre-provisioned passkey
credential. Calling code is responsible for any in-app consent UX on top.

---

## Error model

All orchestration helpers throw typed errors with a string `code` field:

| Error class | Codes |
|-------------|-------|
| `WalletError` (SEP-43 surface) | numeric: -1..-4 (Internal / ExternalService / InvalidRequest / UserRejected) |
| `WalletInstallError` (`installPolicy`) | `E_WALLET_REJECTED`, `E_INSTALL_SUBMIT_FAILED`, `E_INSTALL_POLL_TIMEOUT`, `E_INSTALL_RESULT_DECODE_FAILED`, `E_MAINNET_REQUIRES_CONSENT` |
| `VerifyInstallError` (`verifyInstall`) | `E_VERIFY_SUBPROCESS_SPAWN_FAILED`, `E_VERIFY_SUBPROCESS_TIMEOUT`, `E_VERIFY_SUBPROCESS_CRASHED`, `E_VERIFY_PROTOCOL_ERROR`, `E_VERIFY_TOOL_ERROR` |

A wallet user rejection always surfaces as
`WalletInstallError(E_WALLET_REJECTED)`. All other wallet errors map to
`E_INSTALL_SUBMIT_FAILED` since they happen pre-submit.

---

## `verifyInstall` and the MCP server

`verifyInstall` spawns the `oz-policy-mcp` server as a subprocess and
drives a single JSON-RPC session over STDIO (`initialize` →
`tools/call verify_install`). The default command is:

```
cargo run -p oz-policy-mcp -- --stdio
```

For CI environments without cargo on `PATH`, override with a precompiled
binary:

```ts
await verifyInstall({
  /* ... */
  mcpServerCmd: ["./target/release/oz-policy-mcp", "--stdio"],
});
```

A browser-targeted helper that calls the MCP server's HTTP transport
(`POST /mcp` with bearer auth) is a deliberate v1.1 follow-up — the
subprocess-only API keeps the v1 surface tight.

---

## Testing

```
pnpm install
pnpm build
pnpm test           # mocked Vitest suite
INTEGRATION=1 pnpm test   # passkey-kit testnet integration test (Phase 7 §Verification)
```

The mocked suite covers every error branch (`installPolicy`,
`verifyInstall`, both adapters) without touching the network.
