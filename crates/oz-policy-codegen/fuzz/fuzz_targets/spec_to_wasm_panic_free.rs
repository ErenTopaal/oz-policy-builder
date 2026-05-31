//! cargo-fuzz target: feed arbitrary `PolicySpec`s through
//! `oz-policy-codegen::render::render_contract` and assert the only failure
//! mode is `Error::CodegenCompileFailed`. Any panic (via `unreachable!`,
//! `.unwrap()` failure, askama template error not caught by the typed
//! error path, etc.) is a finding.
//!
//! See `src/lib.rs::run_spec_to_wasm_panic_free` for the actual body — it
//! is factored out so the smoke-test in `src/lib.rs` can exercise the
//! same path under regular `cargo test`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oz_policy_codegen_fuzz::run_spec_to_wasm_panic_free;
use oz_policy_core::spec::PolicySpec;

fuzz_target!(|spec: PolicySpec| {
    run_spec_to_wasm_panic_free(&spec);
});
