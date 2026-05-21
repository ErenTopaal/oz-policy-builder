//! Phase 2 envelope-structure gate — `#[ignore]`-gated (network-dependent).
//!
//! This test reads the frozen `expected-spec-track-a.json` from the
//! `02-sep41-subscription` walkthrough and calls
//! [`oz_policy_installer::build_install_envelope`] against the public
//! Stellar testnet RPC.
//!
//! ## v1 expectation (today): primitive-address blocker
//!
//! The v1 [`crate::registry`] returns `None` for every `(primitive,
//! network)` pair (see `crates/oz-policy-installer/src/registry.rs` for the
//! honest finding — `stellar-accounts = 0.7.1` ships primitives as library
//! modules, not deployed contracts; no canonical addresses exist on chain).
//! [`build_install_envelope`] therefore surfaces
//! `Error::InstallPreflightFailed("primitive_address_unknown ...")`. **This
//! test asserts that exact error message** — it locks in the failure shape
//! so a future regression that silently fabricates an address (e.g. someone
//! adding a placeholder C-address to the registry "to make the test green")
//! breaks loudly here instead of slipping past CI.
//!
//! ## v1.1 upgrade path (when the registry gains real addresses)
//!
//! When either of the following lands, this test MUST be updated to assert
//! the success path — decode the returned
//! `EnvelopeArtifact.envelope_xdr_base64` via
//! `TransactionEnvelope::from_xdr_base64`, count exactly **one**
//! `InvokeHostFunction` op, verify the invoked function name is
//! `add_context_rule`, and verify the policies map carries exactly one
//! entry for the `spending_limit` primitive (the Track-A canonical shape
//! per `docs/oz-internal-shapes.md` §6 and the amended Phase 2 criterion in
//! `plan.md`):
//!
//! 1. The registry gains a published `spending_limit` testnet address
//!    (canonical, traceable source linked in `src/registry.rs`), OR
//! 2. The public API grows a user-supplied address map (post-v1.1).
//!
//! Until then, the failure path is the binary "what we CAN verify today"
//! gate.

use oz_policy_core::spec::PolicySpec;
use oz_policy_installer::{build_install_envelope, AccountRevision};

const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";
const TESTNET_RPC: &str = "https://soroban-testnet.stellar.org";

/// Stand-in C-address used as the `smart_account`. Same well-known USDC SAC
/// address used by the v1 `envelope_against_testnet` ignored test — the
/// `build_install_envelope` call short-circuits on the registry's missing
/// primitive-address path well before any RPC call to this account, so the
/// test never requires it to be a real deployed `SmartAccount`.
const TESTNET_USDC_SAC: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

/// All-zero G-address with the correct CRC16 checksum. Same rationale as
/// above — the failure path short-circuits before any source-account
/// `getLedgerEntries` call.
const TEST_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

/// Path from `CARGO_MANIFEST_DIR` (= `crates/oz-policy-installer/`) up to
/// the workspace root and into the SEP-41 walkthrough directory.
const WALKTHROUGH_DIR: &str = "../../walkthroughs/02-sep41-subscription";

#[tokio::test]
#[ignore = "Phase 2 v1 envelope shape: BLOCKED on registry primitive-address (see test header)"]
async fn phase2_envelope_locks_in_primitive_address_unknown_failure_shape() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let expected_path = format!("{manifest_dir}/{WALKTHROUGH_DIR}/expected-spec-track-a.json");

    let spec_raw = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("read expected spec at {expected_path}: {e}"));
    let spec: PolicySpec = serde_json::from_str(&spec_raw)
        .unwrap_or_else(|e| panic!("parse expected spec at {expected_path}: {e}"));

    let result = build_install_envelope(
        &spec,
        TESTNET_USDC_SAC, // stand-in smart_account (see header)
        TEST_G,
        TESTNET_PASSPHRASE,
        TESTNET_RPC,
        AccountRevision::PostPr655,
    )
    .await;

    // v1 expectation: the registry has no published primitive address for
    // `spending_limit` on testnet, so the envelope builder surfaces
    // `Error::InstallPreflightFailed("primitive_address_unknown ...")`
    // BEFORE any RPC round-trip. We assert the exact prefix of that
    // message so a regression that silently fabricates an address is
    // caught here at the binary level.
    let err = result.expect_err(
        "Phase 2 v1 envelope: build_install_envelope must surface primitive_address_unknown \
         (no canonical addresses in the registry yet). If this test starts passing, the \
         registry has gained published addresses — this test MUST be upgraded per the \
         module doc-comment.",
    );
    assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
    let msg = err.to_string();
    assert!(
        msg.contains("primitive_address_unknown"),
        "expected primitive_address_unknown message; got: {msg}"
    );
    assert!(
        msg.contains("SpendingLimit"),
        "expected the unknown-address error to name the SpendingLimit primitive \
         (the SEP-41 walkthrough composes that primitive); got: {msg}"
    );
}
