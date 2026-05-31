//! Typed mirror of Soroban's `ScVal` discriminated union.
//!
//! `ArgValue` is the deterministic, fully-decoded representation of a
//! Soroban `ScVal`. It is the lingua franca between the recorder
//! (`oz-policy-recorder`), the policy IR ([`crate::spec`]) and any downstream
//! consumer (decision tree, MCP server) that needs to reason about contract
//! arguments without re-parsing XDR.
//!
//! ## History
//!
//! In Phase 1 this enum lived in `oz-policy-recorder::recording`. It was
//! moved into `oz-policy-core` in Phase 2 (P2-T1) so [`crate::spec`] can
//! reference `ArgValue` (e.g. inside `Constraint::ArgumentPattern` /
//! `ArgMatcher::Exact`) without introducing a `core -> recorder` cycle. The
//! recorder still re-exports `ArgValue` from its public surface so existing
//! consumers (CLI, tests, walkthroughs) keep compiling unchanged.
//!
//! ## Wire-format stability
//!
//! * `#[serde(tag = "type", content = "value", rename_all = "snake_case")]` is
//!   load-bearing: every Recording document already on disk depends on this
//!   exact serialisation. Do NOT change variant names, field names, or the
//!   tag/content scheme without bumping
//!   `oz-policy-recorder::recording::RECORDING_SCHEMA_URI`.
//! * `i128` / `u128` / `i256` / `u256` are serialised as JSON **strings** to
//!   preserve full precision for consumers (browsers, jq < 1.7) that lack
//!   arbitrary-precision integer support.

use serde::{Deserialize, Serialize};

/// Fully decoded `ScVal` mirror. Every Soroban `ScVal` variant maps to one of
/// these. Large integers serialise as JSON strings to preserve precision;
/// bytes as hex; addresses as StrKey.
///
/// If the Soroban host ever introduces an `ScVal` variant we cannot decode
/// (none exist today for `stellar-xdr 25.0.0`), the recorder must surface
/// `Error::RecorderXdrDecodeFailed` rather than emit `Unsupported`.
///
/// `Eq` is derived because every contained field type is `Eq`: floats are
/// not part of the `ScVal` shape, all `String`/`Option<String>`/`Vec<T>` /
/// integer payloads are `Eq`. This lets downstream consumers (Phase 4
/// `DenyVector`, set / map collections of recordings) compare values
/// without falling back to manual comparators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ArgValue {
    Bool(bool),
    Void,
    Error {
        kind: String,
        code: String,
    },
    U32(u32),
    I32(i32),
    /// 64-bit unsigned. JSON-encoded as a string to avoid loss in
    /// 53-bit-mantissa consumers.
    U64(String),
    /// 64-bit signed. JSON-encoded as a string for the same reason.
    I64(String),
    /// Timepoint (seconds since epoch, u64). JSON string.
    Timepoint(String),
    /// Duration (seconds, u64). JSON string.
    Duration(String),
    /// 128-bit unsigned. JSON string.
    U128(String),
    /// 128-bit signed. JSON string.
    I128(String),
    /// 256-bit unsigned. JSON string.
    U256(String),
    /// 256-bit signed. JSON string.
    I256(String),
    /// Raw bytes, hex-encoded.
    Bytes {
        hex: String,
    },
    /// UTF-8 string (`ScString`). Note: Soroban allows non-UTF-8 byte payloads
    /// in `ScString`; in that case we fall back to a hex representation here.
    String {
        utf8: Option<String>,
        hex: String,
    },
    /// Symbol — restricted-alphabet UTF-8.
    Symbol(String),
    /// `ScVec` — `None` distinguishes the empty-marker from `Some(vec![])`
    /// matches the on-chain encoding (`Vec(Option<ScVec>)`).
    Vec(Option<Vec<ArgValue>>),
    Map(Option<Vec<MapEntry>>),
    /// StrKey-encoded address (`C…` for contracts, `G…` for accounts,
    /// `M…` muxed, `B…` claimable balance, `L…` liquidity pool).
    Address(String),
    /// `ScContractInstance` — emitted only when the host stores a contract
    /// instance value (rare in user-facing args).
    ContractInstance {
        executable_kind: String,
        executable_value: String,
        storage: Option<Vec<MapEntry>>,
    },
    /// System-reserved sentinel used in contract-data keys for the instance
    /// pseudo-entry. No payload.
    LedgerKeyContractInstance,
    /// System-reserved nonce key (auth replay protection).
    LedgerKeyNonce {
        nonce: String,
    },
}

/// Map entry pair — explicit struct so JSON serialisation is unambiguous and
/// `schemars` produces a clean schema (tuples land as 2-element arrays which
/// hide field intent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(deny_unknown_fields)]
pub struct MapEntry {
    pub key: ArgValue,
    pub value: ArgValue,
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `ArgValue` variant must round-trip via JSON. This guards the
    /// `#[serde(tag, content)]` representation: if a variant ever picks up an
    /// untagged-friendly shape, the round-trip will fail loudly here.
    #[test]
    fn arg_value_round_trips_via_json() {
        let samples = vec![
            ArgValue::Bool(true),
            ArgValue::Void,
            ArgValue::U32(42),
            ArgValue::I32(-7),
            ArgValue::U64("18446744073709551615".to_string()),
            ArgValue::I64("-9223372036854775808".to_string()),
            ArgValue::Timepoint("1700000000".to_string()),
            ArgValue::Duration("60".to_string()),
            ArgValue::U128("340282366920938463463374607431768211455".to_string()),
            ArgValue::I128("-170141183460469231731687303715884105728".to_string()),
            ArgValue::U256(
                "115792089237316195423570985008687907853269984665640564039457584007913129639935"
                    .to_string(),
            ),
            ArgValue::I256(
                "-57896044618658097711785492504343953926634992332820282019728792003956564819968"
                    .to_string(),
            ),
            ArgValue::Bytes {
                hex: "deadbeef".to_string(),
            },
            ArgValue::String {
                utf8: Some("hello".to_string()),
                hex: "68656c6c6f".to_string(),
            },
            ArgValue::Symbol("transfer".to_string()),
            ArgValue::Vec(Some(vec![ArgValue::U32(1), ArgValue::U32(2)])),
            ArgValue::Vec(None),
            ArgValue::Map(Some(vec![MapEntry {
                key: ArgValue::Symbol("k".to_string()),
                value: ArgValue::U32(7),
            }])),
            ArgValue::Map(None),
            ArgValue::Address(
                "GAEEZQIBQHBP3CG3F2BSTQHBHM5LJUFRTL2EFRC6CN4MV3OWJZ74C6XR".to_string(),
            ),
            ArgValue::ContractInstance {
                executable_kind: "Wasm".to_string(),
                executable_value: "00".repeat(32),
                storage: None,
            },
            ArgValue::LedgerKeyContractInstance,
            ArgValue::LedgerKeyNonce {
                nonce: "1".to_string(),
            },
            ArgValue::Error {
                kind: "Contract".to_string(),
                code: "1".to_string(),
            },
        ];
        for v in samples {
            let j = serde_json::to_string(&v).expect("serialize ArgValue");
            let back: ArgValue =
                serde_json::from_str(&j).unwrap_or_else(|e| panic!("roundtrip {j}: {e}"));
            assert_eq!(v, back, "round-trip mismatch for {v:?}");
        }
    }

    /// `i128` must serialise as a JSON string — not a number — so consumers
    /// without arbitrary-precision integer support (browsers, jq before
    /// 1.7, many JS clients) don't silently lose bits on values exceeding
    /// 2^53.
    #[test]
    fn arg_value_decodes_i128_as_string() {
        let v = ArgValue::I128("170141183460469231731687303715884105727".to_string());
        let j = serde_json::to_value(&v).expect("serialize");
        // `j` is { "type": "i128", "value": "170141..." } — the inner value
        // must be a JSON string, never a JSON number.
        assert!(
            j["value"].is_string(),
            "ArgValue::I128 must serialise its value as JSON string, got: {j}"
        );
        // Symmetric guard for u128, the other 128-bit variant.
        let u = ArgValue::U128("340282366920938463463374607431768211455".to_string());
        let ju = serde_json::to_value(&u).expect("serialize u128");
        assert!(
            ju["value"].is_string(),
            "ArgValue::U128 must serialise as string, got: {ju}"
        );
    }
}
