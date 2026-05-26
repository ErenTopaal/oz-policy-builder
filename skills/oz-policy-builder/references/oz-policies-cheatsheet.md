# OpenZeppelin smart-account policies — cheatsheet

This is the reviewer-friendly companion to the synthesizer. Use it when the user
asks **what** a primitive does, or **why** the synthesizer refused a combination.
Everything below is quoted from `docs/oz-internal-shapes.md` (which is itself a
verbatim copy from `OpenZeppelin/stellar-contracts` tag `v0.7.1`, the version
this toolkit pins).

---

## The three composable primitives

OpenZeppelin ships three audited policy primitives. The synthesizer composes
these into Track-A `PolicySlot::Existing` entries when the recording matches
their shape; otherwise it falls back to Track-B (generated) slots.

### 1. `simple_threshold`

Install params: `{ threshold: u32 }` — the minimum number of signers required
for authorization. Accepts **any** `ContextRuleType` (`Default`,
`CallContract(_)`, `CreateContract(_)`).

Use it when:
- The recording shows N signers and you want to require some subset of them
  (e.g. M-of-N).
- The signer set is uniform — no per-signer weight needed.

Error codes: `3200..=3203`. See `error-codes.md` for the synthesizer-facing
remediations.

### 2. `weighted_threshold`

Install params: `{ signer_weights: Map<Signer, u32>, threshold: u32 }` —
per-signer weight plus a total-weight threshold. Accepts any
`ContextRuleType`.

Use it when:
- Some signers should count more than others (e.g. founder counts as 3 votes,
  contributors as 1).
- You'd otherwise need an explosion of `simple_threshold` rules per signer
  subset.

Error codes: `3210..=3214`.

### 3. `spending_limit`

Install params: `{ spending_limit: i128, period_ledgers: u32 }`. **Critically:
no `token` field.** The token contract address lives in the parent context rule
as `ContextRuleType::CallContract(<token_address>)`.

> Quoted from `docs/oz-internal-shapes.md` §4.1:
>
> > "The token is **implicit** in the `ContextRule.context_type` value. The
> > `install` function enforces this — see `spending_limit.rs:376-378`:
> >
> > ```rust
> > if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
> >     panic_with_error!(e, SpendingLimitError::OnlyCallContractAllowed)
> > }
> > ```
> >
> > So the token contract address lives inside
> > `ContextRuleType::CallContract(Address)`. The synthesizer must lift this
> > when emitting a `PolicySpec` — the policy's "token" comes from the parent
> > context rule's type, not from the install params."

`period_ledgers` is in **ledgers**, not seconds. Conversion:
`17280 ledgers ≈ 1 day` (Stellar ledgers close ~5s). So:

| Period intent | `period_ledgers` |
|---|---|
| 1 day        | 17_280            |
| 7 days       | 120_960           |
| 30 days      | 518_400           |
| 90 days      | 1_555_200         |

The plan's example walkthrough uses `lifetime_ledgers = 432_000` (~25 days)
matching the SEP-41 subscription frozen spec
(`walkthroughs/02-sep41-subscription/expected-spec-track-a.json`).

Error codes: `3220..=3227`.

---

## The SEP-41-transfer-only constraint of `spending_limit` (OZ PR-#649)

Quoted from `docs/oz-internal-shapes.md` §9:

> "PR #649 ('Smart account: spending limit policy', merged 2026-03-25, in
> v0.7.0-rc.2+) is the one that gives `spending_limit` its
> `OnlyCallContractAllowed` semantics.
>
> [The rejection path] rejects:
>
> - `ContextRuleType::Default` -> `OnlyCallContractAllowed (3227)`
> - `ContextRuleType::CreateContract(_)` -> `OnlyCallContractAllowed (3227)`
>
> Only `ContextRuleType::CallContract(Address)` is accepted. The
> installer/synthesizer must therefore ensure that any context rule carrying a
> `spending_limit` policy is built with `context_type =
> ContextRuleType::CallContract(token_address)` and never with `Default`."

**Practical implication for the skill.** If the user wants to cap "any
outbound spend up to N USDC monthly", you cannot express that with a single
`spending_limit` policy under a `Default` rule. Choices:

1. Scope to one token (`CallContract(<token>)`) — recommended.
2. Emit one context rule per token (multiple `add_context_rule` calls).
3. Fall back to a Track-B generated slot that bundles its own per-token logic.

The synthesizer surfaces this as `E_SYNTH_NOT_EXPRESSIBLE` with the message
"spending_limit requires a specific token contract; the observed flow touches
multiple tokens. Use a custom policy or split the rule per token."

---

## The signer-set divergence footgun

The smart account stores **two** signer sets:

1. The **account-level** signer set (the contract's own signers).
2. Each **context rule's** signer set (the subset that satisfies that rule).

When you add a context rule via `add_context_rule(context_type, policies,
signers, name)`, the `signers` you pass becomes that rule's own subset. The
on-chain code does **not** require the rule's signers to be a subset of the
account-level signers — but the agent-skill caller almost always wants them
aligned.

**Footgun:** if you pass an empty `signers: Vec<Signer>` (because the original
recording used `Credentials::SourceAccount` and no soroban-auth payload), the
synthesizer leaves the rule's signer set empty. The rule then only triggers
when policies alone authorise it — `simple_threshold` requires at least one
authenticated signer, so an empty signer set + `simple_threshold` rule is
**unreachable**.

The synthesizer guards against this: it refuses to emit a `simple_threshold`
slot when `signers.is_empty()` and surfaces `E_SYNTH_NOT_EXPRESSIBLE` with the
specific message. See `crates/oz-policy-core/src/decision_tree.rs`.

When in doubt, **echo the rule's signer set back to the user** before
exporting and confirm it's the set they expect.

---

## Hard limits

Quoted from `docs/oz-internal-shapes.md` §7
(`packages/accounts/src/smart_account/mod.rs:524-530`):

```rust
/// Maximum number of policies allowed per context rule.
pub const MAX_POLICIES: u32 = 5;
/// Maximum number of signers allowed per context rule.
pub const MAX_SIGNERS: u32 = 15;
/// Maximum length in bytes for a context rule name.
pub const MAX_NAME_SIZE: u32 = 20;
/// Maximum size in bytes for external signer key data.
pub const MAX_EXTERNAL_KEY_SIZE: u32 = 256;
```

Plus a per-policy storage cap:

```rust
pub const MAX_HISTORY_ENTRIES: u32 = 1000;
```

If the synthesizer's chosen composition would exceed any of these, it returns
`E_SYNTH_NOT_EXPRESSIBLE`. Common remediations:

- Too many policies → split into multiple context rules.
- Too many signers → switch to `weighted_threshold` (one rule, weighted set).
- Rule name too long → the synthesizer clamps on a UTF-8 boundary; pick a
  shorter name to avoid the clamp.

---

## Context rule types

Quoted from `docs/oz-internal-shapes.md` §6.2:

```rust
pub enum ContextRuleType {
    /// Default rules that can authorize any context.
    Default,
    /// Restricts the rule to a specific contract address.
    CallContract(Address),
    /// Restricts the rule to contract creation.
    CreateContract(/* …host_function_kind… */),
}
```

The synthesizer prefers `CallContract(<address>)` whenever exactly one
contract target is present in the recording (this is the least-privilege
posture). It falls back to `Default` only when the recording touches
*multiple* contracts and the user explicitly accepts the broader scope.

> **Skill behaviour (from SKILL.md step 4 / clarification trigger 4):** when
> the synthesizer would emit a `Default` rule, the skill **asks** the user
> whether to switch to `CallContract(<target>)` for safety. Don't ship a
> `Default` rule silently.
