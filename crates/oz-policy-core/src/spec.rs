//! `PolicySpec` IR — wire-stable. bump `POLICY_SCHEMA_URI` version on
//! incompatible changes. 128-bit values serialise as json strings.

use crate::arg_value::ArgValue;
use serde::{Deserialize, Serialize};

/// wire-stable schema identifier.
pub const POLICY_SCHEMA_URI: &str = "oz-policy-builder/v1";

/// max policies per context rule (mirrors on-chain `SmartAccount::MAX_POLICIES`).
pub const MAX_POLICIES: u32 = 5;

/// max signers per context rule.
pub const MAX_SIGNERS: u32 = 15;

/// max name length in utf-8 bytes.
pub const MAX_NAME_SIZE: u32 = 20;

/// max external signer key size in bytes.
pub const MAX_EXTERNAL_KEY_SIZE: u32 = 256;

/// root document for install / codegen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PolicySpec {
    /// always [`POLICY_SCHEMA_URI`].
    pub schema: String,
    /// which synthesis path may run.
    pub synthesis_mode: SynthesisMode,
    /// context rule this policy bundle lives under.
    pub context_rule: ContextRuleSpec,
    /// signers (≤ MAX_SIGNERS).
    pub signers: Vec<SignerSpec>,
    /// policy slots (≤ MAX_POLICIES); may mix Existing + Generated.
    pub policies: Vec<PolicySlot>,
    /// ttl in ledgers; None = OZ default.
    pub lifetime_ledgers: Option<u32>,
    /// back-pointer to source recording.
    pub recording_ref: RecordingRef,
}

/// auto = both tracks, composeonly = track A only, codegenonly = track B only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum SynthesisMode {
    Auto,
    ComposeOnly,
    CodegenOnly,
}

/// one context rule. mirrors on-chain `ContextRule` (minus runtime-only fields).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ContextRuleSpec {
    /// ≤ MAX_NAME_SIZE utf-8 bytes.
    pub name: String,
    /// Default = scope everything; CallContract = scope to one target.
    pub context_type: ContextType,
    /// expiry ledger, None = no expiry.
    pub valid_until: Option<u32>,
}

/// what the context rule matches.
/// note: CallContract uses struct variant form because serde's internal-tag
/// doesn't support tagged newtype variants wrapping primitives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextType {
    /// matches every invocation of the smart account.
    Default,
    /// matches calls routed through `address` (strkey `C…`).
    CallContract { address: String },
}

/// one signer entry. strkey/hex so IR stays wire-portable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignerSpec {
    /// ed25519 — `public_key_hex` is 64 hex chars (32 bytes).
    ExternalEd25519 { public_key_hex: String },
    /// webauthn — `public_key_hex` is 130 hex chars (65 bytes, uncompressed P-256).
    ExternalWebAuthn { public_key_hex: String },
    /// delegated to another contract.
    Delegated { address: String },
}

/// one policy slot — track A reuse or track B generated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicySlot {
    /// reuse an audited OZ primitive.
    Existing {
        primitive: ExistingPrimitive,
        params: ExistingPrimitiveParams,
    },
    /// generate a fresh policy via template.
    Generated {
        template_family: TemplateFamily,
        constraints: Vec<Constraint>,
    },
}

/// which existing primitive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum ExistingPrimitive {
    SimpleThreshold,
    WeightedThreshold,
    SpendingLimit,
}

/// install params for an existing primitive. 128-bit fields are decimal strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExistingPrimitiveParams {
    /// minimum signer count.
    SimpleThreshold { threshold: u32 },
    /// per-signer weights + min total weight. order is preserved.
    WeightedThreshold {
        weights: Vec<WeightedSigner>,
        threshold: u32,
    },
    /// `limit_stroops_string` is `i128` as decimal string.
    SpendingLimit {
        period_ledgers: u32,
        limit_stroops_string: String,
    },
}

/// signer→weight pair (named struct so the schema stays readable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct WeightedSigner {
    pub signer: SignerSpec,
    pub weight: u32,
}

/// template family for a track-B slot. one `.rs.jinja` per family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum TemplateFamily {
    FunctionAllowlist,
    ArgumentPattern,
    AmountRange,
    AssetAllowlist,
    TimeWindow,
    CallFrequency,
    SequenceOrdering,
}

/// one declarative constraint. codegen turns these into wasm; simhost
/// interprets them directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Constraint {
    /// only listed functions may be invoked.
    FunctionAllowlist { functions: Vec<String> },
    /// constrain `arg_index` of `fn_name` to match the matcher.
    ArgumentPattern {
        fn_name: String,
        arg_index: u32,
        matcher: ArgMatcher,
    },
    /// `i128` range over `arg_index` of `fn_name`. None bound = open-ended;
    /// both None rejected at validation.
    AmountRange {
        fn_name: String,
        arg_index: u32,
        min_string: Option<String>,
        max_string: Option<String>,
    },
    /// only listed contract addresses may be targets.
    AssetAllowlist { assets: Vec<String> },
    /// only `[start_ledger, end_ledger]` inclusive.
    TimeWindow { start_ledger: u32, end_ledger: u32 },
    /// cap on calls within a rolling window.
    CallFrequency { max_calls: u32, window_ledgers: u32 },
    /// required invocation order; index wraps after `phases.len() - 1`.
    SequenceOrdering { phases: Vec<String> },
}

/// matcher for a single argument value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArgMatcher {
    Exact {
        value: ArgValue,
    },
    Range {
        min_string: Option<String>,
        max_string: Option<String>,
    },
    Allowlist {
        values: Vec<ArgValue>,
    },
    Blocklist {
        values: Vec<ArgValue>,
    },
}

/// pointer to source recording. `hash` = tx hash or sim envelope sha256.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RecordingRef {
    pub hash: Option<String>,
    pub schema: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// lock the wire-stable constants.
    #[test]
    fn hard_limits_match_oz_smart_account() {
        assert_eq!(MAX_POLICIES, 5);
        assert_eq!(MAX_SIGNERS, 15);
        assert_eq!(MAX_NAME_SIZE, 20);
        assert_eq!(MAX_EXTERNAL_KEY_SIZE, 256);
        assert_eq!(POLICY_SCHEMA_URI, "oz-policy-builder/v1");
    }

    /// smoke-test: schemars derive chain works across the IR.
    #[test]
    fn schemars_can_emit_schema_for_policy_spec() {
        let schema = schemars::schema_for!(PolicySpec);
        let json = serde_json::to_value(&schema).expect("serialize schema");
        // schemars 1.x writes under `$defs`; older drafts use `definitions`.
        let defs = json
            .get("$defs")
            .or_else(|| json.get("definitions"))
            .expect("schema must contain $defs or definitions");
        let map = defs.as_object().expect("$defs must be an object");
        assert!(
            !map.is_empty(),
            "schema_for!(PolicySpec) produced empty $defs — derive chain is broken: {json}"
        );
    }
}
