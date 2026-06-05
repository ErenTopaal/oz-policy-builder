//! typed mirror of soroban's `ScVal` enum. wire-stable serde tags — do not
//! rename without bumping `RECORDING_SCHEMA_URI`. 128/256-bit ints serialise
//! as json strings.

use serde::{Deserialize, Serialize};

/// fully decoded `ScVal` mirror. big ints as json strings, bytes as hex,
/// addresses as strkey.
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
    /// json string to avoid 53-bit mantissa loss.
    U64(String),
    /// json string for same reason.
    I64(String),
    /// timepoint (seconds since epoch). json string.
    Timepoint(String),
    /// duration (seconds). json string.
    Duration(String),
    /// json string.
    U128(String),
    /// json string.
    I128(String),
    /// json string.
    U256(String),
    /// json string.
    I256(String),
    /// hex-encoded bytes.
    Bytes {
        hex: String,
    },
    /// `ScString` — falls back to hex when non-utf8.
    String {
        utf8: Option<String>,
        hex: String,
    },
    /// symbol (restricted alphabet).
    Symbol(String),
    /// `ScVec` — `None` is the empty-marker, distinct from `Some(vec![])`.
    Vec(Option<Vec<ArgValue>>),
    Map(Option<Vec<MapEntry>>),
    /// strkey address (`C…`/`G…`/`M…`/`B…`/`L…`).
    Address(String),
    /// `ScContractInstance` — rare in user args.
    ContractInstance {
        executable_kind: String,
        executable_value: String,
        storage: Option<Vec<MapEntry>>,
    },
    /// system-reserved instance pseudo-entry.
    LedgerKeyContractInstance,
    /// system-reserved nonce key (auth replay protection).
    LedgerKeyNonce {
        nonce: String,
    },
}

/// explicit struct so json serialisation is unambiguous and schemars stays clean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(deny_unknown_fields)]
pub struct MapEntry {
    pub key: ArgValue,
    pub value: ArgValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// every variant must round-trip via json.
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

    /// 128-bit ints must serialise as json strings to avoid 2^53 loss.
    #[test]
    fn arg_value_decodes_i128_as_string() {
        let v = ArgValue::I128("170141183460469231731687303715884105727".to_string());
        let j = serde_json::to_value(&v).expect("serialize");
        // inner value must be json string, never number.
        assert!(
            j["value"].is_string(),
            "ArgValue::I128 must serialise its value as JSON string, got: {j}"
        );
        // symmetric guard for u128.
        let u = ArgValue::U128("340282366920938463463374607431768211455".to_string());
        let ju = serde_json::to_value(&u).expect("serialize u128");
        assert!(
            ju["value"].is_string(),
            "ArgValue::U128 must serialise as string, got: {ju}"
        );
    }
}
