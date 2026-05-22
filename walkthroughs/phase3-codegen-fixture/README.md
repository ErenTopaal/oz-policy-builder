# Phase 3 Codegen Fixture

The minimal end-to-end Track-B codegen artifact. Used by the Phase 3 binary
completion gate (`crates/oz-policy-codegen/tests/phase3_completion.rs`).

## Layout

```
phase3-codegen-fixture/
  spec.json                — hand-authored PolicySpec (1 Generated slot)
  README.md                — this file
  expected/
    slot_0/
      source.rs            — actual `render_contract(&spec, 0).src_lib_rs`
      policy.wasm          — actual optimized WASM bytes (post `stellar contract optimize`)
      wasm_hash.txt        — lowercase hex SHA-256 of policy.wasm
```

## Spec

`spec.json` is the smallest valid PolicySpec that exercises Track-B codegen:

- one `PolicySlot::Generated`
- `template_family = function_allowlist`
- a single `Constraint::FunctionAllowlist { functions: ["transfer"] }`
- `ContextType::CallContract { address: "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH" }`
- no signers, no lifetime, no recording back-pointer hash

Hard-validated against the spec invariants enforced by the synthesizer
(≤ 5 policies, ≤ 15 signers, name ≤ 20 chars).

## Pinned WASM SHA-256

```
cb2a8736040711ff831346b20912fc1fe54a9bc096f9dab288014940d72b6fd4
```

This value lives **both** in `expected/slot_0/wasm_hash.txt` (committed) and
in the `#[ignore]`-gated `phase3_compile_hash_pinned` integration test.

## Reproducing the fixture

```sh
rm -rf walkthroughs/phase3-codegen-fixture/expected/
cargo run -p oz-policy-cli -- codegen \
    walkthroughs/phase3-codegen-fixture/spec.json \
    --out walkthroughs/phase3-codegen-fixture/expected/
```

Requirements:

- `cargo` + `rustc 1.89.0` with the `wasm32-unknown-unknown` target
  (the workspace toolchain pin already supplies these)
- `stellar` 25.1.0 on `$PATH` for the optimize pass
- `~/.cargo/registry` pre-populated with the `soroban-sdk = 25.3.0` and
  `stellar-accounts = 0.7.1` dependency closure (a first non-sandboxed
  build of the workspace populates this)

The codegen pipeline is deterministic: re-running this command after
deleting the cache directory must produce byte-equal `source.rs` and
`policy.wasm` and the same WASM hash above. The repository CI's
`phase3_render_byte_equal` test asserts the source-byte-equality on every
run; `phase3_compile_hash_pinned` (`#[ignore]`) asserts the WASM hash on
local runs where the sandbox compile pipeline is available.
