# Security Policy

> **Status:** Pre-release. The OZ Accounts Policy Builder has not yet shipped a
> tagged release and has not yet been externally audited. The disclosure
> contact and signing key fields below contain explicitly-marked
> `<placeholder>` values; the project will replace them with real values
> before the first tagged release (`v1.0.0`). See **Known issues** and
> **Audit history** for the current state.

## Reporting a vulnerability

Please report suspected security issues **privately**, not via public GitHub
issues. We follow a **90-day default disclosure window** measured from the
date we acknowledge your report. If a fix lands sooner and is shipped to
users we will publish jointly; if more time is needed for a coordinated fix
we will request an extension in writing.

- **Email:** `security@<placeholder.example>`
  &nbsp; *(`<placeholder>` — project will set its real address before
  `v1.0.0`. Do not rely on this address; if you are reading this in a
  pre-release commit, open a private GitHub Security Advisory on the
  repository instead.)*
- **GPG fingerprint:** `<placeholder — project signing key not yet
  generated; will be published with the first tagged release and mirrored
  here>`.

When reporting, please include:

1. The commit SHA you observed the issue on.
2. A minimal reproduction (a `PolicySpec`, a recording, a generated WASM
   hash, or a command sequence).
3. Whether the issue affects synthesizer logic, a generated policy, the
   install path, the simhost, the wallet adapter, or the MCP transport.
4. Your preferred name for credit (or "anonymous").

We will acknowledge within five business days of receipt.

## Scope

### In scope

These components are in scope for disclosure and for the external audit
tracked in [`audits/`](audits/):

- The synthesizer logic itself
  ([`crates/oz-policy-core`](crates/oz-policy-core)).
- The codegen pipeline and template library
  ([`crates/oz-policy-codegen`](crates/oz-policy-codegen),
  [`templates/`](templates)).
- The simulation harness
  ([`crates/oz-policy-simhost`](crates/oz-policy-simhost)).
- The installer + on-chain preflight
  ([`crates/oz-policy-installer`](crates/oz-policy-installer)).
- The MCP tool surface exposed by
  [`crates/oz-policy-mcp`](crates/oz-policy-mcp) (the tool *definitions*,
  not the transport).
- The wallet adapter's OZ-SA `AuthPayload` encoder
  ([`wallet-adapter/src/oz_smart_account_auth.ts`](wallet-adapter/src/oz_smart_account_auth.ts))
  — load-bearing for `add_context_rule` auth.

See [`audits/SCOPE.md`](audits/SCOPE.md) for the full file-level breakdown.

### Out of scope

- Upstream OpenZeppelin smart-account contracts (`stellar-accounts`). Report
  issues in those to OpenZeppelin directly.
- The Soroban runtime / `soroban-env-host` itself.
- Freighter, passkey-kit, or other third-party wallet implementations
  (beyond our adapter shim).
- The `oz-policy-mcp` *transport* layer (HTTP framing, bearer auth, rate
  limiting) as distinct from the tool surface — operational hardening of
  the hosted endpoint is Phase 10 work and is not part of the synthesizer
  audit.
- The agent skill prose and example Markdown / Python under
  [`skills/oz-policy-builder/`](skills/oz-policy-builder) — orchestration
  only, no auth or codegen logic.

## Known issues

The maintained inventory of open and resolved findings lives in
[`audits/index.md`](audits/index.md). At present it accurately states that
no external audit has been completed; self-audit by the lint suite
(Phase 9 Stream B) and the fuzz harness (Phase 9 Stream A) is in place.

## Audit history

All audit cycles are tracked under [`audits/`](audits/) with one
sub-directory per engagement (`audits/<auditor>-<date>/`). The handoff
package prepared for the next audit is in
[`audits/handoff-package/`](audits/handoff-package).

## Cryptographic and reproducibility commitments

- Build reproducibility is verified by `scripts/reproducible-build.sh`
  (Phase 9 Stream C). Released WASM artefacts are accompanied by a
  `reproducible-build-manifest.json` pinning the toolchain, `Cargo.lock`,
  `wasm-opt` version, and the produced WASM hashes.
- Generated policy contracts segregate storage by
  `(smart_account, context_rule_id)` — see
  [`audits/THREAT_MODEL.md`](audits/THREAT_MODEL.md) for the threat /
  mitigation / test mapping.
- `overflow-checks = true` is set on both `dev` and `release` profiles in
  the workspace `Cargo.toml`. This is **load-bearing**; do not remove it.

## What this policy does not cover

This policy covers the synthesizer and its outputs as built from this
repository. It does **not** cover:

- Issues you find in a generated policy that you modified by hand after
  codegen — please report those to whoever modified the contract.
- Off-chain operational security of any hosted instance of the MCP server
  (that is the operator's responsibility; we will, however, accept reports
  about the upstream code that the operator deployed).
