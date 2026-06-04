<!--
SPDX-License-Identifier: Apache-2.0
Copyright 2026 OZ Policy Builder contributors

Phase 10 — Mainnet readiness runbook. This document is the human-runnable
counterpart to the CI-automatable surface of Phase 10. Every step here is
labeled HUMAN-REQUIRED for a reason: no automation in this repository will
spend real XLM, register a domain, mint a GPG key, or push a tag on the
operator's behalf.
-->

# Mainnet Readiness Runbook

> **Status:** Pre-v1.0.0. This runbook describes the steps a human operator
> must run, in order, to take the OZ Accounts Policy Builder from "all CI
> gates green on a development branch" to "v1.0.0 published with a verified
> mainnet canary install." Every section header tagged **HUMAN-REQUIRED**
> describes a step that cannot be done by CI or by an agent on the
> operator's behalf — typically because it spends money, registers an
> identity, or touches a system outside this repository's control.

This document is referenced by:

- [`plan.md`](../plan.md) — Phase 10 *Completion Criterion (human-required)*.
- [`audits/READY.md`](../audits/READY.md) — pre-audit checklist.
- [`SECURITY.md`](../SECURITY.md) — disclosure flow (rehearsed here).

## Table of contents

1. [Prerequisites (HUMAN-REQUIRED)](#1-prerequisites-human-required)
2. [Mainnet canary procedure (HUMAN-REQUIRED)](#2-mainnet-canary-procedure-human-required)
3. [`v1.0.0` release procedure (HUMAN-REQUIRED)](#3-v100-release-procedure-human-required)
4. [Disclosure rehearsal](#4-disclosure-rehearsal)
5. [Rollback / reverse procedure](#5-rollback--reverse-procedure)
6. [Completed canaries](#6-completed-canaries)

---

## 1. Prerequisites (HUMAN-REQUIRED)

Every item below requires a real human action with a real-world side
effect: a billing relationship, a custodial decision, or a keypair the
operator personally controls. Do not proceed to §2 until **every** item
in this list is provisioned.

### 1.1 Cloud provider account

A container-friendly host capable of running a single Rust binary behind a
public TLS endpoint. Fly.io is the recommended starting point and is the
provider whose blueprint ships in [`infra/fly/`](../infra/fly/). See
[`infra/README.md`](../infra/README.md) for the rationale (cost, latency,
region pinning) and for the deploy steps.

What the operator must do **personally**:

- Sign up at the provider (Fly.io: `https://fly.io/`), accept the terms of
  service, and register a payment method. *No automation in this
  repository will do this for the operator.*
- Verify email / identity per the provider's requirements.
- Confirm the provider's free / low-cost tier covers expected traffic and
  understand the overage policy. Even a near-idle MCP endpoint can incur
  charges past the included quota.

### 1.2 DNS hostname

The hostname the MCP endpoint will live at — for example
`mcp.your-domain.com`. The operator must:

- Own (or be authorized on) a registered domain.
- Configure `A` / `AAAA` / `CNAME` records pointing at the provider's
  ingress (Fly.io: `<your-app>.fly.dev`).
- Plan for the additional zero-downtime-rotation work needed if the
  hostname ever has to move providers.

### 1.3 TLS certificate

The recommended Fly.io blueprint uses Fly's built-in Let's Encrypt
integration; the operator still runs `fly certs add mcp.your-domain.com`
once per hostname. For any other provider, provision a cert valid for the
hostname above. The MCP server itself does **not** terminate TLS — that is
the platform's job.

### 1.4 Funded mainnet keypair

Mainnet operations require real XLM. There is **no Friendbot on mainnet**
(Friendbot is testnet-only). To run the canary you need:

- A Stellar keypair you personally control (`stellar keys generate
  canary-mainnet`, or import one from a hardware wallet).
- A funding balance: **~5 XLM** is sufficient for one canary install
  (reserve + simulate fees + invoke fee + a margin for slippage on
  Stellar's fee market). Top up via an exchange withdrawal, a peer
  transfer, or any other mechanism you trust.
- A custody plan for the seed. The canary keypair touches mainnet exactly
  once for the canary install; treat it as production material.

### 1.5 Mainnet smart account

Either:

- **Option A — deploy a fresh OZ smart account on mainnet.** This is a
  real-cost transaction. Use the `oz-policy-installer` registry as the
  reference for the SA WASM hash; deploy with `stellar contract deploy`
  per OpenZeppelin's deployment docs.
- **Option B — use an existing OZ-SA-compatible mainnet account.** The
  account must be post-PR-#655 (see
  [`docs/oz-internal-shapes.md`](oz-internal-shapes.md) §8); the
  installer's preflight will reject anything older.

Record the SA contract address (`C…`) — you will paste it into §2 step 5.

### 1.6 GPG signing key

The release workflow (`.github/workflows/release.yml`) optionally signs
the `SHA256SUMS` artifact when the `RELEASE_GPG_KEY` secret is present.
Without the key, the release ships an unsigned `SHA256SUMS` and the CI
emits a `::warning::`. For a real `v1.0.0` you want the signature.

```bash
# 1. Generate a long-lived RSA-4096 key bound to a release-only address.
gpg --full-generate-key

# 2. Export the public key (publish on a keyserver + in the repo).
gpg --armor --export releases@your-domain.com > release-pubkey.asc

# 3. Export the secret key in ASCII-armored form for the GitHub secret.
gpg --armor --export-secret-keys releases@your-domain.com > release-secret.asc

# 4. Capture the fingerprint and update SECURITY.md.
gpg --fingerprint releases@your-domain.com
```

After generation, update [`SECURITY.md`](../SECURITY.md) (currently
ships with a `<placeholder>` for the fingerprint — see the file for the
exact line) with the real fingerprint **before** tagging `v1.0.0`.

### 1.7 crates.io API token

Required for `publish-crates` to publish the 7 workspace member crates.

```bash
# Sign in at https://crates.io/me, mint a scoped token, and store it as
# the GHA secret `CARGO_REGISTRY_TOKEN`.
```

If the token is unset at release time, the `publish-crates` job emits a
warning and skips publishing — the GitHub Release itself still ships.

### 1.8 npm token

Required for `publish-npm` to publish `@oz-policy-builder/wallet-adapter`.

```bash
# 1. Log in: npm login.
# 2. Mint a CI-scoped automation token:
npm token create --read-only=false --cidr=0.0.0.0/0 --otp=<2fa>
# 3. Store as the GHA secret `NPM_TOKEN`.
```

If unset, the `publish-npm` job emits a warning and skips. **Before
running, replace the `oz-policy-builder` segments in
`wallet-adapter/package.json` with the published org name** — see the
file's leading `//placeholder` comment.

### 1.9 MCP bearer token

For the hosted endpoint. Generate a high-entropy value and store it as
the Fly secret `OZ_POLICY_MCP_TOKEN` (and optionally as the GHA secret of
the same name if your CI ever needs to exercise the hosted endpoint).

```bash
fly secrets set OZ_POLICY_MCP_TOKEN="$(openssl rand -hex 32)"
```

---

## 2. Mainnet canary procedure (HUMAN-REQUIRED)

The canary is a single end-to-end install of a real policy on a real
mainnet smart account, exercising the full synthesize → codegen →
simulate → sign → submit → verify path. It is the load-bearing evidence
that the system works on mainnet, not just on testnet.

### Step 1 — Confirm the testnet path is still green

> **Why first:** if `INTEGRATION=1 pnpm test` fails on testnet, mainnet
> will fail too — and burn real XLM finding out.

```bash
cd wallet-adapter
INTEGRATION=1 pnpm test
```

The Phase 7 integration test asserts the full record → install → verify
flow lands a SUCCESS transaction on testnet with `verifyInstall.matches:
true`. As of 2026-05-18 the on-chain readback path is wired (commit
`2606f84`, `crates/oz-policy-mcp/src/verify_chain.rs`); the frozen
SUCCESS evidence is at
[`walkthroughs/phase7-testnet-install/install-result.json`](../walkthroughs/phase7-testnet-install/install-result.json).
Confirm the suite ends green; do **not** proceed otherwise.

### Step 2 — Set GHA secrets

Configure the four secrets enumerated in §1 against the GitHub repo:

| Secret | Source | Used by |
| --- | --- | --- |
| `CARGO_REGISTRY_TOKEN` | §1.7 | `publish-crates` job in `release.yml` |
| `NPM_TOKEN` | §1.8 | `publish-npm` job in `release.yml` |
| `RELEASE_GPG_KEY` | §1.6 (the *secret*-key export) | `publish-release` job (signs SHA256SUMS) |
| `RELEASE_GPG_PASSPHRASE` | §1.6 (if the key has a passphrase) | `publish-release` job |
| `OZ_POLICY_MCP_TOKEN` | §1.9 | Fly app secret (set via `fly secrets set`, **not** as a GHA secret unless CI exercises the hosted endpoint) |

The `release.yml` workflow guards every secret with a `::warning::`
fallback (see lines 222–226 and 288–297), so absent secrets produce a
soft-fail rather than a hard error.

### Step 3 — Deploy `oz-policy-mcp` to Fly.io

```bash
cd infra/fly

# First-time only:
fly launch --name <your-unique-app-name> --copy-config --no-deploy

# Every time:
fly secrets set OZ_POLICY_MCP_TOKEN="$(openssl rand -hex 32)"   # if not already set
./deploy.sh
```

Confirm health:

```bash
curl -fsS "https://<your-app>.fly.dev/healthz"
# Expected: 200 OK with a small JSON or empty body.
```

`GET /healthz` is intentionally **un-authenticated** (see
[`infra/README.md`](../infra/README.md) §"Health check"). Do not put it
behind the bearer-auth layer.

### Step 4 — Scripted 5-tool session over Streamable HTTP

Verify the remote endpoint round-trips the same 5 tools the local STDIO
build does. A minimal `curl`-based driver:

```bash
URL="https://<your-app>.fly.dev/mcp"
TOKEN="<the same value you set as OZ_POLICY_MCP_TOKEN>"

# 1. tools/list
curl -fsS -H "Authorization: Bearer $TOKEN" \
     -H 'Content-Type: application/json' \
     --data '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' \
     "$URL" | jq '.result.tools[].name'
# Expected (sorted): export_policy record_transaction simulate_policy
#                    synthesize_policy verify_install

# 2. record_transaction → 3. synthesize_policy → 4. simulate_policy
#    → 5. export_policy. Use the testnet inputs from
#    walkthroughs/01-blend-yield/ to keep this hermetic — see
#    docs/mcp-clients.md for the full per-tool JSON-RPC examples.
```

Confirm byte-equality of the outputs vs. a local STDIO run of the same
inputs. Any divergence is a P0 bug — file it before proceeding.

### Step 5 — Deploy a mainnet policy WASM

The canary uses the `function_allowlist` policy because its WASM is
already pinned and verified against the codegen golden:

- Source spec: [`walkthroughs/phase3-codegen-fixture/spec.json`](../walkthroughs/phase3-codegen-fixture/spec.json)
- Pinned WASM: [`walkthroughs/phase3-codegen-fixture/expected/slot_0/policy.wasm`](../walkthroughs/phase3-codegen-fixture/expected/slot_0/policy.wasm)
- Pinned hash: `cb2a8736040711ff831346b20912fc1fe54a9bc096f9dab288014940d72b6fd4`
  (see [`walkthroughs/phase3-codegen-fixture/expected/slot_0/wasm_hash.txt`](../walkthroughs/phase3-codegen-fixture/expected/slot_0/wasm_hash.txt))

```bash
# Upload the pinned WASM to mainnet (real-cost).
stellar contract upload \
    --wasm walkthroughs/phase3-codegen-fixture/expected/slot_0/policy.wasm \
    --source-account <canary-mainnet> \
    --network mainnet
# → returns the WASM hash; assert it equals cb2a8736...

# Deploy the policy contract.
stellar contract deploy \
    --wasm-hash cb2a8736040711ff831346b20912fc1fe54a9bc096f9dab288014940d72b6fd4 \
    --source-account <canary-mainnet> \
    --network mainnet
# → returns the policy contract address; record it.
```

Record both addresses (SA from §1.5, policy from this step) in a fresh
`walkthroughs/mainnet-canary/deployed-addresses.json`.

### Step 6 — Run synthesize + codegen against a real mainnet recording

Pick or compose a mainnet transaction that the policy will gate (a
`function_allowlist` is gated by function name, so any contract you want
to bind through an SA works). Run:

```bash
cargo run -p oz-policy-cli -- record \
    --network mainnet \
    --rpc-url <your-mainnet-rpc-url> \
    --tx-hash <real-mainnet-tx-hash> \
    > walkthroughs/mainnet-canary/recording.json

cargo run -p oz-policy-cli -- synthesize \
    walkthroughs/mainnet-canary/recording.json \
    --mode auto \
    > walkthroughs/mainnet-canary/spec.json
```

The `--mode auto` step is the structural test that synthesize is
deterministic against real mainnet input.

### Step 7 — Build, sign, submit the install envelope

```bash
# Build the install envelope (calls simulateTransaction against mainnet
# RPC; no auto-submit).
cargo run -p oz-policy-cli -- prepare-install \
    --spec walkthroughs/mainnet-canary/spec.json \
    --smart-account <SA-from-§1.5> \
    --network mainnet \
    --rpc-url <your-mainnet-rpc-url> \
    > walkthroughs/mainnet-canary/install-envelope.xdr
```

Sign with **either** of the two adapter paths shipped in
`wallet-adapter/`:

- **Freighter:** sign the envelope from the Freighter UI bound to the
  mainnet SA owner. Requires Freighter configured for mainnet
  (`settings → network → public`).
- **passkey-kit:** use the SA-owner keypair from §1.4 plus the
  `ozAuthPayloadEncoder` hook (Phase 7 Round 2 — see
  [`walkthroughs/phase7-testnet-install/BLOCKER.md`](../walkthroughs/phase7-testnet-install/BLOCKER.md)
  §"Remediation path — Option A").

Submit the signed envelope to mainnet RPC and wait for inclusion.
**Capture the transaction hash.**

### Step 8 — Call `verify_install`

```bash
cargo run -p oz-policy-cli -- verify-install \
    --smart-account <SA-from-§1.5> \
    --context-rule-id <id-emitted-by-step-7> \
    --network mainnet \
    --rpc-url <your-mainnet-rpc-url>
```

Assert `matches: true`. **If `matches: false`, do not proceed** — capture
the drift items and treat as a P0 bug per
[`SECURITY.md`](../SECURITY.md).

> **Note:** both halves of the verify path are wired as of 2026-05-18 —
> the write side (`oz_smart_account_auth` encoder, commit `bd60009`) and
> the read side (real on-chain `simulateTransaction(SA.get_context_rule)`
> readback in `crates/oz-policy-mcp/src/verify_chain.rs`, commit
> `2606f84`). On testnet they land `matches: true` end-to-end (see
> [`walkthroughs/phase7-testnet-install/install-result.json`](../walkthroughs/phase7-testnet-install/install-result.json)).
> A mainnet `matches: false` therefore represents a real drift, not a
> pending implementation gap.

### Step 9 — Freeze the evidence

Write the captured material under `walkthroughs/mainnet-canary/`:

```
walkthroughs/mainnet-canary/
  README.md                       # human-readable summary
  deployed-addresses.json         # SA + policy addresses
  recording.json                  # frozen mainnet recording
  spec.json                       # frozen synthesized spec
  install-envelope.xdr            # the envelope that was signed
  signed-tx.xdr                   # the signed envelope (optional)
  tx-hash.txt                     # the mainnet tx hash
  captured-at.txt                 # ISO-8601 timestamp
  verify-install-report.json      # the matches:true verify report
```

Commit with a GPG-signed commit (`git commit -S`) — this is the only
mainnet evidence; its provenance matters.

### Step 10 — Record the canary in §6 of this document

Append a row to the [Completed canaries](#6-completed-canaries) table
below. **Do not** invent or pre-fill a hash. The table starts empty.

---

## 3. `v1.0.0` release procedure (HUMAN-REQUIRED)

The release workflow at `.github/workflows/release.yml` is fully
automatable from the tag push onward — but only after the human gates
below clear.

### Step 1 — Confirm `audits/READY.md` is fully green

Open [`audits/READY.md`](../audits/READY.md). Every box in **"Required
before engagement"** must be ticked with linked evidence. At the time of
this writing, several boxes (notably "OZ engagement plan in place")
remain unchecked — that is by design and is the reason no audit has been
booked. The auditor's report under `audits/<auditor>-<date>/report.pdf`
is a precondition for `v1.0.0`.

### Step 2 — Update `CHANGELOG.md`

```bash
# Add a heading at the top of CHANGELOG.md:
#
#   ## v1.0.0 — <YYYY-MM-DD>
#
#   - External audit (<auditor>, <date>) complete; report in audits/<auditor>-<date>/.
#   - Mainnet canary install verified; tx hash <0x…>.
#   - First crates.io publish of all 7 workspace members.
#   - First npm publish of @oz-policy-builder/wallet-adapter.
```

### Step 3 — Annotated, signed tag

```bash
git tag -a -s v1.0.0 -m "v1.0.0: external audit complete + mainnet canary verified"
```

The `-s` flag signs the tag with the operator's GPG key. The release
workflow does not require a signed tag, but a signed v1.0.0 tag is the
auditable record of who shipped the release.

### Step 4 — Push the tag

```bash
git push origin v1.0.0
```

Pushing to a `v*` tag triggers `.github/workflows/release.yml`. The
workflow:

1. Cross-compiles `oz-policy-cli` + `oz-policy-mcp` for four target /
   OS pairs (linux-amd64, linux-arm64, darwin-amd64, darwin-arm64).
2. Bundles the walkthrough corpus.
3. Publishes a GitHub Release with all binary tarballs + `SHA256SUMS`
   (+ `SHA256SUMS.asc` when `RELEASE_GPG_KEY` is set).
4. Runs `cargo publish` for the 7 workspace member crates in dependency
   order (`oz-policy-core` → `oz-policy-recorder` → `oz-policy-codegen`
   → `oz-policy-simhost` → `oz-policy-installer` → `oz-policy-cli` →
   `oz-policy-mcp`), gated on `CARGO_REGISTRY_TOKEN`.
5. Publishes `wallet-adapter` to npm as
   `@oz-policy-builder/wallet-adapter`, gated on `NPM_TOKEN`.

### Step 5 — Verify the release

```bash
# 5a. The GitHub Releases page shows attached artifacts.
gh release view v1.0.0

# 5b. Verify SHA256SUMS.
gh release download v1.0.0 --pattern 'SHA256SUMS*'
gpg --verify SHA256SUMS.asc SHA256SUMS

# 5c. Verify the crates.io publishes resolved.
cargo search oz-policy-core | grep '^oz-policy-core = "1.0.0"'

# 5d. Verify the npm publish resolved.
npm view @oz-policy-builder/wallet-adapter version   # should print 1.0.0
```

If any verification fails, **do not** delete the tag — open an issue,
diagnose the partial state, and ship `v1.0.1` once corrected. Deleting a
published tag is a security smell.

---

## 4. Disclosure rehearsal

Per [`SECURITY.md`](../SECURITY.md), the project commits to a 90-day
default disclosure window. The rehearsal exercises the flow against a
fabricated finding so the real one is not the first time anyone runs the
process.

### Scenario

Simulate a Tier-1 finding: the synthesizer emits an over-permissive
`function_allowlist` constraint (e.g., it includes a function name the
recording did not exercise). The audit-lint suite at
`crates/oz-policy-codegen/src/audit_lints.rs` is the natural canary for
this class of bug.

### Steps

1. **File a private GitHub Security Advisory** on the repo. Treat it as
   if the reporter is external — the advisory text should reproduce the
   bug from a `PolicySpec` + recording pair.
2. **Acknowledge** within five business days per `SECURITY.md`.
3. **Remediate**: open a PR (private fork, then push to a maintainer
   branch) that tightens the synthesizer. Land it behind a feature flag
   if the fix is risky.
4. **Verify**: extend the `simhost` deny-vector generator to cover the
   tightened constraint. Confirm the previously-permissive vector is now
   rejected.
5. **Publish CVE** if the impact warrants. The OZ Smart Account is
   itself out-of-scope (upstream), so CVE attribution is for the
   synthesizer side.
6. **Record evidence** in `docs/canary/disclosure-rehearsal-<YYYY-MM-DD>.md`
   with: the fabricated finding, the timeline, the PR link, and a
   retrospective on any gaps in the `SECURITY.md` flow.

The rehearsal must complete **before** `v1.0.0`. Re-run annually.

---

## 5. Rollback / reverse procedure

The mainnet canary installs a context rule on the canary smart account.
"Rolling back" is not "undeploying the policy contract" — the policy
WASM stays on-chain (it does no harm in the absence of a binding rule).
What is reversible is the **context-rule binding**.

### Reverting a context rule

```bash
# Build a remove envelope (uses the same oz-policy-installer path).
cargo run -p oz-policy-cli -- prepare-remove \
    --smart-account <SA-from-§1.5> \
    --context-rule-id <id-from-canary> \
    --network mainnet \
    --rpc-url <your-mainnet-rpc-url> \
    > walkthroughs/mainnet-canary/remove-envelope.xdr
```

> **Note:** `prepare-remove` is implied by the OZ smart-account
> `remove_context_rule` host function; if the CLI does not yet expose
> this subcommand, build the equivalent envelope by hand with
> `stellar contract invoke … -- remove_context_rule --rule_id <id>` and
> sign through the same wallet adapter path as the install.

Sign with the same wallet path used for the install (§2 step 7) and
submit. The removal lands as a new transaction; the install transaction
is still on chain (Stellar transactions are immutable — what changes is
the SA's runtime state, not the ledger history).

### When to roll back

- `verify_install` returns `matches: false` and the drift indicates a
  permissive constraint (the policy lets through what it should reject).
- A post-deployment audit finding requires re-issuing the policy with a
  tighter spec.
- The policy was bound to the wrong context-rule scope.

### When **not** to roll back

- Network congestion / submission lag: wait for confirmation, do not
  re-submit.

---

## 6. Completed canaries

| Date (UTC) | Network | SA address | Policy address | Tx hash | Verify result | Evidence dir |
| --- | --- | --- | --- | --- | --- | --- |
| _none yet_ | _none yet_ | _none yet_ | _none yet_ | _none yet_ | _none yet_ | _none yet_ |

> **Do not pre-fill this table.** Every row represents a real,
> human-witnessed mainnet install. Adding a row is the **last** step of
> §2 — after `verify_install` returns `matches: true` and the evidence
> dir has been committed.
