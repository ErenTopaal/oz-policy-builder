//! phase 4 — top-level run orchestrator.
//!
//! [`run_full_suite`] is the binary entry point the CLI (`simulate`
//! subcommand) and the MCP server (`simulate` tool, Phase 5) drive.
//!
//! for a given `(spec, recording, wasm_per_slot, extra_deny)` it:
//!
//! 1. Builds a [`TestHost`] seeded with a deterministic ledger sequence
//!    derived from `recording.ledger.unwrap_or(SIMHOST_DEFAULT_LEDGER)`.
//! 2. Installs the smart account.
//! 3. Installs each Track-B policy slot (in slot order) plus a placeholder
//!    `ArgValue::Map` install-param.
//! 4. Runs [`crate::permit::replay_recording`] → records pass/fail in the
//!    `permit` field of the report.
//! 5. Calls [`crate::deny::generate_deny_vectors`] with the canonical
//!    phase-4 seed (42) and appends `extra_deny`. For each vector, invokes
//!    `__check_auth` and asserts the panic matches `expected_error_code`.
//! 6. Returns a [`SimReport`] — fully `Serialize + Deserialize +
//!    jsonSchema` so it can be JSON-dumped to disk for `oz-policy-cli` or
//!    routed back through the MCP boundary.
//!
//! determinism contract: identical inputs produce identical reports. The
//! only non-determinism the harness tolerates is in the host's address
//! generation, which is seeded by `SIMHOST_PRNG_SEED`.

use oz_policy_codegen::CompiledArtifact;
use oz_policy_core::recording::Recording;
use oz_policy_core::spec::PolicySpec;
use oz_policy_core::{ArgValue, Error, MapEntry};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::deny::{generate_deny_vectors, DenyVector};
use crate::host::{HostExecError, TestHost};
use crate::permit::replay_recording;

/// canonical RNG seed for the deny-generator. Held in `run.rs` (rather
/// than `deny.rs`) because the orchestrator is the contract surface — if
/// a future caller wants a custom seed it goes through `run_full_suite`'s
/// own knob, not by patching the generator's signature.
pub const SIMHOST_DENY_RNG_SEED: u64 = 42;

/// default ledger sequence used when the recording doesn't carry one
/// (i.e. simulation-mode recordings). The simhost's protocol behaviour is
/// independent of the exact ledger number, but we fix one so the report
/// timestamps are stable across runs.
pub const SIMHOST_DEFAULT_LEDGER: u32 = 1_700_000;

/// the single context-rule slot the orchestrator binds policies under.
/// real on-chain SA deployments register policies against rule IDs
/// assigned by `add_context_rule`; the simhost stubs a fixed value here
/// so the report is reproducible.
pub const SIMHOST_DEFAULT_CONTEXT_RULE_ID: u32 = 0;

/// top-level simulation report. Serializable so consumers can route it
/// through the CLI's `--out <path>.json` flag or the MCP transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SimReport {
    /// schema-ish identifier for the spec under test — currently derived
    /// from the spec's `context_rule.name` because `PolicySpec` itself
    /// doesn't carry a stable id. Wire-stable across runs of the same
    /// spec.
    pub spec_id: String,
    pub permit: PermitResult,
    pub deny_results: Vec<DenyResult>,
    /// total deny vectors evaluated (generated + extra). Always equals
    /// `deny_results.len()`; surfaced for backward-compatible JSON
    /// consumers that grep on this field.
    pub total_vectors: usize,
    /// number of `deny_results` that passed (panicked with the expected
    /// code).
    pub passed: usize,
    /// ledger sequence the host was constructed with. Deterministic.
    pub timestamp_ledger: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PermitResult {
    pub passed: bool,
    /// `None` on success; on failure, the canonical `E_SIM_*` code +
    /// detail string (`Error::to_string()` value).
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DenyResult {
    pub name: String,
    /// `true` iff `__check_auth` panicked with exactly
    /// `expected_error_code`. Permit-through (`Ok(())`) or panic-with-
    /// wrong-code both produce `false`.
    pub passed: bool,
    pub expected_error_code: u32,
    /// `Some(code)` if `__check_auth` panicked; the code is the major
    /// bits of the Soroban `Error` value. `None` if it returned `Ok` —
    /// i.e., the deny vector failed open.
    pub actual_error_code: Option<u32>,
}

/// top-level entry point. See module doc-comment.
pub async fn run_full_suite(
    spec: &PolicySpec,
    recording: &Recording,
    wasm_per_slot: &[CompiledArtifact],
    extra_deny: Vec<DenyVector>,
) -> Result<SimReport, Error> {
    let ledger = recording.ledger.unwrap_or(SIMHOST_DEFAULT_LEDGER);
    let mut host = TestHost::new(ledger, &recording.network_passphrase)?;
    let sa = host.install_smart_account("")?;

    // install each Track-B policy slot in declared order. We pair the
    // compiled artifact with its slot index; the orchestrator does NOT
    // pair by `PolicySlot::Generated` filter because the synthesizer
    // already does that and produces `wasm_per_slot` in matching order
    // (see `oz_policy_codegen::synthesize_track_b`).
    for (idx, artifact) in wasm_per_slot.iter().enumerate() {
        host.install_policy(
            &artifact.wasm,
            &sa,
            SIMHOST_DEFAULT_CONTEXT_RULE_ID,
            default_install_params(idx as u32),
        )?;
    }

    // --- Permit branch ------------------------------------------------
    let permit = match replay_recording(&mut host, recording, &sa, SIMHOST_DEFAULT_CONTEXT_RULE_ID)
    {
        Ok(()) => PermitResult {
            passed: true,
            error: None,
        },
        Err(e) => PermitResult {
            passed: false,
            error: Some(e.to_string()),
        },
    };

    // --- Deny branch --------------------------------------------------
    let mut vectors = generate_deny_vectors(spec, recording, SIMHOST_DENY_RNG_SEED);
    vectors.extend(extra_deny);

    let mut deny_results = Vec::with_capacity(vectors.len());
    let mut passed = 0usize;
    for v in &vectors {
        let outcome = host.invoke_check_auth(&sa, v.payload.clone(), v.contexts.clone());
        let (passed_this, actual) = match outcome {
            Ok(()) => (false, None),
            Err(HostExecError::PolicyPanic(code)) => (code == v.expected_error_code, Some(code)),
            Err(_) => (false, None),
        };
        if passed_this {
            passed += 1;
        }
        deny_results.push(DenyResult {
            name: v.name.clone(),
            passed: passed_this,
            expected_error_code: v.expected_error_code,
            actual_error_code: actual,
        });
    }

    Ok(SimReport {
        spec_id: spec.context_rule.name.clone(),
        permit,
        total_vectors: deny_results.len(),
        passed,
        deny_results,
        timestamp_ledger: host.initial_ledger_seq(),
    })
}

/// default install-params struct fed into `install_policy`. Matches the
/// `InstallParams { _marker: u32 }` shape rendered by Track-B codegen
/// (see `walkthroughs/phase3-codegen-fixture/expected/slot_0/source.rs:54-62`).
pub fn default_install_params(marker: u32) -> ArgValue {
    ArgValue::Map(Some(vec![MapEntry {
        key: ArgValue::Symbol("_marker".into()),
        value: ArgValue::U32(marker),
    }]))
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use oz_policy_core::recording::{AuthTree, IngestSource, RECORDING_SCHEMA_URI};
    use oz_policy_core::spec::{
        ContextRuleSpec, ContextType, PolicySpec, RecordingRef, SynthesisMode, POLICY_SCHEMA_URI,
    };

    fn empty_spec(rule_name: &str) -> PolicySpec {
        PolicySpec {
            schema: POLICY_SCHEMA_URI.into(),
            synthesis_mode: SynthesisMode::Auto,
            context_rule: ContextRuleSpec {
                name: rule_name.into(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: vec![],
            policies: vec![],
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: None,
                schema: RECORDING_SCHEMA_URI.into(),
            },
        }
    }

    fn empty_recording() -> Recording {
        Recording {
            schema: RECORDING_SCHEMA_URI.into(),
            network_passphrase: "Test SDF Network ; September 2015".into(),
            ingest: IngestSource::Hash {
                hash: "deadbeef".into(),
            },
            ledger: Some(123),
            contracts: vec![],
            auth_tree: AuthTree { roots: vec![] },
            state_changes: vec![],
            events: vec![],
        }
    }

    /// smoke: empty spec + empty recording + no extra deny vectors → a
    /// permit-passes / zero-deny report, with `timestamp_ledger` reflecting
    /// the recording's ledger.
    #[tokio::test]
    async fn run_full_suite_empty_inputs_produces_clean_report() {
        let spec = empty_spec("smoke-rule");
        let recording = empty_recording();
        let report = run_full_suite(&spec, &recording, &[], vec![])
            .await
            .expect("run_full_suite empty");
        assert_eq!(report.spec_id, "smoke-rule");
        assert!(report.permit.passed, "empty recording must permit");
        assert!(report.permit.error.is_none());
        assert_eq!(report.deny_results.len(), 0);
        assert_eq!(report.total_vectors, 0);
        assert_eq!(report.passed, 0);
        assert_eq!(report.timestamp_ledger, 123);
    }

    /// the report's JSON shape includes every field name we rely on
    /// downstream. Lock this in so a derive rename can't silently break
    /// the CLI's `--out report.json` contract.
    #[tokio::test]
    async fn report_serialises_with_stable_field_names() {
        let spec = empty_spec("rule");
        let recording = empty_recording();
        let report = run_full_suite(&spec, &recording, &[], vec![])
            .await
            .expect("run");
        let j = serde_json::to_value(&report).expect("json");
        for k in [
            "spec_id",
            "permit",
            "deny_results",
            "total_vectors",
            "passed",
            "timestamp_ledger",
        ] {
            assert!(j.get(k).is_some(), "missing top-level field: {k}");
        }
        let p = j.get("permit").unwrap();
        for k in ["passed", "error"] {
            assert!(p.get(k).is_some(), "missing permit field: {k}");
        }
    }

    /// determinism: two identical invocations produce byte-equal JSON.
    /// mirrors the `oz-policy-codegen` Phase 3 byte-equal pattern.
    #[tokio::test]
    async fn run_full_suite_is_deterministic_for_same_input() {
        let spec = empty_spec("rule");
        let recording = empty_recording();
        let a = run_full_suite(&spec, &recording, &[], vec![])
            .await
            .expect("a");
        let b = run_full_suite(&spec, &recording, &[], vec![])
            .await
            .expect("b");
        let a_json = serde_json::to_string(&a).expect("a json");
        let b_json = serde_json::to_string(&b).expect("b json");
        assert_eq!(a_json, b_json, "report must be byte-equal across runs");
    }

    /// `default_install_params` produces the shape rendered Track-B
    /// policies expect — `Map { _marker: U32(marker) }`.
    #[test]
    fn default_install_params_shape() {
        let av = default_install_params(7);
        match av {
            ArgValue::Map(Some(entries)) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].key, ArgValue::Symbol("_marker".into()));
                assert_eq!(entries[0].value, ArgValue::U32(7));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }
}
