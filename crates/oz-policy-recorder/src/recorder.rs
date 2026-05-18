//! RPC + XDR decode pipeline.
//!
//! Two public entrypoints:
//!
//! * [`record_by_hash`] — fetch an already-included transaction via
//!   `getTransaction` and build a `Recording` from `envelope_xdr` +
//!   `result_meta_xdr`.
//! * [`record_by_simulation`] — POST a base64 `TransactionEnvelope` to
//!   `simulateTransaction` and build a `Recording` from the simulation
//!   response (`results[0].auth`, `events`, `state_changes`).
//!
//! Both share the internal walker [`decode_from_xdr_blobs`], which is also
//! used by the integration tests (no network) to exercise the decode logic
//! against committed XDR fixtures.

use oz_policy_core::Error;
use sha2::Digest;
use std::time::Duration;
use stellar_rpc_client::{Client, LedgerEntryChange as RpcLedgerEntryChange};
use stellar_xdr::curr::{
    self as xdr, ContractEventBody, ContractEventType, ContractId, Hash, HostFunction, Int128Parts,
    Int256Parts, LedgerEntryChange, LedgerEntryChanges, LedgerEntryData, LedgerKey, Limits,
    OperationBody, ReadXdr, ScAddress, ScError, ScErrorCode, ScVal, SorobanAuthorizationEntry,
    SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials,
    TransactionEnvelope, TransactionMeta, UInt128Parts, UInt256Parts, WriteXdr,
};
use tokio::time::timeout;

use crate::recording::{
    ArgValue, AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
    IngestSource, MapEntry, Recording, StateDelta, TypedEvent, RECORDING_SCHEMA_URI,
};

/// Hard cap on every RPC `await` so a hung endpoint never blocks the recorder
/// indefinitely. 30s is a deliberate ceiling: longer than a healthy testnet
/// `getTransaction` round-trip (typ. < 1s) and longer than a fresh `simulate`
/// against a non-trivial envelope (typ. < 5s), but short enough that a stuck
/// CI never wedges past the test timeout. Phase 5 (MCP) is expected to layer
/// retries on top; we do not retry here.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Public entrypoints
// ---------------------------------------------------------------------------

/// Ask the RPC endpoint which network it serves and compare against the
/// passphrase the caller asserted. Without this guard, a user can point
/// `--rpc <mainnet-url> --network "Test SDF Network ; September 2015"` and
/// get mainnet data labelled as testnet in the Recording, which would
/// corrupt downstream policy decisions silently.
///
/// Wrapped in the same `RPC_TIMEOUT` as every other RPC await so a stuck
/// endpoint fails fast instead of hanging here.
async fn verify_network_match(
    client: &Client,
    rpc_url: &str,
    asserted_passphrase: &str,
) -> Result<(), Error> {
    let net = timeout(RPC_TIMEOUT, client.get_network())
        .await
        .map_err(|_elapsed| Error::RecorderSimFailed(format!("rpc timeout after 30s: {rpc_url}")))?
        .map_err(|e| Error::RecorderSimFailed(format!("getNetwork({rpc_url}) failed: {e}")))?;
    if net.passphrase != asserted_passphrase {
        return Err(Error::RecorderSimFailed(format!(
            "network mismatch: rpc {rpc_url} reports passphrase '{}' but caller asserted '{}'",
            net.passphrase, asserted_passphrase
        )));
    }
    tracing::debug!(
        passphrase = %net.passphrase,
        protocol_version = net.protocol_version,
        "verified RPC network passphrase"
    );
    Ok(())
}

/// Fetch a confirmed transaction by hash and produce a typed [`Recording`].
///
/// * If `getTransaction` returns `NOT_FOUND` (retention exceeded or wrong
///   network), surfaces [`Error::RecorderHashNotFound`].
/// * If status is `FAILED`, we still decode and emit the `Recording` so the
///   caller can analyse the failed invocation.
/// * Any RPC transport error maps to [`Error::RecorderSimFailed`] with the
///   underlying reason in the payload.
/// * Any XDR decode failure maps to [`Error::RecorderXdrDecodeFailed`].
#[tracing::instrument(skip_all, fields(rpc_url = %rpc_url, hash = %hash, network_passphrase = %network_passphrase))]
pub async fn record_by_hash(
    rpc_url: &str,
    network_passphrase: &str,
    hash: &str,
) -> Result<Recording, Error> {
    let client = Client::new(rpc_url)
        .map_err(|e| Error::RecorderSimFailed(format!("rpc client init failed: {e}")))?;
    verify_network_match(&client, rpc_url, network_passphrase).await?;
    let hash_bytes: Hash = hash.parse().map_err(|e: xdr::Error| {
        Error::RecorderHashNotFound(format!("invalid hash {hash}: {e}"))
    })?;

    let resp = timeout(RPC_TIMEOUT, client.get_transaction(&hash_bytes))
        .await
        .map_err(|_elapsed| Error::RecorderSimFailed(format!("rpc timeout after 30s: {rpc_url}")))?
        .map_err(|e| {
            let msg = format!("{e}");
            // The RPC client maps the JSON-RPC `tx not found` path to a transport
            // error rather than its own variant; we distinguish by string match
            // because there is no typed sentinel.
            if msg.contains("NOT_FOUND") || msg.contains("transaction not found") {
                Error::RecorderHashNotFound(format!("hash {hash}: {msg}"))
            } else {
                Error::RecorderSimFailed(format!("getTransaction({hash}) failed: {msg}"))
            }
        })?;

    if resp.status == "NOT_FOUND" {
        return Err(Error::RecorderHashNotFound(format!(
            "hash {hash}: status NOT_FOUND (retention exceeded or wrong network)"
        )));
    }

    // Re-encode the parsed envelope/result_meta to base64 so the same walker
    // path used by the test fixtures handles them. This keeps a single code
    // path responsible for decoding (`decode_from_xdr_blobs`).
    let envelope_xdr_b64 = match &resp.envelope {
        Some(e) => e
            .to_xdr_base64(Limits::none())
            .map_err(|e| Error::RecorderXdrDecodeFailed(format!("re-encode envelope: {e}")))?,
        None => {
            return Err(Error::RecorderSimFailed(format!(
                "getTransaction({hash}) returned no envelope (status={})",
                resp.status
            )))
        }
    };
    let meta_xdr_b64 = match &resp.result_meta {
        Some(m) => m
            .to_xdr_base64(Limits::none())
            .map_err(|e| Error::RecorderXdrDecodeFailed(format!("re-encode result_meta: {e}")))?,
        None => String::new(),
    };

    let mut rec = decode_from_xdr_blobs(&envelope_xdr_b64, &meta_xdr_b64, network_passphrase)?;
    rec.ingest = IngestSource::Hash {
        hash: hash.to_string(),
    };
    rec.ledger = resp.ledger;
    tracing::info!(
        contract_count = rec.contracts.len(),
        event_count = rec.events.len(),
        auth_root_count = rec.auth_tree.roots.len(),
        state_change_count = rec.state_changes.len(),
        ledger = ?rec.ledger,
        "record_by_hash completed"
    );
    Ok(rec)
}

/// Run `simulateTransaction` on a base64 envelope XDR and produce a Recording.
///
/// `instruction_leeway` defaults to `None` (no resource-config override).
/// Currently the RPC client's stable surface
/// (`simulate_transaction_envelope`) does not expose a resource-config knob;
/// the `next_simulate_transaction_envelope` variant accepts one but is marked
/// "internal, not to be used" in upstream. We honor that and ignore the value
/// for the stable API while still preserving the parameter in our signature
/// so the CLI surface stays stable for when the upstream stabilises.
/// If a caller passes `Some(_)`, we emit a `tracing::warn!` rather than
/// silently dropping the value — the no-op-ness must be visible.
#[tracing::instrument(skip_all, fields(rpc_url = %rpc_url, network_passphrase = %network_passphrase))]
pub async fn record_by_simulation(
    rpc_url: &str,
    network_passphrase: &str,
    envelope_xdr_base64: &str,
    instruction_leeway: Option<u64>,
) -> Result<Recording, Error> {
    // Surface the no-op-ness of `instruction_leeway` so operators are not
    // misled into thinking they tuned the simulation budget. The stable
    // `stellar-rpc-client 25.1.0` `simulate_transaction_envelope` surface
    // accepts no `resourceConfig`; the `next_*` variant does but is marked
    // "internal, not to be used" upstream. We preserve the parameter for
    // forward compatibility (and a stable CLI surface) but must NOT silently
    // accept-and-drop a value the caller explicitly set.
    if let Some(leeway) = instruction_leeway {
        tracing::warn!(
            instruction_leeway = leeway,
            "--instruction-leeway is currently a no-op pending stellar-rpc-client \
             resource_config surface; falling back to default budget"
        );
    }
    let client = Client::new(rpc_url)
        .map_err(|e| Error::RecorderSimFailed(format!("rpc client init failed: {e}")))?;
    verify_network_match(&client, rpc_url, network_passphrase).await?;
    let envelope = TransactionEnvelope::from_xdr_base64(envelope_xdr_base64, Limits::none())
        .map_err(|e| Error::RecorderXdrDecodeFailed(format!("envelope decode: {e}")))?;
    tracing::debug!(
        envelope_b64_len = envelope_xdr_base64.len(),
        "decoded TransactionEnvelope for simulation"
    );

    let sim = timeout(
        RPC_TIMEOUT,
        client.simulate_transaction_envelope(&envelope, None),
    )
    .await
    .map_err(|_elapsed| Error::RecorderSimFailed(format!("rpc timeout after 30s: {rpc_url}")))?
    .map_err(|e| Error::RecorderSimFailed(format!("simulateTransaction: {e}")))?;

    if let Some(err) = &sim.error {
        return Err(Error::RecorderSimFailed(format!(
            "simulateTransaction reported error: {err}"
        )));
    }
    if sim.transaction_data.is_empty() {
        return Err(Error::RecorderSimFailed(
            "simulateTransaction returned no transactionData; treating as failed".to_string(),
        ));
    }

    // For simulation we ingest the envelope + simulation-derived auth/events,
    // because there is no result_meta yet (the tx hasn't landed). Build the
    // skeleton from the envelope and then layer the simulation outputs.
    let mut rec = decode_from_xdr_blobs(envelope_xdr_base64, "", network_passphrase).map_err(
        |e| match e {
            Error::RecorderXdrDecodeFailed(s) => {
                Error::RecorderSimFailed(format!("envelope decode in sim path: {s}"))
            }
            other => other,
        },
    )?;

    // Pull auth from the simulation results (these are *generated* by the
    // host in `record` mode and override whatever the client put in the
    // envelope). The walker is the same as the envelope path.
    let sim_results = sim
        .results()
        .map_err(|e| Error::RecorderXdrDecodeFailed(format!("simulation results decode: {e}")))?;
    if let Some(first) = sim_results.first() {
        rec.auth_tree = walk_auth_entries(&first.auth)?;
    }

    // Pull contract / system events from the simulation. Diagnostic events
    // are intentionally excluded: per `TypedEvent`'s doc-comment, diagnostic
    // events are policy-noise (host-internal counters, fn-entry/exit traces)
    // and must not appear in the Recording. The on-chain `walk_events` path
    // emits the same set (the meta only contains contract+system events
    // outside diagnostic_events buckets), so the two paths stay consistent.
    // We additionally drop events from unsuccessful host frames
    // (`in_successful_contract_call == false`) — those represent unwound
    // sub-invocations and aren't part of the canonical event log either.
    let sim_events = sim
        .events()
        .map_err(|e| Error::RecorderXdrDecodeFailed(format!("simulation events decode: {e}")))?;
    rec.events = sim_events
        .iter()
        .filter(|de| {
            de.in_successful_contract_call && de.event.type_ != ContractEventType::Diagnostic
        })
        .map(|de| typed_event_from_contract_event(&de.event))
        .collect::<Result<Vec<_>, _>>()?;

    // Map simulation state_changes onto our StateDelta vector. We apply the
    // same deterministic ordering as the on-chain path so the simulation
    // and hash recordings of the same envelope can be diffed against each
    // other meaningfully.
    if let Some(changes) = &sim.state_changes {
        rec.state_changes = changes
            .iter()
            .map(state_delta_from_rpc_change)
            .collect::<Result<Vec<_>, _>>()?;
        sort_state_changes_deterministically(&mut rec.state_changes);
    }

    // Tag ingest as Simulation with the envelope's SHA-256 fingerprint.
    let envelope_bytes = envelope.to_xdr(Limits::none()).map_err(|e| {
        Error::RecorderXdrDecodeFailed(format!("envelope reserialise for hash: {e}"))
    })?;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&envelope_bytes);
    let digest = hasher.finalize();
    rec.ingest = IngestSource::Simulation {
        envelope_xdr_sha256: hex::encode(digest),
    };
    rec.ledger = None;
    tracing::info!(
        contract_count = rec.contracts.len(),
        event_count = rec.events.len(),
        auth_root_count = rec.auth_tree.roots.len(),
        state_change_count = rec.state_changes.len(),
        "record_by_simulation completed"
    );
    Ok(rec)
}

// ---------------------------------------------------------------------------
// Pure-decode helper — used by both entrypoints and by integration tests.
// ---------------------------------------------------------------------------

/// Decode a recording from the raw base64 `envelope_xdr` and (optional)
/// `result_meta_xdr`. Pass empty string for `result_meta_b64` when there is
/// no meta (e.g., simulation envelope skeleton); state_changes / events /
/// ledger will be empty in that case and the caller is expected to populate
/// them.
///
/// This helper is `pub` (hidden from docs) so the integration tests in
/// `crates/oz-policy-recorder/tests/` can drive it directly without a network
/// call. Production callers should prefer [`record_by_hash`] /
/// [`record_by_simulation`] which set the `ingest` discriminator correctly.
#[doc(hidden)]
pub fn decode_from_xdr_blobs(
    envelope_b64: &str,
    result_meta_b64: &str,
    network_passphrase: &str,
) -> Result<Recording, Error> {
    let envelope = TransactionEnvelope::from_xdr_base64(envelope_b64, Limits::none())
        .map_err(|e| Error::RecorderXdrDecodeFailed(format!("envelope: {e}")))?;
    tracing::debug!(
        envelope_b64_len = envelope_b64.len(),
        "decoded TransactionEnvelope"
    );

    let (contracts, auth_tree) = walk_envelope_invocations(&envelope)?;
    tracing::debug!(
        contract_count = contracts.len(),
        auth_root_count = auth_tree.roots.len(),
        "walked envelope invocations + auth entries"
    );

    let (state_changes, events) = if result_meta_b64.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let meta = TransactionMeta::from_xdr_base64(result_meta_b64, Limits::none())
            .map_err(|e| Error::RecorderXdrDecodeFailed(format!("result_meta: {e}")))?;
        tracing::debug!(
            meta_b64_len = result_meta_b64.len(),
            "decoded TransactionMeta"
        );
        let mut sc = walk_state_changes(&meta)?;
        // Canonicalise the state-change ordering. The Stellar public testnet
        // RPC is a load-balanced cluster whose backends can return the same
        // transaction's `LedgerEntryChanges` in different orders (notably,
        // `Created`-of-Ttl entries can be interleaved with `Updated`-of-
        // ContractData entries at different positions). Both orderings carry
        // the same set of entries — the host doesn't promise stable indexing
        // across operations — but the policy-builder's Recording is meant to
        // be a *canonical* IR, so we sort post-walk by a deterministic key.
        //
        // The sort is stable on `(contract, key_json, phase)` where `phase`
        // distinguishes (created, updated, removed) for paths where the same
        // (contract, key) could appear with different before/after shapes in
        // the same transaction (rare but legal). See P1-T4 in `plan.md`.
        sort_state_changes_deterministically(&mut sc);
        let ev = walk_events(&meta)?;
        (sc, ev)
    };

    Ok(Recording {
        schema: RECORDING_SCHEMA_URI.to_string(),
        network_passphrase: network_passphrase.to_string(),
        // Placeholder — both public entrypoints overwrite this immediately
        // with `Hash { .. }` or `Simulation { .. }` after a successful decode.
        ingest: IngestSource::Hash {
            hash: String::new(),
        },
        ledger: None,
        contracts,
        auth_tree,
        state_changes,
        events,
    })
}

// ---------------------------------------------------------------------------
// Envelope → ContractRecord[] + AuthTree
// ---------------------------------------------------------------------------

fn walk_envelope_invocations(
    envelope: &TransactionEnvelope,
) -> Result<(Vec<ContractRecord>, AuthTree), Error> {
    // We support v1 envelopes and fee-bump wrappers (the inner tx of a fee
    // bump is always a v1 envelope per `FeeBumpTransactionInnerTx`). v0
    // envelopes pre-date Soroban host functions so we surface a decode
    // failure if we ever see one with `InvokeHostFunction` ops (they can't
    // contain those).
    let ops_iter = match envelope {
        TransactionEnvelope::Tx(v1) => v1.tx.operations.as_vec(),
        TransactionEnvelope::TxFeeBump(fb) => {
            let xdr::FeeBumpTransactionInnerTx::Tx(v1) = &fb.tx.inner_tx;
            v1.tx.operations.as_vec()
        }
        TransactionEnvelope::TxV0(_) => {
            // No InvokeHostFunction ops can appear here; emit an empty
            // recording rather than a decode failure (a v0 wrapper is
            // syntactically valid, just policy-irrelevant).
            return Ok((Vec::new(), AuthTree { roots: Vec::new() }));
        }
    };

    let mut contracts = Vec::new();
    let mut auth_roots = Vec::new();
    // `op_index` counts ALL operations on the envelope, not just
    // `InvokeHostFunction`s, so the value on `AuthEntry::source_op_index`
    // matches the wire-level operation index in the source transaction.
    for (op_index, op) in ops_iter.iter().enumerate() {
        if let OperationBody::InvokeHostFunction(ih) = &op.body {
            match &ih.host_function {
                HostFunction::InvokeContract(ic) => {
                    contracts.push(ContractRecord {
                        address: ic.contract_address.to_string(),
                        function: sc_symbol_to_string(&ic.function_name),
                        args: ic
                            .args
                            .iter()
                            .map(decode_sc_val)
                            .collect::<Result<Vec<_>, _>>()?,
                    });
                }
                HostFunction::CreateContract(_)
                | HostFunction::CreateContractV2(_)
                | HostFunction::UploadContractWasm(_) => {
                    // Create/upload host fns do not produce a ContractRecord
                    // (no function call surface), but they may still have
                    // associated auth entries we want to capture below.
                }
            }
            // Tag each auth root with the source op index so multi-op
            // envelopes preserve op→auth correspondence in the Recording.
            let op_idx_u32 = u32::try_from(op_index).unwrap_or(u32::MAX);
            let tree = walk_auth_entries_with_op_index(ih.auth.as_slice(), op_idx_u32)?;
            auth_roots.extend(tree.roots);
        }
    }
    Ok((contracts, AuthTree { roots: auth_roots }))
}

fn walk_auth_entries(entries: &[SorobanAuthorizationEntry]) -> Result<AuthTree, Error> {
    // Default op-index 0 — used by the simulation path (one envelope = one
    // simulation result, so the op_index is unambiguously 0). The
    // multi-op-aware envelope walker uses
    // `walk_auth_entries_with_op_index` directly.
    walk_auth_entries_with_op_index(entries, 0)
}

fn walk_auth_entries_with_op_index(
    entries: &[SorobanAuthorizationEntry],
    source_op_index: u32,
) -> Result<AuthTree, Error> {
    tracing::debug!(
        entry_count = entries.len(),
        source_op_index,
        "decoding SorobanAuthorizationEntry slice"
    );
    let roots = entries
        .iter()
        .map(|e| {
            Ok(AuthEntry {
                credentials: decode_credentials(&e.credentials)?,
                root_invocation: decode_invocation(&e.root_invocation)?,
                source_op_index,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(AuthTree { roots })
}

fn decode_credentials(c: &SorobanCredentials) -> Result<Credentials, Error> {
    Ok(match c {
        SorobanCredentials::SourceAccount => Credentials::SourceAccount,
        SorobanCredentials::Address(addr) => Credentials::Address {
            signer: addr.address.to_string(),
            nonce: addr.nonce.to_string(),
            signature_expiration_ledger: addr.signature_expiration_ledger,
            signature: decode_sc_val(&addr.signature)?,
        },
    })
}

fn decode_invocation(inv: &SorobanAuthorizedInvocation) -> Result<AuthInvocation, Error> {
    let function = match &inv.function {
        SorobanAuthorizedFunction::ContractFn(ic) => AuthFunction::Contract {
            address: ic.contract_address.to_string(),
            function: sc_symbol_to_string(&ic.function_name),
            args: ic
                .args
                .iter()
                .map(decode_sc_val)
                .collect::<Result<Vec<_>, _>>()?,
        },
        SorobanAuthorizedFunction::CreateContractHostFn(cc) => AuthFunction::CreateContract {
            contract_id_preimage_xdr_hex: hex::encode(
                cc.contract_id_preimage
                    .to_xdr(Limits::none())
                    .map_err(|e| Error::RecorderXdrDecodeFailed(format!("preimage xdr: {e}")))?,
            ),
            executable_kind: executable_kind(&cc.executable),
            executable_value: executable_value(&cc.executable),
        },
        SorobanAuthorizedFunction::CreateContractV2HostFn(cc) => AuthFunction::CreateContractV2 {
            contract_id_preimage_xdr_hex: hex::encode(
                cc.contract_id_preimage
                    .to_xdr(Limits::none())
                    .map_err(|e| Error::RecorderXdrDecodeFailed(format!("preimage xdr: {e}")))?,
            ),
            executable_kind: executable_kind(&cc.executable),
            executable_value: executable_value(&cc.executable),
            constructor_args: cc
                .constructor_args
                .iter()
                .map(decode_sc_val)
                .collect::<Result<Vec<_>, _>>()?,
        },
    };
    let sub_invocations = inv
        .sub_invocations
        .iter()
        .map(decode_invocation)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AuthInvocation {
        function,
        sub_invocations,
    })
}

fn executable_kind(e: &xdr::ContractExecutable) -> String {
    match e {
        xdr::ContractExecutable::Wasm(_) => "Wasm".to_string(),
        xdr::ContractExecutable::StellarAsset => "StellarAsset".to_string(),
    }
}

fn executable_value(e: &xdr::ContractExecutable) -> String {
    match e {
        xdr::ContractExecutable::Wasm(h) => hex::encode(h.0),
        xdr::ContractExecutable::StellarAsset => String::new(),
    }
}

// ---------------------------------------------------------------------------
// TransactionMeta → state_changes + events
// ---------------------------------------------------------------------------

/// Sort a `Vec<StateDelta>` into a deterministic canonical order so the same
/// set of changes produces a byte-equal Recording regardless of the order
/// the RPC happened to emit them.
///
/// Why this is necessary: the public Stellar testnet RPC is a load-balanced
/// cluster where different backends can return the per-operation
/// `LedgerEntryChanges` list for the same transaction in different orders
/// (notably interleaving `Ttl`-Created entries between `ContractData`
/// updates differently). Both orderings carry the same logical state
/// changes — the host doesn't guarantee a stable wire order across nodes —
/// but the policy-builder Recording is positioned as the *canonical* IR a
/// synthesizer reads, so we collapse this ambiguity here.
///
/// The sort key is the JSON representation of `(contract, key, phase)`:
/// * `contract`: `None` (non-ContractData entries) sort before `Some(c)`,
///   then alphabetically by StrKey.
/// * `key`: the decoded `ArgValue`, serialised to canonical JSON so two
///   structurally-equal keys compare equal.
/// * `phase`: a small integer distinguishing Created (0) / Updated (1) /
///   Removed (2) for the rare-but-legal case where the same `(contract, key)`
///   appears multiple times in one transaction.
///
/// Using JSON as the comparison medium is the cheapest way to get a stable
/// total order over the heterogeneous `ArgValue` tree without inventing a
/// custom `Ord` impl that we'd then need to keep in lockstep with the JSON
/// schema. `serde_json::to_string` on `ArgValue` is deterministic because
/// the type contains no maps with non-deterministic iteration order (all
/// `Map` variants serialise as ordered `Vec<MapEntry>`).
fn sort_state_changes_deterministically(changes: &mut [StateDelta]) {
    fn phase(d: &StateDelta) -> u8 {
        match (&d.before, &d.after) {
            (None, Some(_)) => 0,    // Created / Restored
            (Some(_), Some(_)) => 1, // Updated
            (Some(_), None) => 2,    // Removed
            (None, None) => 3,       // shouldn't occur; sort last for visibility
        }
    }
    changes.sort_by_cached_key(|d| {
        // Best-effort JSON encoding for the sort key. If serialization ever
        // fails (it cannot, given the schema, but `serde_json` returns a
        // Result), fall back to `String::new()` so we never panic on the
        // sort path — a less-canonical order is strictly better than a
        // crash for a deterministic-but-not-perfect ordering.
        let key_json = serde_json::to_string(&d.key).unwrap_or_default();
        (d.contract.clone(), key_json, phase(d))
    });
}

fn walk_state_changes(meta: &TransactionMeta) -> Result<Vec<StateDelta>, Error> {
    // We walk per-operation `changes` and tx-level `tx_changes_before` /
    // `tx_changes_after`. The order in `LedgerEntryChanges` is
    // [State, Updated] for an updated entry, [Created] for a new entry,
    // [State, Removed] for a delete — we coalesce State+Updated /
    // State+Removed pairs and emit one StateDelta per logical change.
    let mut out = Vec::new();
    match meta {
        TransactionMeta::V0(_) | TransactionMeta::V1(_) | TransactionMeta::V2(_) => {
            // Pre-Soroban meta variants. No ContractData changes to expose.
        }
        TransactionMeta::V3(v3) => {
            push_changes_from(&v3.tx_changes_before, &mut out)?;
            for op in v3.operations.iter() {
                push_changes_from(&op.changes, &mut out)?;
            }
            push_changes_from(&v3.tx_changes_after, &mut out)?;
        }
        TransactionMeta::V4(v4) => {
            push_changes_from(&v4.tx_changes_before, &mut out)?;
            for op in v4.operations.iter() {
                push_changes_from(&op.changes, &mut out)?;
            }
            push_changes_from(&v4.tx_changes_after, &mut out)?;
        }
    }
    Ok(out)
}

fn push_changes_from(c: &LedgerEntryChanges, out: &mut Vec<StateDelta>) -> Result<(), Error> {
    // Walk linearly; carry the most-recent `State` as the "before" pre-image
    // for the next `Updated` / `Removed` adjacent to it.
    //
    // The Stellar convention encodes an updated entry as the adjacent pair
    // `[State, Updated]` and a deleted entry as `[State, Removed]`. This
    // adjacency is the wire convention but not enforced by the XDR contract,
    // so we guard against ordering surprises:
    //   * In debug builds, a `debug_assert!` catches two consecutive `State`
    //     entries without an intervening pair — this would mean a malformed
    //     stream slipped past CI.
    //   * In production, an orphan `Updated` / `Removed` (no prior `State`
    //     for the same key) emits a `tracing::warn!` and continues with
    //     `before = None`, so the recording stays best-effort rather than
    //     refusing the entire transaction over a meta-encoding quirk.
    let mut last_state: Option<(LedgerKey, ArgValue, Option<String>)> = None;
    for change in c.iter() {
        match change {
            LedgerEntryChange::State(entry) => {
                debug_assert!(
                    last_state.is_none(),
                    "consecutive State entries without paired Updated/Removed; \
                     the second State entry will overwrite the first's pre-image"
                );
                let (lkey, key_arg, contract) = ledger_entry_key(entry)?;
                let val = ledger_entry_value(entry)?;
                last_state = Some((lkey, val.clone(), contract.clone()));
                // Don't emit a delta for State on its own — it's a marker.
                let _ = (key_arg,);
            }
            LedgerEntryChange::Created(entry) => {
                let (_, key_arg, contract) = ledger_entry_key(entry)?;
                let after = ledger_entry_value(entry)?;
                out.push(StateDelta {
                    key: key_arg,
                    contract,
                    before: None,
                    after: Some(after),
                });
                last_state = None;
            }
            LedgerEntryChange::Updated(entry) => {
                let (lkey, key_arg, contract) = ledger_entry_key(entry)?;
                let after = ledger_entry_value(entry)?;
                let prior = last_state.take();
                let before = match prior {
                    Some((prev_key, before_val, _)) if prev_key == lkey => Some(before_val),
                    Some((prev_key, _, _)) => {
                        tracing::warn!(
                            ?prev_key,
                            updated_key = ?lkey,
                            "StateDelta orphan Updated: prior State has a different key; \
                             before will be None"
                        );
                        None
                    }
                    None => {
                        tracing::warn!(
                            updated_key = ?lkey,
                            "StateDelta orphan Updated: no prior State entry; before will be None"
                        );
                        None
                    }
                };
                out.push(StateDelta {
                    key: key_arg,
                    contract,
                    before,
                    after: Some(after),
                });
            }
            LedgerEntryChange::Removed(lkey) => {
                let (key_arg, contract) = ledger_key_to_arg(lkey);
                let prior = last_state.take();
                let before = match prior {
                    Some((prev_key, before_val, _)) if &prev_key == lkey => Some(before_val),
                    Some((prev_key, _, _)) => {
                        tracing::warn!(
                            ?prev_key,
                            removed_key = ?lkey,
                            "StateDelta orphan Removed: prior State has a different key; \
                             before will be None"
                        );
                        None
                    }
                    None => {
                        tracing::warn!(
                            removed_key = ?lkey,
                            "StateDelta orphan Removed: no prior State entry; before will be None"
                        );
                        None
                    }
                };
                out.push(StateDelta {
                    key: key_arg,
                    contract,
                    before,
                    after: None,
                });
            }
            LedgerEntryChange::Restored(entry) => {
                let (_, key_arg, contract) = ledger_entry_key(entry)?;
                let after = ledger_entry_value(entry)?;
                out.push(StateDelta {
                    key: key_arg,
                    contract,
                    before: None,
                    after: Some(after),
                });
            }
        }
    }
    Ok(())
}

fn ledger_entry_key(
    entry: &xdr::LedgerEntry,
) -> Result<(LedgerKey, ArgValue, Option<String>), Error> {
    Ok(match &entry.data {
        LedgerEntryData::ContractData(cd) => {
            let lk = LedgerKey::ContractData(xdr::LedgerKeyContractData {
                contract: cd.contract.clone(),
                key: cd.key.clone(),
                durability: cd.durability,
            });
            (lk, decode_sc_val(&cd.key)?, Some(cd.contract.to_string()))
        }
        other => {
            // Synthesize a placeholder key so the walker doesn't drop the
            // delta entirely. Phase 2 policies only consume ContractData
            // deltas, but preserving the others keeps the recording
            // lossless w.r.t. *which* entry types changed.
            let kind = other.name();
            let placeholder = ArgValue::Symbol(format!("ledger_entry:{kind}"));
            // A best-effort LedgerKey for non-ContractData entries; the only
            // use for it inside this function is the `last_state` pairing,
            // and the placeholder is never compared against a parsed key.
            // We synthesise a benign Account key but it should not be
            // relied on — `last_state` will pair correctly only for
            // ContractData updates.
            (
                LedgerKey::Account(xdr::LedgerKeyAccount {
                    account_id: xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(
                        [0u8; 32],
                    ))),
                }),
                placeholder,
                None,
            )
        }
    })
}

fn ledger_entry_value(entry: &xdr::LedgerEntry) -> Result<ArgValue, Error> {
    Ok(match &entry.data {
        LedgerEntryData::ContractData(cd) => decode_sc_val(&cd.val)?,
        other => ArgValue::Symbol(format!("ledger_entry_value:{}", other.name())),
    })
}

fn ledger_key_to_arg(lk: &LedgerKey) -> (ArgValue, Option<String>) {
    if let LedgerKey::ContractData(cd) = lk {
        if let Ok(v) = decode_sc_val(&cd.key) {
            return (v, Some(cd.contract.to_string()));
        }
    }
    (ArgValue::Symbol(format!("ledger_key:{}", lk.name())), None)
}

fn walk_events(meta: &TransactionMeta) -> Result<Vec<TypedEvent>, Error> {
    match meta {
        TransactionMeta::V3(v3) => {
            // V3 contract events live inside `soroban_meta.events`.
            if let Some(sm) = &v3.soroban_meta {
                sm.events
                    .iter()
                    .map(typed_event_from_contract_event)
                    .collect()
            } else {
                Ok(Vec::new())
            }
        }
        TransactionMeta::V4(v4) => {
            // V4 events appear on each OperationMetaV2.events. Contract
            // events are emitted in order across operations; we flatten them
            // into a single per-recording vector preserving order.
            let mut out = Vec::new();
            for op in v4.operations.iter() {
                for ev in op.events.iter() {
                    out.push(typed_event_from_contract_event(ev)?);
                }
            }
            Ok(out)
        }
        _ => Ok(Vec::new()),
    }
}

fn typed_event_from_contract_event(ev: &xdr::ContractEvent) -> Result<TypedEvent, Error> {
    let kind = match ev.type_ {
        ContractEventType::System => "system",
        ContractEventType::Contract => "contract",
        ContractEventType::Diagnostic => "diagnostic",
    }
    .to_string();
    let contract = ev
        .contract_id
        .as_ref()
        .map(|ContractId(Hash(h))| ScAddress::Contract(ContractId(Hash(*h))).to_string());
    let ContractEventBody::V0(body) = &ev.body;
    let topics = body
        .topics
        .iter()
        .map(decode_sc_val)
        .collect::<Result<Vec<_>, _>>()?;
    let data = decode_sc_val(&body.data)?;
    Ok(TypedEvent {
        contract,
        kind,
        topics,
        data,
    })
}

fn state_delta_from_rpc_change(change: &RpcLedgerEntryChange) -> Result<StateDelta, Error> {
    fn parse_entry(b64: &str) -> Result<(ArgValue, ArgValue, Option<String>), Error> {
        let entry = xdr::LedgerEntry::from_xdr_base64(b64, Limits::none()).map_err(|e| {
            Error::RecorderXdrDecodeFailed(format!("sim state_change ledger entry: {e}"))
        })?;
        let val = ledger_entry_value(&entry)?;
        let (_, key_arg, contract) = ledger_entry_key(&entry)?;
        Ok((key_arg, val, contract))
    }
    fn parse_key(b64: &str) -> Result<(ArgValue, Option<String>), Error> {
        let lk = xdr::LedgerKey::from_xdr_base64(b64, Limits::none()).map_err(|e| {
            Error::RecorderXdrDecodeFailed(format!("sim state_change ledger key: {e}"))
        })?;
        Ok(ledger_key_to_arg(&lk))
    }
    Ok(match change {
        RpcLedgerEntryChange::Created { key: _, after } => {
            let (key_arg, after_v, contract) = parse_entry(after)?;
            StateDelta {
                key: key_arg,
                contract,
                before: None,
                after: Some(after_v),
            }
        }
        RpcLedgerEntryChange::Deleted { key, before } => {
            let (key_arg, contract) = parse_key(key)?;
            let (_, before_v, _) = parse_entry(before)?;
            StateDelta {
                key: key_arg,
                contract,
                before: Some(before_v),
                after: None,
            }
        }
        RpcLedgerEntryChange::Updated {
            key: _,
            before,
            after,
        } => {
            let (key_arg, before_v, contract_b) = parse_entry(before)?;
            let (_, after_v, _) = parse_entry(after)?;
            StateDelta {
                key: key_arg,
                contract: contract_b,
                before: Some(before_v),
                after: Some(after_v),
            }
        }
    })
}

// ---------------------------------------------------------------------------
// ScVal → ArgValue
// ---------------------------------------------------------------------------

fn decode_sc_val(v: &ScVal) -> Result<ArgValue, Error> {
    Ok(match v {
        ScVal::Bool(b) => ArgValue::Bool(*b),
        ScVal::Void => ArgValue::Void,
        ScVal::Error(e) => ArgValue::Error {
            kind: sc_error_kind(e).to_string(),
            code: sc_error_code(e),
        },
        ScVal::U32(u) => ArgValue::U32(*u),
        ScVal::I32(i) => ArgValue::I32(*i),
        ScVal::U64(u) => ArgValue::U64(u.to_string()),
        ScVal::I64(i) => ArgValue::I64(i.to_string()),
        ScVal::Timepoint(t) => ArgValue::Timepoint(t.0.to_string()),
        ScVal::Duration(d) => ArgValue::Duration(d.0.to_string()),
        ScVal::U128(p) => ArgValue::U128(uint128_to_string(p)),
        ScVal::I128(p) => ArgValue::I128(int128_to_string(p)),
        ScVal::U256(p) => ArgValue::U256(uint256_to_string(p)),
        ScVal::I256(p) => ArgValue::I256(int256_to_string(p)),
        ScVal::Bytes(b) => ArgValue::Bytes {
            hex: hex::encode(b.0.as_slice()),
        },
        ScVal::String(s) => {
            let bytes = s.0.as_slice();
            let utf8 = std::str::from_utf8(bytes).ok().map(|x| x.to_string());
            ArgValue::String {
                utf8,
                hex: hex::encode(bytes),
            }
        }
        ScVal::Symbol(s) => ArgValue::Symbol(sc_symbol_to_string(s)),
        ScVal::Vec(opt) => ArgValue::Vec(match opt {
            None => None,
            Some(v) => Some(
                v.0.iter()
                    .map(decode_sc_val)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        }),
        ScVal::Map(opt) => ArgValue::Map(match opt {
            None => None,
            Some(m) => Some(
                m.0.iter()
                    .map(|e| {
                        Ok(MapEntry {
                            key: decode_sc_val(&e.key)?,
                            value: decode_sc_val(&e.val)?,
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
            ),
        }),
        ScVal::Address(a) => ArgValue::Address(a.to_string()),
        ScVal::ContractInstance(ci) => ArgValue::ContractInstance {
            executable_kind: executable_kind(&ci.executable),
            executable_value: executable_value(&ci.executable),
            storage: match &ci.storage {
                None => None,
                Some(m) => Some(
                    m.0.iter()
                        .map(|e| {
                            Ok(MapEntry {
                                key: decode_sc_val(&e.key)?,
                                value: decode_sc_val(&e.val)?,
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()?,
                ),
            },
        },
        ScVal::LedgerKeyContractInstance => ArgValue::LedgerKeyContractInstance,
        ScVal::LedgerKeyNonce(n) => ArgValue::LedgerKeyNonce {
            nonce: n.nonce.to_string(),
        },
    })
}

fn sc_symbol_to_string(s: &xdr::ScSymbol) -> String {
    String::from_utf8_lossy(s.0.as_slice()).into_owned()
}

fn sc_error_kind(e: &ScError) -> &'static str {
    match e {
        ScError::Contract(_) => "Contract",
        ScError::WasmVm(_) => "WasmVm",
        ScError::Context(_) => "Context",
        ScError::Storage(_) => "Storage",
        ScError::Object(_) => "Object",
        ScError::Crypto(_) => "Crypto",
        ScError::Events(_) => "Events",
        ScError::Budget(_) => "Budget",
        ScError::Value(_) => "Value",
        ScError::Auth(_) => "Auth",
    }
}

fn sc_error_code(e: &ScError) -> String {
    match e {
        ScError::Contract(c) => c.to_string(),
        ScError::WasmVm(c)
        | ScError::Context(c)
        | ScError::Storage(c)
        | ScError::Object(c)
        | ScError::Crypto(c)
        | ScError::Events(c)
        | ScError::Budget(c)
        | ScError::Value(c)
        | ScError::Auth(c) => sc_error_code_name(c).to_string(),
    }
}

fn sc_error_code_name(c: &ScErrorCode) -> &'static str {
    // `ScErrorCode` is a flat enum; its Display impl prints the name.
    // We pin it to a stable string for forward compatibility.
    match c {
        ScErrorCode::ArithDomain => "ArithDomain",
        ScErrorCode::IndexBounds => "IndexBounds",
        ScErrorCode::InvalidInput => "InvalidInput",
        ScErrorCode::MissingValue => "MissingValue",
        ScErrorCode::ExistingValue => "ExistingValue",
        ScErrorCode::ExceededLimit => "ExceededLimit",
        ScErrorCode::InvalidAction => "InvalidAction",
        ScErrorCode::InternalError => "InternalError",
        ScErrorCode::UnexpectedType => "UnexpectedType",
        ScErrorCode::UnexpectedSize => "UnexpectedSize",
    }
}

fn uint128_to_string(p: &UInt128Parts) -> String {
    ((u128::from(p.hi) << 64) | u128::from(p.lo)).to_string()
}

fn int128_to_string(p: &Int128Parts) -> String {
    // Reconstruct i128 from (i64 hi, u64 lo) as the host does:
    //   v = (hi as i128) << 64 | (lo as i128 as unsigned bits)
    // The `as i128` cast of the unsigned lo is non-negative so the |
    // composes correctly across signed/unsigned halves (this matches the
    // semantics of Soroban host arithmetic; verified against the upstream
    // helper `i128_str_from_pieces` in stellar-xdr/src/num128.rs).
    let composed = ((p.hi as i128) << 64) | (p.lo as i128);
    composed.to_string()
}

fn uint256_to_string(p: &UInt256Parts) -> String {
    // Use the upstream Display impl which composes hi_hi/hi_lo/lo_hi/lo_lo
    // into a canonical decimal string.
    p.to_string()
}

fn int256_to_string(p: &Int256Parts) -> String {
    p.to_string()
}
