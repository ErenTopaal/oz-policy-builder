//! Canonical error enum for the OZ Accounts Policy Builder.
//!
//! Every variant corresponds 1:1 to one of the `E_*` codes listed under
//! `plan.md` § "Naming Conventions" — this enum is the single source of truth
//! that the MCP server (Phase 5), the CLI (Phase 1+), and the synthesizer
//! (Phase 2) map their failures through. The string returned by
//! [`Error::code`] is the wire-stable code surfaced to MCP clients and CI
//! scripts; the `Display` impl is the human-readable detail.
//!
//! Phase 1 only constructs these variants in unit tests. Later phases attach
//! real semantics:
//!
//! * `oz-policy-recorder`  → `E_RECORDER_*`
//! * `oz-policy-core`      → `E_SYNTH_NOT_EXPRESSIBLE`
//! * `oz-policy-codegen`   → `E_CODEGEN_COMPILE_FAILED`
//! * `oz-policy-simhost`   → `E_SIM_*`, `E_VERIFY_DRIFT`
//! * wallet-adapter        → `E_WALLET_REJECTED`
//! * `oz-policy-installer` → `E_INSTALL_PREFLIGHT_FAILED`

use thiserror::Error;

/// Canonical error enum. Carries a `String` payload per variant for human
/// context; structured payloads will be introduced in later phases where the
/// data is well-defined.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum Error {
    /// Recorder could not locate the transaction by hash on the configured
    /// Soroban RPC endpoint (retention exceeded or wrong network).
    #[error("E_RECORDER_HASH_NOT_FOUND: {0}")]
    RecorderHashNotFound(String),

    /// Recorder's `simulateTransaction` call returned an error or otherwise
    /// failed to produce a decodable auth tree.
    #[error("E_RECORDER_SIM_FAILED: {0}")]
    RecorderSimFailed(String),

    /// Synthesizer determined the requested constraints cannot be expressed
    /// by any combination of OZ primitives or Track-B templates within the
    /// hard limits (max 5 policies, 15 signers, etc.).
    #[error("E_SYNTH_NOT_EXPRESSIBLE: {0}")]
    SynthNotExpressible(String),

    /// Track-B codegen produced Rust source that failed the sandboxed
    /// `cargo build --target wasm32-unknown-unknown`.
    #[error("E_CODEGEN_COMPILE_FAILED: {0}")]
    CodegenCompileFailed(String),

    /// Simulation harness reports that a permit vector the spec is expected
    /// to allow was denied by the compiled policy.
    #[error("E_SIM_PERMIT_DENIED: {0}")]
    SimPermitDenied(String),

    /// Simulation harness reports that a deny vector the spec is expected
    /// to reject was admitted by the compiled policy (false-positive admit).
    #[error("E_SIM_DENY_PASSED: {0}")]
    SimDenyPassed(String),

    /// Verification gate detected a drift between the spec, the generated
    /// source, the compiled WASM hash, or the on-chain installed policy.
    #[error("E_VERIFY_DRIFT: {0}")]
    VerifyDrift(String),

    /// Wallet returned a user-rejection or signing failure when the
    /// install envelope was presented for signature.
    #[error("E_WALLET_REJECTED: {0}")]
    WalletRejected(String),

    /// Install-time preflight failed: e.g., target `SmartAccount` predates
    /// OZ PR-#655 sponsor-substitution fix (see `docs/oz-internal-shapes.md`
    /// §8) or another precondition was not met.
    #[error("E_INSTALL_PREFLIGHT_FAILED: {0}")]
    InstallPreflightFailed(String),
}

impl Error {
    /// Returns the canonical `E_*` wire code for this variant. This string is
    /// the stable identifier surfaced to MCP clients and is what CI and
    /// orchestration scripts grep for.
    pub fn code(&self) -> &'static str {
        match self {
            Error::RecorderHashNotFound(_) => "E_RECORDER_HASH_NOT_FOUND",
            Error::RecorderSimFailed(_) => "E_RECORDER_SIM_FAILED",
            Error::SynthNotExpressible(_) => "E_SYNTH_NOT_EXPRESSIBLE",
            Error::CodegenCompileFailed(_) => "E_CODEGEN_COMPILE_FAILED",
            Error::SimPermitDenied(_) => "E_SIM_PERMIT_DENIED",
            Error::SimDenyPassed(_) => "E_SIM_DENY_PASSED",
            Error::VerifyDrift(_) => "E_VERIFY_DRIFT",
            Error::WalletRejected(_) => "E_WALLET_REJECTED",
            Error::InstallPreflightFailed(_) => "E_INSTALL_PREFLIGHT_FAILED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    /// Construct one of every variant and assert `.code()` maps each to its
    /// canonical `E_*` string. This single test is the binary completion
    /// criterion for the Phase-1 Error scaffolding: every code listed in
    /// `plan.md` § "Naming Conventions" must be present and round-trippable.
    #[test]
    fn every_variant_maps_to_canonical_code() {
        let cases: &[(Error, &str)] = &[
            (
                Error::RecorderHashNotFound("hash 0xdead… not found".into()),
                "E_RECORDER_HASH_NOT_FOUND",
            ),
            (
                Error::RecorderSimFailed("rpc returned host error".into()),
                "E_RECORDER_SIM_FAILED",
            ),
            (
                Error::SynthNotExpressible("constraint count exceeded limit".into()),
                "E_SYNTH_NOT_EXPRESSIBLE",
            ),
            (
                Error::CodegenCompileFailed("rustc reported borrow-check error".into()),
                "E_CODEGEN_COMPILE_FAILED",
            ),
            (
                Error::SimPermitDenied("expected transfer to be admitted".into()),
                "E_SIM_PERMIT_DENIED",
            ),
            (
                Error::SimDenyPassed("expected over-limit transfer to be rejected".into()),
                "E_SIM_DENY_PASSED",
            ),
            (
                Error::VerifyDrift("on-chain wasm hash disagrees with build".into()),
                "E_VERIFY_DRIFT",
            ),
            (
                Error::WalletRejected("user dismissed the signing prompt".into()),
                "E_WALLET_REJECTED",
            ),
            (
                Error::InstallPreflightFailed("smart account predates PR-#655".into()),
                "E_INSTALL_PREFLIGHT_FAILED",
            ),
        ];

        for (err, expected_code) in cases {
            assert_eq!(
                err.code(),
                *expected_code,
                "variant {err:?} returned wrong code"
            );
            // Also confirm the Display impl carries the code as a prefix —
            // this is the contract MCP error renderers depend on.
            assert!(
                err.to_string().starts_with(expected_code),
                "Display for {err:?} did not start with {expected_code}"
            );
        }
    }
}
