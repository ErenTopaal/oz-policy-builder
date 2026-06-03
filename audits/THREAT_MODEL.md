# Threat Model — OZ Accounts Policy Builder

Audience: external auditor. Each threat below pairs (1) a mitigation in
the codebase with (2) the test that exercises it. Walk top-to-bottom and
every claim has both an engineering control and a verifying signal.

The synthesizer-side and generated-policy-side threats follow the order
of [`researches/technical-research.md`](../researches/technical-research.md)
§12. A tenth threat — **AuthPayload encoding bug**, surfaced as the
Phase 7 BLOCKER (resolved 2026-05-18) — is included because the wallet
adapter's `oz_smart_account_auth.ts` encoder sits on the auth path of
every policy install.

---

## Synthesizer-side threats

### T1. Spec underspecification

**Description.** A user gives an ambiguous, partial, or self-contradictory
intent to the synthesizer (e.g. "limit my swaps" with no token or amount),
and the resulting `PolicySpec` encodes an interpretation the user did not
actually consent to.

**Mitigation.** Two-layered:

1. The agent skill enforces a clarification loop: the synthesizer surfaces
   missing fields as structured `clarification_request` records before
   producing a final `PolicySpec`.
2. Before any install envelope is built, the spec is rendered back to the
   user as **plain-English replay**, and an explicit user approval is
   required.

**Test.**
- [`skills/oz-policy-builder/evals/eval_*.yaml`](../skills/oz-policy-builder/evals)
  — the eval corpus contains underspecified prompts and asserts that the
  agent skill emits a clarification request, not a `PolicySpec`.

---

### T2. Codegen template bug

**Description.** An `.rs.jinja` template emits Rust that compiles but
encodes the wrong semantics (e.g. inverted comparison, wrong storage key
shape, missing default-reject branch). Because the template is reused by
every policy of that shape, the bug recurs across every output.

**Mitigation.** Three-layered:

1. **Per-constraint golden tests.** Each template has a frozen golden
   `.rs` output for a fixed `PolicySpec`. Any template change forces a
   golden diff that must be reviewed.
2. **Audit lints (Stream B).** Static checks run over the generated source
   before sandbox compile. Required: `require_auth_first`,
   `storage_keyed_by_pair`, no `unsafe`, no `core::mem::transmute`, no
   bare `panic!`.
3. **Sim deny cross-check.** Every template's positive case is paired with
   negative recordings driven through the simhost; the harness asserts
   the deny path fires for each case the spec declares.

**Test.**
- [`crates/oz-policy-codegen/tests/golden_render.rs`](../crates/oz-policy-codegen/tests/golden_render.rs)
- `crates/oz-policy-codegen/src/audit_lints.rs` — Stream B's lint suite
  (file is produced by Stream B in this same Phase 9; this document is
  written in parallel with that stream).

---

### T3. Reproducibility failure

**Description.** Two builds of the same source at the same commit produce
different WASM bytes (or different hashes), defeating the audit guarantee
that "the audited code is the code that runs."

**Mitigation.**

- `rust-toolchain.toml` pins the compiler version.
- `Cargo.lock` is checked in (this is a binary/workspace, not a library
  consumer pattern).
- `wasm-opt` version is recorded in the build manifest produced by
  `scripts/reproducible-build.sh` (Stream C).
- The reproducible-build script runs inside a hermetic Docker image
  (`ci/Dockerfile`, Stream C) and asserts the produced WASM hashes match
  the pinned values from the walkthrough corpora.

**Test.**
- `scripts/reproducible-build.sh` — the script itself is the test. The
  Stream C completion criterion is: re-running it on a fresh clone on a
  second machine produces a byte-identical
  `reproducible-build-manifest.json`.

---

### T4. LLM non-determinism

**Description.** If the LLM is on the codegen path, two identical user
prompts can produce two different `PolicySpec`s or two different generated
contracts, breaking the audit guarantee from T3.

**Mitigation.** Architectural: the synthesizer is **pure functions** over
the `PolicySpec` shape. LLMs are restricted to the agent skill's
clarification and summarization surfaces only — they never write
`PolicySpec` fields or template parameters directly. Phase 6 of the plan
codifies this boundary.

**Test.**
- Determinism tests in `crates/oz-policy-core/src/decision_tree.rs` —
  passing the same spec / recording twice produces byte-identical decision
  trees.
- `crates/oz-policy-codegen/tests/golden_render.rs` (re-used from T2) —
  byte-identical output for byte-identical input.

---

## Generated-policy-side threats

### T5. Cross-rule replay

**Description.** Two `context_rule_id`s installed on the same smart account
share storage, so state mutated under rule A is observable / mutable under
rule B. An attacker invokes A's hook and indirectly bumps a counter belonging
to B, or vice versa.

**Mitigation.** Every template stores state under a `StorageKey` variant
keyed by the `(Address, u32)` tuple `(smart_account, context_rule_id)`. No
bare-key writes are permitted in any template.

**Test.**
- Stream B's `storage_keyed_by_pair` lint in
  `crates/oz-policy-codegen/src/audit_lints.rs` — fires
  `E_CODEGEN_COMPILE_FAILED` if any storage write is not keyed by the
  `(Address, u32)` pair.

---

### T6. i128 overflow

**Description.** An accumulator (e.g. cumulative spend) overflows `i128`
silently in a release build, wrapping around and bypassing a spending cap.

**Mitigation.** `overflow-checks = true` is set on **both** the `dev` and
`release` profiles in the workspace
[`Cargo.toml`](../Cargo.toml). This matches Blend's published guidance,
quoted verbatim in research §12:
*"Under no circumstances should the overflow-checks flag be removed
otherwise contract math will become unsafe"*.

**Test.**
- [`Cargo.toml`](../Cargo.toml) profile setting — the file contains an
  explicit "load-bearing" comment forbidding removal of the flag.
- The synthesizer's i128 arithmetic tests in
  [`crates/oz-policy-core/src/decision_tree.rs`](../crates/oz-policy-core/src/decision_tree.rs)
  and the recording / arg-value layer
  ([`crates/oz-policy-core/src/arg_value.rs`](../crates/oz-policy-core/src/arg_value.rs)).

---

### T7. Unauthorized state mutation

**Description.** A generated policy's `enforce` (or other mutating hook)
mutates storage without first asserting that the smart account itself is
the caller, letting an arbitrary contract drive state transitions.

**Mitigation.** Every template emits `smart_account.require_auth()` as the
**first** line of `enforce`. Templates are forbidden from interleaving any
storage or computation before that call.

**Test.**
- Stream B's `require_auth_first` lint in
  `crates/oz-policy-codegen/src/audit_lints.rs` — regex-asserts that the
  first statement in every `enforce` body is `smart_account.require_auth()`.

---

### T8. TTL exhaustion

**Description.** Soroban entries expire if not bumped. A long-lived policy
that never re-bumps its TTL eventually drops its state, silently resetting
counters or thresholds.

**Mitigation.** Templates bump TTL during `enforce` so any successful
permit re-extends the entry. The bump is part of the template body, not
opt-in.

**Test.**
- Template inspection: the base template
  [`templates/base.rs.jinja`](../templates/base.rs.jinja) and each
  constraint template under
  [`templates/constraints/`](../templates/constraints) carry an explicit
  TTL-bump block.
- Simhost permit replay: `crates/oz-policy-simhost/src/permit.rs` and
  `crates/oz-policy-simhost/src/run.rs` drive the generated WASM through
  multiple permit invocations and inspect the resulting host budget /
  storage to confirm TTL is re-extended.

---

### T9. Sponsor `context_rule_ids` substitution

**Description.** A sponsoring transaction submits an `AuthPayload`
referencing a different `context_rule_id` than the one the smart account
actually owns, tricking the SA into enforcing the wrong policy. This was
a real upstream issue addressed in OpenZeppelin PR #655.

**Mitigation.**

1. OZ PR #655 fixes the substitution at the smart-account layer.
2. Our installer's **preflight refuses pre-#655 accounts**, falling back to
   the three strategies documented in
   [`docs/oz-internal-shapes.md`](../docs/oz-internal-shapes.md) §8 (WASM
   hash whitelist, behavioural probe, user assertion).

**Test.**
- [`crates/oz-policy-installer/src/preflight.rs`](../crates/oz-policy-installer/src/preflight.rs)
  and its unit tests — assert the preflight emits
  `E_INSTALL_PREFLIGHT_FAILED` against accounts whose runtime markers
  match the pre-#655 fingerprint.

---

### T10. AuthPayload encoding bug

**Description.** The `add_context_rule` call on an OpenZeppelin smart
account requires a custom
`AuthPayload { signers: Map<Signer, Bytes>, context_rule_ids: Vec<u32> }`
ScVal as the second positional arg in the auth-tree signature; the SA's
`__check_auth` recomputes a SHA-256 digest over a canonical encoding and
rejects any mismatch. A wrong encoding fails closed (no rule lands), but a
*subtly* wrong encoding that the SA accepts could authorise the wrong
rule. Surfaced as the Phase 7 BLOCKER; resolved end-to-end on testnet
2026-05-18 via `wallet-adapter/src/oz_smart_account_auth.ts` (commit
`bd60009`) — see
[`../walkthroughs/phase7-testnet-install/install-result.json`](../walkthroughs/phase7-testnet-install/install-result.json).

**Mitigation.** The wallet adapter exports a single encoder
(`buildOzAuthEntry`, `computeAuthDigest`, `encodeAuthPayload`,
`makeOzSmartAccountAuthEncoder`) that produces the OZ-SA AuthPayload and
its SHA-256 digest deterministically. `installPolicy` injects the encoded
payload into any `SorobanCredentials::Address(<SA>)` auth entry before
`sendTransaction`. The digest is independently verifiable.

**Test.**
- [`wallet-adapter/src/oz_smart_account_auth.test.ts`](../wallet-adapter/src/oz_smart_account_auth.test.ts)
  — covers encoding, digest computation, and the install-path injection.
- [`wallet-adapter/src/phase7_integration.test.ts`](../wallet-adapter/src/phase7_integration.test.ts)
  (gated by `INTEGRATION=1`) — end-to-end testnet replay through the
  encoder hook.

---

## Cross-cutting controls

The following are enforced workspace-wide and apply to every threat above.

- **`overflow-checks = true`** on `dev` and `release` profiles
  ([`Cargo.toml`](../Cargo.toml)). Load-bearing for T6 and any constraint
  that does i128 arithmetic.
- **`cargo deny`** runs in CI with the allow-list at
  [`deny.toml`](../deny.toml). Three rustls-webpki RUSTSEC advisories are
  currently ignored with a documented rationale; an auditor should
  re-evaluate them at the time of audit.
- **Per-template golden tests** under
  `crates/oz-policy-codegen/tests/golden_render.rs` freeze the byte
  output of every supported constraint shape.
- **Audit lints** (Stream B) run before sandbox compile and surface lint
  failures as `E_CODEGEN_COMPILE_FAILED`.
- **Fuzz harnesses** (Stream A, nightly CI) cover `enforce` against
  arbitrary `ScVal` contexts, `PolicySpec` → WASM panic-freedom, and the
  recorder's XDR decoder.

## Threats deliberately not enumerated

- **Wallet implementation bugs** beyond our AuthPayload encoder.
- **Off-chain operational compromise** of a hosted MCP instance — Phase 10
  scope.
- **Front-end XSS / phishing of the user** — out of scope; we ship no
  front-end.

These are flagged here so the auditor sees them and confirms the scope cut
intentionally.
