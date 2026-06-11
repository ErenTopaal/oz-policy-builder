//! Integration test for `get_policy_artifacts` MCP tool — playground
//! design §3.4.
//!
//! drives the real `synthesize_policy → get_policy_artifacts` flow
//! against the same `oz-policy-codegen` sandbox the production binary
//! uses. No subprocess; we call the tool handler functions directly on a
//! shared `McpStore` because that is the byte-equivalent path the rmcp
//! server takes inside `call_tool`.
//!
//! ## Why `#[ignore]`?
//!
//! The handler invokes `oz_policy_codegen::synthesize_track_b`, which
//! materialises a Cargo crate under `OZ_POLICY_CODEGEN_CACHE_DIR`, runs
//! `cargo build --release --target wasm32-unknown-unknown --locked`, and
//! pipes the result through `stellar contract optimize`. Both tools
//! must be on `PATH`, AND the host's cargo registry must already be
//! warmed with the codegen template's dependency closure (otherwise
//! `--offline` fails). CI runs this test via
//! `cargo nextest run --workspace --run-ignored all` after a
//! warmup `cargo build` against the codegen template.
//!
//! No mocks, no fakes: per `feedback-no-mock-fallback` the test exits
//! with a structured error if the sandbox build fails instead of
//! pretending the wasm hashes are something they aren't.

use std::sync::Arc;

use oz_policy_core::arg_value::ArgValue;
use oz_policy_core::decision_tree::Tightness;
use oz_policy_core::recording::{
    AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials, IngestSource,
    Recording, RECORDING_SCHEMA_URI,
};
use oz_policy_core::spec::SynthesisMode;
use oz_policy_mcp::tools::{
    get_policy_artifacts, synthesize_policy, GetPolicyArtifactsCache, GetPolicyArtifactsInput,
    SynthesizePolicyInput, TESTNET_PASSPHRASE,
};
use oz_policy_mcp::McpStore;

/// minimal SEP-41 transfer recording. Mirrors the unit-test fixture in
/// `crates/oz-policy-mcp/src/tools/mod.rs` so the two stay aligned.
fn sep41_recording() -> Recording {
    Recording {
        schema: RECORDING_SCHEMA_URI.to_string(),
        network_passphrase: TESTNET_PASSPHRASE.to_string(),
        ingest: IngestSource::Hash {
            hash: "deadbeef".to_string(),
        },
        ledger: Some(1234),
        contracts: vec![ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![
                ArgValue::Address("GFROM".to_string()),
                ArgValue::Address("GTO".to_string()),
                ArgValue::I128("5000000".to_string()),
            ],
        }],
        auth_tree: AuthTree {
            roots: vec![AuthEntry {
                credentials: Credentials::Address {
                    signer: "GSIGNER".to_string(),
                    nonce: "1".to_string(),
                    signature_expiration_ledger: 0,
                    signature: ArgValue::Void,
                },
                root_invocation: AuthInvocation {
                    function: AuthFunction::Contract {
                        address: "CUSDC".to_string(),
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

/// real synthesize_policy → get_policy_artifacts flow on the same
/// spec_id. Forces `CodegenOnly` mode so the resulting spec carries at
/// least one `PolicySlot::Generated`, which is what makes
/// `wasm_sha256` populated (vs. `None` for Existing-only specs).
///
/// Marked `#[ignore]` because it spawns `cargo build` under the codegen
/// sandbox; CI runs it via `--run-ignored all`. Run locally with:
///
/// ```bash
/// cargo test -p oz-policy-mcp --release --test \
///   get_policy_artifacts_integration -- --ignored --nocapture
/// ```
#[ignore]
#[tokio::test]
async fn synthesize_then_get_artifacts_returns_populated_sources_and_64hex_wasm_sha() {
    // isolate the codegen cache to a tempdir so the test doesn't pollute
    // (and isn't polluted by) the developer's shared cache. This is the
    // canonical pattern the codegen crate's own integration test uses
    // — see `crates/oz-policy-codegen/src/lib.rs::synthesize_track_b_tests`.
    let cache_dir = tempfile::tempdir().expect("tempdir");
    std::env::set_var("OZ_POLICY_CODEGEN_CACHE_DIR", cache_dir.path());

    let store = Arc::new(McpStore::new());

    // seed a recording under a fresh id and synthesize a Generated spec.
    let rid = store.new_id("rec");
    store.put_recording(&rid, sep41_recording());

    let synth_input = SynthesizePolicyInput {
        recording_id: rid,
        tightness: Tightness::Exact,
        lifetime_ledgers: Some(432_000),
        delegated_signer: None,
        mode: SynthesisMode::CodegenOnly,
        rule_name: Some("playground-itest".to_string()),
    };
    let synth_out = synthesize_policy(&store, synth_input)
        .await
        .expect("synthesize_policy must succeed under CodegenOnly");
    assert!(
        synth_out.generated_count >= 1,
        "CodegenOnly must emit at least one Generated slot; got {:?}",
        synth_out.spec
    );

    // now call get_policy_artifacts with the same spec_id and verify the
    // documented contract.
    let cache = GetPolicyArtifactsCache::new();
    let art_input = GetPolicyArtifactsInput {
        spec_id: synth_out.spec_id.clone(),
    };
    let out = get_policy_artifacts(&store, &cache, art_input.clone())
        .await
        .expect("get_policy_artifacts must succeed against a Generated spec");

    // round-trip the spec_id verbatim.
    assert_eq!(out.spec_id, synth_out.spec_id);
    // at least one generated source.
    assert!(
        !out.generated_sources.is_empty(),
        "Generated spec must produce at least one source",
    );
    // every source carries non-empty Cargo.toml + lib.rs.
    for src in &out.generated_sources {
        assert!(
            src.cargo_toml.contains("[package]"),
            "Cargo.toml must look like a Cargo manifest; got {}",
            src.cargo_toml
        );
        assert!(
            src.lib_rs.contains("smart_account.require_auth()"),
            "lib.rs must contain the require_auth call (policy entry point)"
        );
    }
    // composed_count + generated_count agree with synthesize.
    assert_eq!(out.composed_count, synth_out.composed_count);
    assert_eq!(out.generated_count, synth_out.generated_count);

    // wasm hashes: lowercase 64-char hex.
    let wasm_sha = out.wasm_sha256.as_ref().expect("wasm_sha256 must be set");
    let opt_sha = out
        .optimized_wasm_sha256
        .as_ref()
        .expect("optimized_wasm_sha256 must be set");
    assert_eq!(wasm_sha.len(), 64, "wasm_sha256 must be 64 hex chars");
    assert_eq!(opt_sha.len(), 64, "optimized_wasm_sha256 must be 64 hex chars");
    assert!(
        wasm_sha.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "wasm_sha256 must be lowercase ASCII hex; got {wasm_sha}",
    );
    assert!(
        opt_sha.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "optimized_wasm_sha256 must be lowercase ASCII hex; got {opt_sha}",
    );

    // second call short-circuits via the in-memory cache. Mutating the
    // store under the same spec_id MUST NOT change the returned payload
    // — that's the cache invariant.
    let cache_size_before = cache.len();
    let out2 = get_policy_artifacts(&store, &cache, art_input)
        .await
        .expect("cached call must succeed");
    assert_eq!(
        serde_json::to_value(&out).unwrap(),
        serde_json::to_value(&out2).unwrap(),
        "cached call must return byte-equal output"
    );
    assert_eq!(
        cache.len(),
        cache_size_before,
        "cache size must be unchanged on a hit"
    );

    std::env::remove_var("OZ_POLICY_CODEGEN_CACHE_DIR");
}
