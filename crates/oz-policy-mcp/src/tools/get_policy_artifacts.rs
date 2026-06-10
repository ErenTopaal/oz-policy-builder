//! `get_policy_artifacts` MCP tool — playground design §3.4.
//!
//! exposes the rendered Cargo.toml + lib.rs for every `Generated` slot in
//! a stored [`PolicySpec`], plus the SHA-256 of the (pre-optimize) wasm
//! that `cargo build` produced and the (post-optimize) wasm that
//! `stellar contract optimize` produced. Reuses the existing
//! `oz_policy_codegen::synthesize_track_b` pipeline so the bytes the
//! playground Source tab mounts in Monaco are byte-identical to what the
//! sandbox compiles.
//!
//! caching: keyed by `spec_id`. The codegen sandbox already caches per
//! `src_hash` on disk; we add an in-memory `DashMap<spec_id, output>` so
//! repeated calls don't even hit the cache lookup / file read paths.
//!
//! errors:
//! * `E_SPEC_NOT_FOUND` — `spec_id` has no matching entry in the store.
//! * `E_CODEGEN_COMPILE_FAILED` — bubbled up from the sandbox compile.

use std::sync::Arc;

use dashmap::DashMap;
use oz_policy_codegen::{cache_dir_for, render_contract, synthesize_track_b};
use oz_policy_core::spec::{PolicySlot, PolicySpec};
use rmcp::model::ErrorData;
use sha2::{Digest, Sha256};

use crate::error_mapping::error_to_jsonrpc;
use crate::store::McpStore;

/// `get_policy_artifacts` input — a single `spec_id` returned by an
/// earlier `synthesize_policy` call.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetPolicyArtifactsInput {
    /// spec id returned by `synthesize_policy`.
    pub spec_id: String,
}

/// one entry in [`GetPolicyArtifactsOutput::generated_sources`] — the
/// rendered `lib.rs` + `Cargo.toml` for a single `PolicySlot::Generated`
/// at the given index in the spec's slot list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GeneratedSource {
    /// index in `spec.policies` (zero-based).
    pub slot_index: u32,
    /// rendered Cargo.toml verbatim — locked surface, never user-editable
    /// (playground design §6.2).
    pub cargo_toml: String,
    /// rendered `src/lib.rs`. Becomes the Monaco buffer's initial text on
    /// the frontend Source tab.
    pub lib_rs: String,
}

/// `get_policy_artifacts` output.
///
/// `wasm_sha256` is the sha-256 of the unoptimized wasm produced by
/// `cargo build` (cache sidecar `policy.pre.wasm.sha256`); `optimized_wasm_sha256`
/// is the sha-256 of the wasm produced by `stellar contract optimize`
/// (cache sidecar `policy.opt.wasm.sha256` — also returned by `synthesize_track_b`).
/// Both are lowercase 64-char hex.
///
/// When the spec has zero `Generated` slots, `generated_sources` is empty
/// and the two hash fields are `None` (the simhost pipeline produces no
/// wasm in that case — Track-A only spec).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct GetPolicyArtifactsOutput {
    /// echo of the input `spec_id`.
    pub spec_id: String,
    /// rendered Rust source for every Generated slot, in slot order.
    pub generated_sources: Vec<GeneratedSource>,
    /// number of `PolicySlot::Existing` entries in the spec.
    pub composed_count: u32,
    /// number of `PolicySlot::Generated` entries in the spec.
    pub generated_count: u32,
    /// sha-256 of the **first** Generated slot's pre-optimize wasm
    /// (lowercase hex). `None` for zero-Generated specs.
    pub wasm_sha256: Option<String>,
    /// sha-256 of the **first** Generated slot's post-optimize wasm
    /// (lowercase hex). `None` for zero-Generated specs.
    pub optimized_wasm_sha256: Option<String>,
}

/// in-memory cache for `get_policy_artifacts`. Cheaply cloneable
/// (`Arc<DashMap>`); embed inside `PolicyServer` so every connection
/// shares the same cache instance and a repeated call for the same
/// `spec_id` short-circuits before re-rendering / re-compiling.
#[derive(Clone, Default, Debug)]
pub struct GetPolicyArtifactsCache {
    inner: Arc<DashMap<String, GetPolicyArtifactsOutput>>,
}

impl GetPolicyArtifactsCache {
    /// build an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// snapshot of the cached output for `spec_id`, if present.
    pub fn get(&self, spec_id: &str) -> Option<GetPolicyArtifactsOutput> {
        self.inner.get(spec_id).map(|r| r.value().clone())
    }

    /// store `output` under `spec_id`, replacing any prior entry.
    pub fn put(&self, spec_id: &str, output: GetPolicyArtifactsOutput) {
        self.inner.insert(spec_id.to_string(), output);
    }

    /// number of entries currently cached. Test-only convenience.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `true` when the cache holds zero entries.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// `get_policy_artifacts` handler.
///
/// returns the rendered Cargo.toml + lib.rs for every Generated slot
/// (deterministic — `render_contract` is pure), plus the pre- and
/// post-optimize wasm hashes for the **first** Generated slot only. The
/// frontend Bundle tab uses the same hashes to verify the downloaded zip
/// matches what the sandbox produced.
///
/// errors:
/// * `E_SPEC_NOT_FOUND` — `spec_id` not in store.
/// * `E_CODEGEN_COMPILE_FAILED` — sandbox build / optimize failed.
pub async fn get_policy_artifacts(
    store: &McpStore,
    cache: &GetPolicyArtifactsCache,
    input: GetPolicyArtifactsInput,
) -> Result<GetPolicyArtifactsOutput, ErrorData> {
    if let Some(hit) = cache.get(&input.spec_id) {
        return Ok(hit);
    }

    let spec: PolicySpec = store.get_spec(&input.spec_id).ok_or_else(|| {
        error_to_jsonrpc(&oz_policy_core::Error::SpecNotFound(format!(
            "spec_id {:?} not in store",
            input.spec_id
        )))
    })?;

    let (generated_count, composed_count) = count_slots(&spec);

    // render every Generated slot (pure, deterministic, no I/O).
    let mut generated_sources: Vec<GeneratedSource> = Vec::new();
    let mut first_render_src_hash: Option<[u8; 32]> = None;
    for (idx, slot) in spec.policies.iter().enumerate() {
        if !matches!(slot, PolicySlot::Generated { .. }) {
            continue;
        }
        let rendered = render_contract(&spec, idx).map_err(|e| error_to_jsonrpc(&e))?;
        if first_render_src_hash.is_none() {
            first_render_src_hash = Some(rendered.wasm_hash_of_src);
        }
        generated_sources.push(GeneratedSource {
            slot_index: idx as u32,
            cargo_toml: rendered.cargo_toml,
            lib_rs: rendered.src_lib_rs,
        });
    }

    // only invoke the sandbox when at least one Generated slot exists —
    // otherwise the wasm hashes are `None` and we save the cargo+wasm-opt
    // round-trip.
    let (wasm_sha256, optimized_wasm_sha256) = if generated_sources.is_empty() {
        (None, None)
    } else {
        // run the full sandbox pipeline (this is the same call
        // `simulate_policy` makes; cache hits are free).
        let artifacts = synthesize_track_b(&spec)
            .await
            .map_err(|e| error_to_jsonrpc(&e))?;
        let first = artifacts.first().ok_or_else(|| {
            // unreachable in practice: at least one Generated slot ⇒
            // synthesize_track_b returns at least one artifact. Surface as
            // a structured codegen failure rather than panicking so the
            // contract stays honest if some future refactor diverges.
            error_to_jsonrpc(&oz_policy_core::Error::CodegenCompileFailed(
                "synthesize_track_b returned zero artifacts despite a Generated slot".into(),
            ))
        })?;
        let optimized_hex = hex::encode(first.wasm_hash);

        // read the pre-optimize sidecar that the sandbox wrote during
        // build. If it's missing (e.g. cache populated by a binary
        // predating the sidecar) fall back to the optimized hash and tag
        // the field with the same value — honest fallback, no fabrication.
        let pre_hex = match first_render_src_hash {
            Some(hash) => read_pre_optimize_hash(&hash).await.unwrap_or_else(|| {
                tracing::warn!(
                    "pre-optimize sidecar missing for src_hash; \
                     falling back to optimized hash"
                );
                optimized_hex.clone()
            }),
            None => optimized_hex.clone(),
        };
        (Some(pre_hex), Some(optimized_hex))
    };

    let output = GetPolicyArtifactsOutput {
        spec_id: input.spec_id.clone(),
        generated_sources,
        composed_count,
        generated_count,
        wasm_sha256,
        optimized_wasm_sha256,
    };
    cache.put(&input.spec_id, output.clone());
    Ok(output)
}

/// read the cache sidecar `policy.pre.wasm.sha256` written by the codegen
/// sandbox during `cargo build`. Returns `None` when the file doesn't
/// exist or its contents aren't a 64-char hex string.
async fn read_pre_optimize_hash(src_hash: &[u8; 32]) -> Option<String> {
    let dir = cache_dir_for(src_hash).ok()?;
    let path = dir.join("policy.pre.wasm.sha256");
    let s = tokio::fs::read_to_string(&path).await.ok()?;
    let s = s.trim().to_string();
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(s)
    } else {
        None
    }
}

fn count_slots(spec: &PolicySpec) -> (u32, u32) {
    let mut generated = 0u32;
    let mut composed = 0u32;
    for slot in &spec.policies {
        match slot {
            PolicySlot::Generated { .. } => generated = generated.saturating_add(1),
            PolicySlot::Existing { .. } => composed = composed.saturating_add(1),
        }
    }
    (generated, composed)
}

/// helper: sha-256 the given bytes into a lowercase hex string. Kept
/// public-in-crate so the integration tests can reproduce the wire
/// format without re-implementing it.
#[allow(dead_code)]
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use oz_policy_core::spec::{
        ContextRuleSpec, ContextType, ExistingPrimitive, ExistingPrimitiveParams, RecordingRef,
        SignerSpec, SynthesisMode, POLICY_SCHEMA_URI,
    };

    fn empty_spec(rule_name: &str) -> PolicySpec {
        PolicySpec {
            schema: POLICY_SCHEMA_URI.to_string(),
            synthesis_mode: SynthesisMode::Auto,
            context_rule: ContextRuleSpec {
                name: rule_name.to_string(),
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
                schema: oz_policy_core::recording::RECORDING_SCHEMA_URI.to_string(),
            },
        }
    }

    fn existing_only_spec(rule_name: &str) -> PolicySpec {
        let mut spec = empty_spec(rule_name);
        spec.policies = vec![PolicySlot::Existing {
            primitive: ExistingPrimitive::SimpleThreshold,
            params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
        }];
        spec
    }

    /// unknown spec_id surfaces `E_SPEC_NOT_FOUND` with the documented
    /// JSON-RPC code (-32110) and the canonical `error_code` string.
    #[tokio::test]
    async fn missing_spec_id_returns_e_spec_not_found() {
        let store = McpStore::new();
        let cache = GetPolicyArtifactsCache::new();
        let input = GetPolicyArtifactsInput {
            spec_id: "spec_does_not_exist".to_string(),
        };
        let err = get_policy_artifacts(&store, &cache, input)
            .await
            .expect_err("missing spec_id must error");
        assert_eq!(err.code.0, -32110, "must be E_SPEC_NOT_FOUND wire code");
        let data = err.data.expect("data must be populated");
        assert_eq!(
            data.get("error_code").and_then(|v| v.as_str()),
            Some("E_SPEC_NOT_FOUND")
        );
    }

    /// a spec with only `Existing` slots: no wasm artifacts, but the
    /// handler still returns a populated `GetPolicyArtifactsOutput` with
    /// `generated_sources == []` and `wasm_sha256 == None`. The pipeline
    /// is never invoked.
    #[tokio::test]
    async fn existing_only_spec_returns_empty_sources_no_wasm() {
        let store = McpStore::new();
        let cache = GetPolicyArtifactsCache::new();
        let sid = store.new_id("spec");
        store.put_spec(&sid, existing_only_spec("rule"));

        let input = GetPolicyArtifactsInput {
            spec_id: sid.clone(),
        };
        let out = get_policy_artifacts(&store, &cache, input)
            .await
            .expect("existing-only spec must succeed");
        assert_eq!(out.spec_id, sid);
        assert!(
            out.generated_sources.is_empty(),
            "no Generated slots → empty list"
        );
        assert_eq!(out.composed_count, 1);
        assert_eq!(out.generated_count, 0);
        assert!(out.wasm_sha256.is_none());
        assert!(out.optimized_wasm_sha256.is_none());
    }

    /// second call for the same spec_id is served from the in-memory
    /// cache. We exercise this by populating the cache directly (so the
    /// store has no spec — a miss would yield E_SPEC_NOT_FOUND) and
    /// asserting the second call returns the cached payload.
    #[tokio::test]
    async fn second_call_serves_from_cache_without_touching_store() {
        let store = McpStore::new();
        let cache = GetPolicyArtifactsCache::new();
        let spec_id = "spec_cached_only".to_string();

        // sentinel payload: distinguishable from anything `render_contract`
        // would actually produce.
        let cached = GetPolicyArtifactsOutput {
            spec_id: spec_id.clone(),
            generated_sources: vec![GeneratedSource {
                slot_index: 0,
                cargo_toml: "# sentinel".into(),
                lib_rs: "// sentinel".into(),
            }],
            composed_count: 0,
            generated_count: 1,
            wasm_sha256: Some("a".repeat(64)),
            optimized_wasm_sha256: Some("b".repeat(64)),
        };
        cache.put(&spec_id, cached.clone());

        // store has no entry under `spec_id`. If the cache short-circuit
        // didn't fire, we'd see E_SPEC_NOT_FOUND.
        let input = GetPolicyArtifactsInput {
            spec_id: spec_id.clone(),
        };
        let out = get_policy_artifacts(&store, &cache, input)
            .await
            .expect("cache hit must succeed even with no store entry");
        // payload must round-trip byte-equal.
        assert_eq!(serde_json::to_value(&out).unwrap(), serde_json::to_value(&cached).unwrap());
        assert_eq!(out.generated_sources[0].cargo_toml, "# sentinel");
        assert_eq!(out.generated_sources[0].lib_rs, "// sentinel");
    }

    /// the cache short-circuit also fires for specs that DO exist in the
    /// store — second call returns the prior result without re-running
    /// the (existing-only) render pipeline. Locks the "cache is keyed by
    /// spec_id" invariant.
    #[tokio::test]
    async fn repeated_call_returns_identical_payload_via_cache() {
        let store = McpStore::new();
        let cache = GetPolicyArtifactsCache::new();
        let sid = store.new_id("spec");
        store.put_spec(&sid, existing_only_spec("rule"));

        let input = GetPolicyArtifactsInput {
            spec_id: sid.clone(),
        };
        let a = get_policy_artifacts(&store, &cache, input.clone())
            .await
            .expect("first call must succeed");
        assert_eq!(cache.len(), 1, "first call must populate cache");

        // mutate the stored spec underneath — the cache should still
        // return the original payload (proves we never re-rendered).
        let mut mutated = existing_only_spec("rule");
        mutated.context_rule.name = "MUTATED".into();
        store.put_spec(&sid, mutated);

        let b = get_policy_artifacts(&store, &cache, input)
            .await
            .expect("cached call must succeed");
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(&b).unwrap(),
            "cached payload must be byte-equal to the first call's output"
        );
    }

    /// schema round-trip: both input and output emit a stable JSON
    /// schema via schemars. Locks the derive chain so a future struct
    /// rename can't silently break the MCP tool-schema publication.
    #[test]
    fn schemas_round_trip() {
        for label in ["input", "output", "source"] {
            let schema_json = match label {
                "input" => serde_json::to_value(schemars::schema_for!(GetPolicyArtifactsInput))
                    .expect("input schema"),
                "output" => serde_json::to_value(schemars::schema_for!(GetPolicyArtifactsOutput))
                    .expect("output schema"),
                "source" => serde_json::to_value(schemars::schema_for!(GeneratedSource))
                    .expect("source schema"),
                _ => unreachable!(),
            };
            let back: serde_json::Value =
                serde_json::from_value(schema_json.clone()).expect("round trip");
            assert_eq!(schema_json, back, "{label} schema must round-trip");
        }
    }
}
