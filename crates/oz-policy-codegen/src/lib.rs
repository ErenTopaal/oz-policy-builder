//! track-b codegen pipeline.
//! sandbox = wasm32 build + stellar optimize. render = askama-templated crate.
//! context = render-context structs (incl. `is_symbol_short_safe`).

#![forbid(unsafe_code)]

pub mod audit_lints;
pub mod context;
pub mod placeholder;
pub mod render;
pub mod sandbox;

pub use audit_lints::{lint_rendered_source, AuditLintError};
pub use render::render_contract;
pub use sandbox::{cache_dir_for, compile, CompiledArtifact, RenderedCrate, SandboxError};

use oz_policy_core::spec::{PolicySlot, PolicySpec};

/// end-to-end orchestrator: render every Generated slot then compile via sandbox.
/// Existing slots are silently skipped (track-A). zero Generated → Ok(empty).
pub async fn synthesize_track_b(
    spec: &PolicySpec,
) -> Result<Vec<CompiledArtifact>, oz_policy_core::Error> {
    let mut artifacts = Vec::new();
    for (idx, slot) in spec.policies.iter().enumerate() {
        if !matches!(slot, PolicySlot::Generated { .. }) {
            // track-A slot, skip.
            continue;
        }
        let rendered = render_contract(spec, idx)?;
        // audit-lints before sandbox compile so violations surface as
        // E_CODEGEN_COMPILE_FAILED with structured detail.
        if let Err(violations) = lint_rendered_source(&rendered.src_lib_rs) {
            let mut detail = format!("audit lints failed: {} violation(s)\n", violations.len());
            for v in &violations {
                detail.push_str(&format!("  - {v}\n"));
            }
            return Err(oz_policy_core::Error::CodegenCompileFailed(detail));
        }
        let artifact = compile(&rendered).await?;
        artifacts.push(artifact);
    }
    Ok(artifacts)
}

#[cfg(test)]
mod synthesize_track_b_tests {
    use super::*;
    use oz_policy_core::spec::{
        Constraint, ContextRuleSpec, ContextType, ExistingPrimitive, ExistingPrimitiveParams,
        PolicySlot, PolicySpec, RecordingRef, SynthesisMode, TemplateFamily,
    };

    fn spec_with_slots(slots: Vec<PolicySlot>) -> PolicySpec {
        PolicySpec {
            schema: "oz-policy-builder/v1".into(),
            synthesis_mode: SynthesisMode::CodegenOnly,
            context_rule: ContextRuleSpec {
                name: "rule".into(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: Vec::new(),
            policies: slots,
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: None,
                schema: "oz-recording/v1".into(),
            },
        }
    }

    /// zero `Generated` slots → empty vec, no error.
    #[tokio::test]
    async fn zero_generated_slots_returns_empty() {
        let spec = spec_with_slots(vec![]);
        let artifacts = synthesize_track_b(&spec)
            .await
            .expect("empty policies must not error");
        assert!(artifacts.is_empty());
    }

    /// Existing-only slots silently skipped.
    #[tokio::test]
    async fn existing_only_slots_are_skipped() {
        let spec = spec_with_slots(vec![PolicySlot::Existing {
            primitive: ExistingPrimitive::SimpleThreshold,
            params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
        }]);
        let artifacts = synthesize_track_b(&spec)
            .await
            .expect("existing-only spec must not error");
        assert!(artifacts.is_empty());
    }

    /// end-to-end smoke. `#[ignore]` because sandbox needs cargo/rustc/stellar
    /// on path + warm registry. ci runs via `--include-ignored`.
    #[ignore]
    #[tokio::test]
    async fn one_generated_slot_composition_compiles() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::env::set_var("OZ_POLICY_CODEGEN_CACHE_DIR", tmp.path());

        let spec = spec_with_slots(vec![PolicySlot::Generated {
            template_family: TemplateFamily::FunctionAllowlist,
            constraints: vec![
                Constraint::FunctionAllowlist {
                    functions: vec!["transfer".into()],
                },
                Constraint::AmountRange {
                    fn_name: "transfer".into(),
                    arg_index: 2,
                    min_string: Some("1".into()),
                    max_string: Some("1000000".into()),
                },
            ],
        }]);

        let artifacts = synthesize_track_b(&spec)
            .await
            .expect("compile must succeed");
        std::env::remove_var("OZ_POLICY_CODEGEN_CACHE_DIR");

        assert_eq!(
            artifacts.len(),
            1,
            "exactly one Generated slot → one artifact"
        );
        let art = &artifacts[0];
        assert!(!art.wasm.is_empty(), "wasm bytes must be non-empty");
        assert_eq!(
            &art.wasm[..4],
            b"\0asm",
            "wasm magic header must be present"
        );
    }
}
