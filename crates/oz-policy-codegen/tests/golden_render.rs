//! golden-render byte-equality tests. set `OZ_POLICY_CODEGEN_BLESS=1` to rewrite.

use std::path::PathBuf;

use oz_policy_codegen::render_contract;
use oz_policy_core::{
    arg_value::ArgValue,
    spec::{
        ArgMatcher, Constraint, ContextRuleSpec, ContextType, PolicySlot, PolicySpec, RecordingRef,
        SynthesisMode, TemplateFamily,
    },
};

/// minimal spec with one Generated slot.
fn build_spec(family: TemplateFamily, constraints: Vec<Constraint>) -> PolicySpec {
    PolicySpec {
        schema: "oz-policy-builder/v1".into(),
        synthesis_mode: SynthesisMode::CodegenOnly,
        context_rule: ContextRuleSpec {
            name: "rule".into(),
            context_type: ContextType::Default,
            valid_until: None,
        },
        signers: Vec::new(),
        policies: vec![PolicySlot::Generated {
            template_family: family,
            constraints,
        }],
        lifetime_ledgers: None,
        recording_ref: RecordingRef {
            hash: None,
            schema: "oz-recording/v1".into(),
        },
    }
}

fn golden_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("golden");
    p.push(name);
    p
}

/// diff actual vs committed golden; bless mode overwrites.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var("OZ_POLICY_CODEGEN_BLESS").is_ok() {
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e}. Run with OZ_POLICY_CODEGEN_BLESS=1 to seed.",
            path.display()
        );
    });
    assert_eq!(
        actual,
        expected,
        "render output differs from golden {}",
        path.display()
    );
}

// per-primitive goldens: one constraint each.

#[test]
fn golden_function_allowlist() {
    let spec = build_spec(
        TemplateFamily::FunctionAllowlist,
        vec![Constraint::FunctionAllowlist {
            functions: vec!["transfer".into(), "approve".into(), "transfer_from".into()],
        }],
    );
    let r = render_contract(&spec, 0).expect("render");
    assert_golden("function_allowlist.rs", &r.src_lib_rs);
}

#[test]
fn golden_argument_pattern() {
    let spec = build_spec(
        TemplateFamily::ArgumentPattern,
        vec![Constraint::ArgumentPattern {
            fn_name: "transfer".into(),
            arg_index: 0,
            matcher: ArgMatcher::Exact {
                value: ArgValue::U32(42),
            },
        }],
    );
    let r = render_contract(&spec, 0).expect("render");
    assert_golden("argument_pattern.rs", &r.src_lib_rs);
}

#[test]
fn golden_amount_range() {
    let spec = build_spec(
        TemplateFamily::AmountRange,
        vec![Constraint::AmountRange {
            fn_name: "transfer".into(),
            arg_index: 2,
            min_string: Some("1".into()),
            max_string: Some("100000000".into()),
        }],
    );
    let r = render_contract(&spec, 0).expect("render");
    assert_golden("amount_range.rs", &r.src_lib_rs);
}

#[test]
fn golden_asset_allowlist() {
    let spec = build_spec(
        TemplateFamily::AssetAllowlist,
        vec![Constraint::AssetAllowlist {
            assets: vec![
                "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC".into(),
                "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
            ],
        }],
    );
    let r = render_contract(&spec, 0).expect("render");
    assert_golden("asset_allowlist.rs", &r.src_lib_rs);
}

#[test]
fn golden_time_window() {
    let spec = build_spec(
        TemplateFamily::TimeWindow,
        vec![Constraint::TimeWindow {
            start_ledger: 1_000_000,
            end_ledger: 1_017_280,
        }],
    );
    let r = render_contract(&spec, 0).expect("render");
    assert_golden("time_window.rs", &r.src_lib_rs);
}

#[test]
fn golden_call_frequency() {
    let spec = build_spec(
        TemplateFamily::CallFrequency,
        vec![Constraint::CallFrequency {
            max_calls: 5,
            window_ledgers: 17_280,
        }],
    );
    let r = render_contract(&spec, 0).expect("render");
    assert_golden("call_frequency.rs", &r.src_lib_rs);
}

#[test]
fn golden_sequence_ordering() {
    let spec = build_spec(
        TemplateFamily::SequenceOrdering,
        vec![Constraint::SequenceOrdering {
            phases: vec!["init".into(), "deposit".into(), "finalize".into()],
        }],
    );
    let r = render_contract(&spec, 0).expect("render");
    assert_golden("sequence_ordering.rs", &r.src_lib_rs);
}

// composition golden: function_allowlist + amount_range + call_frequency.

#[test]
fn golden_composed_3_primitives() {
    let spec = build_spec(
        // The template-family discriminator is purely cosmetic for composed
        // slots — pick the first constraint's family.
        TemplateFamily::FunctionAllowlist,
        vec![
            Constraint::FunctionAllowlist {
                functions: vec!["transfer".into()],
            },
            Constraint::AmountRange {
                fn_name: "transfer".into(),
                arg_index: 2,
                min_string: None,
                max_string: Some("1000000000".into()),
            },
            Constraint::CallFrequency {
                max_calls: 3,
                window_ledgers: 17_280,
            },
        ],
    );
    let r = render_contract(&spec, 0).expect("render");
    assert_golden("composed_3_primitives.rs", &r.src_lib_rs);
}

// determinism: 50× renders must agree byte-for-byte.

#[test]
fn determinism_50x_same_spec_renders_byte_equal() {
    let spec = build_spec(
        TemplateFamily::FunctionAllowlist,
        vec![
            Constraint::FunctionAllowlist {
                functions: vec!["transfer".into(), "approve".into()],
            },
            Constraint::AmountRange {
                fn_name: "transfer".into(),
                arg_index: 2,
                min_string: Some("1".into()),
                max_string: Some("1000".into()),
            },
        ],
    );
    let first = render_contract(&spec, 0).expect("render");
    for i in 0..49 {
        let r = render_contract(&spec, 0).expect("render");
        assert_eq!(
            r.src_lib_rs, first.src_lib_rs,
            "iteration {i} differs from first render"
        );
        assert_eq!(
            r.cargo_toml, first.cargo_toml,
            "iteration {i} Cargo.toml differs"
        );
        assert_eq!(
            r.wasm_hash_of_src, first.wasm_hash_of_src,
            "iteration {i} hash differs"
        );
    }
}

/// determinism stress test — N independent renders must agree byte-for-byte.
#[test]
fn determinism_independent_clones_render_byte_equal() {
    let constraints = vec![Constraint::FunctionAllowlist {
        functions: vec!["transfer".into()],
    }];

    let mut last: Option<String> = None;
    for _ in 0..10 {
        let spec = build_spec(TemplateFamily::FunctionAllowlist, constraints.clone());
        let r = render_contract(&spec, 0).expect("render");
        if let Some(prev) = &last {
            assert_eq!(prev, &r.src_lib_rs);
        }
        last = Some(r.src_lib_rs);
    }
}
