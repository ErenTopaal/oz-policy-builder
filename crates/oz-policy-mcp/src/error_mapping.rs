//! `oz_policy_core::Error` → `rmcp::ErrorData` mapping table.
//!
//! every variant of [`oz_policy_core::Error`] maps to a deterministic
//! integer JSON-RPC `code` plus a structured `data` payload carrying the
//! wire-stable `E_*` identifier and a `details` field (currently the
//! error's `to_string()` body — Phase 5 keeps this opaque; later phases
//! may upgrade it to a structured object).
//!
//! ## Code table
//!
//! | `oz_policy_core::Error` variant       | JSON-RPC `code` | `E_*` code                     |
//! |---------------------------------------|----------------:|--------------------------------|
//! | `RecorderHashNotFound`                | -32100          | `E_RECORDER_HASH_NOT_FOUND`    |
//! | `RecorderSimFailed`                   | -32101          | `E_RECORDER_SIM_FAILED`        |
//! | `SynthNotExpressible`                 | -32102          | `E_SYNTH_NOT_EXPRESSIBLE`      |
//! | `CodegenCompileFailed`                | -32103          | `E_CODEGEN_COMPILE_FAILED`     |
//! | `SimPermitDenied`                     | -32104          | `E_SIM_PERMIT_DENIED`          |
//! | `SimDenyPassed`                       | -32105          | `E_SIM_DENY_PASSED`            |
//! | `VerifyDrift`                         | -32106          | `E_VERIFY_DRIFT`               |
//! | `WalletRejected`                      | -32107          | `E_WALLET_REJECTED`            |
//! | `InstallPreflightFailed`              | -32108          | `E_INSTALL_PREFLIGHT_FAILED`   |
//! | `RecorderXdrDecodeFailed`             | -32109          | `E_RECORDER_XDR_DECODE_FAILED` |
//! | `SpecNotFound`                        | -32110          | `E_SPEC_NOT_FOUND`             |
//! | `SnapshotNotFound`                    | -32111          | `E_SNAPSHOT_NOT_FOUND`         |
//!
//! these codes occupy the JSON-RPC "server-defined" range
//! (-32000 to -32099 is reserved for transport-level errors per the
//! JSON-RPC 2.0 spec; we deliberately allocate -32100 onward so the policy
//! builder's codes never collide with rmcp's own `INVALID_PARAMS` /
//! `METHOD_NOT_FOUND` / `INTERNAL_ERROR` constants which sit in the
//! reserved range).
//!
//! the mapping is `pub` and exhaustive (the `match` in
//! [`code_to_int`] has no wildcard arm) so adding a new `Error` variant is
//! a compile-time failure here, forcing the developer to assign a code.

use oz_policy_core::Error;
use rmcp::model::{ErrorCode, ErrorData};
use serde_json::json;

/// deterministic JSON-RPC `code` for each `oz_policy_core::Error` variant.
///
/// returns an `i32` (matching `rmcp::model::ErrorCode`'s underlying type)
/// so callers can construct an `ErrorCode(code_to_int(&e))` directly. The
/// codes are stable and documented in the module-level table.
pub fn code_to_int(e: &Error) -> i32 {
    match e {
        Error::RecorderHashNotFound(_) => -32100,
        Error::RecorderSimFailed(_) => -32101,
        Error::SynthNotExpressible(_) => -32102,
        Error::CodegenCompileFailed(_) => -32103,
        Error::SimPermitDenied(_) => -32104,
        Error::SimDenyPassed(_) => -32105,
        Error::VerifyDrift(_) => -32106,
        Error::WalletRejected(_) => -32107,
        Error::InstallPreflightFailed(_) => -32108,
        Error::RecorderXdrDecodeFailed(_) => -32109,
        Error::SpecNotFound(_) => -32110,
        Error::SnapshotNotFound(_) => -32111,
    }
}

/// convert an `oz_policy_core::Error` into a fully-populated
/// `rmcp::model::ErrorData` ready to return from a tool handler.
///
/// * `code` — from [`code_to_int`].
/// * `message` — the error's `Display` body (already prefixed with the
///   canonical `E_*` code by `thiserror`, so MCP clients can grep for the
///   code in the plain message too).
/// * `data` — a JSON object `{ "error_code": "E_…", "details": "<msg>" }`
///   so structured clients can branch on the literal `E_*` string without
///   re-parsing `message`.
pub fn error_to_jsonrpc(e: &Error) -> ErrorData {
    let code_int = code_to_int(e);
    let code_str = e.code();
    let details = e.to_string();
    let data = json!({
        "error_code": code_str,
        "details": details,
    });
    ErrorData::new(ErrorCode(code_int), details, Some(data))
}

// tests

#[cfg(test)]
mod tests {
    use super::*;

    /// every `Error` variant produces a distinct integer code, the codes
    /// are inside the documented `-32100 .. -32109` band, and the `data`
    /// payload carries the canonical `E_*` identifier as a JSON string.
    #[test]
    fn every_variant_maps_to_distinct_code_in_band() {
        let cases: &[(Error, i32, &str)] = &[
            (
                Error::RecorderHashNotFound("h".into()),
                -32100,
                "E_RECORDER_HASH_NOT_FOUND",
            ),
            (
                Error::RecorderSimFailed("s".into()),
                -32101,
                "E_RECORDER_SIM_FAILED",
            ),
            (
                Error::SynthNotExpressible("n".into()),
                -32102,
                "E_SYNTH_NOT_EXPRESSIBLE",
            ),
            (
                Error::CodegenCompileFailed("c".into()),
                -32103,
                "E_CODEGEN_COMPILE_FAILED",
            ),
            (
                Error::SimPermitDenied("p".into()),
                -32104,
                "E_SIM_PERMIT_DENIED",
            ),
            (
                Error::SimDenyPassed("d".into()),
                -32105,
                "E_SIM_DENY_PASSED",
            ),
            (Error::VerifyDrift("v".into()), -32106, "E_VERIFY_DRIFT"),
            (
                Error::WalletRejected("w".into()),
                -32107,
                "E_WALLET_REJECTED",
            ),
            (
                Error::InstallPreflightFailed("i".into()),
                -32108,
                "E_INSTALL_PREFLIGHT_FAILED",
            ),
            (
                Error::RecorderXdrDecodeFailed("x".into()),
                -32109,
                "E_RECORDER_XDR_DECODE_FAILED",
            ),
            (
                Error::SpecNotFound("nf".into()),
                -32110,
                "E_SPEC_NOT_FOUND",
            ),
            (
                Error::SnapshotNotFound("sn".into()),
                -32111,
                "E_SNAPSHOT_NOT_FOUND",
            ),
        ];

        let mut seen_codes = std::collections::HashSet::new();
        for (err, expected_code, expected_str) in cases {
            // code matches table.
            assert_eq!(code_to_int(err), *expected_code, "wrong code for {err:?}");
            // codes are unique across the table.
            assert!(
                seen_codes.insert(*expected_code),
                "duplicate code {expected_code} for {err:?}"
            );
            // codes sit in the documented band.
            assert!(
                (-32111..=-32100).contains(expected_code),
                "code {expected_code} outside -32100..-32111"
            );
            // errorData carries the canonical E_* in `data.error_code`.
            let ed = error_to_jsonrpc(err);
            assert_eq!(ed.code.0, *expected_code);
            let data = ed.data.expect("data must be set");
            assert_eq!(
                data.get("error_code").and_then(|v| v.as_str()),
                Some(*expected_str)
            );
            assert!(
                data.get("details")
                    .and_then(|v| v.as_str())
                    .map(|s| s.starts_with(expected_str))
                    .unwrap_or(false),
                "details must start with {expected_str}; got data={data}"
            );
            // the Display body (= the message) also starts with the code,
            // matching the contract `Error::to_string` declares.
            assert!(
                ed.message.starts_with(expected_str),
                "message must start with {expected_str}; got {message}",
                message = ed.message
            );
        }
        assert_eq!(seen_codes.len(), 12, "must cover 12 variants");
    }

    /// `code_to_int` must be a pure function: two calls with the same
    /// variant produce the same integer. (Trivial by `match`, but locking
    /// the property keeps it visible in tests so a future move to a
    /// hashMap-backed lookup can't introduce hash-randomisation drift.)
    #[test]
    fn code_to_int_is_deterministic() {
        let e = Error::VerifyDrift("probe".into());
        for _ in 0..32 {
            assert_eq!(code_to_int(&e), -32106);
        }
    }
}
