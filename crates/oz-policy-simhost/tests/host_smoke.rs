//! end-to-end smoke test exercising the `TestHost` wrapper against a real
//! track-B policy WASM.
//!
//! `#[ignore]` because:
//!   1. It loads the ~48 KB vendored smart-account WASM from disk and runs
//!      a real `register_test_contract_wasm` + per-context `enforce` call
//!      cycle. That's slow relative to the lib's pure-logic tests.
//!   2. It depends on the Phase 3 walkthrough fixture
//!      (`walkthroughs/phase3-codegen-fixture/expected/slot_0/policy.wasm`)
//!      existing on disk. The default `cargo nextest run` pass therefore
//!      should stay decoupled from the walkthrough fixture.
//!
//! run with `cargo nextest run -p oz-policy-simhost --run-ignored=only`.
//!
//! # Phase 4 Round 2 outcome
//!
//! round 1's note here claimed `install_policy` failed with
//! `HostError(WasmVm, InvalidAction)`. The Round 2 investigation showed
//! the test never reached `install_policy` at all: the hardcoded
//! signer-account StrKey
//! `GAEEZQIBQHBP3CG3F2BSTQHBHM5LJUFRTL2EFRC6CN4MV3OWJZ74C6XR` is a bogus
//! checksum (`stellar-strkey::ed25519::PublicKey::from_string` returns
//! `Err(Invalid)`), and the failure surfaced inside
//! `invoke_policy_enforce → build_context_contract_scval` as
//! `SetupFailed("unrecognised StrKey: …")` long before the host saw any
//! contextRule bytes.
//!
//! with the signer rebuilt from a valid 32-byte seed
//! (`stellar_strkey::ed25519::PublicKey([0xaa; 32])`), the full path —
//! `install_smart_account → install_policy → invoke_policy_enforce` for
//! both `transfer` (permit) and `approve` (panic 1010) — succeeds end-to-
//! end. The synthesized `ContextRule` ScVal shape this file documents
//! above is therefore correct against the v0.7.1
//! `#[contracttype]`-generated `TryFromVal<Val, ContextRule>` impl after
//! all; no Round-2 host-side fix was needed.
//!
//! the wider `__check_auth → do_check_auth → add_policy` chain remains
//! deliberately untested here (see `host.rs` "Why not the full
//! `__check_auth → add_policy → enforce` chain?" block): driving that
//! requires wallet-signed `AuthEntry` credentials, which is Phase 7
//! work. The run orchestrator (`crate::run::run_full_suite`) therefore
//! invokes each policy's `enforce` entry point directly per
//! `TestContext`, which is the same observable surface the harness
//! needs to verify permit/deny outcomes — see `plan.md` § "Phase 4 —
//! round 2" for the explicit gap-tracking note.
//!
//! `vendored_smart_account_wasm_hash_is_stable` is unrelated and passes
//! today: it just re-hashes the embedded bytes and confirms they match
//! the pinned `VENDORED_SMART_ACCOUNT_WASM_SHA256`.

use std::path::PathBuf;

use oz_policy_core::ArgValue;
use oz_policy_simhost::{
    host::{TestContext, VENDORED_SMART_ACCOUNT_WASM, VENDORED_SMART_ACCOUNT_WASM_SHA256},
    TestHost,
};

/// path to the Phase 3 fixture policy WASM, resolved relative to the
/// crate's manifest dir.
fn fixture_policy_wasm_path() -> PathBuf {
    // `CARGO_MANIFEST_DIR` of this crate = `crates/oz-policy-simhost`.
    // workspace root is two `..` up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("walkthroughs")
        .join("phase3-codegen-fixture")
        .join("expected")
        .join("slot_0")
        .join("policy.wasm")
}

/// sanity: the embedded smart-account WASM matches the SHA-256 the host
/// wrapper pins. (Re-runs `verify_vendored_smart_account_wasm` from the
/// integration-test crate boundary so a host-rebuild without re-vendoring
/// fails this gate as well.)
#[test]
#[ignore]
fn vendored_smart_account_wasm_hash_is_stable() {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(VENDORED_SMART_ACCOUNT_WASM);
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, VENDORED_SMART_ACCOUNT_WASM_SHA256);
}

/// full host-smoke path:
///   1. Build a TestHost.
///   2. Install the vendored smart account.
///   3. Install the Phase 3 fixture policy (function_allowlist on "transfer").
///   4. Issue a context with `function_name = "transfer"` → must permit.
///   5. Issue a context with `function_name = "approve"` (NOT in allowlist) →
///      must panic with FunctionNotAllowed (= 1010).
#[test]
#[ignore]
fn fixture_policy_allows_transfer_denies_approve() {
    let wasm_path = fixture_policy_wasm_path();
    let wasm =
        std::fs::read(&wasm_path).unwrap_or_else(|e| panic!("read {}: {e}", wasm_path.display()));
    assert!(!wasm.is_empty(), "fixture WASM is empty");
    assert_eq!(&wasm[..4], b"\0asm", "fixture WASM lacks magic header");

    let mut host =
        TestHost::new(1_700_000, "Test SDF Network ; September 2015").expect("TestHost::new");

    let sa = host
        .install_smart_account("")
        .expect("install_smart_account");
    assert!(sa.starts_with('C'), "SA address should be a C-StrKey: {sa}");

    let install_params = oz_policy_simhost::run::default_install_params(0);
    let policy = host
        .install_policy(&wasm, &sa, 0, install_params)
        .expect("install_policy");
    assert!(
        policy.starts_with('C'),
        "policy address should be a C-StrKey: {policy}"
    );

    // -------- Permit case: function_name == "transfer" (in allowlist) ----
    let target_addr = "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH";
    // valid G-StrKey synthesized from a deterministic 32-byte seed
    // (`[0xaa; 32]` → checksum-validated by `stellar-strkey 0.0.13`). The
    // bogus literal previously hardcoded here was rejected by
    // `PublicKey::from_string` *before* the call ever reached the host, so
    // the test was masking the install_policy investigation entirely.
    let signer_strkey = stellar_strkey::ed25519::PublicKey([0xaau8; 32]).to_string();
    let permit_ctx = TestContext {
        contract_address: target_addr.into(),
        function_name: "transfer".into(),
        args: vec![
            ArgValue::Address(signer_strkey.clone()),
            ArgValue::Address(signer_strkey.clone()),
            ArgValue::I128("100".into()),
        ],
    };
    host.invoke_policy_enforce(&policy, 0, &sa, &permit_ctx)
        .expect("transfer must be permitted by function_allowlist");

    // -------- Deny case: function_name == "approve" (NOT in allowlist) ---
    let deny_ctx = TestContext {
        contract_address: target_addr.into(),
        function_name: "approve".into(),
        args: vec![
            ArgValue::Address(signer_strkey.clone()),
            ArgValue::I128("100".into()),
        ],
    };
    match host.invoke_policy_enforce(&policy, 0, &sa, &deny_ctx) {
        Err(oz_policy_simhost::HostExecError::PolicyPanic(code)) => {
            assert_eq!(
                code, 1010,
                "approve must panic with PolicyError::FunctionNotAllowed (1010); got {code}",
            );
        }
        other => panic!("expected PolicyPanic(1010) for approve, got {other:?}",),
    }
}
