//! Library half of the codegen fuzz harness.
//!
//! The `fuzz_targets/spec_to_wasm_panic_free.rs` binary is a one-liner that
//! defers to [`run_spec_to_wasm_panic_free`] here. Keeping the body in a
//! library has two payoffs:
//!
//! * It is reachable from `#[cfg(test)]` smoke tests (see below) so the
//!   `arbitrary::Arbitrary` derives on `PolicySpec` can be exercised by
//!   plain `cargo test`, without `cargo fuzz`. This proves the harness
//!   compiles + runs even when nobody has a nightly toolchain installed.
//! * It lets a future contributor wire a property-test style proptest run
//!   into regular CI by depending on this crate.
//!
//! Invariant under test: `render_contract` on any `PolicySpec` produced
//! from arbitrary bytes either returns `Ok(RenderedCrate)` or
//! `Err(Error::CodegenCompileFailed(_))`. Any *other* error variant or
//! any panic is a finding.

use oz_policy_core::{spec::PolicySpec, Error};

/// Fuzz-target body. Iterates over every slot index in the spec so a single
/// fuzzer mutation covers all slots in the spec, not just the first.
///
/// Returns `()` so the fuzzer can ignore the success/failure path — what
/// matters is whether a panic propagates. The function asserts the error
/// shape inline: a returned [`Error`] *must* be the codegen-declared
/// `CodegenCompileFailed` variant; any other variant is a finding because
/// the render layer only ever surfaces compile-failure-shaped errors.
pub fn run_spec_to_wasm_panic_free(spec: &PolicySpec) {
    // Skip the sandbox compile (intentionally — too slow for fuzz; the
    // template-render path catches the panics we care about).
    for slot_index in 0..spec.policies.len() {
        match oz_policy_codegen::render::render_contract(spec, slot_index) {
            Ok(_rendered) => {
                // Successful render. Nothing else to assert at fuzz speed —
                // determinism + content shape are covered by golden tests in
                // the parent crate.
            }
            Err(err) => {
                assert!(
                    matches!(err, Error::CodegenCompileFailed(_)),
                    "render_contract returned non-codegen error variant: {err:?}"
                );
            }
        }
    }
    // Also exercise the public `render_contract` with an explicitly
    // out-of-range slot. This guarantees the bounds-check branch is hit
    // each iteration (helps libFuzzer's coverage feedback latch onto the
    // path).
    let oob_index = spec.policies.len();
    match oz_policy_codegen::render::render_contract(spec, oob_index) {
        Ok(_) => {
            // An empty policies vector renders nothing — we only assert
            // panic-freedom, not a particular result here.
        }
        Err(Error::CodegenCompileFailed(_)) => {}
        Err(other) => panic!("oob render returned unexpected error variant: {other:?}"),
    }
}

#[cfg(test)]
mod smoke {
    use super::*;
    use arbitrary::{Arbitrary, Unstructured};

    /// Drive the fuzz-target body on a couple of synthetic byte streams.
    /// This exists so the harness is exercised by ordinary `cargo test`
    /// even when nobody is running a nightly fuzz pass — if the
    /// `arbitrary::Arbitrary` derive ever stops compiling, this test will
    /// fail at build time, surfacing the issue in stable CI rather than
    /// in the nightly fuzz job.
    #[test]
    fn arbitrary_policyspec_round_trip_does_not_panic() {
        // Two hand-picked byte streams — one short, one longer — covering
        // both the empty-policies and one-or-more-policies branches in
        // `arbitrary`'s `Vec` impl. The exact contents are not load-bearing;
        // what matters is that `PolicySpec::arbitrary` builds something
        // and `run_spec_to_wasm_panic_free` does not panic on it.
        let inputs: [&[u8]; 2] = [
            &[0u8; 16],
            &[
                0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x07, 0x18, 0x29, 0x3a, 0x4b, 0x5c, 0x6d, 0x7e,
                0x8f, 0x90, 0x01, 0x12, 0x23, 0x34, 0x45, 0x56, 0x67, 0x78, 0x89, 0x9a, 0xab, 0xbc,
                0xcd, 0xde, 0xef, 0xf0,
            ],
        ];
        for bytes in inputs {
            let mut u = Unstructured::new(bytes);
            // It's OK if `Arbitrary` returns an error (insufficient bytes,
            // for instance) — we only care that the *successful* path of
            // the derive doesn't crash the run function.
            if let Ok(spec) = PolicySpec::arbitrary(&mut u) {
                run_spec_to_wasm_panic_free(&spec);
            }
        }
    }

    /// Cover the case where `Arbitrary` produces an empty `policies` vec:
    /// `render_contract` on any slot must return CodegenCompileFailed (slot
    /// index out of range) rather than panic. We construct the spec
    /// directly here rather than relying on `Arbitrary` chance.
    #[test]
    fn empty_policies_does_not_panic() {
        use oz_policy_core::spec::{ContextRuleSpec, ContextType, RecordingRef, SynthesisMode};
        let spec = PolicySpec {
            schema: "oz-policy-builder/v1".into(),
            synthesis_mode: SynthesisMode::Auto,
            context_rule: ContextRuleSpec {
                name: "r".into(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: Vec::new(),
            policies: Vec::new(),
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: None,
                schema: "oz-recording/v1".into(),
            },
        };
        run_spec_to_wasm_panic_free(&spec);
    }
}
