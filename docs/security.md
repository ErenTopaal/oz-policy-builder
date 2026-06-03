# Security

Index into the project's security artefacts. The disclosure policy
itself lives at [`SECURITY.md`](../SECURITY.md) in the worktree root;
this page points to the supporting infrastructure.

---

## Disclosure policy

Primary reference: [`SECURITY.md`](../SECURITY.md).

Highlights:

- **90-day default disclosure window** measured from acknowledgement.
- Pre-release placeholders for `security@…` and the GPG fingerprint are
  marked in `SECURITY.md` and will be replaced before `v1.0.0`.
- Pre-release reporters should open a private GitHub Security Advisory
  on the repository instead of relying on the placeholder address.

---

## Scope

Primary reference: [`audits/SCOPE.md`](../audits/SCOPE.md).

In-scope components (synthesizer + outputs):

- Synthesizer logic — [`crates/oz-policy-core`](../crates/oz-policy-core).
- Codegen pipeline + template library —
  [`crates/oz-policy-codegen`](../crates/oz-policy-codegen),
  [`templates/`](../templates).
- Simulation harness —
  [`crates/oz-policy-simhost`](../crates/oz-policy-simhost).
- Installer + on-chain preflight —
  [`crates/oz-policy-installer`](../crates/oz-policy-installer).
- MCP tool **surface** (definitions only, not transport) —
  [`crates/oz-policy-mcp`](../crates/oz-policy-mcp).
- The wallet adapter's OZ-SA `AuthPayload` encoder —
  [`wallet-adapter/src/oz_smart_account_auth.ts`](../wallet-adapter/src/oz_smart_account_auth.ts).

Out of scope:

- Upstream OpenZeppelin `stellar-accounts` (report to OZ directly).
- The Soroban runtime / `soroban-env-host`.
- Third-party wallets (Freighter, passkey-kit) beyond our adapter shim.
- The `oz-policy-mcp` **transport** layer (HTTP framing, bearer auth,
  rate limiting) as distinct from the tool surface — operator hardening
  of any hosted endpoint is Phase 10 work, not part of the synthesizer
  audit.
- The agent-skill prose under
  [`skills/oz-policy-builder/`](../skills/oz-policy-builder) — pure
  orchestration, no auth or codegen logic.

---

## Threat model

Primary reference: [`audits/THREAT_MODEL.md`](../audits/THREAT_MODEL.md).

The threat model is structured so an external auditor can walk the table
top-to-bottom and find both the engineering control and the verifying
test for every claim. It covers the nine synthesizer / generated-policy
threats from research §12 plus a tenth threat — **AuthPayload encoding
bug** — surfaced as the Phase 7 BLOCKER and closed 2026-05-18 (see
[`audits/THREAT_MODEL.md`](../audits/THREAT_MODEL.md) §T10).

---

## Audit readiness checklist

Primary reference: [`audits/READY.md`](../audits/READY.md).

At time of writing **no external audit has been completed.** The handoff
package being prepared for the next engagement lives at
[`audits/handoff-package/`](../audits/handoff-package/). The full audit
index is [`audits/index.md`](../audits/index.md).

---

## Audit lint suite

Source: [`crates/oz-policy-codegen/src/audit_lints.rs`](../crates/oz-policy-codegen/src/audit_lints.rs).

Five rules gate every Track-B codegen output via `synthesize_track_b`:

1. **`require_auth_first`** — every public mutating entrypoint must call
   `smart_account.require_auth()` before touching storage.
2. **`storage_keyed_by_pair`** — every storage key must be
   `(smart_account, context_rule_id)`-keyed to prevent cross-rule replay.
3. **`no_unsafe`** — generated code must contain no `unsafe` blocks.
4. **`panic_uses_policy_error`** — every panic site must use the
   `panic_with_error!` macro with a typed `PolicyError`, never a bare
   `panic!` or `.unwrap()`.
5. **`no_floats_on_amounts`** — amounts are `i128` only; no `f32`/`f64`
   in policy logic.

`lint_rendered_source` is the single entry point; per-template coverage
tests assert every rule fires on at least one bad-template fixture and
that the committed templates pass.

---

## Fuzz harnesses

Two libfuzzer targets ship today:

- [`crates/oz-policy-codegen/fuzz/fuzz_targets/spec_to_wasm_panic_free.rs`](../crates/oz-policy-codegen/fuzz/fuzz_targets/spec_to_wasm_panic_free.rs)
  — randomises `PolicySpec` shapes and asserts `synthesize_track_b` +
  rendering never panic.
- [`crates/oz-policy-recorder/fuzz/fuzz_targets/recording_decode_panic_free.rs`](../crates/oz-policy-recorder/fuzz/fuzz_targets/recording_decode_panic_free.rs)
  — randomises Soroban RPC response bodies and asserts recording decode
  never panics.

Both targets follow the cargo-fuzz convention and are run nightly via
[`ci/`](../ci) (workflow lives in `.github/workflows/`).

---

## Reproducible build

Primary script: [`scripts/reproducible-build.sh`](../scripts/reproducible-build.sh).
Primary doc: [`docs/reproducible-build.md`](reproducible-build.md).

Re-derives every committed walkthrough WASM from source and asserts
byte-equality with the pinned hashes. The pins live next to each
`policy.wasm` as `wasm_hash.txt`. Hard exit conditions:

- any required tool missing (`rustc`, `cargo`, `stellar`, `jq`,
  `sha256sum`/`shasum`)
- the host `rustc`/`stellar` version doesn't match the workspace pin
- cargo workspace build fails
- any re-derived WASM hash differs from its committed pin (both hashes
  are printed to stderr before exit)

On success the script writes
`reproducible-build-manifest-<release_tag>.json` at the workspace root
— that manifest is the artefact attached to each release tag for
third-party verification.

The toolchain pins are: `rustc 1.89.0` (in
[`rust-toolchain.toml`](../rust-toolchain.toml)), `stellar` CLI 25.1.0
(which embeds `wasm-opt` Binaryen 116 — see
[`docs/oz-internal-shapes.md`](oz-internal-shapes.md) §Reproducible-build
prereqs).

---

## Cryptographic commitments

From [`SECURITY.md`](../SECURITY.md) §"Cryptographic and reproducibility
commitments":

- Generated policy contracts segregate storage by
  `(smart_account, context_rule_id)` — see
  [`audits/THREAT_MODEL.md`](../audits/THREAT_MODEL.md) for the
  threat / mitigation / test mapping.
- `overflow-checks = true` is set on both `dev` and `release` profiles
  in the workspace [`Cargo.toml`](../Cargo.toml). This is
  **load-bearing**; do not remove it.

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
