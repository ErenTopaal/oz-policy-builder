//! typed recording IR — deterministic document the recorder emits.
//! every `ScVal` decoded into `ArgValue`. wire schema = `RECORDING_SCHEMA_URI`;
//! bump the version segment on any incompatible change.

use crate::arg_value::ArgValue;
use serde::{Deserialize, Serialize};

/// wire-stable schema identifier.
pub const RECORDING_SCHEMA_URI: &str = "oz-policy-builder/recording/v1";

/// root document — everything the synthesizer needs for one tx.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recording {
    /// always [`RECORDING_SCHEMA_URI`] when produced by this crate.
    pub schema: String,
    /// network passphrase the tx was scoped to.
    pub network_passphrase: String,
    /// how the tx was sourced.
    pub ingest: IngestSource,
    /// ledger number; None for simulation-only recordings.
    pub ledger: Option<u32>,
    /// decoded `InvokeContract` invocations, one per op.
    pub contracts: Vec<ContractRecord>,
    /// walked auth entries.
    pub auth_tree: AuthTree,
    /// per-entry deltas with before/after.
    pub state_changes: Vec<StateDelta>,
    /// contract & system events (diagnostic excluded — not policy-relevant).
    pub events: Vec<TypedEvent>,
}

/// hash = on-chain getTransaction; simulation = local simulateTransaction.
/// envelope hash lets later phases detect tampering between sim and install.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngestSource {
    Hash { hash: String },
    Simulation { envelope_xdr_sha256: String },
}

/// one `InvokeContract` invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContractRecord {
    /// strkey `C…` of the called contract.
    pub address: String,
    /// function name (utf-8 from `ScSymbol`).
    pub function: String,
    /// fully decoded args, no opaque xdr.
    pub args: Vec<ArgValue>,
}

/// decoded auth tree.
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
    /// op index of the InvokeHostFunction op that owned this entry.
    /// `#[serde(default)]` so older recordings without it decode to 0.
    #[serde(default)]
    pub source_op_index: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credentials {
    /// invoker auth — tx source acts as signature, no nonce/signature.
    SourceAccount,
    /// address auth (smart-account / end-user). signature kept as ScVal so
    /// callers can inspect multisig payloads.
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
    /// standard cross-contract authorization.
    Contract {
        address: String,
        function: String,
        args: Vec<ArgValue>,
    },
    /// host-fn that creates a new contract instance.
    CreateContract {
        /// hex-encoded `ContractIdPreimage` xdr, kept verbatim for audit.
        contract_id_preimage_xdr_hex: String,
        /// `Wasm(hex)` or `StellarAsset` discriminator.
        executable_kind: String,
        executable_value: String,
    },
    /// `CreateContractV2` — same plus constructor args.
    CreateContractV2 {
        contract_id_preimage_xdr_hex: String,
        executable_kind: String,
        executable_value: String,
        constructor_args: Vec<ArgValue>,
    },
}

/// one ledger-entry change with before/after view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateDelta {
    /// `ScVal` key for ContractData, or `ArgValue::Symbol("<entry_type>")`
    /// discriminator for non-contractdata entries.
    pub key: ArgValue,
    /// address scope for ContractData; None otherwise.
    pub contract: Option<String>,
    /// pre-image, None for Created.
    pub before: Option<ArgValue>,
    /// post-image, None for Removed.
    pub after: Option<ArgValue>,
}

/// decoded contract event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedEvent {
    /// strkey `C…` of emitting contract, if present.
    pub contract: Option<String>,
    /// `"system" | "contract" | "diagnostic"`.
    pub kind: String,
    pub topics: Vec<ArgValue>,
    pub data: ArgValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// schema field must serialise to the canonical uri.
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
}
