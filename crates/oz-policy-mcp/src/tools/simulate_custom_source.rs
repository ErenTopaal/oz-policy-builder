//! `simulate_custom_source` MCP tool — playground re-simulate loop.
//!
//! takes a `recording_id` + `spec_id` (used solely for the locked
//! Cargo.toml template) + a user-edited `modified_lib_rs`, then drives
//! the same Track-B sandbox build + simhost pipeline as `simulate_policy`
//! — only with the user's source substituted in place of the rendered
//! source for the spec's first `Generated` slot.
//!
//! security: the `Cargo.toml` is **always** the spec-rendered template
//! (never user-supplied). Before any cargo invocation we run a hardcoded
//! regex sweep over the source ([`check_forbidden`]); this is mirrored
//! verbatim by `frontend/src/playground/preflight.ts`. Pre-flight is
//! belt-and-suspenders against the bwrap sandbox — see
//! `docs/superpowers/specs/2026-06-14-playground-design.md` §6.
//!
//! cache key: `sha256(modified_lib_rs)` — identical edits hit cache,
//! and the user's edited body never collides with the original
//! spec's rendered source under `sandbox::compile`.

use oz_policy_codegen::sandbox::{compile, RenderedCrate};
use oz_policy_codegen::{render_contract, synthesize_track_b, CompiledArtifact};
use oz_policy_core::spec::PolicySlot;
use oz_policy_simhost::deny::DenyVector;
use oz_policy_simhost::run::{run_full_suite, SimReport};
use regex::Regex;
use rmcp::model::{ErrorCode, ErrorData};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error_mapping::error_to_jsonrpc;
use crate::store::McpStore;

/// JSON-RPC error code for `E_PREFLIGHT_FORBIDDEN_PATTERN`. Sits in the
/// same `-321xx` band as the other `oz-policy-mcp` error codes; not
/// represented in `oz_policy_core::Error` because the failure is
/// MCP-tool-local (the codegen / simhost layers never see the user's
/// source for a forbidden submission).
pub const PREFLIGHT_FORBIDDEN_PATTERN_CODE: i32 = -32112;

/// stable wire-string for the preflight error code.
pub const PREFLIGHT_FORBIDDEN_PATTERN_NAME: &str = "E_PREFLIGHT_FORBIDDEN_PATTERN";

/// JSON-RPC error code for cargo-build failures coming from a user-edited
/// source. Distinct from `E_CODEGEN_COMPILE_FAILED` because the failure
/// is reasonably attributable to the user's edit, not to the synthesizer
/// — the frontend renders this with line:col jumps.
pub const CARGO_BUILD_FAILED_CODE: i32 = -32113;

/// stable wire-string for the user-edit build-failure code.
pub const CARGO_BUILD_FAILED_NAME: &str = "E_CARGO_BUILD_FAILED";

// ----- forbidden-pattern table ----------------------------------------

/// description of a single forbidden pattern. Keeping the raw regex
/// string in a const alongside a one-sentence reason makes the audit
/// path trivial: grep for the const to find the rejection site.
struct PatternEntry {
    /// human-readable label included verbatim in the rejection payload.
    /// Frontend's `preflight.ts` keys off this exact string, so any
    /// rename here is a cross-stack break.
    label: &'static str,
    /// regex source. Mirror in `frontend/src/playground/preflight.ts`.
    regex: &'static str,
}

/// the six forbidden patterns from playground design §6.1. Order is
/// significant: the *first* matching pattern is reported (so corpus
/// tests can assert the expected label).
const PATTERNS: &[PatternEntry] = &[
    PatternEntry {
        label: r"\bunsafe\s*(\{|fn|impl|trait)\b",
        regex: r"\bunsafe\s*(\{|fn|impl|trait)\b",
    },
    PatternEntry {
        label: r#"\bextern\s+"[A-Za-z]+""#,
        regex: r#"\bextern\s+"[A-Za-z]+""#,
    },
    PatternEntry {
        label: r"#\[\s*proc_macro(_derive|_attribute)?\s*[\(\]]",
        regex: r"#\[\s*proc_macro(_derive|_attribute)?\s*[\(\]]",
    },
    PatternEntry {
        label: r"#\[\s*link(_name)?\b",
        regex: r"#\[\s*link(_name)?\b",
    },
    PatternEntry {
        label: r"\binclude_(bytes|str)!\s*\(",
        // single regex covers both include_bytes! / include_str! per spec §6.1.
        regex: r"\binclude_(bytes|str)!\s*\(",
    },
    PatternEntry {
        label: r#""build.rs""#,
        // literal string match per spec; quoted with the surrounding ""s.
        regex: r#""build\.rs""#,
    },
];

/// structured rejection produced by [`check_forbidden`]. Fields match the
/// `data` payload of the surfaced `E_PREFLIGHT_FORBIDDEN_PATTERN` error
/// — both the corpus tests AND the frontend mirror branch on these
/// names directly, so renaming is a wire break.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForbiddenPattern {
    /// the regex (`PatternEntry::label`) that fired. Stable identifier.
    pub pattern: &'static str,
    /// 1-based line number of the first match. Lines are counted at
    /// LF boundaries; CRLF inputs are normalised so the count stays
    /// consistent with what Monaco shows the user.
    pub line_number: usize,
    /// the offending source line, verbatim (trailing CR stripped, LF
    /// already excluded by the line iterator).
    pub line_text: String,
}

/// pre-flight regex sweep. Returns `Ok(())` for accepted sources, or the
/// first matching forbidden pattern with location.
///
/// Public so the corpus tests (and a future TypeScript codegen of the
/// pattern list) can target it. The implementation is intentionally a
/// regex sweep rather than an AST walk: adversarial users can defeat
/// AST-based checks with token tricks; the bwrap sandbox in
/// `oz-policy-codegen` is the real security boundary. Pre-flight is
/// belt-and-suspenders.
pub fn check_forbidden(src: &str) -> Result<(), ForbiddenPattern> {
    // compile every regex on first call; cheap (six small patterns) and
    // avoids the `once_cell` dep — the tool runs at human latency, not
    // tight-loop latency.
    let compiled: Vec<(Regex, &'static str)> = PATTERNS
        .iter()
        .map(|p| {
            (
                Regex::new(p.regex).expect("pattern is a valid regex (compile-time constant)"),
                p.label,
            )
        })
        .collect();

    for (line_idx, raw_line) in src.split('\n').enumerate() {
        // strip a trailing CR so CRLF inputs report the same `line_text`
        // a unix-line user would see.
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        for (re, label) in &compiled {
            if re.is_match(line) {
                return Err(ForbiddenPattern {
                    pattern: label,
                    line_number: line_idx + 1,
                    line_text: line.to_string(),
                });
            }
        }
    }
    Ok(())
}

// ----- tool input ----------------------------------------------------

/// `simulate_custom_source` input.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SimulateCustomSourceInput {
    /// recording ID returned by an earlier `record_transaction` call.
    pub recording_id: String,
    /// spec ID returned by an earlier `synthesize_policy` call. Used
    /// solely to look up the locked `Cargo.toml` template (and the
    /// non-edited Generated slots, if any). The user does NOT get to
    /// edit `Cargo.toml`; see playground design §6.2.
    pub spec_id: String,
    /// user-edited `src/lib.rs` body, substituted in place of the
    /// rendered source for the spec's first `Generated` slot.
    pub modified_lib_rs: String,
    /// optional caller-supplied deny vectors appended to the generated
    /// boundary-mutation set. Default empty (mirrors `simulate_policy`).
    pub extra_deny_vectors: Option<Vec<DenyVector>>,
}

// ----- handler -------------------------------------------------------

/// `simulate_custom_source` handler. Outputs `SimReport` directly, the
/// same shape as `simulate_policy`.
///
/// errors:
/// * `E_PREFLIGHT_FORBIDDEN_PATTERN` (-32112) — `modified_lib_rs`
///   contained one of the six hardcoded forbidden patterns. `data`
///   carries `{ "pattern", "line_number", "line_text" }`.
/// * `E_SPEC_NOT_FOUND` (-32110) — `spec_id` not in the in-memory store.
/// * `ErrorData::invalid_params` (-32602) — `recording_id` not in the
///   in-memory store. Mirrors `simulate_policy`'s ergonomics (recorder
///   ids aren't a domain error, they're an MCP-layer state-loss).
/// * `E_CARGO_BUILD_FAILED` (-32113) — user-edited source failed to
///   build under the sandbox. `data` carries `{ "stderr", "stdout",
///   "exit_code" }` so the frontend can render Rust errors with
///   line:col jumps. Falls back to a plain detail string when the
///   sandbox layer's `BuildFailed` payload doesn't follow the
///   `exit=… --- stderr --- … --- stdout --- …` shape (defensive).
/// * Any other `E_CODEGEN_COMPILE_FAILED` (audit lints, sandbox setup,
///   `stellar contract optimize` failure) bubbles up verbatim via
///   `error_to_jsonrpc` — same handler the existing tools use.
/// * `E_SIM_PERMIT_DENIED` / `E_SIM_DENY_PASSED` — simhost outcomes,
///   identical to `simulate_policy`.
pub async fn simulate_custom_source(
    store: &McpStore,
    input: SimulateCustomSourceInput,
) -> Result<SimReport, ErrorData> {
    // 1. preflight — *always* before any FS or process work.
    if let Err(hit) = check_forbidden(&input.modified_lib_rs) {
        return Err(preflight_error(&hit));
    }

    // 2. spec lookup.
    let spec = store.get_spec(&input.spec_id).ok_or_else(|| {
        error_to_jsonrpc(&oz_policy_core::Error::SpecNotFound(format!(
            "simulate_custom_source: spec_id {:?} not found in store",
            input.spec_id
        )))
    })?;

    // 3. recording lookup.
    let recording = store.get_recording(&input.recording_id).ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "simulate_custom_source: recording_id {:?} not found in store",
                input.recording_id
            ),
            None,
        )
    })?;

    // 4. locate the first Generated slot. We substitute the user's
    //    `modified_lib_rs` ONLY into that slot; remaining Generated
    //    slots (if any) keep their rendered source. Existing slots are
    //    skipped by `synthesize_track_b` — same as `simulate_policy`.
    let first_generated_idx = spec
        .policies
        .iter()
        .position(|s| matches!(s, PolicySlot::Generated { .. }));

    // 5. build the artifact list in slot order. For the substituted
    //    slot we hand-build a `RenderedCrate` whose `wasm_hash_of_src`
    //    is `sha256(modified_lib_rs)` — the spec is explicit that the
    //    cache key for the user-edit path keys ONLY on the modified
    //    source (not on cargo+source) so identical edits hit cache
    //    across spec revisions that share the same Cargo.toml template.
    let mut artifacts: Vec<CompiledArtifact> = Vec::new();
    if let Some(target_idx) = first_generated_idx {
        // For all Generated slots BEFORE the target index, render +
        // compile normally. Since target_idx is the *first* Generated
        // slot, this list is empty in practice, but we walk it for
        // forward-compatibility with multi-Generated specs.
        for (idx, slot) in spec.policies.iter().enumerate() {
            if idx == target_idx {
                // substituted slot — render to grab the Cargo.toml,
                // then swap in the user's source body.
                let rendered = render_contract(&spec, idx).map_err(|e| error_to_jsonrpc(&e))?;
                let mut hasher = Sha256::new();
                hasher.update(input.modified_lib_rs.as_bytes());
                let wasm_hash_of_src: [u8; 32] = hasher.finalize().into();
                let edited = RenderedCrate {
                    src_lib_rs: input.modified_lib_rs.clone(),
                    cargo_toml: rendered.cargo_toml,
                    wasm_hash_of_src,
                };
                let artifact = compile(&edited).await.map_err(map_build_error)?;
                artifacts.push(artifact);
            } else if matches!(slot, PolicySlot::Generated { .. }) {
                // un-edited Generated slot — render + compile through
                // the standard path.
                let rendered = render_contract(&spec, idx).map_err(|e| error_to_jsonrpc(&e))?;
                let artifact = compile(&rendered).await.map_err(map_build_error)?;
                artifacts.push(artifact);
            }
            // PolicySlot::Existing → Track-A composition, skipped.
        }
    } else {
        // No Generated slots at all. Track-B has nothing to build; just
        // delegate to `synthesize_track_b` (which returns an empty Vec)
        // so the simhost gets a fresh, empty artifact list rather than
        // a misleading swap.
        artifacts = synthesize_track_b(&spec)
            .await
            .map_err(|e| error_to_jsonrpc(&e))?;
    }

    let extra_deny = input.extra_deny_vectors.unwrap_or_default();
    run_full_suite(&spec, &recording, &artifacts, extra_deny)
        .await
        .map_err(|e| error_to_jsonrpc(&e))
}

// ----- error construction --------------------------------------------

/// build the `E_PREFLIGHT_FORBIDDEN_PATTERN` ErrorData with the
/// structured fields the frontend expects.
fn preflight_error(hit: &ForbiddenPattern) -> ErrorData {
    let message = format!(
        "{}: forbidden pattern matched on line {}: {}",
        PREFLIGHT_FORBIDDEN_PATTERN_NAME, hit.line_number, hit.pattern
    );
    let data = json!({
        "error_code": PREFLIGHT_FORBIDDEN_PATTERN_NAME,
        "pattern": hit.pattern,
        "line_number": hit.line_number,
        "line_text": hit.line_text,
    });
    ErrorData::new(
        ErrorCode(PREFLIGHT_FORBIDDEN_PATTERN_CODE),
        message,
        Some(data),
    )
}

/// map a codegen `Error` arising from `compile(...)` into MCP `ErrorData`,
/// upgrading the `E_CODEGEN_COMPILE_FAILED` case to the more specific
/// `E_CARGO_BUILD_FAILED` with structured `{ stderr, stdout, exit_code }`
/// when the underlying `SandboxError::BuildFailed` shape is recognisable.
///
/// The sandbox layer (see `oz-policy-codegen/src/sandbox.rs::run_build_command`)
/// formats build failures as:
///
/// ```text
/// exit=Some(<code>)
/// --- stderr ---
/// <stderr body>
/// --- stdout ---
/// <stdout body>
/// ```
///
/// We parse that envelope back out for the structured payload. If the
/// shape doesn't match (e.g. the `BuildFailed` came from spawn failure
/// or audit-lints), we fall back to `error_to_jsonrpc` so the original
/// detail string is preserved verbatim.
fn map_build_error(e: oz_policy_core::Error) -> ErrorData {
    let detail = e.to_string();
    if let Some((exit_code, stderr, stdout)) = parse_build_failure(&detail) {
        let message = format!(
            "{}: cargo build of edited lib.rs failed (exit={exit_code:?})",
            CARGO_BUILD_FAILED_NAME
        );
        let data = json!({
            "error_code": CARGO_BUILD_FAILED_NAME,
            "stderr": stderr,
            "stdout": stdout,
            "exit_code": exit_code,
        });
        return ErrorData::new(ErrorCode(CARGO_BUILD_FAILED_CODE), message, Some(data));
    }
    error_to_jsonrpc(&e)
}

/// parse the `exit=… --- stderr --- … --- stdout --- …` envelope. Returns
/// `(exit_code, stderr, stdout)` on success. `exit_code` is the inner
/// numeric value of `Some(<n>)` or `None` when the process was killed by
/// a signal (matches `ExitStatus::code()` shape).
fn parse_build_failure(s: &str) -> Option<(Option<i32>, String, String)> {
    // tolerant of variations in surrounding context (the sandbox layer
    // may someday prefix with `cargo build failed: ` or similar).
    let exit_anchor = s.find("exit=")?;
    let stderr_anchor = s.find("--- stderr ---\n")?;
    let stdout_anchor = s.find("--- stdout ---\n")?;
    if exit_anchor >= stderr_anchor || stderr_anchor >= stdout_anchor {
        return None;
    }
    let exit_span = &s[exit_anchor + "exit=".len()..stderr_anchor];
    let exit_code = parse_exit_code(exit_span.trim_end_matches('\n'));
    let stderr = s[stderr_anchor + "--- stderr ---\n".len()..stdout_anchor].to_string();
    // strip the trailing newline preceding `--- stdout ---` so the
    // body matches what the user would `tail` from the cargo invocation.
    let stderr = stderr.trim_end_matches('\n').to_string();
    let stdout = s[stdout_anchor + "--- stdout ---\n".len()..].to_string();
    Some((exit_code, stderr, stdout))
}

/// parse `Some(<n>)` / `None` exit-status shapes. Returns `None` on
/// anything else so the fallback path takes over.
fn parse_exit_code(s: &str) -> Option<i32> {
    let s = s.trim();
    if s == "None" {
        return None;
    }
    let inner = s.strip_prefix("Some(")?.strip_suffix(')')?;
    inner.trim().parse::<i32>().ok()
}

// ----- pattern-list snapshot test ------------------------------------

#[cfg(test)]
mod pattern_compile_tests {
    use super::*;

    /// every `PATTERNS` entry must compile as a valid `regex::Regex`.
    /// Run by `cargo test -p oz-policy-mcp` so a typo lands in CI
    /// rather than the first end-user submission.
    #[test]
    fn every_pattern_compiles() {
        for p in PATTERNS {
            Regex::new(p.regex)
                .unwrap_or_else(|e| panic!("forbidden pattern {} failed to compile: {e}", p.label));
        }
    }

    /// the table must stay six entries — the spec lists exactly six
    /// forbidden patterns. Catches accidental duplication / removal.
    #[test]
    fn pattern_table_has_exactly_six_entries() {
        assert_eq!(PATTERNS.len(), 6, "spec §6.1 lists 6 forbidden patterns");
    }

    /// the build-failure parser must round-trip the envelope shape the
    /// sandbox layer currently produces. If the sandbox format changes,
    /// this test fails loudly — preferable to silently degrading to the
    /// fallback path on every build error.
    #[test]
    fn build_failure_parser_matches_sandbox_envelope() {
        let body = "exit=Some(101)\n--- stderr ---\nerror[E0432]: unresolved import `foo`\n--- stdout ---\n   Compiling pkg v0.0.1\n";
        let (code, stderr, stdout) = parse_build_failure(body).expect("parses");
        assert_eq!(code, Some(101));
        assert!(stderr.contains("E0432"));
        assert!(stdout.contains("Compiling"));
    }

    #[test]
    fn build_failure_parser_handles_signal_kill() {
        let body = "exit=None\n--- stderr ---\nkilled\n--- stdout ---\n";
        let (code, stderr, stdout) = parse_build_failure(body).expect("parses");
        assert!(code.is_none());
        assert_eq!(stderr, "killed");
        assert_eq!(stdout, "");
    }

    #[test]
    fn build_failure_parser_rejects_non_envelope() {
        let body = "audit lints failed: 1 violation(s)\n  - [no_unsafe] line 3: unsafe block";
        assert!(parse_build_failure(body).is_none());
    }
}
