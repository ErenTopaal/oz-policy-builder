//! integration tests for `PolicySpec` via the public surface only.

use oz_policy_core::spec::{
    ArgMatcher, Constraint, ContextRuleSpec, ContextType, ExistingPrimitive,
    ExistingPrimitiveParams, PolicySlot, PolicySpec, RecordingRef, SignerSpec, SynthesisMode,
    TemplateFamily, WeightedSigner, MAX_EXTERNAL_KEY_SIZE, MAX_NAME_SIZE, MAX_POLICIES,
    MAX_SIGNERS, POLICY_SCHEMA_URI,
};
use oz_policy_core::ArgValue;

/// minimal `PolicySpec` with one signer, no policies.
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

/// limit_stroops_string must serialise as json string, not number.
#[test]
fn spending_limit_serializes_limit_as_json_string() {
    // just inside i128 range, bigger than f64 can exactly represent.
    let limit = "170141183460469231731687303715884105727".to_string();
    let slot = PolicySlot::Existing {
        primitive: ExistingPrimitive::SpendingLimit,
        params: ExistingPrimitiveParams::SpendingLimit {
            period_ledgers: 17_280,
            limit_stroops_string: limit.clone(),
        },
    };

    let mut spec = minimal_spec(SynthesisMode::ComposeOnly);
    // PR-#649: SpendingLimit requires CallContract.
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

    // round-trip is byte-equal.
    let json = serde_json::to_string(&spec).expect("serialize");
    let parsed: PolicySpec = serde_json::from_str(&json).expect("parse");
    assert_eq!(parsed, spec);
}

/// cross-ir smoke: every top-level variant must round-trip byte-equal.
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
                    value: ArgValue::Address(
                        "CRECIPIENT0000000000000000000000000000000000000000".to_string(),
                    ),
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

/// public-surface schema smoke test.
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

/// public constants must stay locked to on-chain SmartAccount limits.
#[test]
fn hard_limit_constants_are_exported_unchanged() {
    assert_eq!(MAX_POLICIES, 5);
    assert_eq!(MAX_SIGNERS, 15);
    assert_eq!(MAX_NAME_SIZE, 20);
    assert_eq!(MAX_EXTERNAL_KEY_SIZE, 256);
}
