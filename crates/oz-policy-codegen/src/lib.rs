//! Track-B codegen pipeline.
//!
//! Phase 3 surface:
//! * `sandbox` — sandboxed `cargo build --target wasm32-unknown-unknown` +
//!   `stellar contract optimize` driver. Produces reproducible WASM and
//!   caches by `sha256(Cargo.toml || "\0" || src/lib.rs)`. (Stream B.)
//! * `render` — turns a `PolicySpec` into a [`sandbox::RenderedCrate`] via
//!   askama templates. See `templates/` at the workspace root. (Stream A.)
//! * `context` — pure-data render-context structs consumed by askama. The
//!   `is_symbol_short_safe` classifier lives here and is the single source
//!   of truth for the `symbol_short!` 9-ASCII-char rule.
//!
//! Phase 1 placeholder (`placeholder.rs`) is retained because external
//! callers (Phase 1 binary completion tests) still reference its symbol; it
//! will be removed in Phase 9 cleanup.
//!
//! See `docs/codegen-dependency-mode.md` for the rationale behind generated
//! crates depending on `stellar-accounts = "=0.7.1"` as a library rather than
//! re-implementing the trait pattern.

#![forbid(unsafe_code)]

pub mod audit_lints;
pub mod context;
pub mod placeholder;
pub mod render;
pub mod sandbox;

pub use audit_lints::{lint_rendered_source, AuditLintError};
pub use render::render_contract;
pub use sandbox::{compile, CompiledArtifact, RenderedCrate, SandboxError};

use oz_policy_core::spec::{PolicySlot, PolicySpec};

/// Phase 3 Round 2 end-to-end orchestrator.
///
/// For every `PolicySlot::Generated` slot in `spec.policies` (in slot order),
/// this routine renders the slot via [`render_contract`] and then drives the
/// rendered crate through the sandbox [`compile`] pipeline. The returned
/// vector contains one [`CompiledArtifact`] per generated slot, preserving the
/// original slot ordering.
///
/// `PolicySlot::Existing` slots are **silently skipped** — those are Track-A
/// reuses of audited OZ primitives and do not go through Track-B codegen.
///
/// A spec with **zero** `Generated` slots is not an error; this returns
/// `Ok(vec![])` so callers can pipe arbitrary specs through without first
/// classifying them.
///
/// Errors surface verbatim from either `render_contract` (template / spec
/// invariants) or `compile` (sandboxed cargo build, `stellar contract
/// optimize`, cache I/O). All errors normalise to the wire-stable
/// `E_CODEGEN_COMPILE_FAILED` (see [`SandboxError`]'s `From` impl).
pub async fn synthesize_track_b(
    spec: &PolicySpec,
) -> Result<Vec<CompiledArtifact>, oz_policy_core::Error> {
    let mut artifacts = Vec::new();
    for (idx, slot) in spec.policies.iter().enumerate() {
        if !matches!(slot, PolicySlot::Generated { .. }) {
            // Track-A slot: not our concern. Continue without touching it.
            continue;
        }
        let rendered = render_contract(spec, idx)?;
        // Phase 9 audit-lint gate. The lints run before the sandbox compile
        // so an unsafe / unkeyed / panic-leaking template surfaces as
        // `E_CODEGEN_COMPILE_FAILED` with a structured violation list,
        // instead of being silently masked by an obscure rustc message
        // (or worse — passing rustc and shipping). See `audit_lints.rs`
        // for the rule set.
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

    /// A spec with zero `Generated` slots must produce an empty vector — not
    /// an error. This lets callers pipe arbitrary specs (Track-A only,
    /// mixed, or empty) through `synthesize_track_b` without first
    /// classifying them.
    #[tokio::test]
    async fn zero_generated_slots_returns_empty() {
        let spec = spec_with_slots(vec![]);
        let artifacts = synthesize_track_b(&spec)
            .await
            .expect("empty policies must not error");
        assert!(artifacts.is_empty());
    }

    /// Track-A `Existing` slots must be silently skipped (no error, no
    /// artifact emitted). With no `Generated` slots present the result is
    /// the same as the empty-policies case.
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

    /// End-to-end smoke: a spec with one `Generated` slot composed of
    /// `FunctionAllowlist + AmountRange` renders cleanly, runs through the
    /// sandbox build, and returns a single artifact with non-empty WASM.
    ///
    /// `#[ignore]` — mirrors the `minimal_compile` ignore pattern. The
    /// sandbox compile path requires `cargo`, `rustc 1.89.0` with the
    /// `wasm32-unknown-unknown` target, `stellar 25.1.0` on `$PATH`, and a
    /// pre-warmed `~/.cargo/registry`. CI runs this only via `cargo nextest
    /// run --include-ignored`.
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
