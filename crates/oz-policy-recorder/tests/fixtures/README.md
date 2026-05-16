# XDR Test Fixtures

This directory contains real Stellar testnet transaction XDR captured once
(non-network) for use by the recorder's integration tests. The unit tests in
`crates/oz-policy-recorder/tests/` read the committed `.xdr.base64` files
directly via `decode_from_xdr_blobs` and never hit the network.

Per the project rules (P1-T3 spec), these are **not** hand-synthesized — they
were fetched from the Stellar testnet's Soroban RPC endpoint
`https://soroban-testnet.stellar.org` using the JSON-RPC `getTransaction`
method on the hashes listed below. To re-derive any of them, run:

```sh
curl -s -X POST -H 'Content-Type: application/json' \
  https://soroban-testnet.stellar.org \
  -d '{"jsonrpc":"2.0","id":1,"method":"getTransaction","params":{"hash":"<HASH>"}}' \
  | jq -r '.result.envelopeXdr'
# and likewise `.result.resultMetaXdr` for the meta blob.
```

## `simple_transfer.*.xdr.base64`

* Tx hash: `52b86b5393b9ee936aa7b62638fb9d40fdbbed93ea6ac685e925205f52d50fcf`
* Ledger: 2566000
* Status: SUCCESS
* Network: Test SDF Network ; September 2015 (testnet)
* Operation: `InvokeHostFunction` → `InvokeContract`
  * Contract: `CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH`
  * Function: `transfer`
  * Args: `(Address from, Address to, I128 amount = 51_613_347)`
  * Auth: 1 entry, `SourceAccount` credentials, root `ContractFn::transfer`,
    no sub-invocations.

This fixture is the basis for `tests/decode_simple_transfer.rs`, which asserts
the decoded `Recording` has exactly one `ContractRecord`, function name
`"transfer"`, three args, and that `args[2]` is `ArgValue::I128`.

## `nested_auth.*.xdr.base64`

* Tx hash: `8d64ac1168f2c35f39364e5539a2f2a30af2e11bdcb3a7e94741fd232d70f3bf`
* Ledger: 2570501
* Status: SUCCESS
* Network: Test SDF Network ; September 2015 (testnet)
* Operation: `InvokeHostFunction` → `InvokeContract`
  * Contract: `CBA5665EZWLWMUKU3YL4ZYDCC72G3MRPS343AKFIG5TI6YP5KCTSBT4O`
  * Function: `fund_escrow`
  * Args: 3 args.
  * Auth: 1 entry, `SourceAccount` credentials, root `ContractFn::fund_escrow`
    with **1 sub-invocation** (`ContractFn::transfer` on
    `CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA`).

This fixture is the basis for `tests/decode_nested_auth.rs`, which asserts the
recorder walks `SorobanAuthorizedInvocation::sub_invocations` correctly:
`auth_tree.roots.len() >= 1`,
`auth_tree.roots[0].root_invocation.sub_invocations.len() >= 1`.
