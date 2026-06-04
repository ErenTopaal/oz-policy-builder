# Audit Handoff Package

This directory describes what the chosen auditor receives at engagement
start. It is **not** a copy of the source — duplicating files would
create a drift surface — it is a pointer index from the auditor's
perspective into the live repository at the pinned commit SHA.

## The pinned commit

The audit operates against a **single commit SHA** that the lead engineer
records at engagement start. The auditor must verify every claim below by
reading the repository checked out at that SHA, not by reading copies of
files elsewhere.

The commit SHA itself is recorded in the per-cycle directory
(`audits/<auditor>-<date>/scope.md`) by the lead engineer when the
engagement opens. Until an engagement opens, this field is intentionally
empty.

## Required reading (in this order)

1. [`../SCOPE.md`](../SCOPE.md) — what is and is not in audit scope. Read
   this first; everything else flows from it.
2. [`../THREAT_MODEL.md`](../THREAT_MODEL.md) — the threat / mitigation /
   test map. Each row points into the in-scope code.
3. [`../../SECURITY.md`](../../SECURITY.md) — the project's disclosure
   policy and contact information.
4. [`../../researches/technical-research.md`](../../researches/technical-research.md)
   §12 — the original research note the threat model is derived from.
5. [`../../researches/analysis.md`](../../researches/analysis.md) §10 —
   the project's enumerated technical and strategic risks.

## Pointers into the in-scope source

### Synthesizer core

- [`../../crates/oz-policy-core/src/spec.rs`](../../crates/oz-policy-core/src/spec.rs)
- [`../../crates/oz-policy-core/src/decision_tree.rs`](../../crates/oz-policy-core/src/decision_tree.rs)
- [`../../crates/oz-policy-core/src/sep41.rs`](../../crates/oz-policy-core/src/sep41.rs)
- [`../../crates/oz-policy-core/src/errors.rs`](../../crates/oz-policy-core/src/errors.rs)
- [`../../crates/oz-policy-core/src/arg_value.rs`](../../crates/oz-policy-core/src/arg_value.rs)
- [`../../crates/oz-policy-core/src/recording.rs`](../../crates/oz-policy-core/src/recording.rs)

### Templates + golden tests + audit lints

- [`../../templates/base.rs.jinja`](../../templates/base.rs.jinja)
- [`../../templates/constraints/`](../../templates/constraints/)
- [`../../crates/oz-policy-codegen/src/render.rs`](../../crates/oz-policy-codegen/src/render.rs)
- [`../../crates/oz-policy-codegen/src/sandbox.rs`](../../crates/oz-policy-codegen/src/sandbox.rs)
- `../../crates/oz-policy-codegen/src/audit_lints.rs` — produced by
  Phase 9 Stream B; the auditor should verify its lint set covers
  `require_auth_first`, `storage_keyed_by_pair`, no `unsafe`, no
  `core::mem::transmute`, no bare `panic!`.
- [`../../crates/oz-policy-codegen/tests/golden_render.rs`](../../crates/oz-policy-codegen/tests/golden_render.rs)
  — golden expected output per constraint primitive.

### Simhost + deny vector generator

- [`../../crates/oz-policy-simhost/src/host.rs`](../../crates/oz-policy-simhost/src/host.rs)
- [`../../crates/oz-policy-simhost/src/permit.rs`](../../crates/oz-policy-simhost/src/permit.rs)
- [`../../crates/oz-policy-simhost/src/deny.rs`](../../crates/oz-policy-simhost/src/deny.rs)
- [`../../crates/oz-policy-simhost/src/run.rs`](../../crates/oz-policy-simhost/src/run.rs)

### Installer + preflight

- [`../../crates/oz-policy-installer/src/envelope.rs`](../../crates/oz-policy-installer/src/envelope.rs)
- [`../../crates/oz-policy-installer/src/preflight.rs`](../../crates/oz-policy-installer/src/preflight.rs)
- [`../../crates/oz-policy-installer/src/registry.rs`](../../crates/oz-policy-installer/src/registry.rs)

### Wallet adapter — sub-component (AuthPayload encoder only)

- [`../../wallet-adapter/src/oz_smart_account_auth.ts`](../../wallet-adapter/src/oz_smart_account_auth.ts)
- [`../../wallet-adapter/src/oz_smart_account_auth.test.ts`](../../wallet-adapter/src/oz_smart_account_auth.test.ts)

## Concrete examples (walkthrough corpora)

Three end-to-end frozen corpora illustrate the synthesizer running against
real recordings. They are the auditor's best on-ramp into "what does the
system do for a real user?":

- [`../../walkthroughs/01-blend-yield/`](../../walkthroughs/01-blend-yield/)
- [`../../walkthroughs/02-sep41-subscription/`](../../walkthroughs/02-sep41-subscription/)
- [`../../walkthroughs/03-soroswap-bounded/`](../../walkthroughs/03-soroswap-bounded/)

Each contains a `PolicySpec`, the recording it was derived from, the
generated WASM hash, the simhost report, and the install envelope.

## Fuzz harness + accumulated corpora (Stream A)

- `../../crates/oz-policy-codegen/fuzz/` — the codegen-side harness
  (target `spec_to_wasm_panic_free`).
- `../../crates/oz-policy-recorder/fuzz/` — the recorder XDR-decode
  harness (target `recording_decode_panic_free`).
- A simhost-side fuzz target is a Phase 9 follow-up; not yet
  implemented.
- The persisted corpora live on the `fuzz-corpora` branch; the auditor
  receives the latest snapshot at the pinned-commit time.

## Reproducible build (Stream C)

- `../../scripts/reproducible-build.sh`
- `../../ci/Dockerfile`
- `../../reproducible-build-manifest.json` (produced at release-tag time;
  the manifest from the pinned commit is included in the handoff bundle
  as a separate file).

## Workspace-wide controls the auditor should re-verify

- [`../../Cargo.toml`](../../Cargo.toml) — confirm `overflow-checks =
  true` is set on `dev` **and** `release`. This is load-bearing for T6.
- [`../../deny.toml`](../../deny.toml) — confirm the allow-list is
  current and re-evaluate the three ignored rustls-webpki RUSTSEC
  advisories.
- [`../../rust-toolchain.toml`](../../rust-toolchain.toml) — confirm the
  pinned toolchain version.
- [`../../Cargo.lock`](../../Cargo.lock) — confirm it is checked in and
  has not drifted from the pinned commit.

## What the auditor does *not* need to read

- `crates/oz-policy-mcp/` — transport surface, not synthesizer logic.
- `skills/oz-policy-builder/` — agent skill prose; orchestration only.
- `wallet-adapter/src/` files other than `oz_smart_account_auth.ts` and
  its test — provided for context only.
- Anything under `target/`, `node_modules/`, or `dist/`.

If anything in this index is unclear or appears to drift from the live
repository at the pinned commit, surface it on day one of the
engagement — accurate pointers are the entire point of this bundle.
