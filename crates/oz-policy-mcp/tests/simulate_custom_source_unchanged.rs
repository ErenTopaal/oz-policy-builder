//! end-to-end integration test for `simulate_custom_source`.
//!
//! drives the full `record_transaction → synthesize_policy →
//! simulate_custom_source(modified_lib_rs = original)` pipeline against
//! the **real** in-tree codegen sandbox + simhost — no mocks, no stubs.
//! The `modified_lib_rs` parameter is byte-equal to the spec's own
//! rendered source, so the substitution path runs but the resulting
//! WASM is the same one `simulate_policy` would have built; the assert
//! is that `report.permit.passed == true`.
//!
//! `#[ignore]` because the codegen sandbox needs cargo+rustc on PATH, the
//! `wasm32-unknown-unknown` target installed, `stellar` on PATH, and a
//! warm `~/.cargo/registry`. CI runs this via `--include-ignored`.
//!
//! pre-warm hint: if you see `E_CODEGEN_COMPILE_FAILED` mentioning
//! "registry empty", first run the regular workspace tests once so the
//! transitive `soroban-sdk` closure lands in the cache, then re-run.

use std::sync::Arc;

use oz_policy_codegen::render_contract;
use oz_policy_core::arg_value::ArgValue;
use oz_policy_core::recording::{
    AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
    IngestSource as RecordingIngestSource, Recording, RECORDING_SCHEMA_URI,
};
use oz_policy_core::spec::{
    Constraint, ContextRuleSpec, ContextType, PolicySlot, PolicySpec, RecordingRef, SignerSpec,
    SynthesisMode, TemplateFamily, POLICY_SCHEMA_URI,
};
use oz_policy_mcp::tools::simulate_custom_source::{
    simulate_custom_source, SimulateCustomSourceInput,
};
use oz_policy_mcp::McpStore;

const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";

/// minimum SEP-41 `transfer` recording — the same shape Phase-5 unit tests
/// use as a stand-in for the recorder's output. We pre-seed it directly
/// because `record_transaction` would require Soroban RPC; the recorder
/// is integration-tested separately in `oz-policy-recorder` and the spec
/// for THIS test is the substitution + simhost path, not the recorder.
// real StrKey-shaped placeholders the simhost's
// `TestHost::build_context_contract_scval` will accept. We compute the
// G-address from a deterministic 32-byte seed because the bogus literal
// `GAEEZ…` used in `crates/oz-policy-simhost/src/permit.rs` has an
// invalid checksum (`stellar-strkey::ed25519::PublicKey::from_string`
// returns `Err(Invalid)`) — see `crates/oz-policy-simhost/tests/host_smoke.rs`
// Round-2 note. The simhost permit test only survived because it never
// reached `build_context_contract_scval`; the moment a Track-B policy
// is installed (this test installs one) the strkey decode runs.
//
// C-address (contract) is the round-tripped checksum from the same test
// file — `stellar-strkey` accepts it directly.
const TOKEN_C_ADDR: &str = "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH";

fn signer_strkey() -> String {
    stellar_strkey::ed25519::PublicKey([0xaau8; 32]).to_string()
}

fn sep41_recording() -> Recording {
    let signer = signer_strkey();
    Recording {
        schema: RECORDING_SCHEMA_URI.to_string(),
        network_passphrase: TESTNET_PASSPHRASE.to_string(),
        ingest: RecordingIngestSource::Hash {
            hash: "deadbeef".to_string(),
        },
        ledger: Some(1234),
        contracts: vec![ContractRecord {
            address: TOKEN_C_ADDR.to_string(),
            function: "transfer".to_string(),
            args: vec![
                ArgValue::Address(signer.clone()),
                ArgValue::Address(signer.clone()),
                ArgValue::I128("100".to_string()),
            ],
        }],
        auth_tree: AuthTree {
            roots: vec![AuthEntry {
                credentials: Credentials::Address {
                    signer: signer.clone(),
                    nonce: "1".to_string(),
                    signature_expiration_ledger: 1000,
                    signature: ArgValue::Void,
                },
                root_invocation: AuthInvocation {
                    function: AuthFunction::Contract {
                        address: TOKEN_C_ADDR.to_string(),
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

/// hand-rolled spec with ONE `Generated` `FunctionAllowlist` slot — the
/// minimum shape that exercises the substitution path in
/// `simulate_custom_source` (which keys off `PolicySlot::Generated`).
fn spec_with_one_generated_slot() -> PolicySpec {
    PolicySpec {
        schema: POLICY_SCHEMA_URI.to_string(),
        synthesis_mode: SynthesisMode::CodegenOnly,
        context_rule: ContextRuleSpec {
            name: "playground-test".to_string(),
            context_type: ContextType::Default,
            valid_until: None,
        },
        signers: vec![SignerSpec::ExternalEd25519 {
            public_key_hex: "00".repeat(32),
        }],
        policies: vec![PolicySlot::Generated {
            template_family: TemplateFamily::FunctionAllowlist,
            constraints: vec![Constraint::FunctionAllowlist {
                functions: vec!["transfer".to_string()],
            }],
        }],
        lifetime_ledgers: None,
        recording_ref: RecordingRef {
            hash: None,
            schema: RECORDING_SCHEMA_URI.to_string(),
        },
    }
}

#[ignore = "needs cargo/rustc/stellar on PATH + warm ~/.cargo/registry"]
#[tokio::test(flavor = "multi_thread")]
async fn simulate_custom_source_unchanged_lib_rs_permit_passes() {
    let store: Arc<McpStore> = Arc::new(McpStore::new());

    // (1) "record_transaction" — pre-seed the recorder cache. The
    // recording-id surfaced here is what the tool would have returned;
    // we use the existing in-memory store path because hitting Soroban
    // RPC from a Rust integration test would couple the test to network
    // weather, which violates the deterministic-test contract.
    let recording_id = store.new_id("rec");
    store.put_recording(&recording_id, sep41_recording());

    // (2) "synthesize_policy" — store the hand-rolled spec. We don't go
    // through the synthesizer because we want an EXACT spec shape (one
    // Generated FunctionAllowlist slot); the synthesizer's choice of
    // Existing vs Generated under `Auto` is independent of the
    // substitution path we're testing.
    let spec_id = store.new_id("spec");
    let spec = spec_with_one_generated_slot();
    store.put_spec(&spec_id, spec.clone());

    // (3) render the spec's first Generated slot through the SAME path
    // `get_policy_artifacts` (and the playground frontend) use, so the
    // `modified_lib_rs` we pass below is byte-equal to what the
    // un-edited Source tab would show.
    let rendered =
        render_contract(&spec, 0).expect("render_contract on the Generated slot must succeed");

    // (4) "simulate_custom_source" — modified_lib_rs is the original
    // rendered source verbatim. The substitution path runs, the
    // sandbox build hits cache (or rebuilds, then caches), and the
    // simhost replays the recording's permit branch.
    let input = SimulateCustomSourceInput {
        recording_id: recording_id.clone(),
        spec_id: spec_id.clone(),
        modified_lib_rs: rendered.src_lib_rs.clone(),
        extra_deny_vectors: None,
    };
    let report = simulate_custom_source(&store, input)
        .await
        .expect("simulate_custom_source on an unchanged source must succeed");

    assert!(
        report.permit.passed,
        "permit branch must pass for an unmodified Track-B source — \
         report = {report:?}"
    );
}
