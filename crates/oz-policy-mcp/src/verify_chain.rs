//! on-chain `ContextRule` readback + drift diff for `verify_install`.
//! goes via `simulateTransaction(SA.get_context_rule)` so we decode off the abi,
//! not the changing internal storage layout. decoder is field-name-keyed
//! (`#[contracttype]` map ordering tolerated).

use std::str::FromStr;
use std::time::Duration;

use oz_policy_core::spec::{ContextType, PolicySlot, PolicySpec, SignerSpec, TemplateFamily};
use oz_policy_installer::registry::{primitive_address, project_deployed_policy_address};
use serde_json::json;
use stellar_rpc_client::Client;
use stellar_strkey::{ed25519::PublicKey as Ed25519PublicKey, Contract};
use stellar_xdr::curr::{
    self as xdr, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, Memo, MuxedAccount,
    Operation, OperationBody, Preconditions, ScAddress, ScSymbol, ScVal, SequenceNumber,
    SorobanAuthorizationEntry, Transaction, TransactionEnvelope, TransactionExt,
    TransactionV1Envelope, Uint256, VecM,
};
use tokio::time::timeout;

use crate::tools::DriftItem;

/// mirrors the recorder + installer 30 s RPC ceiling.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// projection of OZ `Signer` enum — just enough for drift comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnChainSigner {
    /// `Signer::Delegated(Address)`.
    Delegated { address: String },
    /// `Signer::External(verifier, public_key)`. Ed25519/WebAuthn split is
    /// done on the spec side, not here.
    External {
        verifier: String,
        public_key_hex: String,
    },
}

/// on-chain `ContextRule` projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnChainContextRule {
    pub id: u32,
    pub name: String,
    pub context_type: OnChainContextType,
    pub valid_until: Option<u32>,
    pub signers: Vec<OnChainSigner>,
    /// contract addresses (StrKey `C…`) of the policies attached to this
    /// rule. The on-chain ScVal carries `Vec<Address>` directly; the
    /// `policy_ids` parallel array is ignored for drift comparison.
    pub policies: Vec<String>,
}

/// on-chain `ContextRuleType` projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnChainContextType {
    Default,
    CallContract {
        address: String,
    },
    /// the OZ ABI also has `CreateContract(BytesN<32>)` — we surface it for
    /// forward compat (`drift` flags it as "unsupported by spec model").
    CreateContract {
        wasm_hash_hex: String,
    },
}

/// errors emitted by the on-chain readback path. Mapped onto MCP
/// `ErrorData` (or [`crate::error_mapping::error_to_jsonrpc`]) at the
/// `verify_install` handler boundary.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// `getLedgerEntries`/`simulateTransaction` returned a transport-level
    /// error, the RPC endpoint timed out, or the simulator surfaced an
    /// error string. Mapped to `E_VERIFY_DRIFT` with a `rpc-*` detail
    /// prefix at the handler boundary.
    #[error("rpc error: {0}")]
    Rpc(String),
    /// simulator succeeded but returned a host-error variant for the
    /// invocation — most commonly the `ContextRuleNotFound (3000)` host
    /// error when `context_rule_id` does not exist on the SA. Surfaced
    /// as `E_VERIFY_DRIFT` with detail `rule-not-found`.
    #[error("rule not found: {0}")]
    RuleNotFound(String),
    /// simulator returned a `returnValue` we could not decode into a
    /// `ContextRule` (wrong ScVal shape — should be unreachable against
    /// a valid SA but we surface it instead of panicking).
    #[error("xdr decode failed: {0}")]
    Decode(String),
    /// caller passed a malformed StrKey C-address or G-address.
    #[error("invalid strkey: {0}")]
    InvalidStrKey(String),
}

/// build a single-op `TransactionEnvelope` that invokes
/// `SA.get_context_rule(rule_id)` and simulate it against the supplied
/// RPC. Returns the decoded [`OnChainContextRule`] on success.
///
/// `source_account` is any funded G-address on the target network — the
/// simulator only needs a valid sequence number; no transaction is ever
/// signed or submitted. The Phase 7 SA owner (`sa-owner-p7r2`) is the
/// canonical choice for the testnet path; mainnet callers pass their
/// own funded account.
pub async fn read_context_rule_via_simulate(
    rpc_url: &str,
    network_passphrase: &str,
    smart_account_strkey: &str,
    context_rule_id: u32,
    source_account_strkey: &str,
) -> Result<OnChainContextRule, ReadError> {
    // ----- 0. Client + network match guard ------------------------------
    let client = Client::new(rpc_url).map_err(|e| ReadError::Rpc(format!("client init: {e}")))?;
    let net = timeout(RPC_TIMEOUT, client.get_network())
        .await
        .map_err(|_e| ReadError::Rpc(format!("getNetwork timeout: {rpc_url}")))?
        .map_err(|e| ReadError::Rpc(format!("getNetwork failed: {e}")))?;
    if net.passphrase != network_passphrase {
        return Err(ReadError::Rpc(format!(
            "rpc reports passphrase '{}' but caller asserted '{}'",
            net.passphrase, network_passphrase
        )));
    }

    // ----- 1. Resolve source-account seq num ---------------------------
    let source_entry = timeout(RPC_TIMEOUT, client.get_account(source_account_strkey))
        .await
        .map_err(|_e| ReadError::Rpc(format!("getAccount({source_account_strkey}) timeout")))?
        .map_err(|e| ReadError::Rpc(format!("getAccount({source_account_strkey}): {e}")))?;
    let source_seq = source_entry.seq_num.0;

    // ----- 2. Build the invoke-host-function envelope ------------------
    let sa_addr = ScAddress::from_str(smart_account_strkey).map_err(|e| {
        ReadError::InvalidStrKey(format!("smart_account {smart_account_strkey}: {e}"))
    })?;
    let fn_name = ScSymbol::try_from(b"get_context_rule".to_vec())
        .map_err(|e| ReadError::Rpc(format!("scsymbol: {e}")))?;
    let args: VecM<ScVal> = vec![ScVal::U32(context_rule_id)]
        .try_into()
        .map_err(|e| ReadError::Rpc(format!("args vec encode: {e}")))?;
    let invoke = InvokeHostFunctionOp {
        host_function: HostFunction::InvokeContract(InvokeContractArgs {
            contract_address: sa_addr,
            function_name: fn_name,
            args,
        }),
        auth: VecM::<SorobanAuthorizationEntry>::default(),
    };
    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(invoke),
    };
    let ops: VecM<Operation, 100> = vec![op]
        .try_into()
        .map_err(|e| ReadError::Rpc(format!("op vec encode: {e}")))?;

    let source_pk = Ed25519PublicKey::from_string(source_account_strkey).map_err(|e| {
        ReadError::InvalidStrKey(format!("source_account {source_account_strkey}: {e}"))
    })?;
    let source_muxed = MuxedAccount::Ed25519(Uint256(source_pk.0));
    let tx = Transaction {
        source_account: source_muxed,
        fee: 0,
        seq_num: SequenceNumber(source_seq + 1),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: ops,
        ext: TransactionExt::V0,
    };
    let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx,
        signatures: VecM::default(),
    });

    // ----- 3. simulateTransaction --------------------------------------
    let sim = timeout(
        RPC_TIMEOUT,
        client.simulate_transaction_envelope(&envelope, None),
    )
    .await
    .map_err(|_e| ReadError::Rpc(format!("simulate timeout: {rpc_url}")))?
    .map_err(|e| ReadError::Rpc(format!("simulate failed: {e}")))?;

    if let Some(err) = &sim.error {
        // the contract's `get_context_rule` panics with a typed host
        // error when the id is unknown. The simulator surfaces that as
        // the `error` field on the response. Detect the canonical
        // "rule not found" / `ContextRuleNotFound (3000)` substring so
        // we can promote it to a typed [`ReadError::RuleNotFound`] for
        // the `E_VERIFY_DRIFT` path; any other error is generic RPC.
        let e_lower = err.to_lowercase();
        if e_lower.contains("contextrulenotfound")
            || e_lower.contains("3000")
            || e_lower.contains("not found")
        {
            return Err(ReadError::RuleNotFound(err.clone()));
        }
        return Err(ReadError::Rpc(format!("simulate reported: {err}")));
    }

    let results = sim
        .results()
        .map_err(|e| ReadError::Decode(format!("results decode: {e}")))?;
    let first = results
        .first()
        .ok_or_else(|| ReadError::Decode("simulate returned no results[]".to_string()))?;
    let mut rule = decode_context_rule_scval(&first.xdr)
        .map_err(|e| ReadError::Decode(format!("returnValue decode: {e}")))?;
    // the contract ABI states `id: u32` is part of the struct. We pin
    // here that the value matches the requested id — a mismatch would
    // be a contract bug, so surface it loudly.
    if rule.id != context_rule_id {
        return Err(ReadError::Decode(format!(
            "ContextRule.id mismatch: requested {context_rule_id}, on-chain {actual}",
            actual = rule.id
        )));
    }
    // canonicalise downstream comparisons by sorting the policies (the
    // ABI does not promise ordering across upgrades; spec-side ordering
    // is also semantic-free).
    rule.policies.sort();
    Ok(rule)
}

// pure decoder — no network, fully unit-testable.

/// decode a `ContextRule` `ScVal::Map` into the typed projection. The
/// input must be an `ScVal::Map(Some(map))` whose entries are keyed by
/// `ScVal::Symbol(field_name)`; missing fields error.
pub fn decode_context_rule_scval(v: &ScVal) -> Result<OnChainContextRule, String> {
    let map_ref = match v {
        ScVal::Map(Some(m)) => m,
        other => {
            return Err(format!(
                "expected ScVal::Map(Some(_)), got {:?}",
                other_tag(other)
            ))
        }
    };

    let mut id: Option<u32> = None;
    let mut name: Option<String> = None;
    let mut context_type: Option<OnChainContextType> = None;
    let mut valid_until: Option<Option<u32>> = None;
    let mut signers: Option<Vec<OnChainSigner>> = None;
    let mut policies: Option<Vec<String>> = None;
    // policy_ids + signer_ids are present in the on-chain struct but
    // intentionally ignored: ids are opaque per-SA handles, not part of
    // the spec-level identity comparison.

    for entry in map_ref.0.iter() {
        let key = match &entry.key {
            ScVal::Symbol(s) => std::str::from_utf8(s.as_slice())
                .map_err(|e| format!("field-name utf8 decode: {e}"))?,
            other => {
                return Err(format!(
                    "expected ScVal::Symbol field name, got {:?}",
                    other_tag(other)
                ))
            }
        };
        match key {
            "id" => match &entry.val {
                ScVal::U32(n) => id = Some(*n),
                other => return Err(format!("id field must be U32, got {:?}", other_tag(other))),
            },
            "name" => name = Some(decode_string(&entry.val).map_err(|e| format!("name: {e}"))?),
            "context_type" => {
                context_type = Some(
                    decode_context_type(&entry.val).map_err(|e| format!("context_type: {e}"))?,
                )
            }
            "valid_until" => {
                valid_until =
                    Some(decode_option_u32(&entry.val).map_err(|e| format!("valid_until: {e}"))?)
            }
            "signers" => {
                signers = Some(decode_signers(&entry.val).map_err(|e| format!("signers: {e}"))?)
            }
            "policies" => {
                policies = Some(decode_policies(&entry.val).map_err(|e| format!("policies: {e}"))?)
            }
            // `signer_ids` + `policy_ids` are deliberately not decoded
            // here — they're opaque per-SA handles, not part of the
            // spec-level identity. The fields will exist on chain; we
            // tolerate them silently rather than rejecting unknowns.
            "signer_ids" | "policy_ids" => {}
            other => {
                // unknown field — log via tracing for visibility but do
                // not error: forward compat with future ContextRule
                // additions.
                tracing::debug!(field = other, "ignoring unknown ContextRule field");
            }
        }
    }

    Ok(OnChainContextRule {
        id: id.ok_or_else(|| "missing field: id".to_string())?,
        name: name.ok_or_else(|| "missing field: name".to_string())?,
        context_type: context_type.ok_or_else(|| "missing field: context_type".to_string())?,
        valid_until: valid_until.ok_or_else(|| "missing field: valid_until".to_string())?,
        signers: signers.ok_or_else(|| "missing field: signers".to_string())?,
        policies: policies.ok_or_else(|| "missing field: policies".to_string())?,
    })
}

/// decode `soroban_sdk::String` — emitted by the host as
/// `ScVal::String(ScString(vec_of_bytes))`.
fn decode_string(v: &ScVal) -> Result<String, String> {
    match v {
        ScVal::String(s) => std::str::from_utf8(s.0.as_slice())
            .map(|s| s.to_string())
            .map_err(|e| format!("utf8: {e}")),
        other => Err(format!(
            "expected ScVal::String, got {:?}",
            other_tag(other)
        )),
    }
}

/// decode `Option<u32>` — `Some(n) → ScVal::U32(n)`, `None → ScVal::Void`.
/// this matches the soroban-sdk encoding for `Option<T>` fields on
/// `#[contracttype]` structs (a missing value emits `Void`, not absence).
fn decode_option_u32(v: &ScVal) -> Result<Option<u32>, String> {
    match v {
        ScVal::Void => Ok(None),
        ScVal::U32(n) => Ok(Some(*n)),
        other => Err(format!(
            "expected ScVal::Void | ScVal::U32, got {:?}",
            other_tag(other)
        )),
    }
}

/// decode `ContextRuleType` from its `#[contracttype]` enum encoding —
/// `ScVal::Vec(Some([Symbol(variant), <args>...]))`.
fn decode_context_type(v: &ScVal) -> Result<OnChainContextType, String> {
    let vec_ref = match v {
        ScVal::Vec(Some(vec)) => &vec.0,
        other => {
            return Err(format!(
                "expected ScVal::Vec(Some(_)), got {:?}",
                other_tag(other)
            ))
        }
    };
    let tag = match vec_ref.first() {
        Some(ScVal::Symbol(s)) => {
            std::str::from_utf8(s.as_slice()).map_err(|e| format!("variant tag utf8: {e}"))?
        }
        Some(other) => {
            return Err(format!(
                "expected ScVal::Symbol variant tag, got {:?}",
                other_tag(other)
            ))
        }
        None => return Err("variant tag missing".to_string()),
    };
    match tag {
        "Default" => Ok(OnChainContextType::Default),
        "CallContract" => {
            let addr = vec_ref
                .get(1)
                .ok_or_else(|| "CallContract missing address arg".to_string())?;
            Ok(OnChainContextType::CallContract {
                address: decode_address(addr)?,
            })
        }
        "CreateContract" => {
            let wasm_hash = vec_ref
                .get(1)
                .ok_or_else(|| "CreateContract missing wasm_hash arg".to_string())?;
            let bytes = match wasm_hash {
                ScVal::Bytes(b) => b.0.as_slice(),
                other => {
                    return Err(format!(
                        "CreateContract wasm_hash must be Bytes, got {:?}",
                        other_tag(other)
                    ))
                }
            };
            Ok(OnChainContextType::CreateContract {
                wasm_hash_hex: hex::encode(bytes),
            })
        }
        other => Err(format!("unknown ContextRuleType variant '{other}'")),
    }
}

/// decode a `Vec<Signer>` ScVal. The inner `Signer` enum follows the
/// `#[contracttype]` enum layout described at the top of this module.
fn decode_signers(v: &ScVal) -> Result<Vec<OnChainSigner>, String> {
    let vec_ref = match v {
        ScVal::Vec(Some(vec)) => &vec.0,
        other => {
            return Err(format!(
                "expected ScVal::Vec(Some(_)), got {:?}",
                other_tag(other)
            ))
        }
    };
    let mut out = Vec::with_capacity(vec_ref.len());
    for s in vec_ref.iter() {
        out.push(decode_signer(s)?);
    }
    Ok(out)
}

fn decode_signer(v: &ScVal) -> Result<OnChainSigner, String> {
    let vec_ref = match v {
        ScVal::Vec(Some(vec)) => &vec.0,
        other => {
            return Err(format!(
                "expected ScVal::Vec for Signer, got {:?}",
                other_tag(other)
            ))
        }
    };
    let tag = match vec_ref.first() {
        Some(ScVal::Symbol(s)) => {
            std::str::from_utf8(s.as_slice()).map_err(|e| format!("signer variant utf8: {e}"))?
        }
        Some(other) => {
            return Err(format!(
                "expected ScVal::Symbol Signer tag, got {:?}",
                other_tag(other)
            ))
        }
        None => return Err("Signer variant tag missing".to_string()),
    };
    match tag {
        "Delegated" => {
            let addr = vec_ref
                .get(1)
                .ok_or_else(|| "Delegated missing address arg".to_string())?;
            Ok(OnChainSigner::Delegated {
                address: decode_address(addr)?,
            })
        }
        "External" => {
            let verifier = vec_ref
                .get(1)
                .ok_or_else(|| "External missing verifier".to_string())?;
            let pk = vec_ref
                .get(2)
                .ok_or_else(|| "External missing public_key".to_string())?;
            let pk_bytes = match pk {
                ScVal::Bytes(b) => b.0.as_slice(),
                other => {
                    return Err(format!(
                        "External public_key must be Bytes, got {:?}",
                        other_tag(other)
                    ))
                }
            };
            Ok(OnChainSigner::External {
                verifier: decode_address(verifier)?,
                public_key_hex: hex::encode(pk_bytes),
            })
        }
        other => Err(format!("unknown Signer variant '{other}'")),
    }
}

fn decode_policies(v: &ScVal) -> Result<Vec<String>, String> {
    let vec_ref = match v {
        ScVal::Vec(Some(vec)) => &vec.0,
        other => {
            return Err(format!(
                "expected ScVal::Vec for policies, got {:?}",
                other_tag(other)
            ))
        }
    };
    let mut out = Vec::with_capacity(vec_ref.len());
    for entry in vec_ref.iter() {
        out.push(decode_address(entry)?);
    }
    Ok(out)
}

/// decode `ScVal::Address` to a StrKey string (`G…` or `C…`).
fn decode_address(v: &ScVal) -> Result<String, String> {
    let addr = match v {
        ScVal::Address(a) => a,
        other => {
            return Err(format!(
                "expected ScVal::Address, got {:?}",
                other_tag(other)
            ))
        }
    };
    match addr {
        ScAddress::Account(acct_id) => match acct_id {
            xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(Uint256(bytes))) => {
                Ok(Ed25519PublicKey(*bytes).to_string())
            }
        },
        ScAddress::Contract(xdr::ContractId(xdr::Hash(bytes))) => Ok(Contract(*bytes).to_string()),
        other => Err(format!("unsupported ScAddress variant: {other:?}")),
    }
}

/// stable tag string for diagnostics — avoids dumping the entire ScVal in
/// error messages.
fn other_tag(v: &ScVal) -> &'static str {
    match v {
        ScVal::Bool(_) => "Bool",
        ScVal::Void => "Void",
        ScVal::Error(_) => "Error",
        ScVal::U32(_) => "U32",
        ScVal::I32(_) => "I32",
        ScVal::U64(_) => "U64",
        ScVal::I64(_) => "I64",
        ScVal::Timepoint(_) => "Timepoint",
        ScVal::Duration(_) => "Duration",
        ScVal::U128(_) => "U128",
        ScVal::I128(_) => "I128",
        ScVal::U256(_) => "U256",
        ScVal::I256(_) => "I256",
        ScVal::Bytes(_) => "Bytes",
        ScVal::String(_) => "String",
        ScVal::Symbol(_) => "Symbol",
        ScVal::Vec(_) => "Vec",
        ScVal::Map(_) => "Map",
        ScVal::Address(_) => "Address",
        ScVal::ContractInstance(_) => "ContractInstance",
        ScVal::LedgerKeyContractInstance => "LedgerKeyContractInstance",
        ScVal::LedgerKeyNonce(_) => "LedgerKeyNonce",
    }
}

// drift comparator — pure.

/// compare a [`PolicySpec`] against the on-chain rule and emit per-field
/// [`DriftItem`]s. Empty result ⇒ everything matches.
///
/// `network_passphrase` is needed to resolve `PolicySlot::Generated`
/// slots to their deployed contract addresses via
/// [`project_deployed_policy_address`].
pub fn compute_drift(
    expected_spec: &PolicySpec,
    rule_name_expected: &str,
    actual: &OnChainContextRule,
    network_passphrase: &str,
) -> Vec<DriftItem> {
    let mut drift = Vec::new();

    // ----- name ---------------------------------------------------------
    if actual.name != rule_name_expected {
        drift.push(DriftItem {
            field: "context_rule.name".to_string(),
            expected: serde_json::Value::String(rule_name_expected.to_string()),
            actual: serde_json::Value::String(actual.name.clone()),
        });
    }

    // ----- context_type -------------------------------------------------
    let ct_match = match (
        &expected_spec.context_rule.context_type,
        &actual.context_type,
    ) {
        (ContextType::Default, OnChainContextType::Default) => true,
        (
            ContextType::CallContract { address: a },
            OnChainContextType::CallContract { address: b },
        ) => a == b,
        _ => false,
    };
    if !ct_match {
        drift.push(DriftItem {
            field: "context_rule.context_type".to_string(),
            expected: json!(match &expected_spec.context_rule.context_type {
                ContextType::Default => json!({ "kind": "Default" }),
                ContextType::CallContract { address } =>
                    json!({ "kind": "CallContract", "address": address }),
            }),
            actual: json!(match &actual.context_type {
                OnChainContextType::Default => json!({ "kind": "Default" }),
                OnChainContextType::CallContract { address } =>
                    json!({ "kind": "CallContract", "address": address }),
                OnChainContextType::CreateContract { wasm_hash_hex } =>
                    json!({ "kind": "CreateContract", "wasm_hash_hex": wasm_hash_hex }),
            }),
        });
    }

    // ----- valid_until --------------------------------------------------
    if expected_spec.context_rule.valid_until != actual.valid_until {
        drift.push(DriftItem {
            field: "context_rule.valid_until".to_string(),
            expected: json!(expected_spec.context_rule.valid_until),
            actual: json!(actual.valid_until),
        });
    }

    // ----- signers (as a set) -------------------------------------------
    let mut expected_signers: Vec<String> = expected_spec
        .signers
        .iter()
        .map(signer_spec_to_canonical_string)
        .collect();
    expected_signers.sort();
    let mut actual_signers: Vec<String> = actual
        .signers
        .iter()
        .map(on_chain_signer_to_canonical_string)
        .collect();
    actual_signers.sort();
    if expected_signers != actual_signers {
        drift.push(DriftItem {
            field: "signers".to_string(),
            expected: json!(expected_signers),
            actual: json!(actual_signers),
        });
    }

    // ----- policies (as a set of contract addresses) --------------------
    let mut expected_policy_addrs: Vec<String> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    for slot in &expected_spec.policies {
        match resolve_policy_address(slot, network_passphrase) {
            Some(addr) => expected_policy_addrs.push(addr),
            None => unresolved.push(unresolved_slot_label(slot)),
        }
    }
    expected_policy_addrs.sort();
    let mut actual_policies = actual.policies.clone();
    actual_policies.sort();

    if !unresolved.is_empty() {
        // the spec referenced a slot we cannot map to a deployed address
        // for this network. We surface this as drift (rather than silent
        // success) so callers know their expected_spec is under-specified
        // for the comparison.
        drift.push(DriftItem {
            field: "policies.unresolved_slots".to_string(),
            expected: json!(unresolved),
            actual: serde_json::Value::Null,
        });
    }
    if expected_policy_addrs != actual_policies {
        drift.push(DriftItem {
            field: "policies".to_string(),
            expected: json!(expected_policy_addrs),
            actual: json!(actual_policies),
        });
    }

    drift
}

/// canonicalise a [`SignerSpec`] to a string that round-trips through
/// equality comparison against the on-chain projection. We use
/// `"Delegated:<addr>"` and `"External:<verifier>:<pk_hex>"` so the
/// orderings of `expected` and `actual` collapse to the same key.
fn signer_spec_to_canonical_string(s: &SignerSpec) -> String {
    match s {
        SignerSpec::Delegated { address } => format!("Delegated:{address}"),
        SignerSpec::ExternalEd25519 { public_key_hex } => {
            // external-Ed25519's *verifier* address is whichever ed25519
            // verifier contract the OZ project deploys. The PolicySpec
            // does not carry that address (it's project-level) so we
            // compare *only the public_key_hex* — the on-chain side
            // exposes both. If the on-chain side has a non-empty verifier
            // and the spec doesn't, they will not match.
            format!("ExternalEd25519:{public_key_hex}")
        }
        SignerSpec::ExternalWebAuthn { public_key_hex } => {
            format!("ExternalWebAuthn:{public_key_hex}")
        }
    }
}

/// canonicalise an [`OnChainSigner`] to a string matching the
/// `SignerSpec` canonicalisation. Project-level verifier addresses are
/// dropped from the comparison key (see commentary in
/// [`signer_spec_to_canonical_string`]).
fn on_chain_signer_to_canonical_string(s: &OnChainSigner) -> String {
    match s {
        OnChainSigner::Delegated { address } => format!("Delegated:{address}"),
        OnChainSigner::External { public_key_hex, .. } => {
            // without a way to tell Ed25519 vs WebAuthn from the on-chain
            // signer alone (the verifier address is project-specific),
            // surface as "External:<pk_hex>". Comparison strings on the
            // spec side use the typed variant so a mismatch in
            // {Ed25519, WebAuthn} surfaces as drift.
            format!("External:{public_key_hex}")
        }
    }
}

/// resolve a [`PolicySlot`] to the on-chain policy contract address it
/// should appear as. Returns `None` when we cannot determine an address
/// (caller surfaces an `unresolved_slots` drift item).
fn resolve_policy_address(slot: &PolicySlot, network_passphrase: &str) -> Option<String> {
    match slot {
        PolicySlot::Existing { primitive, .. } => {
            // `primitive_address` is `None` for every primitive in v1 —
            // no audited mainnet/testnet deployments are registered yet.
            // we surface this gap rather than silently failing the
            // comparison.
            primitive_address(primitive.clone(), network_passphrase).map(|s| s.to_string())
        }
        PolicySlot::Generated {
            template_family, ..
        } => project_deployed_policy_address(template_family.clone(), network_passphrase)
            .map(|s| s.to_string()),
    }
}

fn unresolved_slot_label(slot: &PolicySlot) -> String {
    match slot {
        PolicySlot::Existing { primitive, .. } => format!("existing:{primitive:?}"),
        PolicySlot::Generated {
            template_family, ..
        } => format!("generated:{}", template_family_label(template_family)),
    }
}

fn template_family_label(t: &TemplateFamily) -> &'static str {
    match t {
        TemplateFamily::FunctionAllowlist => "function_allowlist",
        TemplateFamily::ArgumentPattern => "argument_pattern",
        TemplateFamily::AmountRange => "amount_range",
        TemplateFamily::AssetAllowlist => "asset_allowlist",
        TemplateFamily::TimeWindow => "time_window",
        TemplateFamily::CallFrequency => "call_frequency",
        TemplateFamily::SequenceOrdering => "sequence_ordering",
    }
}

// tests — exercise the decoder + comparator with hand-built ScVal
// fixtures. The simulate path itself is integration-tested through
// `wallet-adapter/src/integration.test.ts`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::TESTNET_PASSPHRASE;
    use oz_policy_core::recording::RECORDING_SCHEMA_URI;
    use oz_policy_core::spec::{
        ContextRuleSpec, ExistingPrimitive, ExistingPrimitiveParams, RecordingRef, SynthesisMode,
        POLICY_SCHEMA_URI,
    };
    use stellar_xdr::curr::{ScBytes, ScMap, ScMapEntry, ScString, ScSymbol, ScVal, ScVec, VecM};

    // ------------------- ScVal fixture builders -----------------------

    fn sym(s: &str) -> ScVal {
        ScVal::Symbol(ScSymbol::try_from(s.as_bytes().to_vec()).unwrap())
    }
    fn s_string(s: &str) -> ScVal {
        ScVal::String(ScString::try_from(s.as_bytes().to_vec()).unwrap())
    }
    fn s_vec(items: Vec<ScVal>) -> ScVal {
        let vm: VecM<ScVal> = items.try_into().unwrap();
        ScVal::Vec(Some(ScVec(vm)))
    }
    fn s_map(entries: Vec<(&str, ScVal)>) -> ScVal {
        let mut sorted: Vec<(&str, ScVal)> = entries;
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        let scme: Vec<ScMapEntry> = sorted
            .into_iter()
            .map(|(k, v)| ScMapEntry {
                key: sym(k),
                val: v,
            })
            .collect();
        let vm: VecM<ScMapEntry> = scme.try_into().unwrap();
        ScVal::Map(Some(ScMap(vm)))
    }
    fn s_addr_c(c_strkey: &str) -> ScVal {
        let c = Contract::from_str(c_strkey).unwrap();
        ScVal::Address(ScAddress::Contract(xdr::ContractId(xdr::Hash(c.0))))
    }
    fn s_addr_g(g_strkey: &str) -> ScVal {
        let pk = Ed25519PublicKey::from_string(g_strkey).unwrap();
        ScVal::Address(ScAddress::Account(xdr::AccountId(
            xdr::PublicKey::PublicKeyTypeEd25519(Uint256(pk.0)),
        )))
    }
    fn s_default_context() -> ScVal {
        s_vec(vec![sym("Default")])
    }
    fn s_signer_delegated(g_strkey: &str) -> ScVal {
        s_vec(vec![sym("Delegated"), s_addr_g(g_strkey)])
    }
    fn s_bytes(b: &[u8]) -> ScVal {
        ScVal::Bytes(ScBytes(b.to_vec().try_into().unwrap()))
    }

    /// build the exact on-chain ScVal the Phase 7 SA returns for bootstrap
    /// rule 0 (captured 2026-05-18 from `stellar contract invoke
    /// --send=no get_context_rule 0`):
    ///
    ///   contextRule {
    ///     id: 0, name: "rule",
    ///     context_type: Default,
    ///     valid_until: None,
    ///     signers: [Delegated(G…)],
    ///     policies: [],
    ///     signer_ids: [0], policy_ids: [],
    ///   }
    fn bootstrap_rule_scval() -> ScVal {
        s_map(vec![
            ("id", ScVal::U32(0)),
            ("name", s_string("rule")),
            ("context_type", s_default_context()),
            ("valid_until", ScVal::Void),
            (
                "signers",
                s_vec(vec![s_signer_delegated(
                    "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ",
                )]),
            ),
            ("policies", s_vec(vec![])),
            ("signer_ids", s_vec(vec![ScVal::U32(0)])),
            ("policy_ids", s_vec(vec![])),
        ])
    }

    fn p7_rule_with_policy_scval() -> ScVal {
        s_map(vec![
            ("id", ScVal::U32(1)),
            ("name", s_string("p7-rule")),
            ("context_type", s_default_context()),
            ("valid_until", ScVal::Void),
            (
                "signers",
                s_vec(vec![s_signer_delegated(
                    "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ",
                )]),
            ),
            (
                "policies",
                s_vec(vec![s_addr_c(
                    "CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR",
                )]),
            ),
            ("signer_ids", s_vec(vec![ScVal::U32(0)])),
            ("policy_ids", s_vec(vec![ScVal::U32(0)])),
        ])
    }

    fn spec_p7() -> PolicySpec {
        PolicySpec {
            schema: POLICY_SCHEMA_URI.to_string(),
            synthesis_mode: SynthesisMode::CodegenOnly,
            context_rule: ContextRuleSpec {
                name: "p7-rule".to_string(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: vec![SignerSpec::Delegated {
                address: "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ".to_string(),
            }],
            policies: vec![PolicySlot::Generated {
                template_family: TemplateFamily::FunctionAllowlist,
                constraints: vec![],
            }],
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: None,
                schema: RECORDING_SCHEMA_URI.to_string(),
            },
        }
    }

    // -------------------------- decode tests ---------------------------

    #[test]
    fn decode_bootstrap_rule_matches_live_capture() {
        let rule = decode_context_rule_scval(&bootstrap_rule_scval()).expect("decode");
        assert_eq!(rule.id, 0);
        assert_eq!(rule.name, "rule");
        assert!(matches!(rule.context_type, OnChainContextType::Default));
        assert_eq!(rule.valid_until, None);
        assert_eq!(rule.signers.len(), 1);
        match &rule.signers[0] {
            OnChainSigner::Delegated { address } => {
                assert_eq!(
                    address,
                    "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ"
                );
            }
            other => panic!("expected Delegated, got {:?}", other),
        }
        assert!(rule.policies.is_empty());
    }

    #[test]
    fn decode_p7_rule_with_policy() {
        let rule = decode_context_rule_scval(&p7_rule_with_policy_scval()).expect("decode");
        assert_eq!(rule.id, 1);
        assert_eq!(rule.name, "p7-rule");
        assert_eq!(rule.policies.len(), 1);
        assert_eq!(
            rule.policies[0],
            "CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR"
        );
    }

    #[test]
    fn decode_rejects_non_map_scval() {
        let v = ScVal::U32(0);
        let err = decode_context_rule_scval(&v).expect_err("must reject");
        assert!(err.contains("expected ScVal::Map"));
    }

    #[test]
    fn decode_rejects_missing_required_field() {
        // omit `name` — decoder must surface a "missing field" diagnostic.
        let v = s_map(vec![
            ("id", ScVal::U32(0)),
            ("context_type", s_default_context()),
            ("valid_until", ScVal::Void),
            ("signers", s_vec(vec![])),
            ("policies", s_vec(vec![])),
            ("signer_ids", s_vec(vec![])),
            ("policy_ids", s_vec(vec![])),
        ]);
        let err = decode_context_rule_scval(&v).expect_err("must reject");
        assert!(err.contains("name"));
    }

    #[test]
    fn decode_external_signer() {
        let verifier = "CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR";
        let pk = vec![0xABu8; 32];
        let v = s_vec(vec![sym("External"), s_addr_c(verifier), s_bytes(&pk)]);
        let signer = decode_signer(&v).expect("decode");
        match signer {
            OnChainSigner::External {
                verifier: vfr,
                public_key_hex,
            } => {
                assert_eq!(vfr, verifier);
                assert_eq!(public_key_hex, hex::encode(&pk));
            }
            other => panic!("expected External, got {:?}", other),
        }
    }

    #[test]
    fn decode_call_contract_context_type() {
        let target = "CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR";
        let v = s_vec(vec![sym("CallContract"), s_addr_c(target)]);
        let ct = decode_context_type(&v).expect("decode");
        match ct {
            OnChainContextType::CallContract { address } => assert_eq!(address, target),
            other => panic!("expected CallContract, got {:?}", other),
        }
    }

    // --------------------------- drift tests ---------------------------

    #[test]
    fn compute_drift_clean_match() {
        let actual = decode_context_rule_scval(&p7_rule_with_policy_scval()).unwrap();
        let drift = compute_drift(&spec_p7(), "p7-rule", &actual, TESTNET_PASSPHRASE);
        assert!(
            drift.is_empty(),
            "expected no drift on matching pair; got {drift:#?}"
        );
    }

    #[test]
    fn compute_drift_name_mismatch() {
        let actual = decode_context_rule_scval(&p7_rule_with_policy_scval()).unwrap();
        let drift = compute_drift(&spec_p7(), "wrong-name", &actual, TESTNET_PASSPHRASE);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].field, "context_rule.name");
    }

    #[test]
    fn compute_drift_signer_set_mismatch() {
        let mut actual = decode_context_rule_scval(&p7_rule_with_policy_scval()).unwrap();
        // replace the on-chain signer set with a different G-key.
        actual.signers = vec![OnChainSigner::Delegated {
            address: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string(),
        }];
        let drift = compute_drift(&spec_p7(), "p7-rule", &actual, TESTNET_PASSPHRASE);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].field, "signers");
    }

    #[test]
    fn compute_drift_policies_address_set_mismatch() {
        let mut actual = decode_context_rule_scval(&p7_rule_with_policy_scval()).unwrap();
        // remove the on-chain policy; spec still references one.
        actual.policies.clear();
        let drift = compute_drift(&spec_p7(), "p7-rule", &actual, TESTNET_PASSPHRASE);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].field, "policies");
    }

    #[test]
    fn compute_drift_unresolved_existing_slot() {
        // existingPrimitive::SimpleThreshold has no registry entry on
        // testnet — surfacing it must produce an `unresolved_slots`
        // drift item plus a `policies` mismatch.
        let mut spec = spec_p7();
        spec.policies = vec![PolicySlot::Existing {
            primitive: ExistingPrimitive::SimpleThreshold,
            params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
        }];
        let actual = decode_context_rule_scval(&p7_rule_with_policy_scval()).unwrap();
        let drift = compute_drift(&spec, "p7-rule", &actual, TESTNET_PASSPHRASE);
        let fields: Vec<_> = drift.iter().map(|d| d.field.as_str()).collect();
        assert!(
            fields.contains(&"policies.unresolved_slots"),
            "expected unresolved_slots drift; got {fields:?}"
        );
    }

    #[test]
    fn compute_drift_valid_until_mismatch() {
        let actual = decode_context_rule_scval(&p7_rule_with_policy_scval()).unwrap();
        let mut spec = spec_p7();
        spec.context_rule.valid_until = Some(1_000_000);
        let drift = compute_drift(&spec, "p7-rule", &actual, TESTNET_PASSPHRASE);
        let fields: Vec<_> = drift.iter().map(|d| d.field.as_str()).collect();
        assert!(fields.contains(&"context_rule.valid_until"));
    }
}
