# Error codes — remediation cheatsheet

Every `E_*` code below is sourced from `crates/oz-policy-core/src/errors.rs`
(the canonical `Error` enum, with each variant's `code()` returning the
wire-stable string used by the MCP server). When a tool returns one of these,
surface the friendly remediation to the user; don't echo the raw enum name.

| Code | Source crate | What it means | One-sentence remediation |
|---|---|---|---|
| `E_RECORDER_HASH_NOT_FOUND` | `oz-policy-recorder` | The recorder could not locate the transaction by hash on the configured Soroban RPC endpoint (retention exceeded, wrong network, or wrong hash). | Re-check the hash and network; if testnet, the 24h retention window may have aged the tx out — switch to envelope simulation. |
| `E_RECORDER_SIM_FAILED` | `oz-policy-recorder` | The recorder's `simulateTransaction` call returned an error or failed to produce a decodable auth tree. | Verify the envelope's source account is funded and the RPC URL is reachable; re-submit with `--instruction-leeway` if a budget limit was hit. |
| `E_RECORDER_XDR_DECODE_FAILED` | `oz-policy-recorder` | Well-formed RPC envelopes (or test fixtures) but the embedded XDR (envelope, result-meta, auth, or `ScVal`) failed to decode. | The RPC may be running a newer protocol than this toolkit pins (Protocol 23); upgrade `stellar-xdr` or re-record on a matching network. |
| `E_SYNTH_NOT_EXPRESSIBLE` | `oz-policy-core` | The synthesizer determined the requested constraints cannot be expressed by any combination of OZ primitives or Track-B templates within the hard limits (max 5 policies, 15 signers). | Loosen `tightness`, switch `mode` from `compose_only` to `auto`, or split the rule across multiple context rules. |
| `E_CODEGEN_COMPILE_FAILED` | `oz-policy-codegen` | Track-B codegen produced Rust source that failed the sandboxed `cargo build --target wasm32-unknown-unknown`. | This is a toolkit bug — file an issue with the spec attached; in the meantime, switch `mode` to `compose_only` if the recording shape allows. |
| `E_SIM_PERMIT_DENIED` | `oz-policy-simhost` | The simulation harness reports that a permit vector the spec is expected to allow was denied by the compiled policy. | The synthesizer is too tight; loosen `tightness` (e.g. from `exact` to `small_margin`) or widen the recording's envelope. |
| `E_SIM_DENY_PASSED` | `oz-policy-simhost` | The simulation harness reports that a deny vector the spec is expected to reject was admitted by the compiled policy (false-positive admit). | The synthesizer is too loose; tighten `tightness` or add caller-supplied `extra_deny_vectors` covering the missing boundary. |
| `E_VERIFY_DRIFT` | `oz-policy-simhost` | The verification gate detected drift between the spec, the generated source, the compiled WASM hash, or the on-chain installed policy. | Re-run `export_policy` to regenerate the artefacts; if drift persists, the on-chain rule has been modified out-of-band. |
| `E_WALLET_REJECTED` | wallet-adapter | The wallet returned a user-rejection or signing failure when the install envelope was presented for signature. | The user declined or the wallet rejected the envelope; check the wallet's error log and re-present after addressing the cause. |
| `E_INSTALL_PREFLIGHT_FAILED` | `oz-policy-installer` | The install-time preflight failed — e.g., target `SmartAccount` predates OZ PR-#655 (sponsor-substitution fix) or another precondition was not met. | Upgrade the smart account to a post-#655 build, or set `account_revision: post_pr_655` only if you've verified the deployed contract's vintage. |

---

## Pointers

- The exhaustive `match` in `crates/oz-policy-core/src/errors.rs` is the
  enforcement gate: any new `E_*` code must extend that match (a compiler
  error otherwise), so this table stays in lockstep with the source.
- `crates/oz-policy-mcp/src/error_mapping.rs` maps each `E_*` to a stable
  JSON-RPC integer code (`-32100` through `-32109`); MCP clients can branch on
  either the integer or the string.
- For protocol-level `-32602 INVALID_PARAMS` errors (e.g. "recording_id not
  found in store", "both `hash` and `envelope_xdr_base64` passed"), the
  remediation is always "fix the call arguments and retry" — they're not
  `E_*` codes.
