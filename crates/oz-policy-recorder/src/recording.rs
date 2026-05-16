//! Typed Recording IR.
//!
//! `Recording` is the deterministic document the recorder emits per P1-T3.
//! Every `ScVal` from the source transaction is walked into the structured
//! [`ArgValue`] enum so downstream consumers (the synthesizer in Phase 2, the
//! MCP server in Phase 5) never need to re-parse XDR.
//!
//! Stability:
//! * The wire schema is identified by [`RECORDING_SCHEMA_URI`]. Any
//!   incompatible change must bump the URI's version segment.
//! * `i128` / `u128` / `i256` / `u256` are serialised as JSON **strings** to
//!   preserve full precision when the document is round-tripped via tools
//!   that lack arbitrary-precision integer support.

use serde::{Deserialize, Serialize};

/// Wire-stable schema identifier emitted in `Recording::schema`. Producers
/// always set this constant; consumers should reject documents whose `schema`
/// field does not match (forward compatibility lives in the version segment).
pub const RECORDING_SCHEMA_URI: &str = "oz-policy-builder/recording/v1";

/// Root document: everything a Phase 2 synthesizer needs to reason about a
/// single Stellar/Soroban transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recording {
    /// Schema URI — always [`RECORDING_SCHEMA_URI`] when produced by this crate.
    pub schema: String,
    /// Network passphrase the transaction was scoped to (e.g.,
    /// `"Test SDF Network ; September 2015"` for testnet).
    pub network_passphrase: String,
    /// How the transaction was sourced.
    pub ingest: IngestSource,
    /// Ledger number the transaction was included in. `None` for
    /// simulation-based recordings (the tx never landed on-chain).
    pub ledger: Option<u32>,
    /// Decoded `InvokeHostFunction → InvokeContract` invocations from the
    /// transaction's operation list. One entry per `InvokeContract` op.
    pub contracts: Vec<ContractRecord>,
    /// Walked `SorobanAuthorizationEntry[]` from the operation auth list.
    pub auth_tree: AuthTree,
    /// Per-`LedgerEntryChange` deltas, keyed by the decoded `ContractData` key
    /// (or other entry-type discriminator embedded in [`StateDelta::key`]).
    pub state_changes: Vec<StateDelta>,
    /// Soroban contract events emitted by the transaction (contract & system
    /// events; diagnostic events are intentionally excluded — they are not
    /// part of the policy-relevant signal).
    pub events: Vec<TypedEvent>,
}

/// Discriminates how a `Recording` was sourced. `Hash` = on-chain
/// `getTransaction`; `Simulation` = local `simulateTransaction` over a
/// caller-supplied envelope. The `envelope_xdr_sha256` lets later phases
/// detect whether the envelope was modified between simulation and
/// installation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngestSource {
    Hash { hash: String },
    Simulation { envelope_xdr_sha256: String },
}

/// One `InvokeContract` invocation from the transaction's host-function op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContractRecord {
    /// StrKey `C…` address of the called contract.
    pub address: String,
    /// Function name (decoded as UTF-8 from `ScSymbol`).
    pub function: String,
    /// Fully decoded args. No opaque XDR.
    pub args: Vec<ArgValue>,
}

/// Fully decoded `ScVal` mirror. Every Soroban `ScVal` variant maps to one of
/// these. Large integers serialise as JSON strings to preserve precision;
/// bytes as hex; addresses as StrKey.
///
/// If the Soroban host ever introduces an `ScVal` variant we cannot decode
/// (none exist today for `stellar-xdr 25.0.0`), the recorder must surface
/// `Error::RecorderXdrDecodeFailed` rather than emit `Unsupported`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MapEntry {
    pub key: ArgValue,
    pub value: ArgValue,
}

/// Decoded `SorobanAuthorizationEntry[]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthTree {
    pub roots: Vec<AuthEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthEntry {
    pub credentials: Credentials,
    pub root_invocation: AuthInvocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credentials {
    /// Invoker auth: the transaction's source account stands in for the
    /// signature. No nonce / signature payload.
    SourceAccount,
    /// Address (smart-account or end-user) auth. The signature is itself an
    /// `ScVal` so callers can inspect multisig payloads, etc.
    Address {
        signer: String,
        nonce: String,
        signature_expiration_ledger: u32,
        signature: ArgValue,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthInvocation {
    pub function: AuthFunction,
    pub sub_invocations: Vec<AuthInvocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthFunction {
    /// Standard cross-contract authorization: invoked function on a contract.
    Contract {
        address: String,
        function: String,
        args: Vec<ArgValue>,
    },
    /// Host-fn that creates a new contract instance.
    CreateContract {
        /// Hex-encoded `ContractIdPreimage` XDR — opaque to policy logic but
        /// preserved verbatim so downstream code can audit it if needed.
        contract_id_preimage_xdr_hex: String,
        /// `Wasm(hex)` or `StellarAsset` discriminator from
        /// `ContractExecutable`.
        executable_kind: String,
        executable_value: String,
    },
    /// `CreateContractV2` — same as above plus constructor args.
    CreateContractV2 {
        contract_id_preimage_xdr_hex: String,
        executable_kind: String,
        executable_value: String,
        constructor_args: Vec<ArgValue>,
    },
}

/// One ledger-entry change extracted from `TransactionMetaV3.operations[].changes`
/// or `TransactionMetaV4.operations[].changes`. We pair `State` + `Updated`
/// entries by `LedgerKey` so the consumer sees a clean `before/after` view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateDelta {
    /// The `ScVal` key (for `ContractData`) or a discriminator string for
    /// other entry types (Account, Trustline, etc.). For non-`ContractData`
    /// entries we emit `ArgValue::Symbol("<entry_type>")` so the caller can
    /// filter them out cheaply.
    pub key: ArgValue,
    /// Address scope for `ContractData` entries — the contract whose storage
    /// is being mutated. `None` for non-contract entries.
    pub contract: Option<String>,
    /// Pre-image value (`None` for `Created`).
    pub before: Option<ArgValue>,
    /// Post-image value (`None` for `Removed`).
    pub after: Option<ArgValue>,
}

/// A decoded contract event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedEvent {
    /// StrKey `C…` of the emitting contract, when present.
    pub contract: Option<String>,
    /// `"system" | "contract" | "diagnostic"` mirroring `ContractEventType`.
    pub kind: String,
    pub topics: Vec<ArgValue>,
    pub data: ArgValue,
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Recording` constructed with the canonical schema URI must serialise
    /// with `"schema": "oz-policy-builder/recording/v1"` so downstream
    /// validators can route the document.
    #[test]
    fn recording_serializes_with_schema_uri() {
        let r = Recording {
            schema: RECORDING_SCHEMA_URI.to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            ingest: IngestSource::Hash {
                hash: "deadbeef".to_string(),
            },
            ledger: Some(1234),
            contracts: vec![],
            auth_tree: AuthTree { roots: vec![] },
            state_changes: vec![],
            events: vec![],
        };
        let j = serde_json::to_value(&r).expect("serialize Recording");
        assert_eq!(j["schema"], "oz-policy-builder/recording/v1");
        assert_eq!(j["ingest"]["kind"], "hash");
        assert_eq!(j["ingest"]["hash"], "deadbeef");
        assert_eq!(j["ledger"], 1234);
    }

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
