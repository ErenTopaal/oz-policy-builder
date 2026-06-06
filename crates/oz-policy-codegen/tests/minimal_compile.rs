//! End-to-end sandbox compile against a HAND-CRAFTED minimal Soroban
//! contract.
//!
//! This test exercises the [`oz_policy_codegen::sandbox::compile`] driver
//! end-to-end: it constructs a `RenderedCrate` whose `src/lib.rs` is a
//! trivial-but-valid Soroban contract authored by hand against the pinned
//! `soroban-sdk = 25.3.0` surface (the same API documented in the crate's
//! README). The askama templates are intentionally NOT used here — this
//! test verifies the sandbox driver in isolation.
//!
//! The test is `#[ignore]` because it requires:
//!
//!   * `cargo` + `rustc 1.89.0` with the `wasm32-unknown-unknown` target
//!     installed (the workspace toolchain pin already supplies these);
//!   * `stellar` 25.1.0 on `$PATH` for the optimize pass; and
//!   * the user's `~/.cargo/registry` to contain a pre-fetched
//!     `soroban-sdk-25.3.0.crate` (the very first build of the workspace
//!     populates this).
//!
//! CI runs this only via `cargo nextest run --include-ignored`.

use oz_policy_codegen::sandbox::{compile, RenderedCrate};
use sha2::{Digest, Sha256};

fn hex(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Author a minimal Soroban contract by hand. The shape mirrors the
/// `Contract` example in `soroban-sdk-25.3.0/README.md`: a single
/// `#[contract]` struct with one `#[contractimpl] fn check(env: Env) -> u32`
/// that returns a constant. This is real Soroban code — it links against
/// `soroban-sdk` and produces a valid `cdylib` WASM. We deliberately do
/// NOT exercise the `Policy` trait here; that surface is the
/// askama-template integration tested elsewhere.
fn hand_crafted_rendered_crate() -> RenderedCrate {
    // The crate name MUST match the Cargo.toml `name = …` below; the
    // sandbox driver locates the built WASM by replacing `-` with `_`
    // and looking for `<snake>.wasm` under `target/wasm32-…/release/`.
    let cargo_toml = r#"[package]
name = "oz-sandbox-minimal-policy"
version = "0.0.0"
edition = "2021"
# rust-version + resolver=3 instructs Cargo to back off newer patch
# releases that raised their MSRV past the workspace toolchain pin
# (`1.89.0`). soroban-sdk-macros / soroban-spec / soroban-spec-rust each
# cut a `25.3.1` patch that requires rustc 1.91.0; without these two
# fields, Cargo greedily picks the patch and the build fails. With them,
# the resolver lands on the `25.3.0` of each, which is buildable on
# 1.89.0. Pinned `soroban-sdk = "=25.3.0"` further forbids the major bump.
rust-version = "1.89.0"
resolver = "3"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
soroban-sdk = "=25.3.0"

[profile.release]
overflow-checks = true
opt-level = "z"
lto = true
codegen-units = 1
strip = "symbols"
"#;

    let src_lib_rs = r#"#![no_std]
//! Minimal valid Soroban contract used as a sandbox-driver smoke test.
//! Authored by hand against soroban-sdk 25.3.0 (see crates.io README).
//! This is NOT a policy contract — Stream A's askama templates emit the
//! real `Policy` impls. This is the smallest thing the sandbox can build.

use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct Policy;

#[contractimpl]
impl Policy {
    /// Trivial entrypoint: returns the answer. Exists so the compiler
    /// keeps `Policy` non-empty and the wasm has at least one exported
    /// host function.
    pub fn check(_env: Env) -> u32 {
        42
    }
}
"#;

    let mut hasher = Sha256::new();
    hasher.update(cargo_toml.as_bytes());
    hasher.update(b"\0");
    hasher.update(src_lib_rs.as_bytes());
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hasher.finalize());

    RenderedCrate {
        src_lib_rs: src_lib_rs.to_string(),
        cargo_toml: cargo_toml.to_string(),
        wasm_hash_of_src: hash,
    }
}

#[ignore]
#[tokio::test]
async fn sandbox_compiles_minimal_soroban_contract() {
    // Route the cache to a fresh temp dir so a stale prior run can't
    // mask a regression. We do NOT clean up — leaving the build artifacts
    // in place makes local debugging easier when this test trips in CI.
    let tmp = tempfile::tempdir().expect("create tempdir for cache");
    std::env::set_var("OZ_POLICY_CODEGEN_CACHE_DIR", tmp.path());

    let rendered = hand_crafted_rendered_crate();

    // First pass: full build through the sandbox.
    let first = compile(&rendered)
        .await
        .expect("first compile must succeed");
    assert!(!first.cache_hit, "first pass should be a cache miss");
    assert!(!first.wasm.is_empty(), "wasm bytes must be non-empty");
    // Sanity: WASM files start with the `\0asm` magic.
    assert_eq!(&first.wasm[..4], b"\0asm", "not a wasm magic header");
    assert_eq!(first.source, rendered.src_lib_rs);
    // Emit the post-optimize WASM hash to test stdout so the verification
    // gate can grep it out. `cargo test -- --nocapture` will surface this.
    println!(
        "minimal_compile: optimized wasm sha256 = {}",
        hex(&first.wasm_hash)
    );

    // Second pass: must hit the cache and return the same hash.
    let second = compile(&rendered)
        .await
        .expect("second compile must succeed");
    assert!(second.cache_hit, "second pass should be a cache hit");
    assert_eq!(
        second.wasm_hash, first.wasm_hash,
        "cache must return identical wasm hash"
    );
    assert_eq!(second.wasm, first.wasm, "cache must return identical bytes");

    std::env::remove_var("OZ_POLICY_CODEGEN_CACHE_DIR");
}
