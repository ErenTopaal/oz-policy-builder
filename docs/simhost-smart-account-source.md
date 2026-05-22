# Simhost smart-account WASM source decision

> **Status:** chosen. Recorded 2026-05-15 during Phase 4 Stream A.
>
> **Chosen path:** **Option C** — minimal in-tree smart-account contract.

## Why Option C

Option A (build OZ's stock `multisig-account-example` from cloned upstream
`OpenZeppelin/stellar-contracts@v0.7.1`) is correct in principle but pulls
the full upstream workspace (~30 sister example crates and `stellar-contract-utils`
for `Upgradeable`). That made an in-band, deterministic, vendor-friendly
build step disproportionately heavy.

Option B (build directly from the cached `stellar-accounts = 0.7.1` source
tree on the local `~/.cargo/registry`) is not viable: the published
`stellar-accounts` crate exposes the `SmartAccount` trait + `do_check_auth`
helper but does **not** ship a concrete `#[contract]` smart-account type.

Option C — write a minimal contract under
`crates/oz-policy-simhost/vendor-src/minimal-smart-account/` that depends on
`stellar-accounts = "=0.7.1"` and `soroban-sdk = "=25.3.0"` as **published
library deps** (no path patches, no upstream workspace clone) — is the
honest, reproducible compromise. The contract is a verbatim derivative of
the upstream example `examples/multisig-smart-account/account/src/contract.rs`
with the `Upgradeable` blanket impl removed (which was the only thing
forcing a `stellar-contract-utils` dep). Surface retained:

| Trait / fn | Source | Purpose |
| --- | --- | --- |
| `SmartAccount` (blanket impl) | `stellar-accounts::smart_account::SmartAccount` | context-rule + signer + policy management mutators (`add_context_rule`, `add_policy`, `add_signer`, etc.) |
| `CustomAccountInterface::__check_auth` | `stellar-accounts::smart_account::do_check_auth` | the Soroban auth entrypoint; the simhost invokes this directly |
| `ExecutionEntryPoint` (blanket impl) | `stellar-accounts::smart_account::ExecutionEntryPoint` | enables policy-routed inner invocations |
| `__constructor(signers, policies)` | OZ example | seeds a single `Default` context rule named `"rule"` |

The source lives at
`crates/oz-policy-simhost/vendor-src/minimal-smart-account/src/lib.rs`
(committed alongside the WASM so the binary remains auditable against the
source we built it from).

## Build recipe (one-shot, out-of-band)

```bash
cd crates/oz-policy-simhost/vendor-src/minimal-smart-account
CARGO_TARGET_DIR=/tmp/simhost-sa-target \
SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1 \
  cargo build --release --target wasm32-unknown-unknown

stellar contract optimize \
  --wasm /tmp/simhost-sa-target/wasm32-unknown-unknown/release/minimal_smart_account.wasm \
  --wasm-out ../../vendor/oz-minimal-smart-account-v0.7.1.wasm
```

- Toolchain pin: `rustc 1.89.0` (per workspace `rust-toolchain.toml`).
- `stellar` CLI pin: `25.1.0` (Binaryen v116) — same pin as the Phase 3
  codegen sandbox.
- `SOROBAN_SDK_BUILD_SYSTEM_SUPPORTS_SPEC_SHAKING_V2=1` is required because
  `stellar-accounts 0.7.1` enables `experimental_spec_shaking_v2` on
  `soroban-sdk = 25.3.0`. Without the env var, soroban-sdk's `build.rs`
  panics.
- `rust-version = "1.89.0"` + `resolver = "3"` in the contract's
  `Cargo.toml` is **load-bearing**: it stops Cargo's MSRV-unaware resolver
  from greedily picking `soroban-sdk-macros 25.3.1` / `soroban-spec 25.3.1`
  (both require rustc 1.91.0) and forces it to back off to the buildable
  `25.3.0` patch. Same trick the Phase 3 codegen uses (see
  `oz-policy-codegen::render::generated_cargo_toml`).

## Vendored artifact

| Field | Value |
| --- | --- |
| Path | `crates/oz-policy-simhost/vendor/oz-minimal-smart-account-v0.7.1.wasm` |
| Size | 48138 bytes (post-`stellar contract optimize`) |
| SHA-256 | `ede4bc15fff69952efe2bc95aaa2149810ef6c8567f50750b5ec8ad88b37d675` |
| Built against | `stellar-accounts = 0.7.1`, `soroban-sdk = 25.3.0`, `rustc 1.89.0`, `stellar-cli 25.1.0` |

The SHA-256 is also pinned in `oz_policy_simhost::host::VENDORED_SMART_ACCOUNT_WASM_SHA256`
so the integration test fails loudly if the on-disk WASM drifts from the
committed source. To re-vendor (e.g., after a `stellar-accounts` upgrade),
rerun the build recipe above and update both this doc + the constant in
sync.
