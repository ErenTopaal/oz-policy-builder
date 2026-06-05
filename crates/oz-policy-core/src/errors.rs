//! canonical error enum.

use thiserror::Error;

/// canonical error enum, one variant per `E_*` wire code.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum Error {
    /// tx hash not found on configured rpc.
    #[error("E_RECORDER_HASH_NOT_FOUND: {0}")]
    RecorderHashNotFound(String),

    /// `simulateTransaction` failed or returned undecodable auth tree.
    #[error("E_RECORDER_SIM_FAILED: {0}")]
    RecorderSimFailed(String),

    /// xdr decode failed on envelope/result-meta/auth/ScVal.
    #[error("E_RECORDER_XDR_DECODE_FAILED: {0}")]
    RecorderXdrDecodeFailed(String),

    /// constraints cannot be expressed within OZ primitives + track-B limits.
    #[error("E_SYNTH_NOT_EXPRESSIBLE: {0}")]
    SynthNotExpressible(String),

    /// generated rust failed sandboxed wasm build.
    #[error("E_CODEGEN_COMPILE_FAILED: {0}")]
    CodegenCompileFailed(String),

    /// permit vector got denied by compiled policy.
    #[error("E_SIM_PERMIT_DENIED: {0}")]
    SimPermitDenied(String),

    /// deny vector got admitted by compiled policy.
    #[error("E_SIM_DENY_PASSED: {0}")]
    SimDenyPassed(String),

    /// drift between spec / source / wasm / on-chain.
    #[error("E_VERIFY_DRIFT: {0}")]
    VerifyDrift(String),

    /// wallet rejected the signing prompt.
    #[error("E_WALLET_REJECTED: {0}")]
    WalletRejected(String),

    /// preflight failed (e.g., target predates PR-#655).
    #[error("E_INSTALL_PREFLIGHT_FAILED: {0}")]
    InstallPreflightFailed(String),
}

impl Error {
    /// canonical wire code for this variant.
    pub fn code(&self) -> &'static str {
        match self {
            Error::RecorderHashNotFound(_) => "E_RECORDER_HASH_NOT_FOUND",
            Error::RecorderSimFailed(_) => "E_RECORDER_SIM_FAILED",
            Error::RecorderXdrDecodeFailed(_) => "E_RECORDER_XDR_DECODE_FAILED",
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

    /// every variant maps to its canonical `E_*` string.
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
                Error::RecorderXdrDecodeFailed("malformed ScVal in transfer args[2]".into()),
                "E_RECORDER_XDR_DECODE_FAILED",
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
            // display impl must carry the code as prefix.
            assert!(
                err.to_string().starts_with(expected_code),
                "Display for {err:?} did not start with {expected_code}"
            );
        }
    }

    /// exhaustive match so a new variant breaks the build here.
    #[test]
    fn variant_coverage_is_exhaustive() {
        fn canonical_code_for(e: &Error) -> &'static str {
            // note: no wildcard arm — adding a variant must break the build.
            match e {
                Error::RecorderHashNotFound(_) => "E_RECORDER_HASH_NOT_FOUND",
                Error::RecorderSimFailed(_) => "E_RECORDER_SIM_FAILED",
                Error::RecorderXdrDecodeFailed(_) => "E_RECORDER_XDR_DECODE_FAILED",
                Error::SynthNotExpressible(_) => "E_SYNTH_NOT_EXPRESSIBLE",
                Error::CodegenCompileFailed(_) => "E_CODEGEN_COMPILE_FAILED",
                Error::SimPermitDenied(_) => "E_SIM_PERMIT_DENIED",
                Error::SimDenyPassed(_) => "E_SIM_DENY_PASSED",
                Error::VerifyDrift(_) => "E_VERIFY_DRIFT",
                Error::WalletRejected(_) => "E_WALLET_REJECTED",
                Error::InstallPreflightFailed(_) => "E_INSTALL_PREFLIGHT_FAILED",
            }
        }

        // sanity: local mapping must agree with production `Error::code()`.
        let probe = Error::VerifyDrift("probe".into());
        assert_eq!(canonical_code_for(&probe), probe.code());
    }
}
