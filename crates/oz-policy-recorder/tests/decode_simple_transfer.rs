//! integration test: decode a committed SAC `transfer` envelope + meta, no network.

use oz_policy_recorder::{ArgValue, IngestSource};

mod common;
use common as helpers;

const NETWORK: &str = "Test SDF Network ; September 2015";

#[test]
fn decodes_simple_transfer_correctly() {
    let envelope = include_str!("fixtures/simple_transfer.envelope.xdr.base64");
    let meta = include_str!("fixtures/simple_transfer.result_meta.xdr.base64");
    let rec = helpers::decode(envelope.trim(), meta.trim(), NETWORK)
        .expect("decode fixture should succeed");
    // canonical wire schema.
    assert_eq!(rec.schema, "oz-policy-builder/recording/v1");
    assert_eq!(rec.network_passphrase, NETWORK);
    // helper doesn't set ingest; only validate contracts / args here.
    assert!(matches!(rec.ingest, IngestSource::Hash { .. }));
    assert_eq!(
        rec.contracts.len(),
        1,
        "exactly one InvokeContract op expected, got {}",
        rec.contracts.len()
    );
    let c = &rec.contracts[0];
    assert_eq!(c.function, "transfer", "function name must be 'transfer'");
    assert_eq!(c.args.len(), 3, "transfer must have 3 args: from,to,amount");
    // amount is i128 (sep-41); pin exact value so xdr-decode regressions fail loud.
    let ArgValue::I128(amount) = &c.args[2] else {
        panic!("args[2] should be ArgValue::I128, got {:?}", c.args[2]);
    };
    assert_eq!(
        amount, "51613347",
        "transfer amount must equal the fixture's captured value, got {amount}"
    );
    // both arg[0] and arg[1] should be addresses.
    assert!(
        matches!(c.args[0], ArgValue::Address(_)),
        "args[0] should be Address, got {:?}",
        c.args[0]
    );
    assert!(
        matches!(c.args[1], ArgValue::Address(_)),
        "args[1] should be Address, got {:?}",
        c.args[1]
    );
    // contract address must be `C…` strkey.
    assert!(
        c.address.starts_with('C'),
        "contract StrKey must start with C, got {}",
        c.address
    );
    // fixture has a `SourceAccount` auth entry with no sub_invocations.
    assert_eq!(rec.auth_tree.roots.len(), 1);
    assert!(matches!(
        rec.auth_tree.roots[0].credentials,
        oz_policy_recorder::Credentials::SourceAccount
    ));
}
