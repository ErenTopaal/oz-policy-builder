//! MCP `resources/list` + `resources/read` surface.
//!
//! phase 5 Stream B (this stream) exposes every artefact the tools produce
//! under one of five URI families, all backed by [`crate::McpStore`]:
//!
//! | URI                                       | MIME type           | Payload          |
//! |-------------------------------------------|---------------------|------------------|
//! | `recording://<id>`                        | `application/json`  | `Recording` JSON |
//! | `spec://<id>`                             | `application/json`  | `PolicySpec` JSON|
//! | `artifact://<id>/source.rs`               | `text/plain`        | Rust source      |
//! | `artifact://<id>/policy.wasm`             | `application/wasm`  | base64 bytes     |
//! | `artifact://<id>/install_envelope.xdr`    | `text/plain`        | base64 XDR text  |
//!
//! these URIs are also the strings handed back to clients in
//! `export_policy` tool output (Stream A surfaces them under
//! `resource_uri` keys).
//!
//! ## rmcp 1.7.0 binding
//!
//! stream C wires these methods into rmcp's `ServerHandler::list_resources` /
//! `ServerHandler::read_resource`. We deliberately stay one level below the
//! trait — returning `rmcp::model::{Resource, ReadResourceResult}` — so the
//! same surface is testable in isolation without spinning up a transport.

use rmcp::{
    model::{AnnotateAble, RawResource, ReadResourceResult, Resource, ResourceContents},
    ErrorData,
};

use crate::store::McpStore;

// URI scheme prefixes — single source of truth so the URI parser and the
// list emitter cannot drift.

const RECORDING_SCHEME: &str = "recording://";
const SPEC_SCHEME: &str = "spec://";
const ARTIFACT_SCHEME: &str = "artifact://";

/// sub-paths under `artifact://<id>/...`. These are part of the wire
/// contract — changing them is a breaking change for any MCP client that
/// stores resource links.
const ARTIFACT_SOURCE_SUFFIX: &str = "/source.rs";
const ARTIFACT_WASM_SUFFIX: &str = "/policy.wasm";
const ARTIFACT_ENVELOPE_SUFFIX: &str = "/install_envelope.xdr";

const MIME_JSON: &str = "application/json";
const MIME_RUST: &str = "text/plain";
const MIME_WASM: &str = "application/wasm";
const MIME_XDR_TEXT: &str = "text/plain";

/// resource façade over the in-memory store. Cheap to construct — just
/// wraps an owned [`McpStore`] handle (which is itself `Arc`-backed).
#[derive(Debug, Clone)]
pub struct Resources {
    store: McpStore,
}

impl Resources {
    /// constructs a Resources surface bound to `store`. Stream C wires
    /// the same `McpStore` into every other surface so the resources
    /// reflect tool output without any intermediate sync step.
    pub fn new(store: McpStore) -> Self {
        Self { store }
    }

    /// implements `resources/list`. Enumerates every entry currently in the
    /// store and emits one `Resource` per artefact (recordings + specs
    /// produce one each; artifact bundles produce up to three — only those
    /// fields that are `Some(_)` appear in the listing so clients don't
    /// see dead URIs).
    pub fn list_resources(&self) -> Vec<Resource> {
        let mut out: Vec<Resource> = Vec::new();

        for id in self.store.recording_ids() {
            out.push(
                RawResource::new(format!("{RECORDING_SCHEME}{id}"), id.clone())
                    .with_description(
                        "Stellar transaction Recording (JSON, oz-policy-builder/recording/v1).",
                    )
                    .with_mime_type(MIME_JSON)
                    .no_annotation(),
            );
        }

        for id in self.store.spec_ids() {
            out.push(
                RawResource::new(format!("{SPEC_SCHEME}{id}"), id.clone())
                    .with_description("PolicySpec IR (JSON, oz-policy-builder/v1).")
                    .with_mime_type(MIME_JSON)
                    .no_annotation(),
            );
        }

        for id in self.store.artifact_ids() {
            // only enumerate URIs that actually have content — listing a
            // URI that read_resource would 404 on is one of the plan's
            // hard Constraints ("Every URI listed comes from a real
            // entry in the store").
            let Some(bundle) = self.store.get_artifact(&id) else {
                continue;
            };
            if bundle.source.is_some() {
                out.push(
                    RawResource::new(
                        format!("{ARTIFACT_SCHEME}{id}{ARTIFACT_SOURCE_SUFFIX}"),
                        format!("{id}/source.rs"),
                    )
                    .with_description("Track-B generated Rust source for the policy contract.")
                    .with_mime_type(MIME_RUST)
                    .no_annotation(),
                );
            }
            if bundle.wasm.is_some() {
                out.push(
                    RawResource::new(
                        format!("{ARTIFACT_SCHEME}{id}{ARTIFACT_WASM_SUFFIX}"),
                        format!("{id}/policy.wasm"),
                    )
                    .with_description("Compiled policy WASM bytes (base64-encoded in blob field).")
                    .with_mime_type(MIME_WASM)
                    .no_annotation(),
                );
            }
            if bundle.install_envelope_xdr.is_some() {
                out.push(
                    RawResource::new(
                        format!("{ARTIFACT_SCHEME}{id}{ARTIFACT_ENVELOPE_SUFFIX}"),
                        format!("{id}/install_envelope.xdr"),
                    )
                    .with_description("Wallet-signable install envelope (base64 XDR).")
                    .with_mime_type(MIME_XDR_TEXT)
                    .no_annotation(),
                );
            }
        }

        out
    }

    /// implements `resources/read`. Parses `uri`, looks up the entry, and
    /// renders it as the appropriate [`ResourceContents`] variant. URIs
    /// that don't parse OR refer to a missing entry produce
    /// [`ErrorCode::RESOURCE_NOT_FOUND`].
    ///
    /// [`ErrorCode::RESOURCE_NOT_FOUND`]: rmcp::model::ErrorCode::RESOURCE_NOT_FOUND
    pub fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, ErrorData> {
        let parsed = parse_uri(uri).ok_or_else(|| not_found(uri))?;
        let contents = match parsed {
            ResourceUri::Recording(id) => {
                let rec = self
                    .store
                    .get_recording(&id)
                    .ok_or_else(|| not_found(uri))?;
                let json = serde_json::to_string(&rec).map_err(|e| internal_serialize(uri, e))?;
                ResourceContents::TextResourceContents {
                    uri: uri.to_string(),
                    mime_type: Some(MIME_JSON.to_string()),
                    text: json,
                    meta: None,
                }
            }
            ResourceUri::Spec(id) => {
                let spec = self.store.get_spec(&id).ok_or_else(|| not_found(uri))?;
                let json = serde_json::to_string(&spec).map_err(|e| internal_serialize(uri, e))?;
                ResourceContents::TextResourceContents {
                    uri: uri.to_string(),
                    mime_type: Some(MIME_JSON.to_string()),
                    text: json,
                    meta: None,
                }
            }
            ResourceUri::ArtifactSource(id) => {
                let bundle = self.store.get_artifact(&id).ok_or_else(|| not_found(uri))?;
                let source = bundle.source.ok_or_else(|| not_found(uri))?;
                ResourceContents::TextResourceContents {
                    uri: uri.to_string(),
                    mime_type: Some(MIME_RUST.to_string()),
                    text: source,
                    meta: None,
                }
            }
            ResourceUri::ArtifactWasm(id) => {
                let bundle = self.store.get_artifact(&id).ok_or_else(|| not_found(uri))?;
                let wasm = bundle.wasm.ok_or_else(|| not_found(uri))?;
                let blob = {
                    use base64::{engine::general_purpose::STANDARD, Engine};
                    STANDARD.encode(&wasm)
                };
                ResourceContents::BlobResourceContents {
                    uri: uri.to_string(),
                    mime_type: Some(MIME_WASM.to_string()),
                    blob,
                    meta: None,
                }
            }
            ResourceUri::ArtifactEnvelope(id) => {
                let bundle = self.store.get_artifact(&id).ok_or_else(|| not_found(uri))?;
                let xdr = bundle.install_envelope_xdr.ok_or_else(|| not_found(uri))?;
                ResourceContents::TextResourceContents {
                    uri: uri.to_string(),
                    mime_type: Some(MIME_XDR_TEXT.to_string()),
                    text: xdr,
                    meta: None,
                }
            }
        };
        Ok(ReadResourceResult::new(vec![contents]))
    }
}

// URI parsing

/// parsed shape of an MCP resource URI handled by this surface.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResourceUri {
    Recording(String),
    Spec(String),
    ArtifactSource(String),
    ArtifactWasm(String),
    ArtifactEnvelope(String),
}

/// best-effort URI parser. Returns `None` for any URI that doesn't match
/// one of the five families — the caller maps that to RESOURCE_NOT_FOUND.
fn parse_uri(uri: &str) -> Option<ResourceUri> {
    if let Some(rest) = uri.strip_prefix(RECORDING_SCHEME) {
        if rest.is_empty() {
            return None;
        }
        return Some(ResourceUri::Recording(rest.to_string()));
    }
    if let Some(rest) = uri.strip_prefix(SPEC_SCHEME) {
        if rest.is_empty() {
            return None;
        }
        return Some(ResourceUri::Spec(rest.to_string()));
    }
    if let Some(rest) = uri.strip_prefix(ARTIFACT_SCHEME) {
        // `artifact://<id>/<suffix>` — `<id>` may not contain `/`.
        if let Some(id) = rest.strip_suffix(ARTIFACT_SOURCE_SUFFIX) {
            if !id.is_empty() && !id.contains('/') {
                return Some(ResourceUri::ArtifactSource(id.to_string()));
            }
        }
        if let Some(id) = rest.strip_suffix(ARTIFACT_WASM_SUFFIX) {
            if !id.is_empty() && !id.contains('/') {
                return Some(ResourceUri::ArtifactWasm(id.to_string()));
            }
        }
        if let Some(id) = rest.strip_suffix(ARTIFACT_ENVELOPE_SUFFIX) {
            if !id.is_empty() && !id.contains('/') {
                return Some(ResourceUri::ArtifactEnvelope(id.to_string()));
            }
        }
    }
    None
}

// error helpers

/// builds the `RESOURCE_NOT_FOUND` (-32002) error envelope per the MCP
/// spec. Stream A's `error_mapping::resource_not_found()` is expected to
/// wrap this exact construction; we inline it here so this stream's tests
/// run without depending on Stream A's module landing first.
fn not_found(uri: &str) -> ErrorData {
    ErrorData::resource_not_found(
        format!("unknown resource URI: {uri}"),
        Some(serde_json::json!({ "uri": uri })),
    )
}

fn internal_serialize(uri: &str, e: serde_json::Error) -> ErrorData {
    ErrorData::internal_error(
        format!("failed to serialise resource {uri}: {e}"),
        Some(serde_json::json!({ "uri": uri })),
    )
}

// tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ArtifactBundle, McpStore};
    use oz_policy_core::recording::{AuthTree, IngestSource, Recording, RECORDING_SCHEMA_URI};
    use oz_policy_core::spec::{
        ContextRuleSpec, ContextType, PolicySpec, RecordingRef, SynthesisMode, POLICY_SCHEMA_URI,
    };
    use rmcp::model::ErrorCode;

    fn sample_recording() -> Recording {
        Recording {
            schema: RECORDING_SCHEMA_URI.to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            ingest: IngestSource::Hash {
                hash: "abc123".to_string(),
            },
            ledger: Some(7),
            contracts: vec![],
            auth_tree: AuthTree { roots: vec![] },
            state_changes: vec![],
            events: vec![],
        }
    }

    fn sample_spec() -> PolicySpec {
        PolicySpec {
            schema: POLICY_SCHEMA_URI.to_string(),
            synthesis_mode: SynthesisMode::Auto,
            context_rule: ContextRuleSpec {
                name: "ctx".to_string(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: vec![],
            policies: vec![],
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: Some("abc123".to_string()),
                schema: RECORDING_SCHEMA_URI.to_string(),
            },
        }
    }

    /// list_resources reflects every put_recording.
    #[test]
    fn list_includes_recording_uris() {
        let store = McpStore::new();
        let res = Resources::new(store.clone());
        let id = "rec_test_1";
        store.put_recording(id, sample_recording());
        let list = res.list_resources();
        let expected_uri = format!("recording://{id}");
        assert!(
            list.iter().any(|r| r.uri == expected_uri),
            "expected {expected_uri} in {list:?}"
        );
    }

    /// read_resource(recording://...) returns JSON that round-trips back to
    /// the same Recording.
    #[test]
    fn read_recording_round_trips_json() {
        let store = McpStore::new();
        let res = Resources::new(store.clone());
        let id = "rec_roundtrip";
        store.put_recording(id, sample_recording());
        let result = res
            .read_resource(&format!("recording://{id}"))
            .expect("read must succeed");
        assert_eq!(result.contents.len(), 1);
        match &result.contents[0] {
            ResourceContents::TextResourceContents {
                text,
                mime_type,
                uri,
                ..
            } => {
                assert_eq!(mime_type.as_deref(), Some(MIME_JSON));
                assert_eq!(uri, &format!("recording://{id}"));
                let back: Recording =
                    serde_json::from_str(text).expect("recording JSON must parse");
                assert_eq!(back, sample_recording());
            }
            other => panic!("expected TextResourceContents, got {other:?}"),
        }
    }

    /// read_resource(spec://...) returns JSON that round-trips back to the
    /// same PolicySpec.
    #[test]
    fn read_spec_round_trips_json() {
        let store = McpStore::new();
        let res = Resources::new(store.clone());
        let id = "spec_42";
        store.put_spec(id, sample_spec());
        let result = res
            .read_resource(&format!("spec://{id}"))
            .expect("read must succeed");
        match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                let back: PolicySpec = serde_json::from_str(text).expect("spec JSON must parse");
                assert_eq!(back, sample_spec());
            }
            other => panic!("expected text contents, got {other:?}"),
        }
    }

    /// read_resource(artifact://.../policy.wasm) returns base64 blob whose
    /// decoded bytes equal the original WASM bytes byte-for-byte.
    #[test]
    fn read_artifact_wasm_blob_round_trips_bytes() {
        let store = McpStore::new();
        let res = Resources::new(store.clone());
        let id = "art_1";
        let raw: Vec<u8> = (0u8..=255).collect();
        store.put_artifact(
            id,
            ArtifactBundle {
                source: None,
                wasm: Some(raw.clone()),
                install_envelope_xdr: None,
            },
        );
        let result = res
            .read_resource(&format!("artifact://{id}/policy.wasm"))
            .expect("read must succeed");
        match &result.contents[0] {
            ResourceContents::BlobResourceContents {
                blob, mime_type, ..
            } => {
                assert_eq!(mime_type.as_deref(), Some(MIME_WASM));
                use base64::{engine::general_purpose::STANDARD, Engine};
                let decoded = STANDARD.decode(blob.as_bytes()).expect("blob must decode");
                assert_eq!(decoded, raw, "wasm bytes must round-trip");
            }
            other => panic!("expected blob contents, got {other:?}"),
        }
    }

    /// source-only artifact bundle exposes the source URI but not the WASM
    /// or envelope URIs in `list_resources`.
    #[test]
    fn list_skips_empty_artifact_fields() {
        let store = McpStore::new();
        let res = Resources::new(store.clone());
        let id = "art_src_only";
        store.put_artifact(
            id,
            ArtifactBundle {
                source: Some("fn main() {}".to_string()),
                wasm: None,
                install_envelope_xdr: None,
            },
        );
        let list = res.list_resources();
        let uris: Vec<&str> = list.iter().map(|r| r.uri.as_str()).collect();
        let src_uri = format!("artifact://{id}/source.rs");
        let wasm_uri = format!("artifact://{id}/policy.wasm");
        let env_uri = format!("artifact://{id}/install_envelope.xdr");
        assert!(uris.contains(&src_uri.as_str()));
        assert!(!uris.contains(&wasm_uri.as_str()));
        assert!(!uris.contains(&env_uri.as_str()));
    }

    /// reading a source URI when the bundle exists but `source = None`
    /// reports RESOURCE_NOT_FOUND — the same as if the bundle didn't
    /// exist at all. This keeps the wire contract uniform.
    #[test]
    fn read_missing_artifact_field_is_not_found() {
        let store = McpStore::new();
        let res = Resources::new(store.clone());
        let id = "art_nowasm";
        store.put_artifact(
            id,
            ArtifactBundle {
                source: Some("// hi".to_string()),
                wasm: None,
                install_envelope_xdr: None,
            },
        );
        let err = res
            .read_resource(&format!("artifact://{id}/policy.wasm"))
            .expect_err("must be NOT_FOUND");
        assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
    }

    /// unknown URI → typed JSON-RPC RESOURCE_NOT_FOUND (-32002).
    #[test]
    fn read_unknown_uri_is_not_found_error() {
        let store = McpStore::new();
        let res = Resources::new(store);
        let err = res
            .read_resource("recording://does-not-exist")
            .expect_err("must be NOT_FOUND");
        assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);

        let err = res
            .read_resource("not-a-supported-scheme://foo")
            .expect_err("must be NOT_FOUND");
        assert_eq!(err.code, ErrorCode::RESOURCE_NOT_FOUND);
    }

    /// URI parser rejects suspicious shapes (nested slashes in `<id>`,
    /// empty IDs) instead of silently treating them as valid lookups.
    #[test]
    fn parser_rejects_malformed_artifact_uris() {
        assert_eq!(parse_uri("artifact:///policy.wasm"), None);
        assert_eq!(parse_uri("artifact://a/b/policy.wasm"), None);
        assert_eq!(parse_uri("recording://"), None);
        assert_eq!(parse_uri("spec://"), None);
        assert_eq!(parse_uri("https://example.com/recording"), None);
    }
}
