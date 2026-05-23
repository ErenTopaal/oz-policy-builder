//! Minimal OZ smart-account contract for the Phase 4 simulation harness.
//!
//! This is a derivative of the upstream
//! `OpenZeppelin/stellar-contracts@v0.7.1` example at
//! `examples/multisig-smart-account/account/src/contract.rs`, stripped of its
//! `Upgradeable` blanket impl (and the `stellar-contract-utils` dependency
//! that came with it). We keep:
//!
//!   * `SmartAccount` blanket impl from `stellar-accounts` (context-rule +
//!     signer + policy management);
//!   * `CustomAccountInterface::__check_auth` delegating to
//!     `stellar_accounts::smart_account::do_check_auth`;
//!   * `ExecutionEntryPoint` blanket impl so policy-driven inner invocations
//!     dispatch correctly.
//!
//! Source is committed in-tree so the vendored WASM under
//! `crates/oz-policy-simhost/vendor/` remains auditable against this file.
//! See `docs/simhost-smart-account-source.md` for the build steps and the
//! WASM SHA-256.

#![no_std]
#![allow(dead_code)]

use soroban_sdk::{
    auth::{Context, CustomAccountInterface},
    contract, contractimpl,
    crypto::Hash,
    Address, Env, Map, String, Symbol, Val, Vec,
};
use stellar_accounts::smart_account::{
    self, AuthPayload, ContextRule, ContextRuleType, ExecutionEntryPoint, Signer, SmartAccount,
    SmartAccountError,
};

#[contract]
pub struct MinimalSmartAccount;

#[contractimpl]
impl MinimalSmartAccount {
    /// Constructor — no-op. The simhost uses the post-deploy
    /// [`Self::init`] entry point so it can construct the host via
    /// `register_test_contract_wasm` (which invokes the constructor with
    /// empty args) and only afterwards seed the smart-account state
    /// with `(signers, policies)`. The OZ upstream example bundles these
    /// into `__constructor` because it expects the deployer to know the
    /// initial composition at create-time; we don't, so we defer.
    pub fn __constructor() {}

    /// Post-deploy seeding: installs a single `Default` context rule named
    /// `"rule"` with the provided signers + policies. Mirrors the upstream
    /// `__constructor` body. Requires `smart_account.require_auth()`
    /// internally via `add_context_rule`'s mutator gate.
    pub fn init(e: &Env, signers: Vec<Signer>, policies: Map<Address, Val>) {
        smart_account::add_context_rule(
            e,
            &ContextRuleType::Default,
            &String::from_str(e, "rule"),
            None,
            &signers,
            &policies,
        );
    }
}

#[contractimpl]
impl CustomAccountInterface for MinimalSmartAccount {
    type Error = SmartAccountError;
    type Signature = AuthPayload;

    fn __check_auth(
        e: Env,
        signature_payload: Hash<32>,
        signatures: AuthPayload,
        auth_contexts: Vec<Context>,
    ) -> Result<(), Self::Error> {
        smart_account::do_check_auth(&e, &signature_payload, &signatures, &auth_contexts)
    }
}

#[contractimpl(contracttrait)]
impl SmartAccount for MinimalSmartAccount {}

#[contractimpl(contracttrait)]
impl ExecutionEntryPoint for MinimalSmartAccount {}
