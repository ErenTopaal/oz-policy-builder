//! Phase 4 Stream A — `soroban-env-host` driver wrapper.
//!
//! Wraps `soroban_env_host::Host` so the higher-level permit / deny / run
//! orchestrators (this crate's `permit.rs` and `run.rs`) can stay XDR-free.
//! Concretely [`TestHost`] exposes:
//!
//! * [`TestHost::new`] — construct a metered host with a deterministic ledger
//!   sequence + the canonical Phase-4 PRNG seed.
//! * [`TestHost::install_smart_account`] — register the vendored OZ smart
//!   account WASM (see `docs/simhost-smart-account-source.md`), return the
//!   SA's StrKey `C…` address.
//! * [`TestHost::install_policy`] — register a generated policy WASM,
//!   record its `(address, context_rule_id)` binding, return the policy's
//!   StrKey address.
//! * [`TestHost::invoke_check_auth`] — drive each `TestContext` through the
//!   installed policy's `enforce` entrypoint, surfacing the underlying host
//!   error code (major bits of the soroban `Error` val) on contract panic.
//!
//! ## Why not the full `__check_auth → add_policy → enforce` chain?
//!
//! The end-to-end SmartAccount entry point requires the host to be in
//! enforcing-auth mode with realistic signed credentials so the SA's
//! `do_check_auth` can verify each signer. That signing is wallet
//! responsibility (Phase 7); replicating it in-process is out of scope for
//! Phase 4 Stream A. We therefore implement `invoke_check_auth` as a
//! per-context dispatch that loops over each `TestContext` and invokes the
//! installed policy's `enforce` directly under recording-auth mode, which
//! is the same observable surface the harness needs to verify
//! permit/deny outcomes. The smart-account WASM is still installed because
//! it pins the on-chain address shape (`stellar-strkey C…`) and reserves
//! storage so later rounds can plumb the auth chain without reshaping the
//! `TestHost` API.
//!
//! ## Cross-stream interface contract
//!
//! [`AuthPayload`] and [`TestContext`] are the shapes Stream B's
//! deny-vector generator (`crate::deny`) imports. The two type bodies are
//! mirrored verbatim in the Phase-4 Stream-B task brief; do not change the
//! field names / shapes here without coordinating with the deny module.

use serde::{Deserialize, Serialize};
use soroban_env_host::xdr::{
    self, AccountId, ContractId, PublicKey, ScAddress, ScErrorType, ScMap, ScMapEntry, ScSymbol,
    ScVal, ScVec, Uint256, VecM,
};
use soroban_env_host::{
    AddressObject, Host, HostError, LedgerInfo, Symbol, TryFromVal, TryIntoVal, Val, VecObject,
};
use thiserror::Error;

use oz_policy_core::ArgValue;

// The `Env` trait carries the macro-generated `call(addr, sym, args)`
// method we use to drive contract calls. We import the trait as a name in
// scope so dot-syntax dispatch resolves cleanly.
use soroban_env_host::Env as _SorobanEnv;

/// SHA-256 of the vendored OZ smart-account WASM at
/// `crates/oz-policy-simhost/vendor/oz-minimal-smart-account-v0.7.1.wasm`.
///
/// Cross-checked by [`TestHost::install_smart_account`] at load time so the
/// simhost fails loudly if the on-disk WASM has drifted from the source
/// committed at `vendor-src/minimal-smart-account/src/lib.rs`. Update both
/// in sync (see `docs/simhost-smart-account-source.md`).
pub const VENDORED_SMART_ACCOUNT_WASM_SHA256: &str =
    "4b855eb5d4be538753d6b99fe570b5b25b8e064123229dc899edf050788d4a7a";

/// Raw bytes of the vendored OZ smart-account WASM, embedded into the
/// crate at build time. The companion constant
/// [`VENDORED_SMART_ACCOUNT_WASM_SHA256`] is verified against the actual
/// hash of these bytes in [`TestHost::verify_vendored_smart_account_wasm`].
pub const VENDORED_SMART_ACCOUNT_WASM: &[u8] =
    include_bytes!("../vendor/oz-minimal-smart-account-v0.7.1.wasm");

/// Canonical PRNG seed for the Phase-4 simulation harness. Fixed so the
/// host's nonce + address generation is deterministic across runs.
pub const SIMHOST_PRNG_SEED: [u8; 32] = *b"oz-policy-simhost-phase-4-seed!!";

fn sha256_bytes(input: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input);
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Build the default ledger-info value used by [`TestHost::new`]. The
/// `network_id` is the SHA-256 of the supplied network passphrase, matching
/// Stellar's network-id convention.
fn default_ledger_info(sequence_number: u32, network_passphrase: &str) -> LedgerInfo {
    LedgerInfo {
        protocol_version: Host::current_test_protocol(),
        sequence_number,
        // Deterministic timestamp anchored to the ledger sequence number.
        // Time-window primitive tests can stamp their own value via
        // `TestHost::set_ledger_seq` if needed.
        timestamp: 1_700_000_000u64.saturating_add(u64::from(sequence_number) * 5),
        network_id: sha256_bytes(network_passphrase.as_bytes()),
        base_reserve: 0,
        min_persistent_entry_ttl: 4096,
        min_temp_entry_ttl: 16,
        max_entry_ttl: 6_312_000,
    }
}

// -------------------------------------------------------------------------
// Cross-stream interface contract — DO NOT change field shapes without
// coordinating with `src/deny.rs` (Stream B).
// -------------------------------------------------------------------------

/// Auth payload handed to `__check_auth`. Mirrors the on-chain
/// `AuthPayload` (see `docs/oz-internal-shapes.md` §10) but uses
/// StrKey-encoded signer addresses + a typed `context_rule_ids` slice so
/// the type is wire-portable above the `soroban-env-host` boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPayload {
    /// StrKey `G…` / `C…` addresses for each signer that participates in
    /// this authorization. Order is preserved to mirror the recording's
    /// signer composition.
    pub signer_addresses: Vec<String>,
    /// Per-context-rule IDs, aligned by index with the `contexts` vector
    /// passed alongside this payload into `invoke_check_auth`.
    pub context_rule_ids: Vec<u32>,
}

/// One decoded `Context::Contract { contract, fn_name, args }` invocation
/// presented to `__check_auth`. The wrapper translates each `TestContext`
/// into a host-side `ScVal` shape during [`TestHost::invoke_check_auth`].
///
/// `Eq` is derivable because [`ArgValue`] is now `Eq` (Phase 4 Stream-A
/// extension; safe — no float variants exist in the `ScVal` shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestContext {
    /// Target contract StrKey `C…` address.
    pub contract_address: String,
    /// Soroban function symbol (UTF-8, ≤ 32 ASCII chars).
    pub function_name: String,
    /// Decoded call arguments.
    pub args: Vec<ArgValue>,
}

// -------------------------------------------------------------------------
// Errors
// -------------------------------------------------------------------------

/// Errors surfaced by [`TestHost::invoke_check_auth`] and friends.
///
/// `PolicyPanic` is the discriminating case the run orchestrator inspects:
/// a deny vector "passes" if and only if `invoke_check_auth` returns
/// `PolicyPanic(expected_error_code)`. Anything else (no panic, panic with
/// the wrong code, or `HostInternal`) is a failure mode for the deny test.
#[derive(Debug, Error)]
pub enum HostExecError {
    /// The on-host contract path panicked via `panic_with_error!`. The
    /// `u32` is the contract-defined error discriminant (e.g.
    /// `PolicyError::FunctionNotAllowed = 1010`).
    #[error("policy panicked with code {0}")]
    PolicyPanic(u32),

    /// The host itself reported an error (budget exceeded, decode failure,
    /// missing storage entry, etc.) that is *not* a contract-emitted
    /// `panic_with_error!`. We keep the diagnostic intact.
    #[error("host error: {0}")]
    HostInternal(String),

    /// A pre-call setup step failed (couldn't encode an argument, the
    /// configured smart-account address is wrong, etc.).
    #[error("setup failed: {0}")]
    SetupFailed(String),
}

impl HostExecError {
    /// Classify a raw [`HostError`] from the env-host into one of our
    /// variants. Contract-type errors (`ScErrorType::Contract`) become
    /// `PolicyPanic(code)`; everything else becomes `HostInternal` with
    /// the original `HostError`'s `Debug` rendering preserved.
    pub fn from_host_error(err: HostError) -> Self {
        if err.error.is_type(ScErrorType::Contract) {
            Self::PolicyPanic(err.error.get_code())
        } else {
            Self::HostInternal(format!("{err:?}"))
        }
    }
}

impl From<HostExecError> for oz_policy_core::Error {
    fn from(e: HostExecError) -> Self {
        // For deny / permit pathways the run orchestrator interprets these
        // results structurally — but when a HostExecError escapes into the
        // canonical error enum (e.g., a TestHost setup failure during
        // run_full_suite) we want it tagged as `E_SIM_PERMIT_DENIED` since
        // it most closely maps to "the harness could not complete a permit
        // evaluation".
        oz_policy_core::Error::SimPermitDenied(e.to_string())
    }
}

/// Internal setup error used by [`TestHost::new`] / `install_*` paths so
/// the public methods return `oz_policy_core::Error` without exposing
/// raw `HostError` shapes in the API.
#[derive(Debug, Error)]
#[error("simhost setup failed: {0}")]
pub struct SetupError(pub String);

impl From<SetupError> for oz_policy_core::Error {
    fn from(e: SetupError) -> Self {
        oz_policy_core::Error::SimPermitDenied(e.to_string())
    }
}

impl From<HostError> for SetupError {
    fn from(e: HostError) -> Self {
        Self(format!("{e:?}"))
    }
}

// -------------------------------------------------------------------------
// TestHost
// -------------------------------------------------------------------------

/// In-process simulation host. One instance per `run_full_suite` invocation;
/// short-lived and not `Send`/`Sync` (the underlying `soroban_env_host::Host`
/// uses interior mutability).
pub struct TestHost {
    host: Host,
    smart_account: Option<String>,
    installed_policies: Vec<InstalledPolicy>,
    network_passphrase: String,
    initial_ledger_seq: u32,
}

/// Internal bookkeeping for an installed policy slot.
#[derive(Debug, Clone)]
struct InstalledPolicy {
    address: String,
    context_rule_id: u32,
}

impl TestHost {
    /// Construct a fresh in-memory host with a deterministic PRNG seed,
    /// the given ledger sequence + network passphrase, and the canonical
    /// budget profile. Storage starts empty.
    pub fn new(ledger_seq: u32, network_passphrase: &str) -> Result<Self, oz_policy_core::Error> {
        // `test_host_with_recording_footprint` initialises an empty
        // recording footprint so `register_test_contract_wasm` can grow
        // storage without tripping `ExceededLimit`. We then layer on:
        //   * recording-auth mode so signed `Address` credentials aren't
        //     needed for the install + per-context enforce calls;
        //   * an unlimited budget so the WASM upload + contract call don't
        //     run out of CPU/mem mid-test (the simhost is a correctness
        //     verifier, not a metering benchmark).
        let host = Host::test_host_with_recording_footprint();
        host.set_ledger_info(default_ledger_info(ledger_seq, network_passphrase))
            .map_err(|e| SetupError(format!("set_ledger_info: {e:?}")))?;
        host.set_base_prng_seed(SIMHOST_PRNG_SEED)
            .map_err(|e| SetupError(format!("set_base_prng_seed: {e:?}")))?;
        host.with_budget(|budget| budget.reset_unlimited())
            .map_err(|e| SetupError(format!("reset_unlimited: {e:?}")))?;
        host.switch_to_recording_auth(true)
            .map_err(|e| SetupError(format!("switch_to_recording_auth: {e:?}")))?;
        Ok(Self {
            host,
            smart_account: None,
            installed_policies: Vec::new(),
            network_passphrase: network_passphrase.to_string(),
            initial_ledger_seq: ledger_seq,
        })
    }

    /// Borrow the underlying env-host — exposed so advanced callers /
    /// integration tests can read host state directly. Most consumers
    /// should use the high-level methods on this type instead.
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// StrKey `C…` of the smart account installed via
    /// [`Self::install_smart_account`]. `None` if not installed yet.
    pub fn smart_account_address(&self) -> Option<&str> {
        self.smart_account.as_deref()
    }

    /// Network passphrase the host was constructed with.
    pub fn network_passphrase(&self) -> &str {
        &self.network_passphrase
    }

    /// Initial ledger sequence number — used by the run orchestrator to
    /// stamp `SimReport.timestamp_ledger`.
    pub fn initial_ledger_seq(&self) -> u32 {
        self.initial_ledger_seq
    }

    /// Verify the embedded WASM bytes match the committed SHA-256.
    pub fn verify_vendored_smart_account_wasm() -> Result<(), oz_policy_core::Error> {
        let actual = sha256_bytes(VENDORED_SMART_ACCOUNT_WASM);
        let actual_hex = hex32(&actual);
        if actual_hex != VENDORED_SMART_ACCOUNT_WASM_SHA256 {
            return Err(oz_policy_core::Error::VerifyDrift(format!(
                "vendored smart-account WASM hash drifted: expected {VENDORED_SMART_ACCOUNT_WASM_SHA256}, got {actual_hex}",
            )));
        }
        Ok(())
    }

    /// Register the vendored OZ smart-account WASM with the host. Returns
    /// the StrKey `C…` of the deployed contract.
    ///
    /// `owner_signer_pubkey_hex` is reserved for forward compatibility
    /// with future external-signer scenarios that need a real Ed25519
    /// key; the present minimal smart-account constructor is invoked with
    /// empty args by `register_test_contract_wasm`. The address is the
    /// stable anchor for `install_policy` + `invoke_check_auth`.
    pub fn install_smart_account(
        &mut self,
        owner_signer_pubkey_hex: &str,
    ) -> Result<String, oz_policy_core::Error> {
        Self::verify_vendored_smart_account_wasm()?;
        let _ = owner_signer_pubkey_hex;

        let sa_addr_obj = self
            .host
            .register_test_contract_wasm(VENDORED_SMART_ACCOUNT_WASM);
        let sa_scaddr = self
            .host
            .scaddress_from_address(sa_addr_obj)
            .map_err(|e| SetupError(format!("scaddress_from_address: {e:?}")))?;
        let sa_strkey = scaddress_to_strkey(&sa_scaddr)
            .map_err(|e| SetupError(format!("scaddress_to_strkey: {e}")))?;
        self.smart_account = Some(sa_strkey.clone());
        Ok(sa_strkey)
    }

    /// Register a generated policy WASM. Returns the StrKey `C…` of the
    /// deployed policy contract and records the `(address, rule_id)`
    /// binding for later `invoke_check_auth` dispatch.
    ///
    /// `install_params` is currently retained for forward compatibility;
    /// the policy's `install` entry point is invoked indirectly through
    /// per-context `enforce` calls (which seed the `Installed(addr,id)`
    /// storage flag if it's the first call — TODO Phase-4 Round-2).
    pub fn install_policy(
        &mut self,
        wasm: &[u8],
        smart_account_addr: &str,
        context_rule_id: u32,
        install_params: ArgValue,
    ) -> Result<String, oz_policy_core::Error> {
        let sa = self.smart_account.as_deref().ok_or_else(|| {
            SetupError("install_policy called before install_smart_account".into())
        })?;
        if sa != smart_account_addr {
            return Err(SetupError(format!(
                "install_policy SA mismatch: registered {sa}, requested {smart_account_addr}",
            ))
            .into());
        }

        let policy_addr_obj = self.host.register_test_contract_wasm(wasm);
        let policy_scaddr = self
            .host
            .scaddress_from_address(policy_addr_obj)
            .map_err(|e| SetupError(format!("scaddress_from_address: {e:?}")))?;
        let policy_strkey = scaddress_to_strkey(&policy_scaddr)
            .map_err(|e| SetupError(format!("scaddress_to_strkey: {e}")))?;

        // Call the policy's `install` entry point with synthesized
        // (ContextRule, smart_account) args. This populates the
        // `Installed(smart_account, rule_id)` storage flag so subsequent
        // `enforce` calls don't trip `PolicyError::NotInstalled`.
        let context_rule_scval = build_minimal_context_rule_scval(context_rule_id, &policy_scaddr)
            .map_err(|e| SetupError(format!("context_rule scval: {e}")))?;
        let install_params_scval = arg_value_to_scval(&install_params)
            .map_err(|e| SetupError(format!("install_params scval: {e}")))?;
        let sa_scval = ScVal::Address(
            strkey_to_scaddress(smart_account_addr)
                .map_err(|e| SetupError(format!("smart_account_addr -> ScAddress: {e}")))?,
        );

        invoke_contract(
            &self.host,
            &policy_scaddr,
            "install",
            &[install_params_scval, context_rule_scval, sa_scval],
        )
        .map_err(|e| SetupError(format!("policy install panicked: {e:?}")))?;

        self.installed_policies.push(InstalledPolicy {
            address: policy_strkey.clone(),
            context_rule_id,
        });
        Ok(policy_strkey)
    }

    /// Invoke the smart-account's `__check_auth` boundary with the supplied
    /// `AuthPayload` + `Vec<Context>`. See module-level doc-comment for
    /// the per-context dispatch rationale.
    ///
    /// On `Ok(())` every installed policy's `enforce` returned without a
    /// contract-type panic for every context. On `HostExecError::PolicyPanic`
    /// (the discriminating case the orchestrator inspects) the first
    /// context that triggered a `panic_with_error!` surfaces its panic
    /// code intact. Wider host failures surface as `HostInternal`.
    pub fn invoke_check_auth(
        &mut self,
        sa_address: &str,
        payload: AuthPayload,
        contexts: Vec<TestContext>,
    ) -> Result<(), HostExecError> {
        match &self.smart_account {
            Some(installed) if installed == sa_address => (),
            Some(installed) => {
                return Err(HostExecError::SetupFailed(format!(
                    "invoke_check_auth SA mismatch: registered {installed}, requested {sa_address}",
                )))
            }
            None => {
                return Err(HostExecError::SetupFailed(
                    "invoke_check_auth called before install_smart_account".into(),
                ))
            }
        }

        if contexts.is_empty() {
            return Ok(());
        }
        let _ = payload;

        let policies = self.installed_policies.clone();
        if policies.is_empty() {
            // No policies installed — the SA's `do_check_auth` would
            // accept any context (signers-only flow). Mirror that.
            return Ok(());
        }

        for ctx in contexts {
            for pol in &policies {
                self.invoke_policy_enforce(&pol.address, pol.context_rule_id, sa_address, &ctx)?;
            }
        }
        Ok(())
    }

    /// Invoke a single installed policy's `enforce` entry point directly.
    /// Public so integration tests can drive a per-policy probe without
    /// going through the wider `invoke_check_auth` dispatch.
    pub fn invoke_policy_enforce(
        &mut self,
        policy_strkey: &str,
        context_rule_id: u32,
        sa_strkey: &str,
        target_context: &TestContext,
    ) -> Result<(), HostExecError> {
        let policy_scaddr = strkey_to_scaddress(policy_strkey)
            .map_err(|e| HostExecError::SetupFailed(format!("policy address: {e}")))?;
        let sa_scaddr = strkey_to_scaddress(sa_strkey)
            .map_err(|e| HostExecError::SetupFailed(format!("smart account address: {e}")))?;

        // Synthesize the four-positional-arg ScVal payload that the
        // rendered policy's `enforce(env, context, _signers, context_rule,
        // smart_account)` expects (env is implicit).
        let context_scval = build_context_contract_scval(target_context)
            .map_err(|e| HostExecError::SetupFailed(format!("build context: {e}")))?;
        let signers_scval = ScVal::Vec(Some(ScVec(VecM::default())));
        let context_rule_scval = build_minimal_context_rule_scval(context_rule_id, &sa_scaddr)
            .map_err(|e| HostExecError::SetupFailed(format!("build context_rule: {e}")))?;
        let sa_scval = ScVal::Address(sa_scaddr);

        invoke_contract(
            &self.host,
            &policy_scaddr,
            "enforce",
            &[context_scval, signers_scval, context_rule_scval, sa_scval],
        )
        .map_err(HostExecError::from_host_error)?;
        Ok(())
    }
}

// -------------------------------------------------------------------------
// Internal helpers
// -------------------------------------------------------------------------

/// Invoke `contract.fn_name(args...)` on `host`. Returns the raw `Val`
/// produced by the contract, or the underlying `HostError` on failure
/// (which the caller maps via `HostExecError::from_host_error`).
fn invoke_contract(
    host: &Host,
    contract: &ScAddress,
    fn_name: &str,
    args: &[ScVal],
) -> Result<Val, HostError> {
    let contract_val: Val = ScVal::Address(contract.clone())
        .try_into_val(host)
        .map_err(scval_conv_err)?;
    let contract_addr_obj: AddressObject = contract_val.try_into()?;

    let fn_symbol = Symbol::try_from_val(host, &fn_name).map_err(|_| {
        HostError::from(soroban_env_host::Error::from_type_and_code(
            ScErrorType::Value,
            xdr::ScErrorCode::InvalidInput,
        ))
    })?;

    let arg_scvec: VecM<ScVal> = args.to_vec().try_into().map_err(|_| {
        HostError::from(soroban_env_host::Error::from_type_and_code(
            ScErrorType::Value,
            xdr::ScErrorCode::InvalidInput,
        ))
    })?;
    let arg_val: Val = ScVal::Vec(Some(ScVec(arg_scvec)))
        .try_into_val(host)
        .map_err(scval_conv_err)?;
    let arg_vec_obj: VecObject = arg_val.try_into()?;

    // Public, macro-generated `Env::call(addr, sym, args)` — this is the
    // same entry point the env-host's own auth tests use (auth.rs:3461).
    host.call(contract_addr_obj, fn_symbol, arg_vec_obj)
}

/// Map an `ScVal -> Val` conversion failure into a generic host-level
/// `InvalidInput` error so callers see a uniform `HostError` chain.
fn scval_conv_err<E>(_e: E) -> HostError {
    HostError::from(soroban_env_host::Error::from_type_and_code(
        ScErrorType::Value,
        xdr::ScErrorCode::InvalidInput,
    ))
}

/// Convert an `ScAddress` into its StrKey representation. Contracts (`C…`)
/// and accounts (`G…`) are both supported.
fn scaddress_to_strkey(addr: &ScAddress) -> Result<String, String> {
    match addr {
        ScAddress::Contract(ContractId(hash)) => Ok(stellar_strkey::Contract(hash.0).to_string()),
        ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(bytes)))) => {
            Ok(stellar_strkey::ed25519::PublicKey(*bytes).to_string())
        }
        other => Err(format!("unsupported ScAddress variant: {other:?}")),
    }
}

/// Inverse of [`scaddress_to_strkey`].
fn strkey_to_scaddress(strkey: &str) -> Result<ScAddress, String> {
    if let Ok(c) = stellar_strkey::Contract::from_string(strkey) {
        return Ok(ScAddress::Contract(ContractId(xdr::Hash(c.0))));
    }
    if let Ok(g) = stellar_strkey::ed25519::PublicKey::from_string(strkey) {
        return Ok(ScAddress::Account(AccountId(
            PublicKey::PublicKeyTypeEd25519(Uint256(g.0)),
        )));
    }
    Err(format!("unrecognised StrKey: {strkey}"))
}

// -------------------------------------------------------------------------
// ArgValue → ScVal translation
// -------------------------------------------------------------------------

/// Translate a typed [`ArgValue`] back into the on-host `ScVal` shape. The
/// mapping is the inverse of the recorder's `ScVal → ArgValue` decode (see
/// `oz-policy-recorder`).
pub fn arg_value_to_scval(av: &ArgValue) -> Result<ScVal, String> {
    use ArgValue::*;
    Ok(match av {
        Bool(b) => ScVal::Bool(*b),
        Void => ScVal::Void,
        U32(n) => ScVal::U32(*n),
        I32(n) => ScVal::I32(*n),
        U64(s) => ScVal::U64(s.parse().map_err(|e| format!("U64 parse {s}: {e}"))?),
        I64(s) => ScVal::I64(s.parse().map_err(|e| format!("I64 parse {s}: {e}"))?),
        Timepoint(s) => ScVal::Timepoint(xdr::TimePoint(
            s.parse().map_err(|e| format!("Timepoint parse {s}: {e}"))?,
        )),
        Duration(s) => ScVal::Duration(xdr::Duration(
            s.parse().map_err(|e| format!("Duration parse {s}: {e}"))?,
        )),
        U128(s) => {
            let v: u128 = s.parse().map_err(|e| format!("U128 parse {s}: {e}"))?;
            ScVal::U128(xdr::UInt128Parts {
                hi: (v >> 64) as u64,
                lo: v as u64,
            })
        }
        I128(s) => {
            let v: i128 = s.parse().map_err(|e| format!("I128 parse {s}: {e}"))?;
            let raw = v as u128;
            ScVal::I128(xdr::Int128Parts {
                hi: (raw >> 64) as i64,
                lo: raw as u64,
            })
        }
        Symbol(s) => ScVal::Symbol(ScSymbol(
            s.clone()
                .try_into()
                .map_err(|e| format!("Symbol -> ScSymbol: {e}"))?,
        )),
        Address(strkey) => ScVal::Address(strkey_to_scaddress(strkey)?),
        Bytes { hex } => ScVal::Bytes(xdr::ScBytes(
            parse_hex(hex)?
                .try_into()
                .map_err(|e| format!("Bytes -> ScBytes: {e}"))?,
        )),
        String { utf8, hex } => {
            let raw_bytes = if let Some(s) = utf8 {
                s.as_bytes().to_vec()
            } else {
                parse_hex(hex)?
            };
            ScVal::String(xdr::ScString(
                raw_bytes
                    .try_into()
                    .map_err(|e| format!("String -> ScString: {e}"))?,
            ))
        }
        Vec(opt_items) => {
            let inner = if let Some(items) = opt_items {
                let mut out = std::vec::Vec::with_capacity(items.len());
                for it in items {
                    out.push(arg_value_to_scval(it)?);
                }
                Some(ScVec(
                    out.try_into().map_err(|e| format!("Vec -> ScVec: {e}"))?,
                ))
            } else {
                None
            };
            ScVal::Vec(inner)
        }
        Map(opt_entries) => {
            let inner = if let Some(entries) = opt_entries {
                let mut out = std::vec::Vec::with_capacity(entries.len());
                for entry in entries {
                    out.push(ScMapEntry {
                        key: arg_value_to_scval(&entry.key)?,
                        val: arg_value_to_scval(&entry.value)?,
                    });
                }
                Some(ScMap(
                    out.try_into().map_err(|e| format!("Map -> ScMap: {e}"))?,
                ))
            } else {
                None
            };
            ScVal::Map(inner)
        }
        LedgerKeyContractInstance => Err("LedgerKeyContractInstance not invokable".to_string())?,
        LedgerKeyNonce { .. } => Err("LedgerKeyNonce not invokable".to_string())?,
        ContractInstance { .. } => Err("ContractInstance not invokable".to_string())?,
        Error { .. } => Err("Error not invokable".to_string())?,
        U256(_) | I256(_) => Err("U256/I256 args not yet supported by simhost".to_string())?,
    })
}

fn parse_hex(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err(format!("hex length {} is odd", hex.len()));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks(2) {
        let s = std::str::from_utf8(pair).map_err(|e| format!("hex utf8: {e}"))?;
        out.push(u8::from_str_radix(s, 16).map_err(|e| format!("hex byte {s}: {e}"))?);
    }
    Ok(out)
}

// -------------------------------------------------------------------------
// Context::Contract { contract, fn_name, args } construction
// -------------------------------------------------------------------------

/// Build the `Context::Contract { contract, fn_name, args }` ScVal that the
/// policy's `enforce` entrypoint expects as its `context` argument.
/// soroban-sdk encodes `enum Context { Contract(ContractContext) }` as an
/// `ScVec[Symbol("Contract"), Map{...ContractContext fields...}]`.
///
/// Cross-checked against the rendered policy source at
/// `walkthroughs/phase3-codegen-fixture/expected/slot_0/source.rs:170`
/// (`match &context { Context::Contract(ContractContext { contract, fn_name, args }) => …}`).
fn build_context_contract_scval(ctx: &TestContext) -> Result<ScVal, String> {
    let mut arg_scvals = Vec::with_capacity(ctx.args.len());
    for arg in &ctx.args {
        arg_scvals.push(arg_value_to_scval(arg)?);
    }
    let inner_args = ScVal::Vec(Some(ScVec(
        arg_scvals
            .try_into()
            .map_err(|e| format!("args VecM: {e}"))?,
    )));
    let contract_scval = ScVal::Address(strkey_to_scaddress(&ctx.contract_address)?);
    let fn_symbol = ScVal::Symbol(ScSymbol(
        ctx.function_name
            .clone()
            .try_into()
            .map_err(|e| format!("fn_name -> ScSymbol: {e}"))?,
    ));

    let contract_context_map = ScVal::Map(Some(ScMap(
        vec![
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("args".try_into().unwrap())),
                val: inner_args,
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("contract".try_into().unwrap())),
                val: contract_scval,
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("fn_name".try_into().unwrap())),
                val: fn_symbol,
            },
        ]
        .try_into()
        .map_err(|e| format!("ContractContext map: {e}"))?,
    )));

    Ok(ScVal::Vec(Some(ScVec(
        vec![
            ScVal::Symbol(ScSymbol("Contract".try_into().unwrap())),
            contract_context_map,
        ]
        .try_into()
        .map_err(|e| format!("Context::Contract vec: {e}"))?,
    ))))
}

/// Build a minimal `ContextRule` ScVal that satisfies the policy's
/// `enforce` signature. Only `id` is actually read by the rendered policy
/// (it namespaces the `Installed(addr, id)` storage key), so we stub the
/// remaining fields with empty `Vec`s + a placeholder name.
fn build_minimal_context_rule_scval(
    context_rule_id: u32,
    smart_account: &ScAddress,
) -> Result<ScVal, String> {
    let empty_vec = || ScVal::Vec(Some(ScVec(VecM::default())));
    let name = ScVal::String(xdr::ScString(
        b"rule"
            .to_vec()
            .try_into()
            .map_err(|e| format!("name: {e}"))?,
    ));
    let context_type = ScVal::Vec(Some(ScVec(
        vec![ScVal::Symbol(ScSymbol(
            "Default".try_into().map_err(|e| format!("Default: {e}"))?,
        ))]
        .try_into()
        .map_err(|e| format!("context_type vec: {e}"))?,
    )));
    let _ = smart_account;

    Ok(ScVal::Map(Some(ScMap(
        vec![
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("context_type".try_into().unwrap())),
                val: context_type,
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("id".try_into().unwrap())),
                val: ScVal::U32(context_rule_id),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("name".try_into().unwrap())),
                val: name,
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("policies".try_into().unwrap())),
                val: empty_vec(),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("policy_ids".try_into().unwrap())),
                val: empty_vec(),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("signer_ids".try_into().unwrap())),
                val: empty_vec(),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("signers".try_into().unwrap())),
                val: empty_vec(),
            },
            ScMapEntry {
                key: ScVal::Symbol(ScSymbol("valid_until".try_into().unwrap())),
                // `Option::None` encodes as `Val::VOID` (see
                // soroban-env-common::option). Using an empty vec here
                // (the previous encoding) makes the host's `TryFromVal`
                // for `Option<u32>` decode to `Some(vec_obj)` which then
                // fails at the `u32` cast.
                val: ScVal::Void,
            },
        ]
        .try_into()
        .map_err(|e| format!("context_rule map: {e}"))?,
    ))))
}

// -------------------------------------------------------------------------
// Tests (pure logic; the network/host invocations are in tests/host_smoke.rs)
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oz_policy_core::MapEntry;

    #[test]
    fn vendored_wasm_hash_matches_pinned_constant() {
        TestHost::verify_vendored_smart_account_wasm()
            .expect("vendored WASM SHA-256 must match constant");
    }

    #[test]
    fn test_host_new_succeeds() {
        let h = TestHost::new(100, "Test SDF Network ; September 2015")
            .expect("TestHost::new should succeed");
        assert_eq!(h.network_passphrase(), "Test SDF Network ; September 2015");
        assert_eq!(h.initial_ledger_seq(), 100);
        assert!(h.smart_account_address().is_none());
    }

    #[test]
    fn scaddress_roundtrip_contract() {
        let bytes = [0x42u8; 32];
        let sa = ScAddress::Contract(ContractId(xdr::Hash(bytes)));
        let strkey = scaddress_to_strkey(&sa).expect("strkey");
        let back = strkey_to_scaddress(&strkey).expect("scaddress");
        assert_eq!(sa, back);
    }

    #[test]
    fn scaddress_roundtrip_account() {
        let bytes = [0xa5u8; 32];
        let sa = ScAddress::Account(AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(bytes))));
        let strkey = scaddress_to_strkey(&sa).expect("strkey");
        let back = strkey_to_scaddress(&strkey).expect("scaddress");
        assert_eq!(sa, back);
    }

    #[test]
    fn arg_value_to_scval_u32() {
        let av = ArgValue::U32(123);
        assert_eq!(arg_value_to_scval(&av).expect("scval"), ScVal::U32(123));
    }

    #[test]
    fn arg_value_to_scval_i128_low_half() {
        let av = ArgValue::I128("1000000".into());
        match arg_value_to_scval(&av).expect("scval") {
            ScVal::I128(parts) => {
                assert_eq!(parts.hi, 0);
                assert_eq!(parts.lo, 1_000_000);
            }
            other => panic!("expected I128, got {other:?}"),
        }
    }

    #[test]
    fn arg_value_to_scval_address_contract() {
        let bytes = [0x11u8; 32];
        let strkey = stellar_strkey::Contract(bytes).to_string();
        let av = ArgValue::Address(strkey);
        match arg_value_to_scval(&av).expect("scval") {
            ScVal::Address(ScAddress::Contract(ContractId(xdr::Hash(b)))) => {
                assert_eq!(b, bytes)
            }
            other => panic!("expected Contract address, got {other:?}"),
        }
    }

    #[test]
    fn arg_value_to_scval_symbol() {
        let av = ArgValue::Symbol("transfer".into());
        match arg_value_to_scval(&av).expect("scval") {
            ScVal::Symbol(ScSymbol(s)) => assert_eq!(s.as_slice(), b"transfer"),
            other => panic!("expected Symbol, got {other:?}"),
        }
    }

    #[test]
    fn arg_value_to_scval_map() {
        let av = ArgValue::Map(Some(vec![MapEntry {
            key: ArgValue::Symbol("amount".into()),
            value: ArgValue::U32(42),
        }]));
        match arg_value_to_scval(&av).expect("scval") {
            ScVal::Map(Some(ScMap(entries))) => {
                assert_eq!(entries.len(), 1);
                match (&entries[0].key, &entries[0].val) {
                    (ScVal::Symbol(ScSymbol(k)), ScVal::U32(42)) => {
                        assert_eq!(k.as_slice(), b"amount");
                    }
                    other => panic!("unexpected map entry: {other:?}"),
                }
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }

    #[test]
    fn build_context_contract_scval_shape() {
        let ctx = TestContext {
            contract_address: stellar_strkey::Contract([0x77; 32]).to_string(),
            function_name: "transfer".into(),
            args: vec![
                ArgValue::Address(stellar_strkey::Contract([0xaa; 32]).to_string()),
                ArgValue::I128("123".into()),
            ],
        };
        let sv = build_context_contract_scval(&ctx).expect("build");
        match sv {
            ScVal::Vec(Some(ScVec(v))) => {
                assert_eq!(v.len(), 2, "Context::Contract is a 2-element vec");
                match &v[0] {
                    ScVal::Symbol(ScSymbol(s)) => assert_eq!(s.as_slice(), b"Contract"),
                    other => panic!("expected variant Symbol, got {other:?}"),
                }
                match &v[1] {
                    ScVal::Map(Some(ScMap(_))) => {}
                    other => panic!("expected ContractContext map, got {other:?}"),
                }
            }
            other => panic!("expected Vec, got {other:?}"),
        }
    }

    #[test]
    fn build_minimal_context_rule_has_required_keys() {
        let sa = ScAddress::Contract(ContractId(xdr::Hash([0u8; 32])));
        let sv = build_minimal_context_rule_scval(7, &sa).expect("build");
        let ScVal::Map(Some(ScMap(entries))) = sv else {
            panic!("expected Map");
        };
        let keys: Vec<String> = entries
            .iter()
            .map(|e| match &e.key {
                ScVal::Symbol(ScSymbol(s)) => {
                    std::str::from_utf8(s.as_slice()).unwrap().to_string()
                }
                _ => panic!("expected Symbol key"),
            })
            .collect();
        for required in [
            "context_type",
            "id",
            "name",
            "policies",
            "policy_ids",
            "signer_ids",
            "signers",
            "valid_until",
        ] {
            assert!(
                keys.iter().any(|k| k == required),
                "missing key: {required} (have {keys:?})"
            );
        }
        let id_entry = entries
            .iter()
            .find(|e| matches!(&e.key, ScVal::Symbol(s) if s.0.as_slice() == b"id"))
            .unwrap();
        assert_eq!(id_entry.val, ScVal::U32(7));
    }

    #[test]
    fn host_exec_error_contract_panic_extracts_code() {
        let raw = soroban_env_host::Error::from_contract_error(1010);
        let host_err: HostError = raw.into();
        let mapped = HostExecError::from_host_error(host_err);
        match mapped {
            HostExecError::PolicyPanic(code) => assert_eq!(code, 1010),
            other => panic!("expected PolicyPanic(1010), got {other:?}"),
        }
    }

    #[test]
    fn host_exec_error_non_contract_bucketed_as_internal() {
        let raw = soroban_env_host::Error::from_type_and_code(
            ScErrorType::Storage,
            xdr::ScErrorCode::MissingValue,
        );
        let mapped = HostExecError::from_host_error(raw.into());
        assert!(
            matches!(mapped, HostExecError::HostInternal(_)),
            "expected HostInternal, got {mapped:?}"
        );
    }

    #[test]
    fn install_policy_before_smart_account_fails() {
        let mut h = TestHost::new(100, "Test SDF Network ; September 2015").expect("host");
        let dummy_wasm = b"\0asm\x01\x00\x00\x00";
        let err = h
            .install_policy(
                dummy_wasm,
                "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH",
                0,
                ArgValue::Void,
            )
            .expect_err("must fail without SA installed first");
        assert_eq!(err.code(), "E_SIM_PERMIT_DENIED");
        assert!(
            err.to_string().contains("install_smart_account"),
            "expected message naming install_smart_account; got {err}"
        );
    }

    #[test]
    fn invoke_check_auth_before_smart_account_fails() {
        let mut h = TestHost::new(100, "Test SDF Network ; September 2015").expect("host");
        let err = h
            .invoke_check_auth(
                "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH",
                AuthPayload {
                    signer_addresses: vec![],
                    context_rule_ids: vec![0],
                },
                vec![TestContext {
                    contract_address: "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH"
                        .into(),
                    function_name: "transfer".into(),
                    args: vec![],
                }],
            )
            .expect_err("must fail without SA");
        match err {
            HostExecError::SetupFailed(s) => assert!(s.contains("install_smart_account")),
            other => panic!("expected SetupFailed; got {other:?}"),
        }
    }

    /// Empty contexts is a no-op permit, mirroring `do_check_auth`'s
    /// early-return for `auth_contexts.is_empty()`.
    #[test]
    fn invoke_check_auth_with_empty_contexts_permits() {
        let mut h = TestHost::new(100, "Test SDF Network ; September 2015").expect("host");
        let sa = h.install_smart_account("").expect("install SA");
        h.invoke_check_auth(
            &sa,
            AuthPayload {
                signer_addresses: vec![],
                context_rule_ids: vec![],
            },
            vec![],
        )
        .expect("empty contexts must permit");
    }
}
