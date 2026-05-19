//! `PolicySpec` IR — the versioned, wire-stable representation of a synthesised
//! policy.
//!
//! ## Stability contract
//!
//! * The schema URI is [`POLICY_SCHEMA_URI`]. Any incompatible change must
//!   bump the URI's version segment so downstream tools can refuse to load
//!   unrecognised versions.
//! * All public types derive `serde::Serialize`, `serde::Deserialize`,
//!   `schemars::JsonSchema`, plus `Debug`, `Clone`, `PartialEq` so spec
//!   documents can round-trip through JSON, surface in MCP tool schemas, and
//!   be diffed deterministically in tests.
//! * 128-bit integer values are serialised as JSON **strings** (suffix
//!   `_string` on the field name) — same convention as
//!   [`crate::arg_value::ArgValue::I128`] — to preserve full precision in
//!   consumers without arbitrary-precision integer support.
//! * The constants [`MAX_POLICIES`], [`MAX_SIGNERS`], [`MAX_NAME_SIZE`] and
//!   [`MAX_EXTERNAL_KEY_SIZE`] mirror the hard limits enforced by the
//!   on-chain OZ `SmartAccount` contract (see
//!   `docs/oz-internal-shapes.md` §7). They are exposed here so the Phase 2
//!   decision tree and Phase 2 installer can validate against them without
//!   linking to the on-chain crate.
//!
//! Field-by-field semantics are documented inline.

use crate::arg_value::ArgValue;
use serde::{Deserialize, Serialize};

// -------------------------------------------------------------------------
// Wire-stable identifiers + hard limits
// -------------------------------------------------------------------------

/// Wire-stable schema identifier emitted in [`PolicySpec::schema`]. Producers
/// always set this constant; consumers should reject documents whose `schema`
/// field does not match (forward compatibility lives in the version segment).
pub const POLICY_SCHEMA_URI: &str = "oz-policy-builder/v1";

/// Maximum number of policies allowed per context rule. Mirrored from
/// `openzeppelin-stellar-contracts::accounts::smart_account::MAX_POLICIES`
/// (see `docs/oz-internal-shapes.md` §7).
pub const MAX_POLICIES: u32 = 5;

/// Maximum number of signers allowed per context rule. Mirrored from
/// `openzeppelin-stellar-contracts::accounts::smart_account::MAX_SIGNERS`.
pub const MAX_SIGNERS: u32 = 15;

/// Maximum length in bytes for a context rule name. Mirrored from
/// `openzeppelin-stellar-contracts::accounts::smart_account::MAX_NAME_SIZE`.
/// Note: this is a **byte** count (UTF-8 byte length), not a character count.
pub const MAX_NAME_SIZE: u32 = 20;

/// Maximum size in bytes for external signer key data. Mirrored from
/// `openzeppelin-stellar-contracts::accounts::smart_account::MAX_EXTERNAL_KEY_SIZE`.
pub const MAX_EXTERNAL_KEY_SIZE: u32 = 256;

// -------------------------------------------------------------------------
// Top-level spec
// -------------------------------------------------------------------------

/// Root document — everything the Phase 2 installer and Phase 3 codegen
/// need to install or render a policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PolicySpec {
    /// Schema URI — always [`POLICY_SCHEMA_URI`] when produced by this crate.
    pub schema: String,
    /// Selects which synthesis path the builder is permitted to take.
    pub synthesis_mode: SynthesisMode,
    /// Context rule that this policy bundle will live under.
    pub context_rule: ContextRuleSpec,
    /// Signers belonging to the context rule. Limited to [`MAX_SIGNERS`] by
    /// the on-chain `SmartAccount` contract.
    pub signers: Vec<SignerSpec>,
    /// Policy slots. Limited to [`MAX_POLICIES`] by the on-chain
    /// `SmartAccount` contract. May mix `Existing` (Track A) and `Generated`
    /// (Track B) slots.
    pub policies: Vec<PolicySlot>,
    /// Optional time-to-live, in ledgers, for the context rule. `None` means
    /// the OZ default storage TTL applies.
    pub lifetime_ledgers: Option<u32>,
    /// Back-pointer to the `Recording` document this spec was synthesised
    /// from (or that should govern verification).
    pub recording_ref: RecordingRef,
}

/// Three-way switch controlling which synthesizer track may run.
///
/// * `Auto`         — synthesizer is free to compose existing primitives (Track A)
///                    and / or emit generated policy slots (Track B).
/// * `ComposeOnly`  — synthesizer must succeed using only existing OZ primitives
///                    (Track A). If the constraints cannot be expressed,
///                    `E_SYNTH_NOT_EXPRESSIBLE`.
/// * `CodegenOnly`  — synthesizer must emit a `Generated` policy slot for every
///                    constraint (Track B). Useful for testing the codegen path
///                    end-to-end against constraints that *could* compose to
///                    `simple_threshold` etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SynthesisMode {
    Auto,
    ComposeOnly,
    CodegenOnly,
}

// -------------------------------------------------------------------------
// Context rule
// -------------------------------------------------------------------------

/// One context rule. Mirrors the on-chain `ContextRule` minus the runtime-only
/// fields (`id`, `policies: Map<Address, Val>`) — those are resolved at install
/// time by the installer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContextRuleSpec {
    /// Human-readable name (≤ [`MAX_NAME_SIZE`] UTF-8 bytes).
    pub name: String,
    /// Type of context rule: scope-everything (`Default`) or scope to a
    /// single target contract (`CallContract(address)`).
    pub context_type: ContextType,
    /// Optional expiration ledger. `None` = no expiry.
    pub valid_until: Option<u32>,
}

/// What the context rule matches.
///
/// Note: function-allowlist matching is a *policy* responsibility (encoded
/// via `Constraint::FunctionAllowlist`), never a rule-level filter — the
/// on-chain `ContextRuleType::CallContract(Address)` only carries the target
/// contract address.
///
/// JSON-shape note: `CallContract` is encoded as a struct variant
/// `{ "kind": "call_contract", "address": "..." }` rather than a Rust
/// newtype tuple variant — serde's internal-tag representation
/// (`#[serde(tag = "kind")]`) does not support tagging newtype variants that
/// wrap primitives (`serde` rejects this at runtime with
/// *"cannot serialize tagged newtype variant ... containing a string"*).
/// The on-chain semantics are unchanged; the wire shape is just `{kind,address}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContextType {
    /// Matches all invocations of the smart account.
    Default,
    /// Matches invocations routed through the named target contract.
    /// `address` is a StrKey `C…` address.
    CallContract { address: String },
}

// -------------------------------------------------------------------------
// Signers
// -------------------------------------------------------------------------

/// One signer entry. Mirrors the on-chain `Signer` discriminated union
/// (see `docs/oz-internal-shapes.md` §11) but uses StrKey / hex string
/// representations so the IR is wire-portable.
///
/// External public keys are validated lazily by the decision tree: Ed25519
/// keys must be exactly 32 bytes (64 hex chars); WebAuthn keys must be
/// exactly 65 bytes (130 hex chars); both must fit under
/// [`MAX_EXTERNAL_KEY_SIZE`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignerSpec {
    /// External Ed25519 signer. `public_key_hex` is a 64-character hex string
    /// (32 raw bytes).
    ExternalEd25519 { public_key_hex: String },
    /// External WebAuthn signer. `public_key_hex` is a 130-character hex
    /// string (65 raw bytes; uncompressed EC P-256 public key).
    ExternalWebAuthn { public_key_hex: String },
    /// Delegated signer: authority is delegated to another contract.
    /// `address` is a StrKey `C…` contract address.
    Delegated { address: String },
}

// -------------------------------------------------------------------------
// Policy slots
// -------------------------------------------------------------------------

/// One policy slot inside a context rule. Either a Track-A reuse of an
/// existing OZ primitive (with the primitive's exact install parameters) or a
/// Track-B generated contract (described by a template family and a list of
/// constraints — Phase 3 codegen turns this into compilable Rust).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicySlot {
    /// Track A: reuse one of the audited OZ primitives.
    Existing {
        primitive: ExistingPrimitive,
        params: ExistingPrimitiveParams,
    },
    /// Track B: generate a fresh policy contract via templates.
    Generated {
        template_family: TemplateFamily,
        constraints: Vec<Constraint>,
    },
}

/// Discriminator for which existing OZ primitive a Track-A slot reuses.
/// One of `simple_threshold`, `weighted_threshold`, `spending_limit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExistingPrimitive {
    SimpleThreshold,
    WeightedThreshold,
    SpendingLimit,
}

/// Install parameters for an existing primitive. Field names match
/// `docs/oz-internal-shapes.md` §2-§4 verbatim where possible.
///
/// `SpendingLimit::limit_stroops_string` is the `i128` spending limit encoded
/// as a JSON string (same precision-preserving convention used by
/// `ArgValue::I128`). The installer parses it back into `i128` when building
/// the `IntoVal` payload for `add_policy`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExistingPrimitiveParams {
    /// `SimpleThresholdAccountParams { threshold }` — minimum number of
    /// signers required for authorisation.
    SimpleThreshold { threshold: u32 },
    /// `WeightedThresholdAccountParams { signer_weights, threshold }` —
    /// per-signer weights plus minimum total weight required. `weights`
    /// preserves declaration order so two specs that pair the same signers
    /// to the same weights round-trip byte-equal.
    WeightedThreshold {
        weights: Vec<WeightedSigner>,
        threshold: u32,
    },
    /// `SpendingLimitAccountParams { spending_limit, period_ledgers }`.
    /// `limit_stroops_string` is the `i128` limit serialised as a JSON
    /// string. `period_ledgers` is in Soroban ledger sequence units.
    SpendingLimit {
        period_ledgers: u32,
        limit_stroops_string: String,
    },
}

/// One signer→weight pair for `WeightedThreshold`. Modelled as a struct
/// (not a `(SignerSpec, u32)` tuple) so the JSON shape has named fields and
/// the JSON Schema produced by `schemars` keeps field intent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WeightedSigner {
    pub signer: SignerSpec,
    pub weight: u32,
}

// -------------------------------------------------------------------------
// Generated-slot constraint language
// -------------------------------------------------------------------------

/// Which template family a Track-B slot uses. Each family corresponds to one
/// `.rs.jinja` template under `oz-policy-codegen/templates/constraints/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

/// One declarative constraint. The Track-B codegen pipeline turns a list of
/// these into compiled WASM; the simulator (Phase 4) interprets them
/// directly to verify the spec before installation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Constraint {
    /// Only the listed functions may be invoked. Function names are
    /// canonical Soroban symbols (UTF-8, ≤ 32 chars per Soroban convention).
    FunctionAllowlist { functions: Vec<String> },
    /// Constrain a specific argument of a specific function to match the
    /// supplied matcher. `arg_index` is zero-based.
    ArgumentPattern {
        fn_name: String,
        arg_index: u32,
        matcher: ArgMatcher,
    },
    /// Numeric range over an `i128` argument of `fn_name`. `min_string` /
    /// `max_string` are JSON strings (same convention as
    /// `ArgValue::I128`). `None` for either bound means open-ended on that
    /// side; both `None` is a no-op and rejected at validation time.
    AmountRange {
        fn_name: String,
        arg_index: u32,
        min_string: Option<String>,
        max_string: Option<String>,
    },
    /// Only the listed contract addresses (StrKey `C…`) may be used. Applied
    /// to the target of `Context::Contract` invocations.
    AssetAllowlist { assets: Vec<String> },
    /// Only ledgers in `[start_ledger, end_ledger]` (inclusive) are allowed.
    TimeWindow { start_ledger: u32, end_ledger: u32 },
    /// Cap on number of admitted calls within a rolling window of
    /// `window_ledgers` ledgers.
    CallFrequency {
        max_calls: u32,
        window_ledgers: u32,
    },
    /// Required invocation order. `phases[i]` is the function name allowed
    /// at the i-th call in the cycle; on advancing past `phases.len() - 1`
    /// the index wraps to 0.
    SequenceOrdering { phases: Vec<String> },
}

/// Matcher for a single argument value. `Exact` carries a fully-decoded
/// [`ArgValue`]; `Range` carries `i128` string bounds (mirroring
/// `Constraint::AmountRange`); `Allowlist` / `Blocklist` carry `ArgValue`
/// vectors so heterogeneous types (address, symbol, etc.) can be expressed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

// -------------------------------------------------------------------------
// Recording linkage
// -------------------------------------------------------------------------

/// Pointer to the Recording document the spec was synthesised from. `hash`
/// is the on-chain transaction hash (or simulation-envelope SHA-256) when
/// available; `schema` mirrors the recorder's `RECORDING_SCHEMA_URI` so a
/// consumer that finds a spec without the source Recording can at least
/// verify the schema version it was produced against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RecordingRef {
    pub hash: Option<String>,
    pub schema: String,
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Sanity-check the wire-stable constants — these are tested explicitly
    /// because any drift between them and the on-chain `SmartAccount` would
    /// silently cause synthesised specs to over-shoot the on-chain limits.
    #[test]
    fn hard_limits_match_oz_smart_account() {
        assert_eq!(MAX_POLICIES, 5);
        assert_eq!(MAX_SIGNERS, 15);
        assert_eq!(MAX_NAME_SIZE, 20);
        assert_eq!(MAX_EXTERNAL_KEY_SIZE, 256);
        assert_eq!(POLICY_SCHEMA_URI, "oz-policy-builder/v1");
    }

    /// `schemars::schema_for!(PolicySpec)` must succeed without panicking and
    /// must populate the `$defs`/`definitions` map (every nested type
    /// referenced from `PolicySpec` should be inlined). A non-empty
    /// definitions map is the smoke-test that the derive macro is wired
    /// across the whole IR.
    #[test]
    fn schemars_can_emit_schema_for_policy_spec() {
        let schema = schemars::schema_for!(PolicySpec);
        let json = serde_json::to_value(&schema).expect("serialize schema");
        // `schemars 1.x` writes its definitions under `$defs` (JSON Schema 2020-12).
        // Fall back to `definitions` for older draft compatibility.
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
