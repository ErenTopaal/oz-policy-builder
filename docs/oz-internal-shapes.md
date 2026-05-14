# OpenZeppelin `stellar-contracts` v0.7.1 — Internal Shapes Reference

> **Source pin:** `OpenZeppelin/stellar-contracts` tag `v0.7.1` (released 2026-04-10). Released artifact on crates.io: `stellar-accounts = 0.7.1`. Workspace license verified: **MIT** (NOT Apache-2.0 as the plan claimed — see "Discrepancies" at the bottom).
>
> All struct/enum/function definitions below are copy-pasted **verbatim** from source. Local clone path used for inspection: `/tmp/stellar-contracts-clone`.

---

## 1. The `Policy` trait

Source: `packages/accounts/src/policies/mod.rs:47-163`.

```rust
pub trait Policy {
    type AccountParams: FromVal<Env, Val>;

    fn enforce(
        e: &Env,
        context: Context,
        authenticated_signers: Vec<Signer>,
        context_rule: ContextRule,
        smart_account: Address,
    );

    fn install(
        e: &Env,
        install_params: Self::AccountParams,
        context_rule: ContextRule,
        smart_account: Address,
    );

    fn uninstall(e: &Env, context_rule: ContextRule, smart_account: Address);
}
```

Notes:
- The associated type is `AccountParams` (no trailing word).
- Concrete implementations (`simple_threshold`, `weighted_threshold`, `spending_limit`) do **not** name their install-param struct `AccountParams`. Each uses a long-form name (`SimpleThresholdAccountParams`, etc.) — see below.
- There is also an internal `PolicyClientInterface` (mod.rs:171-185) used by the `#[contractclient]` macro because traits with associated types are not supported by that macro. Callers always use the public trait.

---

## 2. `simple_threshold` — install params + errors + constants

Source: `packages/accounts/src/policies/simple_threshold.rs:96-129`.

```rust
/// Installation parameters for the simple threshold policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SimpleThresholdAccountParams {
    /// The minimum number of signers required for authorization.
    pub threshold: u32,
}

/// Error codes for simple threshold policy operations.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum SimpleThresholdError {
    /// The smart account does not have a simple threshold policy installed.
    SmartAccountNotInstalled = 3200,
    /// When threshold is 0 or exceeds the number of available signers.
    InvalidThreshold = 3201,
    /// The transaction is not allowed by this policy.
    NotAllowed = 3202,
    /// The context rule for the smart account has been already installed.
    AlreadyInstalled = 3203,
}

const DAY_IN_LEDGERS: u32 = 17280;
pub const SIMPLE_THRESHOLD_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const SIMPLE_THRESHOLD_TTL_THRESHOLD: u32 = SIMPLE_THRESHOLD_EXTEND_AMOUNT - DAY_IN_LEDGERS;
```

The `install()` function (simple_threshold.rs:278-302) validates:
- `threshold == 0` -> `InvalidThreshold`
- `threshold > context_rule.signers.len()` -> `InvalidThreshold`
- Already-installed -> `AlreadyInstalled`

There is **no** `OnlyCallContractAllowed` restriction on `simple_threshold`; it accepts any `ContextRuleType` (`Default`, `CallContract(_)`, `CreateContract(_)`).

---

## 3. `weighted_threshold` — install params + errors + constants

Source: `packages/accounts/src/policies/weighted_threshold.rs:125-165`.

```rust
/// Installation parameters for the weighted threshold policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct WeightedThresholdAccountParams {
    /// Mapping of signers to their respective weights.
    pub signer_weights: Map<Signer, u32>,
    /// The minimum total weight required for authorization.
    pub threshold: u32,
}

/// Error codes for weighted threshold policy operations.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum WeightedThresholdError {
    /// The smart account does not have a weighted threshold policy installed.
    SmartAccountNotInstalled = 3210,
    /// The threshold value is invalid.
    InvalidThreshold = 3211,
    /// A mathematical operation would overflow.
    MathOverflow = 3212,
    /// The transaction is not allowed by this policy.
    NotAllowed = 3213,
    /// The context rule for the smart account has been already installed.
    AlreadyInstalled = 3214,
}

const DAY_IN_LEDGERS: u32 = 17280;
pub const WEIGHTED_THRESHOLD_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const WEIGHTED_THRESHOLD_TTL_THRESHOLD: u32 = WEIGHTED_THRESHOLD_EXTEND_AMOUNT - DAY_IN_LEDGERS;
```

`install()` (weighted_threshold.rs:482-512) validates:
- `params.threshold == 0` -> `InvalidThreshold`
- `params.threshold > sum_of_signer_weights` -> `InvalidThreshold`
- Sum overflow -> `MathOverflow`
- Already-installed -> `AlreadyInstalled`

Like `simple_threshold`, weighted accepts any `ContextRuleType`.

---

## 4. `spending_limit` — install params + errors + constants

Source: `packages/accounts/src/policies/spending_limit.rs:85-158`.

```rust
/// Installation parameters for the spending limit policy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SpendingLimitAccountParams {
    /// The maximum amount that can be spent within the specified period (in
    /// stroops).
    pub spending_limit: i128,
    /// The period in ledgers over which the spending limit applies.
    pub period_ledgers: u32,
}

/// Internal storage structure for spending limit tracking.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SpendingLimitData {
    /// The spending limit for the period.
    pub spending_limit: i128,
    /// The period in ledgers over which the spending limit applies.
    pub period_ledgers: u32,
    /// History of spending transactions with their ledger sequences.
    pub spending_history: Vec<SpendingEntry>,
    /// Cached total of all amounts in spending_history.
    pub cached_total_spent: i128,
}

/// Individual spending entry for tracking purposes.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SpendingEntry {
    /// The amount spent in this transaction.
    pub amount: i128,
    /// The ledger sequence when this transaction occurred.
    pub ledger_sequence: u32,
}

/// Error codes for spending limit policy operations.
#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum SpendingLimitError {
    /// The smart account does not have a spending limit policy installed.
    SmartAccountNotInstalled = 3220,
    /// The spending limit has been exceeded.
    SpendingLimitExceeded = 3221,
    /// The spending limit or period is invalid.
    InvalidLimitOrPeriod = 3222,
    /// The transaction is not allowed by this policy.
    NotAllowed = 3223,
    /// The spending history has reached maximum capacity.
    HistoryCapacityExceeded = 3224,
    /// The context rule for the smart account has been already installed.
    AlreadyInstalled = 3225,
    /// The transfer amount is negative.
    LessThanZero = 3226,
    /// Only the `CallContract` context rule type is allowed.
    OnlyCallContractAllowed = 3227,
}

const DAY_IN_LEDGERS: u32 = 17280;
pub const SPENDING_LIMIT_EXTEND_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
pub const SPENDING_LIMIT_TTL_THRESHOLD: u32 = SPENDING_LIMIT_EXTEND_AMOUNT - DAY_IN_LEDGERS;

/// Maximum number of spending entries to keep in history.
/// This prevents storage DoS by capping the vector size.
pub const MAX_HISTORY_ENTRIES: u32 = 1000;
```

### 4.1 Does `SpendingLimitAccountParams` include a `token: Address` field?

**NO.** The struct has exactly two fields: `spending_limit: i128` and `period_ledgers: u32`.

The token is **implicit** in the `ContextRule.context_type` value. The `install` function enforces this — see `spending_limit.rs:376-378`:

```rust
if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
    panic_with_error!(e, SpendingLimitError::OnlyCallContractAllowed)
}
```

So the token contract address lives inside `ContextRuleType::CallContract(Address)`. The synthesizer must lift this when emitting a `PolicySpec` — the policy's "token" comes from the parent context rule's type, not from the install params.

### 4.2 Period unit (ledgers vs seconds)

The unit is **ledgers** (Soroban ledger sequence number), not seconds.

- Field type: `period_ledgers: u32`
- Doc comment (spending_limit.rs:92-93): *"The period in ledgers over which the spending limit applies."*
- The rolling-window eviction logic (spending_limit.rs:460-481) uses `e.ledger().sequence()` and `entry.ledger_sequence: u32` — both ledger sequences, never timestamps.
- `cleanup_old_entries` computes the cutoff as `current_ledger.saturating_sub(period_ledgers)` and evicts entries with `ledger_sequence <= cutoff_ledger`.

Conversion note for synthesizer UX (informational, not in source): on Stellar, ledgers close roughly every 5 seconds, so `17280 ledgers ≈ 1 day`. The constant `DAY_IN_LEDGERS = 17280` appears in all three policy files as a sanity reference.

---

## 5. `#[contracterror]` variants — full list

Used later to map to `E_SYNTH_NOT_EXPRESSIBLE` and friends in the synthesizer.

### `SimpleThresholdError` (simple_threshold.rs:104-117)
| Variant | Code |
|---|---|
| `SmartAccountNotInstalled` | 3200 |
| `InvalidThreshold` | 3201 |
| `NotAllowed` | 3202 |
| `AlreadyInstalled` | 3203 |

### `WeightedThresholdError` (weighted_threshold.rs:135-150)
| Variant | Code |
|---|---|
| `SmartAccountNotInstalled` | 3210 |
| `InvalidThreshold` | 3211 |
| `MathOverflow` | 3212 |
| `NotAllowed` | 3213 |
| `AlreadyInstalled` | 3214 |

### `SpendingLimitError` (spending_limit.rs:120-141)
| Variant | Code |
|---|---|
| `SmartAccountNotInstalled` | 3220 |
| `SpendingLimitExceeded` | 3221 |
| `InvalidLimitOrPeriod` | 3222 |
| `NotAllowed` | 3223 |
| `HistoryCapacityExceeded` | 3224 |
| `AlreadyInstalled` | 3225 |
| `LessThanZero` | 3226 |
| `OnlyCallContractAllowed` | 3227 |

### `SmartAccountError` (smart_account/mod.rs:538-572)
| Variant | Code |
|---|---|
| `ContextRuleNotFound` | 3000 |
| `UnvalidatedContext` | 3002 |
| `ExternalVerificationFailed` | 3003 |
| `NoSignersAndPolicies` | 3004 |
| `PastValidUntil` | 3005 |
| `SignerNotFound` | 3006 |
| `DuplicateSigner` | 3007 |
| `PolicyNotFound` | 3008 |
| `DuplicatePolicy` | 3009 |
| `TooManySigners` | 3010 |
| `TooManyPolicies` | 3011 |
| `MathOverflow` | 3012 |
| `KeyDataTooLarge` | 3013 |
| `ContextRuleIdsLengthMismatch` | 3014 |
| `NameTooLong` | 3015 |
| `UnauthorizedSigner` | 3016 |

Note: code `3001` is unused/reserved in v0.7.1.

---

## 6. `SmartAccount` trait — full surface

Source: `packages/accounts/src/smart_account/mod.rs:136-477`.

### 6.1 Read queries

```rust
fn get_context_rules_count(e: &Env) -> u32;

fn get_context_rule(e: &Env, context_rule_id: u32) -> ContextRule;

fn get_signer_id(e: &Env, signer: Signer) -> u32;

fn get_policy_id(e: &Env, policy: Address) -> u32;
```

### 6.2 Mutators (each begins with `e.current_contract_address().require_auth()`)

```rust
fn add_context_rule(
    e: &Env,
    context_type: ContextRuleType,
    name: String,
    valid_until: Option<u32>,
    signers: Vec<Signer>,
    policies: Map<Address, Val>,
) -> ContextRule;

fn update_context_rule_name(e: &Env, context_rule_id: u32, name: String) -> ContextRule;

fn update_context_rule_valid_until(
    e: &Env,
    context_rule_id: u32,
    valid_until: Option<u32>,
) -> ContextRule;

fn remove_context_rule(e: &Env, context_rule_id: u32);

fn add_signer(e: &Env, context_rule_id: u32, signer: Signer) -> u32;

fn remove_signer(e: &Env, context_rule_id: u32, signer_id: u32);

fn add_policy(e: &Env, context_rule_id: u32, policy: Address, install_param: Val) -> u32;

fn remove_policy(e: &Env, context_rule_id: u32, policy_id: u32);
```

Notes for the installer (Phase 2):
- `add_context_rule` takes `policies: Map<Address, Val>`. The `Val` is the install param for each policy — the installer must encode the appropriate `*AccountParams` struct via `IntoVal`/`FromVal`.
- All mutators call `e.current_contract_address().require_auth()` first, so the install envelope must include a `require_auth` clause for the smart account itself.
- The trait extends `CustomAccountInterface` (the Soroban-defined `__check_auth` entrypoint).

---

## 7. Limit constants

Source: `packages/accounts/src/smart_account/mod.rs:524-530`.

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

- `MAX_POLICIES = 5` — `packages/accounts/src/smart_account/mod.rs:524`
- `MAX_SIGNERS = 15` — `packages/accounts/src/smart_account/mod.rs:526`
- `MAX_NAME_SIZE = 20` — `packages/accounts/src/smart_account/mod.rs:528` (in bytes — UTF-8 byte length, not character count)
- `MAX_EXTERNAL_KEY_SIZE = 256` — `packages/accounts/src/smart_account/mod.rs:530` (bytes)

Plus a per-policy storage cap:
- `MAX_HISTORY_ENTRIES = 1000` — `packages/accounts/src/policies/spending_limit.rs:158` (spending-history vector cap)

---

## 8. On-chain marker for pre/post-#655 smart accounts

**Status: NO CLEAN MARKER EXISTS IN SOURCE.**

Searched `packages/accounts/src/`, `examples/multisig-smart-account/`, and the entire workspace for `contractmeta`, `CONTRACT_META`, embedded version constants, or any sentinel that would let the installer's preflight distinguish a smart-account contract built before PR #655 (the new auth-digest scheme) from one built after.

Findings:
- No `contractmeta!` macro usage anywhere in `packages/` or in the `examples/multisig-smart-account/` example account.
- No public `VERSION` or `version()` accessor on the `SmartAccount` trait.
- The trait does not surface its own protocol revision; behavior change is entirely internal to `do_check_auth`'s digest computation (storage.rs:493-495).
- `SmartAccountError` does not include a "wrong signature scheme" variant — a pre-#655 client sending a raw `signature_payload`-keyed signature to a post-#655 account simply fails as `ExternalVerificationFailed` (3003).

**Implication for Phase 2 installer preflight:** the planned "is this account post-#655?" check cannot be answered by introspecting the deployed contract. Available fallback strategies:

1. **Bytecode-hash whitelist.** Compute the WASM hash of OZ-released smart-account example builds at tags `>= v0.7.0-rc.2` and require the deployed account's `LedgerKeyContractCode.hash` to match one of those hashes. This is brittle to custom forks but correct for OZ-stock accounts.

   **PR #655 release-trail evidence (verified 2026-05-15):**
   - PR URL: https://github.com/OpenZeppelin/stellar-contracts/pull/655 ("Smart account: new sign digest")
   - Merged: `2026-03-26T13:09:07Z` (per `gh pr view 655 --json mergedAt`)
   - Merge commit SHA: `5958551051a0bba1a007c8dbb44f35fd547edf0f` (per `gh pr view 655 --json mergeCommit`)
   - First tag containing the merge commit: `v0.7.0-rc.2` (released `2026-03-26T15:00:50Z`, same day as merge) — verified via `git tag --contains 5958551051a0bba1a007c8dbb44f35fd547edf0f`, which returns exactly `v0.7.0-rc.2`, `v0.7.0`, `v0.7.1`.
   - First *stable* tag containing the merge commit: `v0.7.0` (released `2026-04-03T13:19:45Z`).
   - Tag-list verification: `gh release list -R OpenZeppelin/stellar-contracts` confirms `v0.7.0-rc.2` is the next tag after the merge commit; no intermediate tag exists between `v0.7.0-rc.1` (2026-03-02, pre-merge) and `v0.7.0-rc.2`.
2. **Behavioral probe.** Submit a simulated `__check_auth` with a known-good post-#655-format signature against a `Default` rule; if it succeeds, the account is post-#655. This costs one simulate call and surfaces the new-digest behavior directly.
3. **Document the limitation** and require the *user* to assert (via an `--account-revision=post-655` flag or wallet-metadata) that their deployed account is current. The installer rejects without an assertion.

**Recommendation:** option 3 for v1, with option 1 added in v1.1 once we have a curated whitelist of audited WASM hashes. Option 2 is correct but expensive; defer.

This is a real defect to flag upstream — file an issue suggesting OZ add `pub const SMART_ACCOUNT_AUTH_DIGEST_REV: u8 = 1;` in `smart_account/mod.rs` and surface it via the trait so installers can introspect without WASM-hash lookups.

---

## 9. PR #649 mechanism: `install` rejection of `Default` for `spending_limit`

PR #649 ("Smart account: spending limit policy", merged 2026-03-25, in v0.7.0-rc.2+) is the one that gives `spending_limit` its `OnlyCallContractAllowed` semantics.

The rejection path is in `packages/accounts/src/policies/spending_limit.rs:376-378`:

```rust
if !matches!(context_rule.context_type, ContextRuleType::CallContract(_)) {
    panic_with_error!(e, SpendingLimitError::OnlyCallContractAllowed)
}
```

This is the first check inside `install()` after the `smart_account.require_auth()` call. It rejects:
- `ContextRuleType::Default` -> `OnlyCallContractAllowed (3227)`
- `ContextRuleType::CreateContract(_)` -> `OnlyCallContractAllowed (3227)`

Only `ContextRuleType::CallContract(Address)` is accepted. The installer/synthesizer must therefore ensure that any context rule carrying a `spending_limit` policy is built with `context_type = ContextRuleType::CallContract(token_address)` and never with `Default`.

This validates the architecture's design intent: the synthesizer cannot legally emit a "spending limit applies to anything" policy. If the user's source transaction can only be expressed by such a policy, the synthesizer must emit `E_SYNTH_NOT_EXPRESSIBLE` and surface this as "spending_limit requires a specific token contract; the observed flow touches multiple tokens. Use a custom policy or split the rule per token."

---

## 10. `AuthPayload` shape

Source: `packages/accounts/src/smart_account/storage.rs:131-138`.

```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AuthPayload {
    /// Signature data mapped to each signer.
    pub signers: Map<Signer, Bytes>,
    /// Per-context rule IDs, aligned by index with `auth_contexts`.
    pub context_rule_ids: Vec<u32>,
}
```

The plan's research-§5 description matches the source verbatim.

Related types referenced in `AuthPayload`:

### `Signer` (storage.rs:93-102)
```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Signer {
    /// A delegated signer that uses built-in signature verification.
    Delegated(Address),
    /// An external signer with custom verification logic.
    /// Contains the verifier contract address and the public key data.
    External(Address, Bytes),
}
```

### `ContextRuleType` (storage.rs:140-150)
```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextRuleType {
    /// Default rules that can authorize any context.
    Default,
    /// Rules specific to calling a particular contract.
    CallContract(Address),
    /// Rules specific to creating a contract with a particular WASM hash.
    CreateContract(BytesN<32>),
}
```

### `ContextRule` (storage.rs:153-174)
```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ContextRule {
    /// Unique identifier for the context rule.
    pub id: u32,
    /// The type of context this rule applies to.
    pub context_type: ContextRuleType,
    /// Human-readable name for the context rule.
    pub name: String,
    /// List of signers authorized by this rule.
    pub signers: Vec<Signer>,
    /// Global registry IDs for each signer, positionally aligned with
    /// `signers`.
    pub signer_ids: Vec<u32>,
    /// List of policy contracts that must be satisfied.
    pub policies: Vec<Address>,
    /// Global registry IDs for each policy, positionally aligned with
    /// `policies`.
    pub policy_ids: Vec<u32>,
    /// Optional expiration ledger sequence for the rule.
    pub valid_until: Option<u32>,
}
```

### PR #655 auth digest computation

Per the doc comment on `AuthPayload` (storage.rs:126-130):

> *"`context_rule_ids` are bound into the digest that signers authenticate against: `auth_digest = sha256(signature_payload || context_rule_ids.to_xdr())`. Signers must sign `auth_digest`, not the raw `signature_payload` from the host. This prevents rule-selection downgrade attacks."*

Verified in source at storage.rs:493-495:

```rust
let mut preimage = signature_payload.to_bytes().to_bytes();
preimage.append(&signatures.context_rule_ids.clone().to_xdr(e));
let auth_digest = e.crypto().sha256(&preimage);
```

This is the post-#655 behavior. Wallet adapters (Phase 7) must compute this digest client-side before signing.

---

## Reproducible-build prereqs

The CI matrix pins `stellar-cli = v25.1.0` (matching the local install). That CLI binary embeds the Rust crate `wasm-opt = 0.116.1` (verified at `https://github.com/stellar/stellar-cli/blob/v25.1.0/cmd/soroban-cli/Cargo.toml`), which wraps upstream **Binaryen version 116**.

System-installed `wasm-opt` (Binaryen 125 from Homebrew, in our local dev environment) is NOT used by `stellar contract build --optimize` — the CLI uses its embedded copy via the `additional-libs` feature. This is the intended behavior for reproducibility: the optimizer version is locked to the CLI version, not to the host.

If any CI step bypasses `stellar contract` and shells out to system `wasm-opt`, builds will diverge. Phase 3 must run optimization exclusively through `stellar contract build --optimize` or `stellar contract optimize` (deprecated alias).

---

## Discrepancies between `plan.md` claims and v0.7.1 source

1. **License of `OpenZeppelin/stellar-contracts`.** `plan.md` line 37 says *"ensure Apache-2.0 license still applies"* for OZ-derived deps, and line 62 declares Apache-2.0 across all repos. Reality: `stellar-contracts` is **MIT** (`/tmp/stellar-contracts-clone/LICENSE` line 1, and `[workspace.package] license = "MIT"`). `stellar-accounts 0.7.1` on crates.io is published as MIT. The plan's claim that "this matches OpenZeppelin `stellar-contracts`" is incorrect; OZ uses MIT for this repo. Our toolkit can stay Apache-2.0 because Apache-2.0 + MIT is the standard Rust dual-license pattern and MIT-licensed deps are downstream-compatible with Apache-2.0 distribution, but the plan's note needs amendment to acknowledge this.

2. **Policy install-param struct names.** The plan and research consistently refer to `simple_threshold::AccountParams`. The actual public type names are `SimpleThresholdAccountParams`, `WeightedThresholdAccountParams`, and `SpendingLimitAccountParams`. The trait's associated type is named `AccountParams`, but the impl types are not. Codegen and the synthesizer's import paths must use the long-form names.

3. **`stellar-accounts` version discrepancy.** Plan line 29 says to reconcile crates.io `=0.7.1` vs GitHub `v0.7.0-rc.1`. Reality: both `v0.7.1` (GitHub tag, 2026-04-10) and `stellar-accounts = 0.7.1` (crates.io publish) exist and align. No amendment to the pin is needed — `0.7.1` is correct on both sides.

4. **Spending limit `token` field.** Research §5 / §13 may imply (or be ambiguous about) a `token` field on the spending-limit install params. Source confirms: there is no `token: Address` on `SpendingLimitAccountParams`. The token is exclusively carried by the parent `ContextRule.context_type = CallContract(Address)`. The synthesizer must lift it from there and reject `Default`-scoped spending limits at synthesis time.

5. **Pre/post-#655 marker.** Research and plan assume a clean on-chain marker exists or can be constructed. None exists in source. See section 8 for available fallbacks.

6. **Era of policies' rejection codes.** All policy error codes are in the range 3200-3227; smart-account errors are 3000-3016. The `E_SYNTH_NOT_EXPRESSIBLE` mapping table (Phase 2) should reserve these as user-facing reasons but should not duplicate them as numeric codes (avoid namespace collisions).

---

All line numbers above verified against `git show v0.7.1:packages/accounts/...` on 2026-05-15. Verification method: shallow clone of `OpenZeppelin/stellar-contracts` at tag `v0.7.1` (commit `3f81125bed3114cc93f5fca6d13240082050269a`), then `grep -n <unique_anchor_phrase>` for every `file:line` reference in §1–§10. Every line ref resolves to its cited symbol at the pinned tag.

When refreshing this doc against a newer tag, regenerate every `file:line` reference with grep against the pinned tag and update this date.
