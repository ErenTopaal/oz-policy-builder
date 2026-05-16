//! Integration test: decode a committed real-testnet `fund_escrow` envelope
//! whose `SorobanAuthorizationEntry.root_invocation` carries a nested
//! `transfer` sub-invocation, exercising the recursive auth walker. No
//! network access.
//!
//! Source of fixture: `tests/fixtures/nested_auth.*.xdr.base64`
//! (testnet tx `8d64ac1168f2c35f39364e5539a2f2a30af2e11bdcb3a7e94741fd232d70f3bf`
//! at ledger 2570501). See `tests/fixtures/README.md`.

use oz_policy_recorder::AuthFunction;

mod common;
use common as helpers;

const NETWORK: &str = "Test SDF Network ; September 2015";

#[test]
fn auth_tree_walker_recurses_into_sub_invocations() {
    let envelope = include_str!("fixtures/nested_auth.envelope.xdr.base64");
    let meta = include_str!("fixtures/nested_auth.result_meta.xdr.base64");
    let rec = helpers::decode(envelope.trim(), meta.trim(), NETWORK)
        .expect("decode nested-auth fixture should succeed");

    // At least one auth root.
    assert!(
        !rec.auth_tree.roots.is_empty(),
        "expected >= 1 auth root, got {}",
        rec.auth_tree.roots.len()
    );

    let root = &rec.auth_tree.roots[0];
    // Root invocation is a contract fn call.
    let AuthFunction::Contract {
        function: root_fn, ..
    } = &root.root_invocation.function
    else {
        panic!(
            "expected root invocation to be Contract fn, got {:?}",
            root.root_invocation.function
        );
    };
    // Per the fixture, the root invocation is `fund_escrow`.
    assert_eq!(root_fn, "fund_escrow", "root fn must be fund_escrow");

    // There must be at least one nested sub-invocation (the inner `transfer`).
    assert!(
        !root.root_invocation.sub_invocations.is_empty(),
        "expected >= 1 sub_invocation under fund_escrow, got {}",
        root.root_invocation.sub_invocations.len()
    );
    let sub = &root.root_invocation.sub_invocations[0];
    let AuthFunction::Contract {
        function: sub_fn, ..
    } = &sub.function
    else {
        panic!(
            "expected sub_invocation to be Contract fn, got {:?}",
            sub.function
        );
    };
    assert_eq!(sub_fn, "transfer", "sub-invocation must be transfer");
}
