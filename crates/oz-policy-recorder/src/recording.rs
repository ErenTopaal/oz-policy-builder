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
//! * In Phase 2 (P2-T1) [`ArgValue`] and [`MapEntry`] were physically moved
//!   into `oz-policy-core::arg_value` so the policy IR can reference them
//!   without a `core -> recorder` cycle. The types are re-exported below so
//!   the recorder's public surface (and its wire format) is unchanged.

use serde::{Deserialize, Serialize};

// Re-export the relocated types so existing consumers (`oz-policy-cli`,
// integration tests, walkthrough fixtures) continue to find them under the
// recorder's public surface unchanged. Local code in this module refers to
// these via the `pub use` import directly (no separate `use` needed).
pub use oz_policy_core::{ArgValue, MapEntry};

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

// NOTE: `ArgValue` and `MapEntry` were moved to `oz_policy_core::arg_value`
// in Phase 2 (P2-T1). They are re-exported at the top of this module so the
// recorder's public surface and wire format are unchanged.

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
    /// Zero-based index of the `InvokeHostFunction` operation in the source
    /// envelope that owned this auth entry. Always `0` for the common
    /// single-op envelope; multi-op envelopes (rare in practice but legal
    /// per XDR) preserve their op→auth correspondence here.
    ///
    /// Marked `#[serde(default)]` for forward/backward compatibility: older
    /// Recordings produced before this field existed deserialise with
    /// `source_op_index == 0`, which is the correct value for the
    /// overwhelmingly common single-op case.
    #[serde(default)]
    pub source_op_index: u32,
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

    // `ArgValue` round-trip + i128-as-string tests live alongside the
    // type itself in `oz_policy_core::arg_value::tests` since Phase 2
    // (P2-T1). They are not duplicated here.
}
