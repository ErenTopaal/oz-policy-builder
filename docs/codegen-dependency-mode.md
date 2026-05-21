# Codegen Dependency Mode — Track-B Generated Policy Crates

> **Decision:** Generated Track-B policy `cdylib` crates **DEPEND DIRECTLY** on
> `stellar-accounts = "=0.7.1"` as a library and `use stellar_accounts::{ContextRule, Policy, Signer};`.
> They do **NOT** re-implement the trait pattern locally.
>
> **Verified:** 2026-05-15, against `stellar-accounts 0.7.1` published on crates.io.

---

## The question

Phase 3 of `plan.md` asks: does a generated Soroban policy `cdylib` crate
depend on `stellar-accounts` directly as a library (so it can
`use stellar_accounts::Policy;`), or must it re-implement the trait pattern
inline — the way pollywallet's `e2e-policy-test/src/lib.rs` did?

`research §5.2` describes pollywallet's approach as:

> "a standalone single-file Soroban Rust contract that re-implements
> `spending_limit` correctly **(not depending on `stellar-accounts` as a
> library, because OZ ships the policies as `lib` not `cdylib`)**"

If still true, generated contracts would need to vendor or duplicate the
`Policy` trait definition, the `ContextRule` / `Signer` / `AuthPayload`
structs, etc. That is brittle (every OZ point release would force a vendor
refresh) and audit-hostile (the duplicate would silently drift).

We need to verify whether this is still the case at the pinned `v0.7.1`.

---

## Verification

### 1. `cargo search stellar-accounts` — confirm published on crates.io

```text
$ cargo search stellar-accounts
stellar-accounts = "0.7.1"           # Smart Account Contracts and Utilities.
...
```

The crate exists at version `0.7.1` and is named `stellar-accounts`. Good.

### 2. Inspect the crate's `[lib]` section at the pinned tag

A shallow clone of `OpenZeppelin/stellar-contracts` at tag `v0.7.1`
(commit `3f81125bed3114cc93f5fca6d13240082050269a`, the same SHA verified by
`docs/oz-internal-shapes.md`) was used. Local path:
`/tmp/stellar-contracts-verify/`.

```text
$ grep -E '^(crate-type|cdylib|name|\[)' /tmp/stellar-contracts-verify/packages/accounts/Cargo.toml
[package]
name = "stellar-accounts"
[package.metadata.stellar]
[lib]
crate-type = ["lib", "cdylib"]
[dependencies]
[dev-dependencies]
```

`crate-type = ["lib", "cdylib"]` — the crate is published as **both** a Rust
library AND a Soroban `cdylib`. The pollywallet observation
("OZ ships the policies as `lib` not `cdylib`") was correct at an
earlier version but is **no longer accurate at v0.7.1**.

Implications:
- A downstream Soroban contract crate can write
  `stellar-accounts = "=0.7.1"` in its `Cargo.toml` and import items via
  `use stellar_accounts::{...};` — the `lib` half exposes the trait,
  the `cdylib` half is what OZ deploys; the two half-products coexist by
  design.
- The library exports (verified by reading
  `/tmp/stellar-contracts-verify/packages/accounts/src/lib.rs`):
  - `pub mod policies;` — contains `pub trait Policy { ... }`
    (`packages/accounts/src/policies/mod.rs:47-163`).
  - `pub mod smart_account;` — contains `ContextRule`, `ContextRuleType`,
    `Signer`, `AuthPayload`, `SmartAccountError`, etc.
  - `pub mod verifiers;` — Ed25519 / WebAuthn verifier helpers.

### 3. No `[no_std]` re-entry issue

`packages/accounts/src/lib.rs:7` declares `#![no_std]`. Generated policy
crates are also `#![no_std]` (Soroban requirement), so there is no std/no-std
mismatch.

---

## Decision

**Generated Track-B policy crates depend on `stellar-accounts = "=0.7.1"`
as a library.** They:

1. Write `stellar-accounts = "=0.7.1"` in the generated `Cargo.toml`'s
   `[dependencies]`.
2. Use `use stellar_accounts::{ContextRule, Policy, Signer};` at the top of
   the generated `src/lib.rs`.
3. `impl Policy for Policy { type AccountParams = InstallParams; ... }` —
   the trait's `enforce` / `install` / `uninstall` are implemented with the
   exact signature required by the canonical trait.

### Trade-offs of the chosen mode (link vs re-implement)

| Aspect | Link `stellar-accounts` (chosen) | Re-implement locally |
|---|---|---|
| Trait drift across OZ releases | Detected at compile time | Silent until runtime |
| Generated crate size | Smaller (~one extern crate) | Slightly larger (duplicated types) |
| WASM payload | Identical — `cdylib` linker dead-code-eliminates unused items either way | Identical |
| Audit surface | Templates emit a thin wrapper; audited boundary = stellar-accounts | Templates emit the full trait surface |
| `Cargo.lock` reproducibility | Pinned `=0.7.1` flows transitively | N/A |
| Soroban host compatibility | OZ ensures `stellar-accounts` works with `soroban-sdk =25.3.0` | Generated crate's hand-rolled types must independently track soroban-sdk |

The audit-surface argument is decisive: keeping the boundary at the
`stellar-accounts` crate means a future Phase 9 audit can lint
"`use stellar_accounts::Policy;` is the only trait import in any generated
crate" as a binary check. Re-implementation would require auditing each
generated trait shape — combinatorial in template count.

---

## What the templates emit

The base template (`templates/base.rs.jinja`) emits, at the top of every
generated policy contract:

```rust
#![no_std]
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype,
    panic_with_error, Address, Bytes, Env, Map, String, Symbol, Val, Vec,
};
use stellar_accounts::{ContextRule, Policy, Signer};
```

…followed by `#[contracttype]`-derived `InstallParams` / `StorageKey`,
`#[contracterror]` `PolicyError`, the three `#[contractevent]` types, and
`#[contractimpl] impl Policy for Policy { ... }`.

The generated `Cargo.toml` declares:

```toml
[dependencies]
soroban-sdk    = "=25.3.0"
stellar-accounts = "=0.7.1"

[lib]
crate-type = ["cdylib"]
```

(Workspace-resolution: when the generated crate is materialized into a
sandbox tempdir for Phase 3 Stream B's compile driver, it is built as a
standalone crate — not as a workspace member — so the pinned `=` versions
flow through `Cargo.lock` on a per-build basis.)

---

## Re-verification commands (record for Phase 9 audit)

```bash
# 1. Confirm crate still publishes both lib and cdylib at v0.7.1.
cargo search stellar-accounts | head -1
git clone --depth 1 --branch v0.7.1 \
    https://github.com/OpenZeppelin/stellar-contracts /tmp/stellar-contracts-verify
grep -E '^(crate-type|cdylib)' \
    /tmp/stellar-contracts-verify/packages/accounts/Cargo.toml
# Expect: crate-type = ["lib", "cdylib"]

# 2. Confirm Policy trait is pub-visible from the lib.rs surface.
grep -n 'pub trait Policy' \
    /tmp/stellar-contracts-verify/packages/accounts/src/policies/mod.rs
# Expect a hit at line 47.

# 3. Confirm pub mod re-exports.
grep -E '^pub mod' \
    /tmp/stellar-contracts-verify/packages/accounts/src/lib.rs
# Expect: pub mod policies; pub mod smart_account; pub mod verifiers;
```

If a future OZ release flips `crate-type` back to `["cdylib"]` alone, this
decision must be revisited — switch templates to the re-implementation mode.
The Phase 9 audit's reproducible-build script should run command 1 above
and fail loudly if the output changes.
