//! Integration tests for `oz_policy_core::spec::PolicySpec`.
//!
//! These tests live outside `src/` so they exercise the crate via its public
//! surface only — exactly how downstream consumers (`oz-policy-installer`,
//! `oz-policy-codegen`, `oz-policy-mcp`) will see the IR.
//!
//! Three properties are pinned here:
//!
//! 1. A minimal `PolicySpec` round-trips byte-equal through JSON for every
//!    [`SynthesisMode`] variant (`Auto`, `ComposeOnly`, `CodegenOnly`).
//! 2. `ExistingPrimitiveParams::SpendingLimit::limit_stroops_string`
//!    serialises as a JSON **string**, not a JSON number — protecting
//!    consumers without arbitrary-precision integer support from silent
//!    precision loss on `i128` values above 2^53.
//! 3. `schemars::schema_for!(PolicySpec)` produces a JSON Schema document
//!    whose `$defs` / `definitions` map is non-empty (smoke test that every
//!    derive in the IR is wired correctly).

use oz_policy_core::spec::{
    ArgMatcher, Constraint, ContextRuleSpec, ContextType, ExistingPrimitive,
    ExistingPrimitiveParams, PolicySlot, PolicySpec, RecordingRef, SignerSpec, SynthesisMode,
    TemplateFamily, WeightedSigner, MAX_EXTERNAL_KEY_SIZE, MAX_NAME_SIZE, MAX_POLICIES,
    MAX_SIGNERS, POLICY_SCHEMA_URI,
};
use oz_policy_core::ArgValue;

/// Construct a minimal `PolicySpec` parameterised on the synthesis mode. The
/// rest of the spec is intentionally inert (one Ed25519 signer, no policies)
/// so the test isolates the round-trip property to the enum encoding.
fn minimal_spec(mode: SynthesisMode) -> PolicySpec {
    PolicySpec {
        schema: POLICY_SCHEMA_URI.to_string(),
        synthesis_mode: mode,
        context_rule: ContextRuleSpec {
            name: "minimal".to_string(),
            context_type: ContextType::Default,
            valid_until: None,
        },
        signers: vec![SignerSpec::ExternalEd25519 {
            public_key_hex: "00".repeat(32),
        }],
        policies: vec![],
        lifetime_ledgers: None,
        recording_ref: RecordingRef {
            hash: None,
            schema: "oz-policy-builder/recording/v1".to_string(),
        },
    }
}

#[test]
fn minimal_spec_round_trips_for_every_synthesis_mode() {
    for mode in [
        SynthesisMode::Auto,
        SynthesisMode::ComposeOnly,
        SynthesisMode::CodegenOnly,
    ] {
        let spec = minimal_spec(mode.clone());
        let json = serde_json::to_string(&spec).expect("serialize PolicySpec");
        let parsed: PolicySpec = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("round-trip for {mode:?} failed: {e} (json was: {json})"));
        assert_eq!(spec, parsed, "round-trip mismatch for {mode:?}");
        assert_eq!(parsed.schema, POLICY_SCHEMA_URI);
    }
}

/// `ExistingPrimitiveParams::SpendingLimit::limit_stroops_string` must
/// serialise as a JSON string, never a JSON number — otherwise consumers
/// without arbitrary-precision integer support (browsers, jq < 1.7) silently
/// lose precision on `i128` values above 2^53.
#[test]
fn spending_limit_serializes_limit_as_json_string() {
    // Just inside the positive `i128` range — bigger than any IEEE 754 double
    // can represent exactly, so a JSON-number encoding would lose bits.
    let limit = "170141183460469231731687303715884105727".to_string();
    let slot = PolicySlot::Existing {
        primitive: ExistingPrimitive::SpendingLimit,
        params: ExistingPrimitiveParams::SpendingLimit {
            period_ledgers: 17_280,
            limit_stroops_string: limit.clone(),
        },
    };

    let mut spec = minimal_spec(SynthesisMode::ComposeOnly);
    // The SpendingLimit primitive requires CallContract(_) per PR-#649 — even
    // though this test only checks JSON encoding, set the context_type
    // correctly so the fixture mirrors a real synthesised spec.
    spec.context_rule.context_type = ContextType::CallContract {
        address: "CA00000000000000000000000000000000000000000000000000000000".to_string(),
    };
    spec.policies = vec![slot];

    let value = serde_json::to_value(&spec).expect("serialize");
    let params = &value["policies"][0]["params"];
    assert_eq!(params["kind"], "spending_limit");
    assert_eq!(
        params["period_ledgers"], 17_280,
        "period_ledgers must serialise as a JSON number: {params}"
    );
    let stroops = &params["limit_stroops_string"];
    assert!(
        stroops.is_string(),
        "limit_stroops_string must serialise as a JSON string, got: {stroops}"
    );
    assert_eq!(stroops.as_str().unwrap(), limit);

    // And round-trip is byte-equal.
    let json = serde_json::to_string(&spec).expect("serialize");
    let parsed: PolicySpec = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed, spec);
}

/// Cross-IR smoke test: a spec exercising every top-level variant (Track A
/// SpendingLimit, Track B Generated with three constraints, every signer
/// kind, every ArgMatcher kind) must round-trip byte-equal. Catches any
/// derive that silently drops fields or reorders enum content.
#[test]
fn comprehensive_spec_round_trips() {
    let ed25519 = SignerSpec::ExternalEd25519 {
        public_key_hex: "aa".repeat(32),
    };
    let webauthn = SignerSpec::ExternalWebAuthn {
        public_key_hex: "bb".repeat(65),
    };
    let delegated = SignerSpec::Delegated {
        address: "CDELEGATED000000000000000000000000000000000000000000000000".to_string(),
    };

    let weighted = PolicySlot::Existing {
        primitive: ExistingPrimitive::WeightedThreshold,
        params: ExistingPrimitiveParams::WeightedThreshold {
            weights: vec![
                WeightedSigner {
                    signer: ed25519.clone(),
                    weight: 2,
                },
                WeightedSigner {
                    signer: webauthn.clone(),
                    weight: 1,
                },
            ],
            threshold: 2,
        },
    };

    let generated = PolicySlot::Generated {
        template_family: TemplateFamily::ArgumentPattern,
        constraints: vec![
            Constraint::FunctionAllowlist {
                functions: vec!["transfer".to_string(), "approve".to_string()],
            },
            Constraint::ArgumentPattern {
                fn_name: "transfer".to_string(),
                arg_index: 1,
                matcher: ArgMatcher::Exact {
                    value: ArgValue::Address("CRECIPIENT0000000000000000000000000000000000000000".to_string()),
                },
            },
            Constraint::AmountRange {
                fn_name: "transfer".to_string(),
                arg_index: 2,
                min_string: Some("0".to_string()),
                max_string: Some("1000000000".to_string()),
            },
        ],
    };

    let spec = PolicySpec {
        schema: POLICY_SCHEMA_URI.to_string(),
        synthesis_mode: SynthesisMode::Auto,
        context_rule: ContextRuleSpec {
            name: "comprehensive".to_string(),
            context_type: ContextType::CallContract {
                address: "CTOKEN00000000000000000000000000000000000000000000000000".to_string(),
            },
            valid_until: Some(1_000_000),
        },
        signers: vec![ed25519, webauthn, delegated],
        policies: vec![weighted, generated],
        lifetime_ledgers: Some(518_400),
        recording_ref: RecordingRef {
            hash: Some("deadbeef".to_string()),
            schema: "oz-policy-builder/recording/v1".to_string(),
        },
    };

    let json = serde_json::to_string(&spec).expect("serialize");
    let parsed: PolicySpec = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed, spec, "comprehensive round-trip failed: {json}");
}

/// Smoke test for the JSON-schema export path — separate from the in-crate
/// test in `spec.rs` so the public surface is exercised end-to-end.
#[test]
fn schema_for_policy_spec_has_non_empty_definitions() {
    let schema = schemars::schema_for!(PolicySpec);
    let json = serde_json::to_value(&schema).expect("serialize schema");
    let defs = json
        .get("$defs")
        .or_else(|| json.get("definitions"))
        .expect("schema must contain $defs or definitions");
    let map = defs.as_object().expect("$defs must be an object");
    assert!(
        !map.is_empty(),
        "schema $defs is empty — derives not propagating: {json}"
    );
}

/// Public constants must stay locked to the on-chain SmartAccount limits.
#[test]
fn hard_limit_constants_are_exported_unchanged() {
    assert_eq!(MAX_POLICIES, 5);
    assert_eq!(MAX_SIGNERS, 15);
    assert_eq!(MAX_NAME_SIZE, 20);
    assert_eq!(MAX_EXTERNAL_KEY_SIZE, 256);
}
