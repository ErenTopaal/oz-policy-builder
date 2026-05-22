//! Pure-function `PolicySpec → RenderedCrate` Track-B codegen.
//!
//! This module owns:
//!
//! * The askama `Template`-deriving struct that consumes
//!   `templates/base.rs.jinja`.
//! * The `render_contract` entry point which projects a `PolicySpec`'s
//!   `Generated` slot into the template context.
//! * The `RenderedCrate` value object materialised onto disk by Stream B's
//!   sandbox driver.
//!
//! The render is deterministic: identical specs (modulo unrelated ordering
//! perturbations in unrelated `Vec`s) produce byte-equal `src_lib_rs` outputs.
//! That property is enforced by the golden + determinism tests under
//! `tests/golden/`.

use askama::Template;
use oz_policy_core::{
    arg_value::ArgValue,
    errors::Error,
    spec::{ArgMatcher, Constraint, PolicySlot, PolicySpec},
};
use sha2::{Digest, Sha256};

use crate::context::{
    is_symbol_short_safe, AmountRangeEntry, ArgumentPatternEntry, CallFrequencyCtx, FunctionEntry,
    PhaseEntry, SequenceOrderingCtx, TimeWindowCtx,
};
use crate::sandbox::RenderedCrate;

/// The render context. Every field referenced by `base.rs.jinja` (or any of
/// its includes) must be a field here.
///
/// `escape = "none"` is **load-bearing**: askama defaults to HTML escaping on
/// `.jinja` files, which would mangle Rust source (replacing `<`/`>`/`&`
/// inside generic syntax). We render plain text.
#[derive(Template, Debug, Clone)]
#[template(path = "base.rs.jinja", escape = "none")]
pub struct BaseContext {
    /// Hex digest of the canonicalised input spec slot. Used as a deterministic
    /// audit trailer in the rendered source header.
    pub spec_hash: String,
    /// Lowercase snake-case template-family identifier (eg `"function_allowlist"`).
    /// Cosmetic: written into the source banner only.
    pub template_family: String,

    pub has_function_allowlist: bool,
    pub has_argument_pattern: bool,
    pub has_amount_range: bool,
    pub has_asset_allowlist: bool,
    pub has_time_window: bool,
    pub has_call_frequency: bool,
    pub has_sequence_ordering: bool,

    pub function_allowlist: Vec<FunctionEntry>,
    pub argument_patterns: Vec<ArgumentPatternEntry>,
    pub amount_ranges: Vec<AmountRangeEntry>,
    pub asset_allowlist: Vec<String>,
    pub time_window: TimeWindowCtx,
    pub call_frequency: CallFrequencyCtx,
    pub sequence_ordering: SequenceOrderingCtx,
}

/// Render the `Generated` policy slot at `slot_index` of `spec` to a
/// `RenderedCrate` (in-memory `src/lib.rs` + `Cargo.toml` plus their composite
/// hash).
///
/// Errors:
/// * `Error::CodegenCompileFailed` — slot index out of range, slot is not a
///   `Generated` slot, the constraint vector is empty, or askama itself
///   reports a render error.
pub fn render_contract(spec: &PolicySpec, slot_index: usize) -> Result<RenderedCrate, Error> {
    let slot = spec.policies.get(slot_index).ok_or_else(|| {
        Error::CodegenCompileFailed(format!(
            "slot_index {slot_index} out of range (have {} slots)",
            spec.policies.len()
        ))
    })?;

    let constraints = match slot {
        PolicySlot::Existing { .. } => {
            return Err(Error::CodegenCompileFailed(format!(
                "slot {slot_index} is an Existing slot — Track-B codegen requires Generated"
            )));
        }
        PolicySlot::Generated { constraints, .. } => constraints,
    };
    if constraints.is_empty() {
        return Err(Error::CodegenCompileFailed(format!(
            "slot {slot_index} has empty constraints — refusing to emit a do-nothing policy"
        )));
    }

    let template_family = match slot {
        PolicySlot::Generated {
            template_family, ..
        } => format!("{template_family:?}")
            .to_lowercase()
            .replace(' ', "_"),
        _ => unreachable!(),
    };
    let spec_hash = hash_slot_inputs(spec, slot_index);

    let mut ctx = BaseContext {
        spec_hash,
        template_family,
        has_function_allowlist: false,
        has_argument_pattern: false,
        has_amount_range: false,
        has_asset_allowlist: false,
        has_time_window: false,
        has_call_frequency: false,
        has_sequence_ordering: false,
        function_allowlist: Vec::new(),
        argument_patterns: Vec::new(),
        amount_ranges: Vec::new(),
        asset_allowlist: Vec::new(),
        time_window: TimeWindowCtx::default(),
        call_frequency: CallFrequencyCtx::default(),
        sequence_ordering: SequenceOrderingCtx::default(),
    };

    for c in constraints {
        project_constraint(c, &mut ctx)?;
    }

    let src_lib_rs = ctx
        .render()
        .map_err(|e| Error::CodegenCompileFailed(format!("askama render failed: {e}")))?;
    let cargo_toml = generated_cargo_toml(slot_index);

    // Hash convention `sha256(cargo_toml || "\0" || src_lib_rs)` matches
    // Stream B's `sandbox::compile` cache-key contract verbatim — see the
    // doc comment on `sandbox::RenderedCrate`.
    let mut hasher = Sha256::new();
    hasher.update(cargo_toml.as_bytes());
    hasher.update(b"\0");
    hasher.update(src_lib_rs.as_bytes());
    let wasm_hash_of_src: [u8; 32] = hasher.finalize().into();

    Ok(RenderedCrate {
        src_lib_rs,
        cargo_toml,
        wasm_hash_of_src,
    })
}

fn project_constraint(c: &Constraint, ctx: &mut BaseContext) -> Result<(), Error> {
    match c {
        Constraint::FunctionAllowlist { functions } => {
            ctx.has_function_allowlist = true;
            for f in functions {
                validate_symbol(f)?;
                ctx.function_allowlist.push(FunctionEntry::new(f));
            }
        }
        Constraint::ArgumentPattern {
            fn_name,
            arg_index,
            matcher,
        } => {
            ctx.has_argument_pattern = true;
            validate_symbol(fn_name)?;
            let entry = project_argument_pattern(fn_name, *arg_index, matcher)?;
            ctx.argument_patterns.push(entry);
        }
        Constraint::AmountRange {
            fn_name,
            arg_index,
            min_string,
            max_string,
        } => {
            if min_string.is_none() && max_string.is_none() {
                return Err(Error::CodegenCompileFailed(
                    "AmountRange with both bounds open is a no-op; reject at synthesis".into(),
                ));
            }
            ctx.has_amount_range = true;
            validate_symbol(fn_name)?;
            let (has_min, min) = match min_string {
                Some(s) => (true, parse_i128(s)?),
                None => (false, "0".into()),
            };
            let (has_max, max) = match max_string {
                Some(s) => (true, parse_i128(s)?),
                None => (false, "0".into()),
            };
            ctx.amount_ranges.push(AmountRangeEntry {
                fn_name: fn_name.clone(),
                fn_use_symbol_short: is_symbol_short_safe(fn_name),
                arg_index: *arg_index,
                has_min,
                min,
                has_max,
                max,
            });
        }
        Constraint::AssetAllowlist { assets } => {
            ctx.has_asset_allowlist = true;
            for a in assets {
                validate_strkey_contract(a)?;
                ctx.asset_allowlist.push(a.clone());
            }
        }
        Constraint::TimeWindow {
            start_ledger,
            end_ledger,
        } => {
            if ctx.has_time_window {
                return Err(Error::CodegenCompileFailed(
                    "more than one TimeWindow constraint per slot is unsupported".into(),
                ));
            }
            if end_ledger < start_ledger {
                return Err(Error::CodegenCompileFailed(format!(
                    "TimeWindow: end_ledger {end_ledger} < start_ledger {start_ledger}"
                )));
            }
            ctx.has_time_window = true;
            ctx.time_window = TimeWindowCtx {
                start_ledger: *start_ledger,
                end_ledger: *end_ledger,
            };
        }
        Constraint::CallFrequency {
            max_calls,
            window_ledgers,
        } => {
            if ctx.has_call_frequency {
                return Err(Error::CodegenCompileFailed(
                    "more than one CallFrequency constraint per slot is unsupported".into(),
                ));
            }
            if *max_calls == 0 || *window_ledgers == 0 {
                return Err(Error::CodegenCompileFailed(
                    "CallFrequency: max_calls and window_ledgers must be non-zero".into(),
                ));
            }
            ctx.has_call_frequency = true;
            ctx.call_frequency = CallFrequencyCtx {
                max_calls: *max_calls,
                window_ledgers: *window_ledgers,
            };
        }
        Constraint::SequenceOrdering { phases } => {
            if ctx.has_sequence_ordering {
                return Err(Error::CodegenCompileFailed(
                    "more than one SequenceOrdering constraint per slot is unsupported".into(),
                ));
            }
            if phases.is_empty() {
                return Err(Error::CodegenCompileFailed(
                    "SequenceOrdering with zero phases is a no-op".into(),
                ));
            }
            ctx.has_sequence_ordering = true;
            let entries: Vec<PhaseEntry> = phases.iter().map(|p| PhaseEntry::new(p)).collect();
            for p in phases {
                validate_symbol(p)?;
            }
            ctx.sequence_ordering = SequenceOrderingCtx {
                phases_len: entries.len() as u32,
                phases: entries,
            };
        }
    }
    Ok(())
}

fn project_argument_pattern(
    fn_name: &str,
    arg_index: u32,
    matcher: &ArgMatcher,
) -> Result<ArgumentPatternEntry, Error> {
    let mut entry = ArgumentPatternEntry {
        fn_name: fn_name.to_string(),
        fn_use_symbol_short: is_symbol_short_safe(fn_name),
        arg_index,
        kind: String::new(),
        address: String::new(),
        value: String::new(),
        bytes_csv: String::new(),
        has_min: false,
        min: String::new(),
        has_max: false,
        max: String::new(),
    };
    match matcher {
        ArgMatcher::Exact { value } => match value {
            ArgValue::Address(a) => {
                validate_strkey_contract(a)?;
                entry.kind = "exact_address".into();
                entry.address = a.clone();
            }
            ArgValue::U32(v) => {
                entry.kind = "exact_u32".into();
                entry.value = v.to_string();
            }
            ArgValue::U64(s) => {
                entry.kind = "exact_u64".into();
                let _: u64 = s.parse().map_err(|_| {
                    Error::CodegenCompileFailed(format!("invalid u64 string {s:?}"))
                })?;
                entry.value = s.clone();
            }
            ArgValue::Bytes { hex } => {
                let bytes = hex_decode(hex)?;
                entry.kind = "exact_bytes".into();
                entry.bytes_csv = bytes
                    .iter()
                    .map(|b| format!("{}u8", b))
                    .collect::<Vec<_>>()
                    .join(", ");
            }
            other => {
                return Err(Error::CodegenCompileFailed(format!(
                    "ArgumentPattern::Exact: unsupported ArgValue variant {other:?}"
                )));
            }
        },
        ArgMatcher::Range {
            min_string,
            max_string,
        } => {
            if min_string.is_none() && max_string.is_none() {
                return Err(Error::CodegenCompileFailed(
                    "ArgumentPattern::Range with both bounds open is a no-op".into(),
                ));
            }
            entry.kind = "i128_range".into();
            if let Some(s) = min_string {
                entry.has_min = true;
                entry.min = parse_i128(s)?;
            }
            if let Some(s) = max_string {
                entry.has_max = true;
                entry.max = parse_i128(s)?;
            }
        }
        ArgMatcher::Allowlist { .. } | ArgMatcher::Blocklist { .. } => {
            return Err(Error::CodegenCompileFailed(
                "ArgumentPattern Allowlist/Blocklist not yet supported in templates".into(),
            ));
        }
    }
    Ok(entry)
}

/// Validate that a string can be a Soroban `Symbol`: ≤ 32 chars, ASCII alpha-
/// numeric + underscore. (The 9-char rule for `symbol_short!` is enforced
/// separately by `is_symbol_short_safe`.)
fn validate_symbol(s: &str) -> Result<(), Error> {
    if s.is_empty() || s.len() > 32 {
        return Err(Error::CodegenCompileFailed(format!(
            "symbol {s:?} must be 1..=32 chars"
        )));
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::CodegenCompileFailed(format!(
            "symbol {s:?} contains non-[A-Za-z0-9_] chars"
        )));
    }
    Ok(())
}

/// Surface-level StrKey contract-address sanity. Full StrKey base32 decoding
/// happens at install time on-chain; here we only assert the shape so the
/// generated template doesn't embed an obviously-malformed literal.
fn validate_strkey_contract(s: &str) -> Result<(), Error> {
    if s.len() != 56 || !s.starts_with('C') {
        return Err(Error::CodegenCompileFailed(format!(
            "expected 56-char StrKey starting with 'C', got {s:?}"
        )));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err(Error::CodegenCompileFailed(format!(
            "StrKey {s:?} contains lowercase or non-alphanumeric chars"
        )));
    }
    Ok(())
}

fn parse_i128(s: &str) -> Result<String, Error> {
    let v: i128 = s
        .parse()
        .map_err(|_| Error::CodegenCompileFailed(format!("invalid i128 string {s:?}")))?;
    Ok(v.to_string())
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, Error> {
    if hex.len() % 2 != 0 {
        return Err(Error::CodegenCompileFailed(
            "hex string must have even length".into(),
        ));
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    for chunk in hex.as_bytes().chunks(2) {
        let s = core::str::from_utf8(chunk)
            .map_err(|_| Error::CodegenCompileFailed("non-utf8 hex".into()))?;
        let b = u8::from_str_radix(s, 16)
            .map_err(|_| Error::CodegenCompileFailed(format!("non-hex byte {s:?}")))?;
        out.push(b);
    }
    Ok(out)
}

fn hash_slot_inputs(spec: &PolicySpec, slot_index: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(spec.schema.as_bytes());
    hasher.update(b"|");
    hasher.update((slot_index as u64).to_be_bytes());
    if let Some(PolicySlot::Generated {
        template_family,
        constraints,
    }) = spec.policies.get(slot_index)
    {
        hasher.update(format!("{template_family:?}").as_bytes());
        for c in constraints {
            hasher.update(b"|");
            hasher.update(format!("{c:?}").as_bytes());
        }
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        use core::fmt::Write;
        let _ = write!(out, "{:02x}", b);
    }
    out
}

fn generated_cargo_toml(slot_index: usize) -> String {
    // Pinned versions mirror the workspace Cargo.toml. The generated crate is
    // a standalone cdylib that is built outside the workspace by the sandbox
    // driver (Stream B), so the two direct deps are inlined here.
    // Transitive sub-crates (soroban-sdk-macros / soroban-spec / etc.) are
    // resolved deterministically via `cargo build --locked` with a
    // pre-prepared `Cargo.lock` that Stream B's sandbox driver writes
    // alongside this Cargo.toml on materialisation.
    //
    // `rust-version = "1.89.0"` + `resolver = "3"` are LOAD-BEARING:
    // soroban-sdk-macros / soroban-spec / soroban-spec-rust each cut a
    // `25.3.1` patch that requires rustc 1.91.0. Without these two fields
    // Cargo's MSRV-unaware resolver greedily picks the patch and the build
    // fails with `rustc 1.89.0 is not supported`. With them, the resolver
    // backs off to `25.3.0`, which is buildable on 1.89.0. The exact
    // soroban-sdk pin (`=25.3.0`) further forbids the major bump.
    format!(
        r#"# AUTO-GENERATED by oz-policy-codegen (Phase 3 Track-B). DO NOT EDIT.
# See `docs/codegen-dependency-mode.md` for the rationale behind depending on
# `stellar-accounts` as a library.
[package]
name = "oz-policy-generated-slot-{slot_index}"
version = "0.0.0"
edition = "2021"
license = "Apache-2.0"
publish = false
rust-version = "1.89.0"
resolver = "3"

[lib]
crate-type = ["cdylib"]

[dependencies]
soroban-sdk = "=25.3.0"
stellar-accounts = "=0.7.1"

[profile.release]
overflow-checks = true
lto = "fat"
codegen-units = 1
strip = "symbols"
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use oz_policy_core::spec::{
        ContextRuleSpec, ContextType, PolicySlot, PolicySpec, RecordingRef, SynthesisMode,
        TemplateFamily,
    };

    fn minimal_spec(constraint: Constraint) -> PolicySpec {
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
                template_family: TemplateFamily::FunctionAllowlist,
                constraints: vec![constraint],
            }],
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: None,
                schema: "oz-recording/v1".into(),
            },
        }
    }

    #[test]
    fn render_function_allowlist_minimal() {
        let spec = minimal_spec(Constraint::FunctionAllowlist {
            functions: vec!["transfer".into()],
        });
        let r = render_contract(&spec, 0).expect("render ok");
        assert!(r.src_lib_rs.contains("symbol_short!(\"transfer\")"));
        assert!(r.src_lib_rs.contains("PolicyError::FunctionNotAllowed"));
        assert!(r.src_lib_rs.contains("smart_account.require_auth()"));
        assert!(r.cargo_toml.contains("stellar-accounts = \"=0.7.1\""));
        assert_eq!(r.wasm_hash_of_src.len(), 32);
    }

    #[test]
    fn render_rejects_out_of_range_slot() {
        let spec = minimal_spec(Constraint::FunctionAllowlist {
            functions: vec!["transfer".into()],
        });
        let err = render_contract(&spec, 99).unwrap_err();
        assert_eq!(err.code(), "E_CODEGEN_COMPILE_FAILED");
    }

    #[test]
    fn render_rejects_empty_constraints() {
        let mut spec = minimal_spec(Constraint::FunctionAllowlist {
            functions: vec!["x".into()],
        });
        spec.policies[0] = PolicySlot::Generated {
            template_family: TemplateFamily::FunctionAllowlist,
            constraints: Vec::new(),
        };
        let err = render_contract(&spec, 0).unwrap_err();
        assert_eq!(err.code(), "E_CODEGEN_COMPILE_FAILED");
    }

    #[test]
    fn long_function_name_uses_symbol_new() {
        let spec = minimal_spec(Constraint::FunctionAllowlist {
            functions: vec!["transfer_from".into()], // 13 chars
        });
        let r = render_contract(&spec, 0).expect("render ok");
        assert!(r.src_lib_rs.contains("Symbol::new(e, \"transfer_from\")"));
        assert!(!r.src_lib_rs.contains("symbol_short!(\"transfer_from\")"));
    }

    #[test]
    fn render_is_deterministic() {
        let spec = minimal_spec(Constraint::FunctionAllowlist {
            functions: vec!["transfer".into(), "approve".into()],
        });
        let r1 = render_contract(&spec, 0).unwrap();
        let r2 = render_contract(&spec, 0).unwrap();
        assert_eq!(r1.src_lib_rs, r2.src_lib_rs);
        assert_eq!(r1.wasm_hash_of_src, r2.wasm_hash_of_src);
    }

    #[test]
    fn render_rejects_existing_slot() {
        use oz_policy_core::spec::{ExistingPrimitive, ExistingPrimitiveParams};
        let spec = PolicySpec {
            schema: "oz-policy-builder/v1".into(),
            synthesis_mode: SynthesisMode::CodegenOnly,
            context_rule: ContextRuleSpec {
                name: "r".into(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: Vec::new(),
            policies: vec![PolicySlot::Existing {
                primitive: ExistingPrimitive::SimpleThreshold,
                params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
            }],
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: None,
                schema: "oz-recording/v1".into(),
            },
        };
        let err = render_contract(&spec, 0).unwrap_err();
        assert_eq!(err.code(), "E_CODEGEN_COMPILE_FAILED");
        assert!(err.to_string().contains("Existing"));
    }

    #[test]
    fn empty_amount_range_is_rejected() {
        let spec = minimal_spec(Constraint::AmountRange {
            fn_name: "transfer".into(),
            arg_index: 2,
            min_string: None,
            max_string: None,
        });
        let err = render_contract(&spec, 0).unwrap_err();
        assert!(err.to_string().contains("AmountRange"));
    }
}
