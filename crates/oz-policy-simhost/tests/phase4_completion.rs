//! Phase 4 binary completion gate.
//!
//! Mirrors the Phase 3 pattern (`oz-policy-codegen/tests/phase3_completion.rs`)
//! one level up: the simhost completion test exercises the full
//! `run_full_suite` pipeline end-to-end against the frozen
//! `walkthroughs/phase3-codegen-fixture` artifacts. It is **never
//! `#[ignore]`** — every `cargo nextest run --workspace` run hits it.
//!
//! ## Surface tested
//!
//! 1. Loads `walkthroughs/phase3-codegen-fixture/spec.json` (Track-B,
//!    `function_allowlist = ["transfer"]`).
//! 2. Loads the prebuilt
//!    `walkthroughs/phase3-codegen-fixture/expected/slot_0/policy.wasm` —
//!    no recompile, no fabricated bytes (per the Phase 4 Round 2 brief's
//!    "no fabricated WASM" rule). Hash is recomputed over the on-disk
//!    bytes via the same `sha2` 0.10.9 the simhost uses for its
//!    smart-account verify path, and asserted against
//!    `expected/slot_0/wasm_hash.txt` so a drift between the two on-disk
//!    files trips this test rather than silently passing.
//! 3. Synthesizes a minimal `Recording` that matches the spec's single
//!    `FunctionAllowlist` constraint — one `ContractRecord` with
//!    `function == "transfer"`. The recording's `auth_tree` carries one
//!    `Address`-credentialed signer (deterministically derived from a
//!    32-byte seed; see `valid_signer_strkey()` below).
//! 4. Calls `run_full_suite(&spec, &recording, &[artifact], vec![])`.
//! 5. Asserts:
//!    * `report.permit.passed` (the recording's `transfer` call is
//!      admitted by the installed policy).
//!    * Every `report.deny_results[i].passed == true` — the auto-
//!      generated deny vectors (currently the
//!      `function_allowlist_wrong_function` boundary mutation) all panic
//!      with the expected `PolicyError::FunctionNotAllowed (1010)` code.
//!    * `report.total_vectors > 0` so a future bug that disables the
//!      generator (returning an empty vector list) trips the gate.
//!
//! ## What this test is honest about
//!
//! Per `crates/oz-policy-simhost/src/host.rs` "Why not the full
//! `__check_auth → add_policy → enforce` chain?", the run orchestrator
//! invokes each installed policy's `enforce` directly per `TestContext`
//! rather than driving the smart-account's `__check_auth` boundary
//! (which would require wallet-signed `AuthEntry` credentials — Phase 7
//! work). This is the same observable surface deny vectors and the
//! permit replay need; the Phase 4 binary completion criterion is "every
//! constraint primitive in the spec produces at least one passing permit
//! AND at least one passing deny vector," NOT "the full __check_auth
//! wrapper works."

use oz_policy_codegen::CompiledArtifact;
use oz_policy_core::recording::{
    AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials, IngestSource,
    Recording, RECORDING_SCHEMA_URI,
};
use oz_policy_core::spec::PolicySpec;
use oz_policy_core::ArgValue;
use std::path::PathBuf;

const FIXTURE_DIR: &str = "../../walkthroughs/phase3-codegen-fixture";

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(FIXTURE_DIR);
    p
}

fn load_spec() -> PolicySpec {
    let path = fixture_dir().join("spec.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read spec at {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse spec at {}: {e}", path.display()))
}

fn load_policy_wasm() -> Vec<u8> {
    let path = fixture_dir().join("expected/slot_0/policy.wasm");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("read policy WASM at {}: {e}", path.display()));
    assert!(
        !bytes.is_empty(),
        "Phase 4 completion: policy WASM at {} is empty",
        path.display()
    );
    assert_eq!(
        &bytes[..4],
        b"\0asm",
        "Phase 4 completion: policy WASM at {} lacks magic header",
        path.display()
    );
    bytes
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn assert_hash_matches_pinned(wasm: &[u8]) {
    let actual = sha256_hex(wasm);
    let path = fixture_dir().join("expected/slot_0/wasm_hash.txt");
    let pinned_raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read wasm_hash.txt at {}: {e}", path.display()));
    let pinned = pinned_raw.trim();
    assert_eq!(
        actual, pinned,
        "Phase 4 completion: policy.wasm hash drifted from pinned wasm_hash.txt — \
         re-run the codegen fixture refresh to update both in sync"
    );
}

/// Build a minimal `Recording` whose single `ContractRecord` matches the
/// fixture spec's `function_allowlist = ["transfer"]` constraint, so the
/// permit branch is *expected* to pass.
///
/// One `AuthEntry` is included with a deterministically-derived
/// `Address`-credentialed signer; the simhost's `collect_signer_addresses`
/// walks the entry into `AuthPayload.signer_addresses`.
fn construct_minimal_matching_recording() -> Recording {
    // The fixture's `context_type` is `CallContract("CDG7N5LG...")`. Use
    // that as the target contract for the one `transfer` call.
    let target = "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH";
    let signer = valid_signer_strkey();

    Recording {
        schema: RECORDING_SCHEMA_URI.into(),
        network_passphrase: "Test SDF Network ; September 2015".into(),
        ingest: IngestSource::Hash {
            hash: "phase4-completion-fixture".into(),
        },
        ledger: Some(1_700_000),
        contracts: vec![ContractRecord {
            address: target.into(),
            function: "transfer".into(),
            args: vec![
                ArgValue::Address(signer.clone()),
                ArgValue::Address(signer.clone()),
                ArgValue::I128("100".into()),
            ],
        }],
        auth_tree: AuthTree {
            roots: vec![AuthEntry {
                credentials: Credentials::Address {
                    signer: signer.clone(),
                    nonce: "1".into(),
                    signature_expiration_ledger: 2_000_000,
                    signature: ArgValue::Void,
                },
                root_invocation: AuthInvocation {
                    function: AuthFunction::Contract {
                        address: target.into(),
                        function: "transfer".into(),
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

/// Build a deterministic G-StrKey from a fixed 32-byte seed. The
/// previously-hardcoded literal in this codebase (`GAEEZQIBQHBP...`) is a
/// bogus checksum that `stellar-strkey 0.0.13` rejects with
/// `Err(Invalid)` — see the Phase 4 Round 2 fix in
/// `tests/host_smoke.rs` for the full story.
fn valid_signer_strkey() -> String {
    stellar_strkey::ed25519::PublicKey([0xaau8; 32]).to_string()
}

/// Phase 4 binary completion gate.
///
/// Drives `run_full_suite` against the frozen `phase3-codegen-fixture`
/// artifacts and asserts the resulting `SimReport` is fully passing.
/// See the module doc-comment for the surface this test exercises and
/// the wider `__check_auth` gap it does NOT (deliberately).
#[tokio::test]
async fn phase4_simulate_emits_passing_report() {
    // 1. Spec + WASM.
    let spec = load_spec();
    let wasm = load_policy_wasm();
    assert_hash_matches_pinned(&wasm);

    // 2. Recording.
    let recording = construct_minimal_matching_recording();

    // 3. Build the CompiledArtifact directly from the on-disk WASM.
    //    `source` is empty (not needed for simulation); `cache_hit` is
    //    `false` to make the artifact's provenance explicit (the bytes
    //    came from disk, not from a sandbox cache hit). The hash field
    //    is recomputed here so a drift between the WASM bytes and the
    //    artifact metadata is impossible.
    let mut wasm_hash_arr = [0u8; 32];
    {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&wasm);
        let out = h.finalize();
        wasm_hash_arr.copy_from_slice(&out);
    }
    let artifact = CompiledArtifact {
        wasm: wasm.clone(),
        wasm_hash: wasm_hash_arr,
        source: String::new(),
        cache_hit: false,
    };

    // 4. Run the full suite.
    let report = oz_policy_simhost::run::run_full_suite(&spec, &recording, &[artifact], vec![])
        .await
        .expect("Phase 4 completion: run_full_suite must succeed on the frozen fixture");

    // 5. Assertions.
    assert!(
        report.permit.passed,
        "Phase 4 completion: permit case failed: {:?}",
        report.permit.error
    );
    assert!(
        report.total_vectors > 0,
        "Phase 4 completion: deny generator returned zero vectors — the \
         function_allowlist constraint should have produced at least one \
         wrong-function boundary mutation"
    );
    for r in &report.deny_results {
        assert!(
            r.passed,
            "Phase 4 completion: deny vector failed: name={} expected_error_code={} \
             actual_error_code={:?}",
            r.name, r.expected_error_code, r.actual_error_code
        );
    }
    assert_eq!(
        report.passed, report.total_vectors,
        "Phase 4 completion: passed-count ({}) != total_vectors ({})",
        report.passed, report.total_vectors
    );

    // 6. Spec id is the rule name (stable across runs).
    assert_eq!(report.spec_id, "phase3-fixture");
}
