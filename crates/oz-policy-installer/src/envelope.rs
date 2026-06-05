//! install-envelope builder. preflight → resolve registry → fetch source seq →
//! build `add_context_rule` op → simulate → assemble → return unsigned envelope.
//! v1 bundles all Existing slots into the add_context_rule map (single op).

use oz_policy_core::spec::TemplateFamily;
use oz_policy_core::spec::{
    ContextType, ExistingPrimitive, ExistingPrimitiveParams, PolicySlot, PolicySpec, SignerSpec,
    WeightedSigner,
};
use oz_policy_core::Error;
use std::str::FromStr;
use std::time::Duration;
use stellar_rpc_client::Client;
use stellar_xdr::curr::{
    self as xdr, HostFunction, Int128Parts, InvokeContractArgs, InvokeHostFunctionOp, Limits, Memo,
    MuxedAccount, Operation, OperationBody, Preconditions, ReadXdr, ScAddress, ScMap, ScMapEntry,
    ScSymbol, ScVal, ScVec, SequenceNumber, SorobanAuthorizationEntry, SorobanTransactionData,
    Transaction, TransactionEnvelope, TransactionExt, TransactionV1Envelope, Uint256, VecM,
    WriteXdr,
};
use tokio::time::timeout;

use crate::preflight::{self, AccountRevision};
use crate::registry;

/// hard timeout on every rpc await.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// per-op inclusion fee, on top of simulator's min_resource_fee.
const INCLUSION_FEE_STROOPS: u32 = 100;

/// result of a successful `build_install_envelope`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeArtifact {
    /// base64-encoded `TransactionEnvelope` XDR ready for wallet
    /// `signTransaction`. **Do not submit directly** — sign first.
    pub envelope_xdr_base64: String,
    /// `simulateTransaction.minResourceFee`. Useful for diagnostics and
    /// for the CLI to show the user before they sign.
    pub min_resource_fee: i64,
    /// number of `InvokeHostFunction` operations the envelope contains.
    /// always `>= 1` (the `add_context_rule` call); see the module
    /// doc-comment for the v1 invariant that this is exactly `1`.
    pub host_function_count: u32,
}

/// build an unsigned install-envelope XDR for the given `PolicySpec`.
/// see the module doc-comment for the full pipeline and the honest
/// deviations from the task spec.
///
/// **This function never submits.** It returns base64 XDR only.
#[tracing::instrument(
    skip_all,
    fields(
        rpc_url = %rpc_url,
        network_passphrase = %network_passphrase,
        smart_account = %smart_account,
        source_account = %source_account,
        policy_count = spec.policies.len(),
        signer_count = spec.signers.len(),
    )
)]
pub async fn build_install_envelope(
    spec: &PolicySpec,
    smart_account: &str,
    source_account: &str,
    network_passphrase: &str,
    rpc_url: &str,
    account_revision: AccountRevision,
) -> Result<EnvelopeArtifact, Error> {
    // 1. pure-logic preflight — surface errors before any rpc round-trip.
    preflight::check(
        spec,
        smart_account,
        source_account,
        network_passphrase,
        rpc_url,
        account_revision,
    )?;

    // 2. open the rpc client and verify network passphrase.
    let client = Client::new(rpc_url).map_err(|e| {
        Error::InstallPreflightFailed(format!("rpc client init failed for {rpc_url}: {e}"))
    })?;
    verify_network_match(&client, rpc_url, network_passphrase).await?;

    // 3. resolve registry addresses for every slot.
    let mut policies_map_entries: Vec<ScMapEntry> = Vec::with_capacity(spec.policies.len());
    for slot in &spec.policies {
        let (addr_str, install_param): (&str, ScVal) = match slot {
            PolicySlot::Existing { primitive, params } => {
                let addr = registry::primitive_address(primitive.clone(), network_passphrase)
                    .ok_or_else(|| {
                        Error::InstallPreflightFailed(format!(
                            "primitive_address_unknown for {primitive:?} on \
                                 {network_passphrase}"
                        ))
                    })?;
                let param = encode_install_param(primitive, params)?;
                (addr, param)
            }
            PolicySlot::Generated {
                template_family, ..
            } => {
                let addr = registry::project_deployed_policy_address(
                    template_family.clone(),
                    network_passphrase,
                )
                .ok_or_else(|| {
                    Error::InstallPreflightFailed(format!(
                        "generated_policy_address_unknown for {template_family:?} on \
                         {network_passphrase} — deploy the contract first (see \
                         crates/oz-policy-installer/src/registry.rs::\
                         project_deployed_policy_address)"
                    ))
                })?;
                let param = encode_generated_install_param(template_family)?;
                (addr, param)
            }
        };
        let primitive_addr = ScAddress::from_str(addr_str).map_err(|e| {
            Error::InstallPreflightFailed(format!(
                "registry returned an invalid C-address {addr_str}: {e}"
            ))
        })?;
        policies_map_entries.push(ScMapEntry {
            key: ScVal::Address(primitive_addr),
            val: install_param,
        });
    }
    // sort by canonical xdr — required by soroban's map ordering.
    policies_map_entries
        .sort_by_cached_key(|entry| entry.key.to_xdr(Limits::none()).unwrap_or_default());

    // 4. fetch source account sequence number.
    let source_seq = fetch_source_seq(&client, source_account, rpc_url).await?;

    // 5. build the InvokeHostFunction(add_context_rule) op.
    let smart_account_address = ScAddress::from_str(smart_account).map_err(|e| {
        // preflight already rejected non-Cxxx strkeys; reach here only
        // on internal logic errors.
        Error::InstallPreflightFailed(format!("smart_account not a valid C-address: {e}"))
    })?;

    let add_context_rule_args =
        build_add_context_rule_args(spec, &smart_account_address, policies_map_entries)?;

    let invoke_op = InvokeHostFunctionOp {
        host_function: HostFunction::InvokeContract(InvokeContractArgs {
            contract_address: smart_account_address,
            function_name: scsymbol("add_context_rule")?,
            args: add_context_rule_args.try_into().map_err(|e| {
                Error::InstallPreflightFailed(format!(
                    "add_context_rule arg vector exceeded XDR VecM limit: {e}"
                ))
            })?,
        }),
        // simulation fills the auth tree in `record` mode.
        auth: VecM::<SorobanAuthorizationEntry>::default(),
    };

    let op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(invoke_op),
    };
    let ops: VecM<Operation, 100> = vec![op].try_into().map_err(|e| {
        Error::InstallPreflightFailed(format!("operation vector encode failed: {e}"))
    })?;

    // 6. Wrap into a Transaction skeleton (pre-simulate, fee=0,
    //    ext=V0). The simulator will give us back the resources we need.
    let source_muxed = muxed_account_from_g_strkey(source_account)?;
    let tx_skeleton = Transaction {
        source_account: source_muxed.clone(),
        // fee is replaced after simulation; 0 is acceptable for the
        // simulate call itself.
        fee: 0,
        seq_num: SequenceNumber(source_seq + 1),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: ops.clone(),
        ext: TransactionExt::V0,
    };
    let skeleton_envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: tx_skeleton,
        signatures: VecM::default(),
    });

    // 7. Simulate to fetch transactionData + auth + min_resource_fee.
    let sim = timeout(
        RPC_TIMEOUT,
        client.simulate_transaction_envelope(&skeleton_envelope, None),
    )
    .await
    .map_err(|_elapsed| {
        Error::RecorderSimFailed(format!(
            "rpc timeout after 30s while simulating install envelope: {rpc_url}"
        ))
    })?
    .map_err(|e| {
        Error::RecorderSimFailed(format!("simulateTransaction for install envelope: {e}"))
    })?;

    if let Some(err) = &sim.error {
        return Err(Error::RecorderSimFailed(format!(
            "simulateTransaction reported error: {err}"
        )));
    }
    if sim.transaction_data.is_empty() {
        return Err(Error::RecorderSimFailed(
            "simulateTransaction returned no transactionData for install envelope".to_string(),
        ));
    }

    let txn_data = sim
        .transaction_data()
        .map_err(|e| Error::RecorderSimFailed(format!("transactionData decode failed: {e}")))?;

    // pull auth entries the host generated in record mode and stitch
    // them into the operation. There is exactly one
    // `InvokeHostFunction` op in v1 so there is exactly one
    // results[0].auth vector to consume.
    let sim_results = sim
        .results()
        .map_err(|e| Error::RecorderSimFailed(format!("simulation results decode: {e}")))?;
    let auth_entries: Vec<SorobanAuthorizationEntry> = sim_results
        .first()
        .map(|r| r.auth.clone())
        .unwrap_or_default();
    let auth_vecm: VecM<SorobanAuthorizationEntry> = auth_entries.try_into().map_err(|e| {
        Error::InstallPreflightFailed(format!("auth-tree vector encode failed: {e}"))
    })?;

    // re-clone the op with auth filled in.
    let invoke_op_with_auth = match &ops.as_slice()[0].body {
        OperationBody::InvokeHostFunction(ih) => InvokeHostFunctionOp {
            host_function: ih.host_function.clone(),
            auth: auth_vecm,
        },
        _ => unreachable!("only InvokeHostFunction ops are constructed"),
    };
    let assembled_op = Operation {
        source_account: None,
        body: OperationBody::InvokeHostFunction(invoke_op_with_auth),
    };
    let assembled_ops: VecM<Operation, 100> = vec![assembled_op].try_into().map_err(|e| {
        Error::InstallPreflightFailed(format!("assembled op vector encode failed: {e}"))
    })?;

    // assembleTransaction: fee = inclusion_fee + min_resource_fee; ext = V1(txn_data).
    let total_fee = i64::from(INCLUSION_FEE_STROOPS).saturating_add(txn_data.resource_fee);
    let total_fee_u32: u32 = u32::try_from(total_fee).map_err(|_| {
        Error::InstallPreflightFailed(format!(
            "assembled fee {total_fee} exceeds u32 max; refusing to emit"
        ))
    })?;

    let assembled_tx = Transaction {
        source_account: source_muxed,
        fee: total_fee_u32,
        seq_num: SequenceNumber(source_seq + 1),
        cond: Preconditions::None,
        memo: Memo::None,
        operations: assembled_ops.clone(),
        ext: TransactionExt::V1(SorobanTransactionData {
            ext: txn_data.ext.clone(),
            resources: txn_data.resources.clone(),
            resource_fee: txn_data.resource_fee,
        }),
    };
    let assembled_envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
        tx: assembled_tx,
        signatures: VecM::default(),
    });

    let envelope_xdr_base64 = assembled_envelope
        .to_xdr_base64(Limits::none())
        .map_err(|e| {
            Error::InstallPreflightFailed(format!("envelope encode to xdr base64 failed: {e}"))
        })?;

    // sanity: the encoded envelope must round-trip through ReadXdr.
    let _ = TransactionEnvelope::from_xdr_base64(&envelope_xdr_base64, Limits::none()).map_err(
        |e| {
            Error::InstallPreflightFailed(format!(
                "assembled envelope failed round-trip decode (will reject in wallet): {e}"
            ))
        },
    )?;

    let host_function_count: u32 = u32::try_from(assembled_ops.len()).unwrap_or(u32::MAX);
    let min_resource_fee: i64 = sim.min_resource_fee.try_into().unwrap_or(i64::MAX);

    tracing::info!(
        host_function_count,
        min_resource_fee,
        envelope_b64_len = envelope_xdr_base64.len(),
        "build_install_envelope completed"
    );

    Ok(EnvelopeArtifact {
        envelope_xdr_base64,
        min_resource_fee,
        host_function_count,
    })
}

// helpers

/// mirror of the recorder's `verify_network_match`. We do not import it
/// (recorder is frozen) so we re-implement to keep crate-boundary
/// independence.
async fn verify_network_match(
    client: &Client,
    rpc_url: &str,
    asserted_passphrase: &str,
) -> Result<(), Error> {
    let net = timeout(RPC_TIMEOUT, client.get_network())
        .await
        .map_err(|_elapsed| {
            Error::InstallPreflightFailed(format!("rpc timeout after 30s on getNetwork: {rpc_url}"))
        })?
        .map_err(|e| Error::InstallPreflightFailed(format!("getNetwork({rpc_url}) failed: {e}")))?;
    if net.passphrase != asserted_passphrase {
        return Err(Error::InstallPreflightFailed(format!(
            "network mismatch: rpc reports '{}' but caller asserted '{}'",
            net.passphrase, asserted_passphrase
        )));
    }
    Ok(())
}

/// pull the `seq_num` for `source_account` via `getLedgerEntries` (which
/// is what `client.get_account` does under the hood). We don't use
/// `client.get_account` directly because it would re-encode the strkey
/// for a check we already did in preflight.
async fn fetch_source_seq(
    client: &Client,
    source_account: &str,
    rpc_url: &str,
) -> Result<i64, Error> {
    let entry = timeout(RPC_TIMEOUT, client.get_account(source_account))
        .await
        .map_err(|_elapsed| {
            Error::InstallPreflightFailed(format!(
                "rpc timeout after 30s on getLedgerEntries(source_account): {rpc_url}"
            ))
        })?
        .map_err(|e| {
            Error::InstallPreflightFailed(format!(
                "getLedgerEntries(source_account = {source_account}) failed: {e}"
            ))
        })?;
    Ok(entry.seq_num.0)
}

/// build a `MuxedAccount::Ed25519(...)` from a `G…` StrKey. The caller has
/// already validated the strkey shape via preflight.
fn muxed_account_from_g_strkey(g_strkey: &str) -> Result<MuxedAccount, Error> {
    let pk = stellar_strkey::ed25519::PublicKey::from_string(g_strkey).map_err(|e| {
        Error::InstallPreflightFailed(format!(
            "source_account {g_strkey} failed strkey decode (should have been caught \
             in preflight): {e}"
        ))
    })?;
    Ok(MuxedAccount::Ed25519(Uint256(pk.0)))
}

/// build the positional argument list for `add_context_rule(context_type,
/// name, valid_until, signers, policies)`.
fn build_add_context_rule_args(
    spec: &PolicySpec,
    _smart_account_address: &ScAddress,
    policies_map_entries: Vec<ScMapEntry>,
) -> Result<Vec<ScVal>, Error> {
    let context_type = encode_context_type(&spec.context_rule.context_type)?;
    let name = encode_string(&spec.context_rule.name)?;
    let valid_until = encode_option_u32(spec.context_rule.valid_until);
    let signers = encode_signers(&spec.signers)?;
    let policies_entries: VecM<ScMapEntry> = policies_map_entries.try_into().map_err(|e| {
        Error::InstallPreflightFailed(format!("policies map exceeded XDR VecM limit: {e}"))
    })?;
    let policies = ScVal::Map(Some(ScMap(policies_entries)));
    Ok(vec![context_type, name, valid_until, signers, policies])
}

/// encode `ContextRuleType::{Default, CallContract(Address)}` as an
/// scVal. Soroban `#[contracttype]` enum encoding is
/// `ScVal::Vec([Symbol(variant), args...])` (the soroban-sdk
/// `Val::from_contracttype` helper does exactly this).
fn encode_context_type(c: &ContextType) -> Result<ScVal, Error> {
    Ok(match c {
        ContextType::Default => {
            let tag = ScVal::Symbol(scsymbol("Default")?);
            ScVal::Vec(Some(ScVec(vec![tag].try_into().map_err(|e| {
                Error::InstallPreflightFailed(format!("Default tag vec encode failed: {e}"))
            })?)))
        }
        ContextType::CallContract { address } => {
            let tag = ScVal::Symbol(scsymbol("CallContract")?);
            let addr = ScAddress::from_str(address).map_err(|e| {
                Error::InstallPreflightFailed(format!(
                    "CallContract target {address} is not a valid C-address: {e}"
                ))
            })?;
            let arg = ScVal::Address(addr);
            ScVal::Vec(Some(ScVec(vec![tag, arg].try_into().map_err(|e| {
                Error::InstallPreflightFailed(format!(
                    "CallContract variant vec encode failed: {e}"
                ))
            })?)))
        }
    })
}

/// encode a Rust `String` as an `ScVal::String`. The host enforces
/// MAX_NAME_SIZE separately; we honour that in preflight.
fn encode_string(s: &str) -> Result<ScVal, Error> {
    let scs = xdr::ScString::try_from(s.as_bytes().to_vec())
        .map_err(|e| Error::InstallPreflightFailed(format!("string encode failed: {e}")))?;
    Ok(ScVal::String(scs))
}

/// encode `Option<u32>` as the soroban-sdk `Option::None`/`Option::Some`
/// `#[contracttype]` shape (`Void` / `Vec([U32])`).
fn encode_option_u32(o: Option<u32>) -> ScVal {
    match o {
        None => ScVal::Void,
        Some(v) => ScVal::U32(v),
    }
}

/// encode the signers vector. Each `SignerSpec` becomes a Soroban
/// `Signer` `#[contracttype]` enum value:
/// * `External(Address verifier, Bytes pubkey)`
/// * `Delegated(Address)`
///
/// per `docs/oz-internal-shapes.md` §10, `External` carries a
/// verifier-contract address plus the raw key bytes. The verifier
/// address selection (ed25519 vs webauthn verifier) is a network-level
/// decision that lives in the registry. The v1 registry has no
/// addresses, so we surface the same `primitive_address_unknown`-flavour
/// error here for external signers.
fn encode_signers(signers: &[SignerSpec]) -> Result<ScVal, Error> {
    let mut vec_inner: Vec<ScVal> = Vec::with_capacity(signers.len());
    for signer in signers {
        let v = match signer {
            SignerSpec::Delegated { address } => {
                let tag = ScVal::Symbol(scsymbol("Delegated")?);
                let addr = ScAddress::from_str(address).map_err(|e| {
                    Error::InstallPreflightFailed(format!(
                        "Delegated signer address {address} invalid: {e}"
                    ))
                })?;
                ScVal::Vec(Some(ScVec(
                    vec![tag, ScVal::Address(addr)].try_into().map_err(|e| {
                        Error::InstallPreflightFailed(format!(
                            "Delegated signer vec encode failed: {e}"
                        ))
                    })?,
                )))
            }
            SignerSpec::ExternalEd25519 { public_key_hex }
            | SignerSpec::ExternalWebAuthn { public_key_hex } => {
                return Err(Error::InstallPreflightFailed(format!(
                    "external signer encoding requires a verifier contract address; the v1 \
                     registry has none for this network — see crates/oz-policy-installer/\
                     src/registry.rs. signer pubkey: 0x{}…",
                    &public_key_hex[..public_key_hex.len().min(8)]
                )));
            }
        };
        vec_inner.push(v);
    }
    let vecm: VecM<ScVal> = vec_inner
        .try_into()
        .map_err(|e| Error::InstallPreflightFailed(format!("signers vector encode failed: {e}")))?;
    Ok(ScVal::Vec(Some(ScVec(vecm))))
}

/// encode `*AccountParams` for a given primitive. The on-chain types
/// (`SimpleThresholdAccountParams`, etc.) are `#[contracttype]` structs
/// whose ScVal encoding is `ScVal::Map([{key: Symbol(field), val:
/// <value>}])` with keys in declaration order.
fn encode_install_param(
    primitive: &ExistingPrimitive,
    params: &ExistingPrimitiveParams,
) -> Result<ScVal, Error> {
    match (primitive, params) {
        (
            ExistingPrimitive::SimpleThreshold,
            ExistingPrimitiveParams::SimpleThreshold { threshold },
        ) => struct_map(vec![("threshold", ScVal::U32(*threshold))]),
        (
            ExistingPrimitive::WeightedThreshold,
            ExistingPrimitiveParams::WeightedThreshold { weights, threshold },
        ) => {
            let weight_entries = encode_signer_weights(weights)?;
            struct_map(vec![
                ("signer_weights", weight_entries),
                ("threshold", ScVal::U32(*threshold)),
            ])
        }
        (
            ExistingPrimitive::SpendingLimit,
            ExistingPrimitiveParams::SpendingLimit {
                period_ledgers,
                limit_stroops_string,
            },
        ) => {
            let limit_i128: i128 = limit_stroops_string.parse().map_err(|e| {
                Error::InstallPreflightFailed(format!(
                    "spending_limit.limit_stroops_string '{limit_stroops_string}' \
                     not parseable as i128: {e}"
                ))
            })?;
            let parts = Int128Parts {
                hi: (limit_i128 >> 64) as i64,
                lo: limit_i128 as u64,
            };
            struct_map(vec![
                ("spending_limit", ScVal::I128(parts)),
                ("period_ledgers", ScVal::U32(*period_ledgers)),
            ])
        }
        // the mismatched (primitive, params) variants are caught at
        // `PolicySpec` construction time (decision tree never emits
        // them), but we still need a total match.
        _ => Err(Error::InstallPreflightFailed(format!(
            "primitive/params mismatch: {primitive:?} with params {params:?}"
        ))),
    }
}

/// encode the install param for a Phase-3-generated policy contract.
/// mirrors the source rendered by `oz-policy-codegen` (see
/// `walkthroughs/phase3-codegen-fixture/expected/slot_0/source.rs`):
///
/// ```ignore
/// #[contracttype]
/// pub struct InstallParams {
///     pub _marker: u32,
/// }
/// ```
///
/// always `{ _marker: 0 }` in v1 — the codegen pipeline does not emit any
/// installer-time configuration today (the constraint values are baked into
/// the WASM at codegen time, not passed at install). Future-enhancement
/// note inside the source.rs explicitly calls out per-rule installer-time
/// overrides as "future work"; until they land, every generated policy
/// installs with the same `_marker: 0` shape.
///
/// soroban encodes `#[contracttype]` structs as
/// `ScVal::Map([{Symbol(field), value}])`, same as
/// `encode_install_param` does for the OZ primitive structs.
fn encode_generated_install_param(_template: &TemplateFamily) -> Result<ScVal, Error> {
    struct_map(vec![("_marker", ScVal::U32(0))])
}

fn encode_signer_weights(weights: &[WeightedSigner]) -> Result<ScVal, Error> {
    let mut entries: Vec<ScMapEntry> = Vec::with_capacity(weights.len());
    for w in weights {
        // encode signer as ScVal (re-use the single-signer encoder by
        // wrapping in a one-element slice).
        let signer_scval = match encode_signers(std::slice::from_ref(&w.signer))? {
            ScVal::Vec(Some(ScVec(v))) => v.as_slice().first().cloned().ok_or_else(|| {
                Error::InstallPreflightFailed("signer encoding returned empty vec".to_string())
            })?,
            _ => unreachable!("encode_signers always returns ScVal::Vec(Some(_))"),
        };
        entries.push(ScMapEntry {
            key: signer_scval,
            val: ScVal::U32(w.weight),
        });
    }
    // canonical map ordering — same rationale as the outer policies map.
    entries.sort_by_cached_key(|e| e.key.to_xdr(Limits::none()).unwrap_or_default());
    let vecm: VecM<ScMapEntry> = entries.try_into().map_err(|e| {
        Error::InstallPreflightFailed(format!("signer_weights map encode failed: {e}"))
    })?;
    Ok(ScVal::Map(Some(ScMap(vecm))))
}

/// encode a `#[contracttype]` struct as `ScVal::Map([{Symbol(field),
/// value}, ...])`. Soroban requires the map keys to be sorted by their
/// host `Val::cmp`; for `Symbol` keys that is byte-lex over the symbol
/// payload, which matches Rust's `String` `Ord`. We sort to be safe.
fn struct_map(fields: Vec<(&str, ScVal)>) -> Result<ScVal, Error> {
    let mut entries: Vec<ScMapEntry> = Vec::with_capacity(fields.len());
    for (k, v) in fields {
        entries.push(ScMapEntry {
            key: ScVal::Symbol(scsymbol(k)?),
            val: v,
        });
    }
    // soroban canonical map ordering: keys sorted ascending. Symbol
    // keys compare as their UTF-8 bytes.
    entries.sort_by(|a, b| {
        let ak = match &a.key {
            ScVal::Symbol(s) => s.0.as_slice(),
            _ => &[],
        };
        let bk = match &b.key {
            ScVal::Symbol(s) => s.0.as_slice(),
            _ => &[],
        };
        ak.cmp(bk)
    });
    let vecm: VecM<ScMapEntry> = entries
        .try_into()
        .map_err(|e| Error::InstallPreflightFailed(format!("struct map encode failed: {e}")))?;
    Ok(ScVal::Map(Some(ScMap(vecm))))
}

/// build an `ScSymbol` from a Rust string. Symbols are capped at 32
/// bytes by Soroban; anything longer is rejected at the host level —
/// we surface the error early.
fn scsymbol(s: &str) -> Result<ScSymbol, Error> {
    ScSymbol::try_from(s.as_bytes().to_vec()).map_err(|e| {
        Error::InstallPreflightFailed(format!("symbol '{s}' rejected by XDR encoder: {e}"))
    })
}

// tests

#[cfg(test)]
mod tests {
    /// confirm the production code never *calls* the stellar-rpc-client
    /// submit / send transaction surfaces. The task contract is "no
    /// auto-submit"; this is the binary check.
    ///
    /// we grep the on-disk source rather than the compiled artifact so
    /// the test is independent of optimisation / dead-code elimination.
    /// to avoid the test tripping on itself, the search needles are
    /// constructed at runtime via `format!` rather than typed as
    /// literals (see the `needles` array body for the construction).
    #[test]
    fn envelope_module_does_not_auto_submit() {
        let src = include_str!("envelope.rs");
        // build the search needles at runtime by concatenation so the
        // bytes never appear verbatim in this source file (otherwise
        // the test would trip on itself). This is the only auto-submit
        // surface stellar-rpc-client exposes.
        let needles = [
            format!(".{}_transaction(", "send"),
            format!(".{}_transaction_polling(", "send"),
            format!(".{}_transaction(", "submit"),
            format!("Send{}Response", "Transaction"),
        ];
        for needle in &needles {
            assert!(
                !src.contains(needle),
                "envelope.rs must not call `{needle}` (auto-submit guard)"
            );
        }
    }
}
