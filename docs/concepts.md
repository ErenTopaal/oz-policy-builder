# Concepts

What the **OZ Accounts Policy Builder** is, what it produces, and what each
piece of the pipeline actually does. Read this once before the walkthroughs.

For exact OZ source-level shapes (the `ContextRule`, `ContextRuleType`,
`AuthPayload`, primitive install params), see
[`docs/oz-internal-shapes.md`](oz-internal-shapes.md).

---

## Context Rule

A **context rule** is the OpenZeppelin smart-account record that maps "when
this kind of call happens" to "these signers are accepted" plus "these
policies must approve". It lives entirely inside the smart-account contract
at `(smart_account, context_rule_id)`.

Verbatim from `stellar-accounts v0.7.1`
(`packages/accounts/src/smart_account/storage.rs:153-174`, transcribed in
[`docs/oz-internal-shapes.md`](oz-internal-shapes.md) §6):

```rust
pub struct ContextRule {
    pub id: u32,
    pub context_type: ContextRuleType,   // Default | CallContract(Address) | CreateContract(BytesN<32>)
    pub name: String,
    pub signers: Vec<Signer>,
    pub signer_ids: Vec<u32>,            // global registry IDs, positional
    pub policies: Vec<Address>,          // policy contract addresses
    pub policy_ids: Vec<u32>,            // global registry IDs, positional
    pub valid_until: Option<u32>,        // expiry ledger sequence
}
```

The two `ContextRuleType` variants this builder emits today:

- `Default` — the rule applies to any call not otherwise scoped. Used for
  the bootstrap rule installed by `init` (see
  [`walkthroughs/phase7-testnet-install/README.md`](../walkthroughs/phase7-testnet-install/README.md)).
- `CallContract(Address)` — the rule applies only when the call targets a
  specific contract. **Required** for `spending_limit` per OZ PR-#649 (see
  [`docs/oz-internal-shapes.md`](oz-internal-shapes.md) §9).

`CreateContract(BytesN<32>)` is part of the upstream surface but not
currently emitted by the synthesizer.

---

## Policy

A **policy** is the on-chain contract address called by the smart account's
`__check_auth` to decide whether a given invocation is allowed. The contract
implements the `Policy` trait
(`packages/accounts/src/policies/mod.rs:47-163` — verbatim in
[`docs/oz-internal-shapes.md`](oz-internal-shapes.md) §1):

```rust
pub trait Policy {
    type AccountParams: FromVal<Env, Val>;
    fn enforce(e: &Env, context: Context, authenticated_signers: Vec<Signer>,
               context_rule: ContextRule, smart_account: Address);
    fn install(e: &Env, install_params: Self::AccountParams,
               context_rule: ContextRule, smart_account: Address);
    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address);
}
```

The builder emits two flavours of policy in a `PolicySpec`:

- **Track A — Existing** (`PolicySlot::Existing`). References an upstream OZ
  primitive by template family (e.g., `spending_limit`,
  `simple_threshold`, `weighted_threshold`). The on-chain address is
  resolved at install time from
  [`crates/oz-policy-installer/src/registry.rs`](../crates/oz-policy-installer/src/registry.rs).
  No new WASM is produced — Track A composes already-deployed primitives.

- **Track B — Generated** (`PolicySlot::Generated`). A purpose-built
  policy contract synthesized from a `template_family` + a constraint set
  (e.g., `function_allowlist`, `bounded_swap`). Codegen produces Rust
  source, a built `policy.wasm`, and its lowercase-hex SHA-256 hash. See
  [`crates/oz-policy-codegen/src/`](../crates/oz-policy-codegen/src) and
  the per-walkthrough `wasm/slot_0/` directories.

---

## What the synthesizer does

The synthesizer is the **decision tree** in
[`crates/oz-policy-core/src/decision_tree.rs`](../crates/oz-policy-core/src/decision_tree.rs).
Given a frozen `Recording` and a `SynthesisOptions` (mode + tightness +
lifetime + rule name), it emits a deterministic `PolicySpec`.

The three synthesis modes from
[`crates/oz-policy-cli/src/main.rs`](../crates/oz-policy-cli/src/main.rs):

| Mode             | Behaviour                                                                                                                              |
|------------------|----------------------------------------------------------------------------------------------------------------------------------------|
| `auto`           | Prefer Track A when the constraint shape fits an existing primitive; fall through to Track B otherwise. Default.                       |
| `compose-only`   | Track A only. Errors with `E_NO_TRACK_A_FIT` if no primitive composes the recording's constraints.                                     |
| `codegen-only`   | Track B only. Always emits a Generated slot, even when an Existing primitive would suffice.                                            |

Tightness (`exact`, `small-margin`, `loose`) is the knob that determines
how strictly the observed argument values are bound into the constraint set.
The current Phase 2 emission for swap traces does not yet bind amount ranges
under `small-margin`; that is a Phase 9 follow-up (see
[`walkthroughs/03-soroswap-bounded/README.md`](../walkthroughs/03-soroswap-bounded/README.md)
"Synthesized policy").

**Determinism contract**: identical inputs always produce byte-equal output,
modulo synthesizer-generated UUIDs which are explicitly out of band. This
is enforced by [`plan.md`](../plan.md) §"Cross-Phase Invariants → 2.
Deterministic synthesizer" and tested by every walkthrough's
re-run-and-diff step.

---

## What the simulator verifies

The simulator
([`crates/oz-policy-simhost/`](../crates/oz-policy-simhost)) replays the
recording inside a Soroban test host, **installs every Generated policy
WASM** from the per-slot `wasm-dir`, and runs two suites:

1. **Permit case** — the original recording. The policy must NOT panic
   (`permit.passed = true`).
2. **Deny vectors** — programmatically derived "what if" mutations of the
   recording (e.g., flip the function name; substitute a different asset
   address). For each, the policy **must** panic with the exact error code
   the constraint family advertises (e.g., `FunctionNotAllowed = 1010`,
   `AssetNotAllowed = 1040`). Vector pass = panicked with the right code.

The output is a `SimReport` JSON — see
[`walkthroughs/01-blend-yield/expected-sim-report.json`](../walkthroughs/01-blend-yield/expected-sim-report.json)
for the canonical shape.

Known simulator gaps are surfaced honestly in each walkthrough's README
(e.g., the Track-A `spending_limit` WASM is not vendored, so SEP-41 deny
vectors fail open — `actual_error_code: null` — until Phase 9 vendors the
upstream WASM). The corpus reflects current observable behaviour, not the
aspirational future.

---

## What the agent skill orchestrates

The agent skill (`skills/oz-policy-builder/SKILL.md`) is the human-facing
loop. It walks the user through:

```
record → summarize → clarify → synthesize → simulate → export → user signs
```

Step-by-step:

1. **Record** — call `record_transaction` MCP tool with a hash or a
   simulation envelope. Get back a deterministic `Recording`.
2. **Summarize** — call `summarize_recording.py` to produce a plain-English
   description of what the recording does.
3. **Clarify** — call `propose_clarifications.py` to ask the user
   targeted yes/no questions about constraint tightness and signer scope.
4. **Synthesize** — call `synthesize_policy` with the user-confirmed
   `SynthesisOptions`. Get back a `PolicySpec`.
5. **Simulate** — call `simulate_policy` to verify the spec permits the
   recording and denies the deny-vector mutations.
6. **Export** — call `export_policy` to compile any Generated slots,
   build the install envelope via `oz-policy-installer`, and return the
   base64 XDR.
7. **User signs** — the agent hands the envelope to the user's wallet
   ([Wallets](wallets.md)). The agent **never** auto-submits.

See [`skills/oz-policy-builder/SKILL.md`](../skills/oz-policy-builder/SKILL.md)
for the full prompt + scripts.

---

## What we DON'T do

These are load-bearing non-features, enforced by
[`plan.md`](../plan.md) §"Cross-Phase Invariants":

- **No auto-deployment.** No tool in any phase submits a transaction
  without a prior wallet signature. The only submission paths are
  Phase 7 (testnet, headless passkey-kit in tests) and Phase 10 Stream D
  (mainnet canary, manual). Reviewers reject any PR adding an unattended
  submission path.
- **No LLM in the codegen path.** The synthesizer, codegen, simhost, and
  installer are pure functions. LLMs appear only in the agent skill's
  clarification/summarization role — never in the artefact pipeline.
- **No silent template selection.** When `--mode auto` picks Track A, the
  synthesizer records `synthesis_mode: "auto"` and the chosen primitive
  family in the spec; the user can re-derive the choice by reading the
  decision tree against the recording.
- **No fabricated constraints.** A constraint must come from an observed
  value in the recording or from a user-confirmed clarification. The
  synthesizer never invents an upper bound, a deadline window, or a
  signer set.

---

<!-- Licensed under the Apache License, Version 2.0 — see LICENSE-APACHE. -->
