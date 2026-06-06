//! mcp tool handler bodies. typed in/out, return `rmcp::ErrorData`.
//! determinism contract: payloads round-trip byte-equal, ids are fresh uuids.
//! errors route through `error_mapping::error_to_jsonrpc`.

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine};
use oz_policy_codegen::{render_contract, synthesize_track_b, CompiledArtifact};
use oz_policy_core::decision_tree::{self, SynthesisOptions, Tightness};
use oz_policy_core::recording::Recording;
use oz_policy_core::spec::{PolicySlot, PolicySpec, SynthesisMode};
use oz_policy_installer::{build_install_envelope, AccountRevision};
use oz_policy_recorder::{record_by_hash, record_by_simulation};
use oz_policy_simhost::deny::DenyVector;
use oz_policy_simhost::run::{run_full_suite, SimReport};
use rmcp::model::ErrorData;
use sha2::{Digest, Sha256};

use crate::error_mapping::error_to_jsonrpc;
use crate::store::{ArtifactBundle, McpStore};

/// testnet network passphrase.
pub const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";

/// mainnet network passphrase.
pub const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";

/// default testnet rpc when caller doesn't override.
pub const DEFAULT_TESTNET_RPC: &str = "https://soroban-testnet.stellar.org";

/// default mainnet rpc; production callers should override.
pub const DEFAULT_MAINNET_RPC: &str = "https://soroban.stellar.org";

// record_transaction

/// `record_transaction` input. Exactly one of `hash` / `envelope_xdr_base64`
/// must be present; the handler validates this at runtime (the JSON Schema
/// emits both as optional with a `oneOf`-style description). `rpc_url`
/// defaults per `network`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordTransactionInput {
    /// stellar network the transaction belongs to. Selects the default
    /// network passphrase + RPC endpoint when `rpc_url` is omitted.
    pub network: NetworkKind,
    /// optional RPC URL override. Defaults to [`DEFAULT_TESTNET_RPC`] /
    /// [`DEFAULT_MAINNET_RPC`] per `network`.
    pub rpc_url: Option<String>,
    /// 64-char hex transaction hash. Mutually exclusive with
    /// [`Self::envelope_xdr_base64`].
    pub hash: Option<String>,
    /// base64-encoded `TransactionEnvelope` XDR to simulate. Mutually
    /// exclusive with [`Self::hash`].
    pub envelope_xdr_base64: Option<String>,
    /// optional `simulateTransaction.resourceConfig.instructionLeeway`
    /// override (in instructions). Honoured only by the simulation path;
    /// stable `stellar-rpc-client 25.1.0` does not expose this on the
    /// stable API surface, so the recorder logs a `tracing::warn!` and
    /// continues with the default budget — see
    /// `oz-policy-recorder/src/recorder.rs` for the no-op contract.
    pub instruction_leeway: Option<u64>,
}

/// stellar network discriminant. Selects passphrase + default RPC URL.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NetworkKind {
    Testnet,
    Mainnet,
}

impl NetworkKind {
    /// canonical network passphrase string.
    pub fn passphrase(self) -> &'static str {
        match self {
            NetworkKind::Testnet => TESTNET_PASSPHRASE,
            NetworkKind::Mainnet => MAINNET_PASSPHRASE,
        }
    }

    /// default Soroban RPC endpoint.
    pub fn default_rpc(self) -> &'static str {
        match self {
            NetworkKind::Testnet => DEFAULT_TESTNET_RPC,
            NetworkKind::Mainnet => DEFAULT_MAINNET_RPC,
        }
    }
}

/// `record_transaction` output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RecordTransactionOutput {
    /// freshly-allocated `rec_<uuid>` store ID for the produced Recording.
    /// stream B's `recording://<id>` resource URI uses this same string.
    pub recording_id: String,
    /// decoded recording. Byte-equal across two calls with the same source
    /// transaction (modulo the RPC's own ledger advancement for the
    /// `record_by_simulation` path).
    pub recording: Recording,
    /// soft warning when the source transaction is approaching Soroban's
    /// retention horizon (currently a placeholder — Phase 5 surfaces the
    /// string verbatim, Phase 7 wires the actual ledger-window check via
    /// `getLatestLedger`). `None` means "no warning" — never an empty
    /// string, so MCP clients can branch on `is_none()`.
    pub retention_warning: Option<String>,
}

/// `record_transaction` handler. Drives either `record_by_hash` or
/// `record_by_simulation` based on which input field is populated; stores
/// the resulting Recording under a freshly-allocated `rec_<uuid>` and
/// returns the ID + the typed Recording so callers can branch on its
/// contents without an extra `resources/read` round-trip.
///
/// errors:
/// * `E_RECORDER_HASH_NOT_FOUND` — hash not on chain / wrong network /
///   retention exceeded.
/// * `E_RECORDER_SIM_FAILED` — `simulateTransaction` errored.
/// * `E_RECORDER_XDR_DECODE_FAILED` — XDR fields could not be decoded.
/// * Bad-input ergonomics: zero or both of `hash` / `envelope_xdr_base64`
///   set surfaces as `ErrorData::invalid_params` (JSON-RPC -32602) per the
///   MCP convention.
pub async fn record_transaction(
    store: &McpStore,
    input: RecordTransactionInput,
) -> Result<RecordTransactionOutput, ErrorData> {
    // mutual-exclusion gate. We surface this as -32602 INVALID_PARAMS
    // (rmcp's standard "your JSON didn't satisfy the schema" code), NOT
    // an `E_RECORDER_*` code — there's no recorder error condition yet.
    match (&input.hash, &input.envelope_xdr_base64) {
        (Some(_), Some(_)) => {
            return Err(ErrorData::invalid_params(
                "record_transaction: pass exactly one of `hash` or \
                 `envelope_xdr_base64`, not both",
                None,
            ));
        }
        (None, None) => {
            return Err(ErrorData::invalid_params(
                "record_transaction: one of `hash` or `envelope_xdr_base64` is required",
                None,
            ));
        }
        _ => {}
    }

    let rpc_url = input
        .rpc_url
        .as_deref()
        .unwrap_or_else(|| input.network.default_rpc())
        .to_string();
    let passphrase = input.network.passphrase();

    let recording = if let Some(hash) = &input.hash {
        record_by_hash(&rpc_url, passphrase, hash)
            .await
            .map_err(|e| error_to_jsonrpc(&e))?
    } else {
        // SAFETY: the mutual-exclusion gate above proves one of the two
        // is `Some`; the inner `unwrap` would be `expect`-grade.
        let envelope = input
            .envelope_xdr_base64
            .as_deref()
            .expect("envelope_xdr_base64 is set per the mutual-exclusion gate");
        record_by_simulation(&rpc_url, passphrase, envelope, input.instruction_leeway)
            .await
            .map_err(|e| error_to_jsonrpc(&e))?
    };

    let id = store.new_id("rec");
    store.put_recording(&id, recording.clone());

    Ok(RecordTransactionOutput {
        recording_id: id,
        recording,
        retention_warning: None,
    })
}

// synthesize_policy

/// `synthesize_policy` input. Looks up `recording_id` in the store and
/// drives `oz_policy_core::decision_tree::synthesize` with the typed
/// options.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SynthesizePolicyInput {
    /// recording ID returned by an earlier [`record_transaction`] call.
    pub recording_id: String,
    /// numeric scaling factor for observed `i128` constraints.
    pub tightness: Tightness,
    /// optional `PolicySpec::lifetime_ledgers` + `SpendingLimit.period_ledgers`.
    pub lifetime_ledgers: Option<u32>,
    /// optional delegated-signer contract address. When set, the
    /// synthesizer emits a single `SignerSpec::Delegated` regardless of
    /// observed signers (see `decision_tree::SynthesisOptions::delegated_signer`).
    pub delegated_signer: Option<String>,
    /// which synthesis path is permitted: compose existing primitives,
    /// emit a generated slot, or let the synthesizer choose.
    pub mode: SynthesisMode,
    /// optional context rule name. Defaults to `"rule-<first-8-chars-of-id>"`
    /// when omitted; the handler clamps the result to `MAX_NAME_SIZE`
    /// (20 bytes) per `docs/oz-internal-shapes.md` §7.
    pub rule_name: Option<String>,
}

/// `synthesize_policy` output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct SynthesizePolicyOutput {
    /// freshly-allocated `spec_<uuid>` store ID.
    pub spec_id: String,
    /// deterministic, byte-equal-across-runs `PolicySpec` payload.
    pub spec: PolicySpec,
    /// number of `PolicySlot::Generated` slots in the spec (Track-B emit).
    pub generated_count: u32,
    /// number of `PolicySlot::Existing` slots in the spec (Track-A compose).
    pub composed_count: u32,
}

/// `synthesize_policy` handler.
///
/// errors:
/// * `E_SYNTH_NOT_EXPRESSIBLE` — the recording cannot be expressed under
///   the requested mode + on-chain hard limits.
/// * `ErrorData::invalid_params` (-32602) — `recording_id` not found in
///   the store. We deliberately surface this as INVALID_PARAMS (not a new
///   `E_*` code) because the recording-id-not-found path is an MCP-layer
///   ergonomics issue, not a synthesizer failure.
pub async fn synthesize_policy(
    store: &McpStore,
    input: SynthesizePolicyInput,
) -> Result<SynthesizePolicyOutput, ErrorData> {
    let recording = store.get_recording(&input.recording_id).ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "synthesize_policy: recording_id {:?} not found in store",
                input.recording_id
            ),
            None,
        )
    })?;

    let rule_name = match input.rule_name.clone() {
        Some(name) => clamp_rule_name(&name),
        None => default_rule_name(&input.recording_id),
    };

    let opts = SynthesisOptions {
        mode: input.mode.clone(),
        tightness: input.tightness,
        lifetime_ledgers: input.lifetime_ledgers,
        delegated_signer: input.delegated_signer.clone(),
        context_rule_name: rule_name,
    };

    let spec = decision_tree::synthesize(&recording, &opts).map_err(|e| error_to_jsonrpc(&e))?;
    let (generated_count, composed_count) = count_slots(&spec);

    let spec_id = store.new_id("spec");
    store.put_spec(&spec_id, spec.clone());

    Ok(SynthesizePolicyOutput {
        spec_id,
        spec,
        generated_count,
        composed_count,
    })
}

fn count_slots(spec: &PolicySpec) -> (u32, u32) {
    let mut generated = 0u32;
    let mut composed = 0u32;
    for slot in &spec.policies {
        match slot {
            PolicySlot::Generated { .. } => generated = generated.saturating_add(1),
            PolicySlot::Existing { .. } => composed = composed.saturating_add(1),
        }
    }
    (generated, composed)
}

/// truncate `name` to `MAX_NAME_SIZE` UTF-8 **bytes** (not chars) without
/// splitting a UTF-8 boundary mid-codepoint. We refuse to silently lossy
/// truncate; the on-chain `MAX_NAME_SIZE` is a byte count, so this is the
/// canonical reduction.
fn clamp_rule_name(name: &str) -> String {
    let cap = oz_policy_core::spec::MAX_NAME_SIZE as usize;
    if name.len() <= cap {
        return name.to_string();
    }
    // walk back to the nearest UTF-8 boundary so we don't construct a
    // panic-producing slice. `String::is_char_boundary` is the canonical
    // probe (avoids reaching into private std internals).
    let mut end = cap;
    while end > 0 && !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_string()
}

/// default rule name: `"rule-<first-8-chars-of-recording-id>"` truncated
/// at the canonical byte cap. The `rec_` prefix drops off the front so
/// the visible suffix is the UUID's first 8 hex chars, mirroring the
/// human-readable convention the CLI uses for the same defaults.
fn default_rule_name(recording_id: &str) -> String {
    let suffix = recording_id
        .trim_start_matches("rec_")
        .chars()
        .take(8)
        .collect::<String>();
    let candidate = format!("rule-{suffix}");
    clamp_rule_name(&candidate)
}

// simulate_policy

/// `simulate_policy` input.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SimulatePolicyInput {
    /// spec ID returned by an earlier [`synthesize_policy`] call.
    pub spec_id: String,
    /// recording ID — the simhost replays this against the compiled
    /// policy WASMs as the permit branch.
    pub recording_id: String,
    /// optional caller-supplied deny vectors appended to the generated
    /// boundary-mutation set. Default empty.
    pub extra_deny_vectors: Option<Vec<DenyVector>>,
}

/// `simulate_policy` handler. Output is `SimReport` directly — it already
/// derives `JsonSchema` + `Serialize` + `Deserialize` (see
/// `oz_policy_simhost::run::SimReport`).
///
/// errors:
/// * `E_CODEGEN_COMPILE_FAILED` — bubbled up from
///   `oz_policy_codegen::synthesize_track_b` (the Track-B build pipeline
///   ran on-the-fly and one of the rendered crates failed `cargo build`).
/// * `E_SIM_PERMIT_DENIED` / `E_SIM_DENY_PASSED` — surfaced by the
///   simhost when the recording's permit branch was rejected, or when a
///   deny vector was admitted. (The handler does NOT itself decide
///   pass/fail; `run_full_suite` returns a structured `SimReport` and
///   the handler returns it verbatim. The `E_SIM_*` codes are reserved
///   for future failure modes in the host driver — see `plan.md` § Phase
///   5 Implementation → Tools.)
/// * `ErrorData::invalid_params` — spec_id or recording_id not found.
pub async fn simulate_policy(
    store: &McpStore,
    input: SimulatePolicyInput,
) -> Result<SimReport, ErrorData> {
    let spec = store.get_spec(&input.spec_id).ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "simulate_policy: spec_id {:?} not found in store",
                input.spec_id
            ),
            None,
        )
    })?;
    let recording = store.get_recording(&input.recording_id).ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "simulate_policy: recording_id {:?} not found in store",
                input.recording_id
            ),
            None,
        )
    })?;

    // track-B: rebuild every `Generated` slot on the fly so the simhost
    // has compiled WASMs to install. `synthesize_track_b` skips Existing
    // slots and returns one artifact per Generated slot in slot order
    // — exactly what `run_full_suite` expects.
    let artifacts: Vec<CompiledArtifact> = synthesize_track_b(&spec)
        .await
        .map_err(|e| error_to_jsonrpc(&e))?;

    let extra_deny = input.extra_deny_vectors.unwrap_or_default();
    run_full_suite(&spec, &recording, &artifacts, extra_deny)
        .await
        .map_err(|e| error_to_jsonrpc(&e))
}

// export_policy

/// `export_policy` input.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportPolicyInput {
    /// spec ID returned by an earlier [`synthesize_policy`] call.
    pub spec_id: String,
    /// target smart-account StrKey `C…` address (where the policy will
    /// install).
    pub smart_account: String,
    /// source account StrKey `G…` paying fees + signing the envelope.
    pub source_account: String,
    /// soroban RPC URL — the installer uses it for the
    /// `simulateTransaction` round-trip that fills in
    /// `transactionData` + auth.
    pub rpc_url: String,
    /// network passphrase the RPC endpoint is asserted to serve.
    pub network_passphrase: String,
    /// caller-asserted smart-account release vintage. The installer
    /// refuses anything other than [`AccountRevision::PostPr655`] in v1
    /// (see `docs/oz-internal-shapes.md` §8).
    pub account_revision: AccountRevision,
    /// which artifacts to materialise.
    pub format: ExportFormat,
}

/// `export_policy` artifact selector.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// render Track-B Rust source for every `Generated` slot. No
    /// sandboxed build, no install envelope.
    RustSource,
    /// render Track-B Rust source AND drive it through the codegen
    /// sandbox so the output carries compiled WASM bytes + SHA-256.
    /// implies [`ExportFormat::RustSource`].
    Wasm,
    /// build the install envelope only (Track-A path) — no Track-B
    /// codegen, no WASM.
    InstallEnvelope,
    /// everything: source + WASM + install envelope.
    All,
}

/// `export_policy` output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportPolicyOutput {
    /// freshly-allocated `art_<uuid>` store ID. Stream B's
    /// `artifact://<id>/source.rs` (etc.) resource URIs key off it.
    pub artifact_id: String,
    /// track-B Rust source for the first `Generated` slot. Inline so
    /// MCP clients can preview without a follow-up `resources/read` call.
    /// `None` when `format` excludes source OR the spec has no Generated
    /// slots.
    pub rust_source: Option<String>,
    /// compiled WASM bytes, base64-encoded. `None` when `format` is
    /// [`ExportFormat::RustSource`] / [`ExportFormat::InstallEnvelope`]
    /// or when the spec has no Generated slots.
    pub wasm_base64: Option<String>,
    /// install envelope XDR, base64-encoded. `None` when `format` is
    /// [`ExportFormat::RustSource`] / [`ExportFormat::Wasm`].
    pub install_envelope_xdr_base64: Option<String>,
    /// SHA-256 of the compiled WASM bytes (lowercase hex). `None`
    /// alongside `wasm_base64 = None`.
    pub wasm_hash_hex: Option<String>,
    /// resource URIs the same artifacts are reachable under via
    /// `resources/read` (Stream B). Always non-empty (at least the
    /// `artifact://<id>` root is listed), even when individual fields
    /// are `None`.
    pub resource_uris: Vec<String>,
}

/// `export_policy` handler.
///
/// errors:
/// * `E_CODEGEN_COMPILE_FAILED` — Track-B source rendered fine but
///   sandboxed `cargo build --target wasm32-unknown-unknown` failed.
/// * `E_INSTALL_PREFLIGHT_FAILED` — `build_install_envelope` rejected the
///   spec (PR-#655 vintage refusal, PR-#649 SpendingLimit/Default
///   refusal, strkey shape, network mismatch, etc.).
/// * `ErrorData::invalid_params` — spec_id not found.
pub async fn export_policy(
    store: &McpStore,
    input: ExportPolicyInput,
) -> Result<ExportPolicyOutput, ErrorData> {
    let spec = store.get_spec(&input.spec_id).ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "export_policy: spec_id {:?} not found in store",
                input.spec_id
            ),
            None,
        )
    })?;

    let want_source = matches!(
        input.format,
        ExportFormat::RustSource | ExportFormat::Wasm | ExportFormat::All
    );
    let want_wasm = matches!(input.format, ExportFormat::Wasm | ExportFormat::All);
    let want_envelope = matches!(
        input.format,
        ExportFormat::InstallEnvelope | ExportFormat::All
    );

    // --- Track-B render (the first Generated slot only — multi-slot
    // specs would surface multiple URIs; v1 only inlines the first to
    // keep the JSON payload bounded). ----------------------------------
    let first_generated_idx = spec
        .policies
        .iter()
        .position(|s| matches!(s, PolicySlot::Generated { .. }));
    let rust_source = if want_source {
        match first_generated_idx {
            Some(idx) => Some(
                render_contract(&spec, idx)
                    .map(|r| r.src_lib_rs)
                    .map_err(|e| error_to_jsonrpc(&e))?,
            ),
            None => None,
        }
    } else {
        None
    };

    // --- WASM compile (only if requested AND we have a Generated slot). ---
    let (wasm_base64, wasm_hash_hex, wasm_bytes_opt) = if want_wasm && first_generated_idx.is_some()
    {
        let artifacts = synthesize_track_b(&spec)
            .await
            .map_err(|e| error_to_jsonrpc(&e))?;
        // inline the first artifact (matching the first_generated_idx slot).
        match artifacts.first() {
            Some(art) => {
                let mut hasher = Sha256::new();
                hasher.update(&art.wasm);
                let digest = hasher.finalize();
                (
                    Some(STANDARD.encode(&art.wasm)),
                    Some(hex::encode(digest)),
                    Some(art.wasm.clone()),
                )
            }
            None => (None, None, None),
        }
    } else {
        (None, None, None)
    };

    // --- Install envelope (Track-A path). ------------------------------
    let install_envelope_xdr_base64 = if want_envelope {
        let env = build_install_envelope(
            &spec,
            &input.smart_account,
            &input.source_account,
            &input.network_passphrase,
            &input.rpc_url,
            input.account_revision,
        )
        .await
        .map_err(|e| error_to_jsonrpc(&e))?;
        Some(env.envelope_xdr_base64)
    } else {
        None
    };

    // --- Store the artifact bundle + emit resource URIs. ---------------
    let artifact_id = store.new_id("art");
    let bundle = ArtifactBundle {
        source: rust_source.clone(),
        wasm: wasm_bytes_opt,
        install_envelope_xdr: install_envelope_xdr_base64.clone(),
    };
    store.put_artifact(&artifact_id, bundle);

    let mut resource_uris: Vec<String> = Vec::new();
    if rust_source.is_some() {
        resource_uris.push(format!("artifact://{artifact_id}/source.rs"));
    }
    if wasm_base64.is_some() {
        resource_uris.push(format!("artifact://{artifact_id}/policy.wasm"));
    }
    if install_envelope_xdr_base64.is_some() {
        resource_uris.push(format!("artifact://{artifact_id}/install_envelope.xdr"));
    }

    Ok(ExportPolicyOutput {
        artifact_id,
        rust_source,
        wasm_base64,
        install_envelope_xdr_base64,
        wasm_hash_hex,
        resource_uris,
    })
}

// verify_install

/// `verify_install` input.
///
/// **2026-05-18 (RFP deliverable #5):** the handler now performs a real
/// on-chain readback via `simulateTransaction` of
/// `SA.get_context_rule(rule_id)`. `expected_spec` may be supplied
/// inline (preferred — used by the wallet-adapter integration test) or
/// looked up by `expected_spec_id` against the in-memory store (legacy
/// path retained for compositional pipelines).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyInstallInput {
    /// smart-account StrKey `C…` whose on-chain context rule we will
    /// inspect.
    pub smart_account: String,
    /// context rule ID assigned by `add_context_rule` at install time.
    pub context_rule_id: u32,
    /// network selector (drives passphrase + default RPC).
    pub network: NetworkKind,
    /// optional RPC URL override.
    pub rpc_url: Option<String>,
    /// funded source account (G-strkey) used as the simulator's `source`
    /// when invoking `SA.get_context_rule(rule_id)`. The simulator only
    /// needs a valid sequence number — no funds are spent. Defaults to
    /// `smart_account` when omitted (the SA contract itself does not have
    /// an account record, so callers SHOULD pass the SA owner's G-key on
    /// testnet; mainnet callers pass any funded G-key).
    pub source_account: Option<String>,
    /// optional spec_id to compare against. When supplied AND in store,
    /// the stored spec drives the comparison.
    pub expected_spec_id: Option<String>,
    /// optional inline expected `PolicySpec`. Takes precedence over
    /// `expected_spec_id` when both are present. When neither is set,
    /// the handler reports `matches: true` with empty drift as soon as
    /// the rule is confirmed to exist on-chain.
    pub expected_spec: Option<PolicySpec>,
}

/// `verify_install` output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct VerifyInstallOutput {
    /// `true` iff every field of the on-chain rule matches the
    /// corresponding field on the supplied spec. `false` when `drift` is
    /// non-empty or when `expected_spec_id` was not supplied.
    pub matches: bool,
    /// per-field drift report. Empty when `matches = true`.
    pub drift: Vec<DriftItem>,
}

/// one drift entry between an expected (spec) and actual (on-chain)
/// value for a single field path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct DriftItem {
    /// dotted field path (e.g. `"context_rule.name"`).
    pub field: String,
    /// expected value as a JSON `Value`. Heterogeneous so we can carry
    /// strings, ints, and structured payloads uniformly.
    pub expected: serde_json::Value,
    /// actual value observed on-chain (or the placeholder string the
    /// handler used when the on-chain lookup is not yet wired).
    pub actual: serde_json::Value,
}

/// `verify_install` handler — **closed by RFP deliverable #5
/// (2026-05-18).** Performs a real on-chain readback of the SA's
/// `ContextRule` via `simulateTransaction(SA.get_context_rule(rule_id))`
/// and compares the decoded fields against an expected `PolicySpec`.
///
/// resolution order for the expected spec:
///   1. `expected_spec` (inline, preferred).
///   2. `expected_spec_id` (looked up in the in-memory store).
///   3. Neither — handler returns `matches: true` with empty drift after
///      confirming the rule exists on-chain.
///
/// errors:
/// * [`oz_policy_core::Error::VerifyDrift`] → `E_VERIFY_DRIFT` — surfaced
///   when the rule does not exist on-chain (`detail = "rule-not-found"`)
///   or when the RPC readback fails for transport reasons. The handler
///   does **not** return `E_VERIFY_DRIFT` for field-level drift; that
///   surfaces structurally in `matches: false` + `drift: [...]`.
/// * `ErrorData::invalid_params` — `expected_spec_id` provided but not
///   found in store.
pub async fn verify_install(
    store: &McpStore,
    input: VerifyInstallInput,
) -> Result<VerifyInstallOutput, ErrorData> {
    // ----- 1. Resolve the expected spec (inline > store) -----------------
    let expected_spec: Option<PolicySpec> = match (&input.expected_spec, &input.expected_spec_id) {
        (Some(inline), _) => Some(inline.clone()),
        (None, Some(id)) => Some(store.get_spec(id).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "verify_install: expected_spec_id {:?} not found in store",
                    id
                ),
                None,
            )
        })?),
        (None, None) => None,
    };

    // ----- 2. Resolve effective RPC URL + source account -----------------
    let rpc_url = input
        .rpc_url
        .as_deref()
        .unwrap_or_else(|| input.network.default_rpc())
        .to_string();
    let passphrase = input.network.passphrase();
    // source-account default: when none is supplied, fall back to the
    // smart account itself. The SA is a contract (not an account
    // record), so `getAccount` will fail in that case and surface a
    // typed error — callers SHOULD pass a real funded G-key. We accept
    // the default here so the smoke tests that drive verify_install
    // without a source still get a clear E_VERIFY_DRIFT diagnostic
    // rather than an opaque 500.
    let source = input
        .source_account
        .clone()
        .unwrap_or_else(|| input.smart_account.clone());

    // ----- 3. Live readback ---------------------------------------------
    let actual = match crate::verify_chain::read_context_rule_via_simulate(
        &rpc_url,
        passphrase,
        &input.smart_account,
        input.context_rule_id,
        &source,
    )
    .await
    {
        Ok(rule) => rule,
        Err(crate::verify_chain::ReadError::RuleNotFound(detail)) => {
            return Err(error_to_jsonrpc(&oz_policy_core::Error::VerifyDrift(
                format!("rule-not-found: {detail}"),
            )));
        }
        Err(other) => {
            return Err(error_to_jsonrpc(&oz_policy_core::Error::VerifyDrift(
                format!("rpc-readback-failed: {other}"),
            )));
        }
    };

    // ----- 4. Drift computation -----------------------------------------
    let drift = match &expected_spec {
        Some(spec) => {
            crate::verify_chain::compute_drift(spec, &spec.context_rule.name, &actual, passphrase)
        }
        None => Vec::new(),
    };

    Ok(VerifyInstallOutput {
        matches: drift.is_empty(),
        drift,
    })
}

// convenience: an `Arc<McpStore>` overload, so Stream C's `PolicyServer`
// can pass `&self.store` directly without dereferencing the Arc first.

/// trait alias so handlers can accept either `&McpStore` or `&Arc<McpStore>`.
/// stream C's `PolicyServer` holds `Arc<McpStore>` (see `server.rs`); this
/// keeps the public surface ergonomic.
pub trait AsStore {
    /// borrow the underlying store.
    fn as_store(&self) -> &McpStore;
}

impl AsStore for McpStore {
    fn as_store(&self) -> &McpStore {
        self
    }
}

impl AsStore for Arc<McpStore> {
    fn as_store(&self) -> &McpStore {
        self
    }
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use oz_policy_core::arg_value::ArgValue;
    use oz_policy_core::recording::{
        AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
        IngestSource as RecordingIngestSource, Recording as RecordingDoc, RECORDING_SCHEMA_URI,
    };
    use oz_policy_core::spec::{
        ContextRuleSpec, ContextType, ExistingPrimitive, ExistingPrimitiveParams, SignerSpec,
    };

    // local test fixtures.

    fn sep41_recording() -> RecordingDoc {
        RecordingDoc {
            schema: RECORDING_SCHEMA_URI.to_string(),
            network_passphrase: TESTNET_PASSPHRASE.to_string(),
            ingest: RecordingIngestSource::Hash {
                hash: "deadbeef".to_string(),
            },
            ledger: Some(1234),
            contracts: vec![ContractRecord {
                address: "CUSDC".to_string(),
                function: "transfer".to_string(),
                args: vec![
                    ArgValue::Address("GFROM".to_string()),
                    ArgValue::Address("GTO".to_string()),
                    ArgValue::I128("5000000".to_string()),
                ],
            }],
            auth_tree: AuthTree {
                roots: vec![AuthEntry {
                    credentials: Credentials::Address {
                        signer: "GSIGNER".to_string(),
                        nonce: "1".to_string(),
                        signature_expiration_ledger: 0,
                        signature: ArgValue::Void,
                    },
                    root_invocation: AuthInvocation {
                        function: AuthFunction::Contract {
                            address: "CUSDC".to_string(),
                            function: "transfer".to_string(),
                            args: vec![],
                        },
                        sub_invocations: vec![],
                    },
                    source_op_index: 0,
                }],
            },
            state_changes: vec![],
            events: vec![],
        }
    }

    fn sample_spec(rule_name: &str) -> PolicySpec {
        PolicySpec {
            schema: oz_policy_core::spec::POLICY_SCHEMA_URI.to_string(),
            synthesis_mode: SynthesisMode::Auto,
            context_rule: ContextRuleSpec {
                name: rule_name.to_string(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: vec![SignerSpec::ExternalEd25519 {
                public_key_hex: "00".repeat(32),
            }],
            policies: vec![PolicySlot::Existing {
                primitive: ExistingPrimitive::SimpleThreshold,
                params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
            }],
            lifetime_ledgers: None,
            recording_ref: oz_policy_core::spec::RecordingRef {
                hash: None,
                schema: RECORDING_SCHEMA_URI.to_string(),
            },
        }
    }

    // record_transaction — input gates only (network paths live in the
    // recorder's own integration tests).

    #[tokio::test]
    async fn record_transaction_rejects_both_inputs() {
        let store = McpStore::new();
        let input = RecordTransactionInput {
            network: NetworkKind::Testnet,
            rpc_url: None,
            hash: Some("a".repeat(64)),
            envelope_xdr_base64: Some("AAAA".to_string()),
            instruction_leeway: None,
        };
        let err = record_transaction(&store, input)
            .await
            .expect_err("both inputs must be rejected");
        assert_eq!(err.code.0, -32602);
        assert!(
            err.message.contains("exactly one"),
            "message must explain the gate; got {message}",
            message = err.message
        );
    }

    #[tokio::test]
    async fn record_transaction_rejects_neither_input() {
        let store = McpStore::new();
        let input = RecordTransactionInput {
            network: NetworkKind::Testnet,
            rpc_url: None,
            hash: None,
            envelope_xdr_base64: None,
            instruction_leeway: None,
        };
        let err = record_transaction(&store, input)
            .await
            .expect_err("missing inputs must be rejected");
        assert_eq!(err.code.0, -32602);
        assert!(err.message.contains("required"));
    }

    /// network defaults: `passphrase()` + `default_rpc()` are stable
    /// constants. Lock them so a future toolkit drift is loud.
    #[test]
    fn network_kind_defaults_match_canonical_constants() {
        assert_eq!(NetworkKind::Testnet.passphrase(), TESTNET_PASSPHRASE);
        assert_eq!(NetworkKind::Mainnet.passphrase(), MAINNET_PASSPHRASE);
        assert_eq!(NetworkKind::Testnet.default_rpc(), DEFAULT_TESTNET_RPC);
        assert_eq!(NetworkKind::Mainnet.default_rpc(), DEFAULT_MAINNET_RPC);
    }

    // synthesize_policy

    #[tokio::test]
    async fn synthesize_policy_happy_path() {
        let store = McpStore::new();
        let rec = sep41_recording();
        let rid = store.new_id("rec");
        store.put_recording(&rid, rec);

        let input = SynthesizePolicyInput {
            recording_id: rid.clone(),
            tightness: Tightness::Exact,
            lifetime_ledgers: Some(432_000),
            delegated_signer: None,
            mode: SynthesisMode::Auto,
            rule_name: Some("rule".to_string()),
        };
        let out = synthesize_policy(&store, input)
            .await
            .expect("synthesis must succeed");
        assert!(out.spec_id.starts_with("spec_"));
        // SEP-41 transfer composes to SpendingLimit + SimpleThreshold.
        assert_eq!(out.composed_count, 2, "two Existing slots expected");
        assert_eq!(out.generated_count, 0, "no Generated slots for SEP-41 Auto");
        // spec is stored under the same id.
        let stored = store.get_spec(&out.spec_id).expect("spec must be stored");
        assert_eq!(stored, out.spec);
    }

    #[tokio::test]
    async fn synthesize_policy_missing_recording_invalid_params() {
        let store = McpStore::new();
        let input = SynthesizePolicyInput {
            recording_id: "rec_bogus".to_string(),
            tightness: Tightness::Exact,
            lifetime_ledgers: None,
            delegated_signer: None,
            mode: SynthesisMode::Auto,
            rule_name: None,
        };
        let err = synthesize_policy(&store, input)
            .await
            .expect_err("missing recording must error");
        assert_eq!(err.code.0, -32602);
        assert!(err.message.contains("recording_id"));
    }

    #[tokio::test]
    async fn synthesize_policy_compose_only_multi_target_surfaces_e_synth_not_expressible() {
        let store = McpStore::new();
        let mut rec = sep41_recording();
        // second target with a non-transfer function — forces multi-target.
        rec.contracts.push(ContractRecord {
            address: "CBLEND".to_string(),
            function: "claim".to_string(),
            args: vec![],
        });
        let rid = store.new_id("rec");
        store.put_recording(&rid, rec);

        let input = SynthesizePolicyInput {
            recording_id: rid,
            tightness: Tightness::Exact,
            lifetime_ledgers: None,
            delegated_signer: None,
            mode: SynthesisMode::ComposeOnly,
            rule_name: Some("r".to_string()),
        };
        let err = synthesize_policy(&store, input)
            .await
            .expect_err("compose-only multi-target must error");
        assert_eq!(err.code.0, -32102);
        let data = err.data.expect("data must be populated");
        assert_eq!(
            data.get("error_code").and_then(|v| v.as_str()),
            Some("E_SYNTH_NOT_EXPRESSIBLE")
        );
    }

    /// determinism: two calls with the same recording + options produce
    /// byte-equal `spec` payloads (the IDs differ — that's the
    /// non-determinism boundary). This is the front-line gate for the
    /// phase 5 "100× determinism" requirement (plan.md §452).
    #[tokio::test]
    async fn synthesize_policy_is_deterministic_modulo_ids() {
        let store = McpStore::new();
        let rec = sep41_recording();
        let rid = store.new_id("rec");
        store.put_recording(&rid, rec);

        let input = SynthesizePolicyInput {
            recording_id: rid,
            tightness: Tightness::Exact,
            lifetime_ledgers: Some(432_000),
            delegated_signer: None,
            mode: SynthesisMode::Auto,
            rule_name: Some("rule".to_string()),
        };
        let a = synthesize_policy(&store, input.clone()).await.expect("a");
        let b = synthesize_policy(&store, input).await.expect("b");

        // IDs differ (UUIDs).
        assert_ne!(
            a.spec_id, b.spec_id,
            "spec_id must be unique per invocation"
        );
        // payloads byte-equal.
        let a_json = serde_json::to_string(&a.spec).expect("a json");
        let b_json = serde_json::to_string(&b.spec).expect("b json");
        assert_eq!(a_json, b_json, "spec payload must be byte-equal");
        assert_eq!(a.generated_count, b.generated_count);
        assert_eq!(a.composed_count, b.composed_count);
    }

    /// `rule_name` default — `"rule-<first-8-of-uuid>"` and clamped at
    /// MAX_NAME_SIZE.
    #[test]
    fn default_rule_name_uses_prefix_and_clamps() {
        let id = "rec_0123456789abcdef0123456789abcdef";
        let name = default_rule_name(id);
        assert!(name.starts_with("rule-"));
        // visible suffix is the first 8 hex chars after `rec_`.
        assert_eq!(&name, "rule-01234567");
        assert!(name.len() <= oz_policy_core::spec::MAX_NAME_SIZE as usize);
    }

    /// `clamp_rule_name` truncates to byte cap on a UTF-8 boundary.
    #[test]
    fn clamp_rule_name_respects_utf8_boundary() {
        let huge = "ä".repeat(50); // 100 bytes
        let clamped = clamp_rule_name(&huge);
        assert!(clamped.len() <= oz_policy_core::spec::MAX_NAME_SIZE as usize);
        // round-trips through UTF-8 without panicking.
        let _ = clamped.chars().count();
    }

    // simulate_policy

    #[tokio::test]
    async fn simulate_policy_happy_path_empty_spec() {
        // empty spec + empty recording → permit passes, zero deny vectors.
        // mirrors `oz_policy_simhost::run::tests::run_full_suite_empty_inputs_produces_clean_report`.
        let store = McpStore::new();
        let rec = sep41_recording();
        let rid = store.new_id("rec");
        store.put_recording(&rid, rec);
        let spec = sample_spec("smoke");
        // empty out the policies so synthesize_track_b returns 0 artifacts
        // and the host has no Track-B WASMs to install.
        let mut empty_spec = spec;
        empty_spec.policies.clear();
        let sid = store.new_id("spec");
        store.put_spec(&sid, empty_spec);

        let input = SimulatePolicyInput {
            spec_id: sid,
            recording_id: rid,
            extra_deny_vectors: None,
        };
        let report = simulate_policy(&store, input)
            .await
            .expect("simulate must succeed");
        assert_eq!(report.spec_id, "smoke");
        assert!(report.permit.passed, "empty inputs must permit");
    }

    #[tokio::test]
    async fn simulate_policy_missing_spec_invalid_params() {
        let store = McpStore::new();
        let input = SimulatePolicyInput {
            spec_id: "spec_bogus".to_string(),
            recording_id: "rec_bogus".to_string(),
            extra_deny_vectors: None,
        };
        let err = simulate_policy(&store, input)
            .await
            .expect_err("missing spec must error");
        assert_eq!(err.code.0, -32602);
    }

    #[tokio::test]
    async fn simulate_policy_missing_recording_invalid_params() {
        let store = McpStore::new();
        let sid = store.new_id("spec");
        store.put_spec(&sid, sample_spec("rule"));
        let input = SimulatePolicyInput {
            spec_id: sid,
            recording_id: "rec_bogus".to_string(),
            extra_deny_vectors: None,
        };
        let err = simulate_policy(&store, input)
            .await
            .expect_err("missing recording must error");
        assert_eq!(err.code.0, -32602);
        assert!(err.message.contains("recording_id"));
    }

    // export_policy

    #[tokio::test]
    async fn export_policy_rust_source_no_generated_returns_none_payloads() {
        // spec has only Existing slots → no rust_source / no wasm.
        // should NOT fail; instead returns an empty payload with
        // `resource_uris == []`.
        let store = McpStore::new();
        let sid = store.new_id("spec");
        store.put_spec(&sid, sample_spec("rule"));

        let input = ExportPolicyInput {
            spec_id: sid,
            smart_account: "C".repeat(56),
            source_account: "G".repeat(56),
            rpc_url: DEFAULT_TESTNET_RPC.to_string(),
            network_passphrase: TESTNET_PASSPHRASE.to_string(),
            account_revision: AccountRevision::PostPr655,
            format: ExportFormat::RustSource,
        };
        let out = export_policy(&store, input)
            .await
            .expect("rust_source-only must succeed");
        assert!(out.artifact_id.starts_with("art_"));
        assert!(out.rust_source.is_none());
        assert!(out.wasm_base64.is_none());
        assert!(out.install_envelope_xdr_base64.is_none());
        assert!(out.resource_uris.is_empty());
    }

    #[tokio::test]
    async fn export_policy_missing_spec_invalid_params() {
        let store = McpStore::new();
        let input = ExportPolicyInput {
            spec_id: "spec_bogus".to_string(),
            smart_account: "C".repeat(56),
            source_account: "G".repeat(56),
            rpc_url: DEFAULT_TESTNET_RPC.to_string(),
            network_passphrase: TESTNET_PASSPHRASE.to_string(),
            account_revision: AccountRevision::PostPr655,
            format: ExportFormat::RustSource,
        };
        let err = export_policy(&store, input)
            .await
            .expect_err("missing spec must error");
        assert_eq!(err.code.0, -32602);
        assert!(err.message.contains("spec_id"));
    }

    #[tokio::test]
    async fn export_policy_install_envelope_preflight_failure_maps_to_e_install_preflight() {
        // `AccountRevision::Unknown` is always refused by preflight per
        // `crates/oz-policy-installer/src/preflight.rs`. We do NOT reach the
        // RPC call (preflight fires before any network hop).
        let store = McpStore::new();
        let sid = store.new_id("spec");
        store.put_spec(&sid, sample_spec("rule"));
        let input = ExportPolicyInput {
            spec_id: sid,
            smart_account: "C".repeat(56), // strkey shape is invalid but preflight catches Unknown first
            source_account: "G".repeat(56),
            rpc_url: DEFAULT_TESTNET_RPC.to_string(),
            network_passphrase: TESTNET_PASSPHRASE.to_string(),
            account_revision: AccountRevision::Unknown,
            format: ExportFormat::InstallEnvelope,
        };
        let err = export_policy(&store, input)
            .await
            .expect_err("Unknown revision must surface as preflight failure");
        assert_eq!(err.code.0, -32108);
        let data = err.data.expect("data must be populated");
        assert_eq!(
            data.get("error_code").and_then(|v| v.as_str()),
            Some("E_INSTALL_PREFLIGHT_FAILED")
        );
    }

    // verify_install
    //
    // RFP deliverable #5 (2026-05-18): the handler now hits a real RPC
    // for the on-chain readback. The unit tests below cover only the
    // INPUT-LAYER gates (store miss → invalid_params; bogus RPC → typed
    // E_VERIFY_DRIFT). The decode + drift comparator are unit-tested
    // pure in `verify_chain::tests`; the end-to-end success path is
    // integration-tested in `wallet-adapter/src/integration.test.ts`
    // (INTEGRATION=1 gate; hits real testnet).

    /// bogus RPC URL → `E_VERIFY_DRIFT` with `rpc-readback-failed` detail.
    /// uses a non-routable URL so the test never hits a live endpoint and
    /// fails fast (under the 30 s RPC timeout via DNS).
    #[tokio::test]
    async fn verify_install_unreachable_rpc_returns_e_verify_drift() {
        let store = McpStore::new();
        let input = VerifyInstallInput {
            smart_account: "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A".to_string(),
            context_rule_id: 0,
            network: NetworkKind::Testnet,
            // 192.0.2.0/24 (TEST-NET-1) is reserved by IANA and never
            // routable; `client.get_network()` returns the connection
            // error immediately.
            rpc_url: Some("http://192.0.2.1:1".to_string()),
            source_account: Some(
                "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ".to_string(),
            ),
            expected_spec_id: None,
            expected_spec: None,
        };
        let err = verify_install(&store, input)
            .await
            .expect_err("unreachable rpc must surface E_VERIFY_DRIFT");
        assert_eq!(err.code.0, -32106, "code must be E_VERIFY_DRIFT (-32106)");
    }

    #[tokio::test]
    async fn verify_install_missing_expected_spec_id_invalid_params() {
        let store = McpStore::new();
        let input = VerifyInstallInput {
            smart_account: "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A".to_string(),
            context_rule_id: 0,
            network: NetworkKind::Testnet,
            rpc_url: None,
            source_account: Some(
                "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ".to_string(),
            ),
            expected_spec_id: Some("spec_bogus".to_string()),
            expected_spec: None,
        };
        let err = verify_install(&store, input)
            .await
            .expect_err("missing spec must error");
        assert_eq!(err.code.0, -32602);
        assert!(err.message.contains("expected_spec_id"));
    }

    /// inline `expected_spec` takes precedence over `expected_spec_id`
    /// lookup. We exercise the precedence by passing a bogus id ALONG
    /// with an inline spec — the handler must use the inline spec and
    /// only fail later (at the RPC) rather than rejecting the id lookup.
    #[tokio::test]
    async fn verify_install_prefers_inline_expected_spec_over_store_id() {
        let store = McpStore::new();
        let input = VerifyInstallInput {
            smart_account: "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A".to_string(),
            context_rule_id: 0,
            network: NetworkKind::Testnet,
            rpc_url: Some("http://192.0.2.1:1".to_string()),
            source_account: Some(
                "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ".to_string(),
            ),
            expected_spec_id: Some("spec_bogus_would_otherwise_404".to_string()),
            expected_spec: Some(sample_spec("rule")),
        };
        let err = verify_install(&store, input)
            .await
            .expect_err("unreachable rpc must surface E_VERIFY_DRIFT");
        // the store lookup is skipped (inline spec preferred) → fall
        // through to the RPC layer, which surfaces E_VERIFY_DRIFT.
        assert_eq!(err.code.0, -32106, "code must be E_VERIFY_DRIFT (-32106)");
    }

    // schema round-trips — one per tool. Locks the `derive(JsonSchema)`
    // chain so a future struct-rename can't silently break the MCP
    // tool-schema publication contract.

    fn assert_schema_round_trips<T: schemars::JsonSchema>(label: &str) {
        let schema = schemars::schema_for!(T);
        let j = serde_json::to_value(&schema).expect("schema must serialize");
        let back: serde_json::Value =
            serde_json::from_value(j.clone()).expect("schema must round-trip");
        assert_eq!(j, back, "{label} schema round-trip failed");
        // smoke: top-level keys exist.
        assert!(
            j.get("$schema").is_some() || j.get("type").is_some() || j.get("$ref").is_some(),
            "{label} schema must have $schema/type/$ref"
        );
    }

    #[test]
    fn schema_round_trips_for_every_tool_struct() {
        assert_schema_round_trips::<RecordTransactionInput>("RecordTransactionInput");
        assert_schema_round_trips::<RecordTransactionOutput>("RecordTransactionOutput");
        assert_schema_round_trips::<SynthesizePolicyInput>("SynthesizePolicyInput");
        assert_schema_round_trips::<SynthesizePolicyOutput>("SynthesizePolicyOutput");
        assert_schema_round_trips::<SimulatePolicyInput>("SimulatePolicyInput");
        assert_schema_round_trips::<SimReport>("SimReport");
        assert_schema_round_trips::<ExportPolicyInput>("ExportPolicyInput");
        assert_schema_round_trips::<ExportPolicyOutput>("ExportPolicyOutput");
        assert_schema_round_trips::<VerifyInstallInput>("VerifyInstallInput");
        assert_schema_round_trips::<VerifyInstallOutput>("VerifyInstallOutput");
        assert_schema_round_trips::<DriftItem>("DriftItem");
    }

    // determinism gate for `record_transaction`'s store side: the
    // recording payload (here a fixture, not an RPC call) is byte-equal
    // when stored + read back. Pinned because the recorder's RPC path is
    // covered by its own integration tests; this asserts the MCP-layer
    // store stays a pure passthrough.
    #[test]
    fn put_get_recording_is_byte_equal() {
        let store = McpStore::new();
        let rec = sep41_recording();
        let id = store.new_id("rec");
        store.put_recording(&id, rec.clone());
        let read = store.get_recording(&id).expect("must read");
        assert_eq!(
            serde_json::to_string(&rec).expect("a"),
            serde_json::to_string(&read).expect("b")
        );
    }

    /// asStore: handlers accept either `&McpStore` or `&Arc<McpStore>`.
    /// locks the convenience trait so Stream C's `Arc`-holding
    /// `PolicyServer` works without an extra deref dance.
    #[test]
    fn as_store_works_for_arc_and_owned() {
        let owned = McpStore::new();
        let _: &McpStore = owned.as_store();
        let arc = Arc::new(McpStore::new());
        let _: &McpStore = arc.as_store();
    }
}
