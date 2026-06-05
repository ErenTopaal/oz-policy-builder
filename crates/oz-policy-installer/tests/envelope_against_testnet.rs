//! integration test for [`oz_policy_installer::build_install_envelope`]
//! against the public Stellar testnet RPC.
//!
//! ## Status (v1): BLOCKED — does not run end-to-end
//!
//! the test is `#[ignore]`-gated and currently asserts only that the
//! function returns the **expected** typed error for the v1 state of
//! the code: the [`oz_policy_installer::registry`] has no published
//! primitive contract addresses (see that module's doc-comment for the
//! honest finding), so [`oz_policy_installer::build_install_envelope`]
//! surfaces
//! `Error::InstallPreflightFailed("primitive_address_unknown ...")`
//! before it gets to encode the envelope.
//!
//! once any of the following lands, this test is upgraded to assert
//! the success path (decode the returned `EnvelopeArtifact.envelope_xdr_base64`
//! via `TransactionEnvelope::from_xdr_base64`, count
//! `host_function_count == 1`, confirm the first invocation's
//! `function_name == "add_context_rule"`):
//!
//! 1. The registry gains a published `simple_threshold` testnet address
//!    (canonical published source linked in `src/registry.rs`).
//! 2. **OR** the public API grows a `with_primitive_addresses` constructor
//!    accepting a user-supplied address map (post-v1.1).
//!
//! until then, this test exists to lock the failure surface — if the
//! v1 implementation ever silently fabricates an address, this assertion
//! flips and CI rejects the regression.
//!
//! ## Why not point at a known smart-account on testnet?
//!
//! the task brief asks us to look up "a known testnet smart-account
//! address (via stellarexpert testnet history — search for txns that
//! invoked `add_context_rule`)". A targeted stellarexpert audit on
//! 2026-05-15 surfaced zero confirmed canonical-published deployments
//! of the OZ `examples/multisig-smart-account/` on testnet — every
//! historical invocation we located belongs to a project-private
//! deployment with no published verification trail. We refuse to embed
//! an address we cannot publicly trace; the test is BLOCKED until a
//! traceable testnet deployment is available.

use oz_policy_core::spec::{
    ContextRuleSpec, ContextType, ExistingPrimitive, ExistingPrimitiveParams, PolicySlot,
    PolicySpec, RecordingRef, SignerSpec, SynthesisMode, POLICY_SCHEMA_URI,
};
use oz_policy_installer::{build_install_envelope, AccountRevision};

const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";
const TESTNET_RPC: &str = "https://soroban-testnet.stellar.org";

/// USDC SAC on testnet — published asset.
const TESTNET_USDC_SAC: &str = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

/// friendbot funds this account on testnet; we use it as a stand-in
/// for the source/smart_account in this BLOCKED test. The test never
/// actually requires the account to exist — it short-circuits on the
/// registry's missing-primitive-address path before any RPC call to
/// the source account.
const TEST_G: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

fn baseline_spec() -> PolicySpec {
    PolicySpec {
        schema: POLICY_SCHEMA_URI.to_string(),
        synthesis_mode: SynthesisMode::Auto,
        context_rule: ContextRuleSpec {
            name: "testnet_install".to_string(),
            context_type: ContextType::CallContract {
                address: TESTNET_USDC_SAC.to_string(),
            },
            valid_until: None,
        },
        signers: vec![SignerSpec::Delegated {
            // use the smart account itself as the delegated signer
            // target so the test exercises a complete, addressed
            // signers vec.
            address: TESTNET_USDC_SAC.to_string(),
        }],
        policies: vec![PolicySlot::Existing {
            primitive: ExistingPrimitive::SimpleThreshold,
            params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
        }],
        lifetime_ledgers: None,
        recording_ref: RecordingRef {
            hash: None,
            schema: "oz-recording/v1".to_string(),
        },
    }
}

#[tokio::test]
#[ignore = "BLOCKED: registry has no published primitive addresses for testnet (v1); see test header"]
async fn envelope_against_testnet() {
    let spec = baseline_spec();
    let result = build_install_envelope(
        &spec,
        TESTNET_USDC_SAC, // stand-in smart_account
        TEST_G,
        TESTNET_PASSPHRASE,
        TESTNET_RPC,
        AccountRevision::PostPr655,
    )
    .await;

    // v1 expectation: registry has no published primitive address, so
    // we expect `Error::InstallPreflightFailed("primitive_address_unknown ...")`.
    // the test is structured so that when the registry gains addresses
    // (or the API grows a user-supplied map), this assertion is the
    // signal to upgrade the test to a real round-trip decode.
    let err = result.expect_err(
        "v1 baseline: build_install_envelope must surface primitive_address_unknown \
         (no canonical addresses in the registry yet)",
    );
    assert_eq!(err.code(), "E_INSTALL_PREFLIGHT_FAILED");
    assert!(
        err.to_string().contains("primitive_address_unknown"),
        "expected primitive_address_unknown message; got: {err}"
    );

    // upgrade path (kept as documentation of the future assertion):
    //
    // use stellar_xdr::curr::{Limits, ReadXdr, TransactionEnvelope, OperationBody,
    //     hostFunction};
    // let artifact = result.expect("envelope build succeeded");
    // assert_eq!(artifact.host_function_count, 1);
    // let env = TransactionEnvelope::from_xdr_base64(
    //     &artifact.envelope_xdr_base64, Limits::none(),
    // ).expect("envelope decodes");
    // let TransactionEnvelope::Tx(v1) = env else { panic!("expected v1 envelope") };
    // assert_eq!(v1.tx.operations.len(), 1);
    // let OperationBody::InvokeHostFunction(ih) = &v1.tx.operations[0].body
    //     else { panic!("expected InvokeHostFunction op") };
    // let HostFunction::InvokeContract(ic) = &ih.host_function
    //     else { panic!("expected InvokeContract host fn") };
    // assert_eq!(
    //     std::str::from_utf8(ic.function_name.0.as_slice()).unwrap(),
    //     "add_context_rule",
    // );
}
