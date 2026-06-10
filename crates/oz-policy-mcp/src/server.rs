//! `ServerHandler` implementation for the OZ Accounts Policy Builder MCP
//! server.
//!
//! phase 5 Stream C (this stream) owns the rmcp `ServerHandler` impl that
//! advertises the server's capabilities during the `initialize` handshake
//! and dispatches incoming requests to the per-stream module functions:
//!
//! * `tools/list` + `tools/call` → Stream A's [`crate::tools`]
//! * `resources/list` + `resources/read` → Stream B's [`crate::resources::Resources`]
//! * `prompts/list` + `prompts/get` → Stream B's [`crate::prompts::Prompts`]
//!
//! stream A's tools are exposed as five top-level MCP tools:
//! `record_transaction`, `synthesize_policy`, `simulate_policy`,
//! `export_policy`, `verify_install`. Each accepts a JSON object that
//! deserialises into the matching `*Input` struct from `tools.rs`; each
//! returns a JSON object built from the matching `*Output` struct (or a
//! plain `SimReport` for `simulate_policy`). Schemas are emitted directly
//! by `schemars` on those structs — no hand-written schema declarations.

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::{
    handler::server::wrapper::Json,
    model::{
        CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
        Implementation, InitializeResult, ListPromptsResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ProtocolVersion, ReadResourceRequestParams, ReadResourceResult,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    ErrorData as McpError, RoleServer, ServerHandler,
};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};

use crate::store::McpStore;
use crate::{
    prompts::Prompts,
    resources::Resources,
    tools::{
        export_policy, record_transaction, simulate_custom_source, simulate_policy,
        synthesize_policy, verify_install, ExportPolicyInput, RecordTransactionInput,
        SimulateCustomSourceInput, SimulatePolicyInput, SynthesizePolicyInput, VerifyInstallInput,
    },
};

/// MCP server handler. Cheaply cloneable — clones share the same
/// `Arc<McpStore>` so every per-connection handler observes the same
/// in-memory cache.
///
/// each rmcp transport (`StreamableHttpService` for HTTP,
/// `ServiceExt::serve` for STDIO) constructs a fresh `PolicyServer` per
/// connection via the `service_factory` closure (HTTP) or once at startup
/// (STDIO). Sharing state across connections is intentional and is what
/// makes a `recording_id` returned from `tools/call record_transaction` on
/// one HTTP request resolvable from `resources/read recording://<id>` on
/// the next.
#[derive(Debug, Clone)]
pub struct PolicyServer {
    /// shared in-memory store backing every tool / resource / prompt call.
    /// `Arc<McpStore>` rather than `McpStore` directly so the `ServerHandler`
    /// future bodies (which run after `&self` borrows expire) can cheap-clone
    /// the handle without locking.
    pub store: Arc<McpStore>,
}

impl PolicyServer {
    /// construct a `PolicyServer` over a freshly-created `McpStore`.
    pub fn new() -> Self {
        Self {
            store: Arc::new(McpStore::new()),
        }
    }

    /// construct a `PolicyServer` over a caller-supplied (already-warmed)
    /// `McpStore`. Used by tests that want to pre-seed recordings / specs
    /// without going through the recorder / synthesizer first, and by the
    /// transport-wiring `main.rs` (which constructs one shared `McpStore`
    /// for the whole binary and hands `Arc::clone`s to each per-connection
    /// `PolicyServer` factory).
    pub fn with_store(store: Arc<McpStore>) -> Self {
        Self { store }
    }

    /// returns a `Resources` façade over `self.store`. Cheap — `Resources`
    /// just wraps an owned (Arc-backed) `McpStore` handle.
    fn resources(&self) -> Resources {
        Resources::new((*self.store).clone())
    }

    /// returns a `Prompts` registry (stateless — the three templates are
    /// baked into the module).
    fn prompts(&self) -> Prompts {
        Prompts::new()
    }
}

impl Default for PolicyServer {
    fn default() -> Self {
        Self::new()
    }
}

// tool descriptors

/// tool name constants. Pulled into `pub const` so the smoke tests can
/// reference them without re-stringifying.
pub const TOOL_RECORD_TRANSACTION: &str = "record_transaction";
pub const TOOL_SYNTHESIZE_POLICY: &str = "synthesize_policy";
pub const TOOL_SIMULATE_POLICY: &str = "simulate_policy";
pub const TOOL_EXPORT_POLICY: &str = "export_policy";
pub const TOOL_VERIFY_INSTALL: &str = "verify_install";
/// playground `/playground` re-simulate loop — see design §3.4.
pub const TOOL_SIMULATE_CUSTOM_SOURCE: &str = "simulate_custom_source";

/// the fixed surface — order matters for `tools/list` test determinism.
const TOOL_NAMES: &[&str] = &[
    TOOL_RECORD_TRANSACTION,
    TOOL_SYNTHESIZE_POLICY,
    TOOL_SIMULATE_POLICY,
    TOOL_EXPORT_POLICY,
    TOOL_VERIFY_INSTALL,
    TOOL_SIMULATE_CUSTOM_SOURCE,
];

/// build a `Tool` descriptor for the given input type. Pulls the JSON
/// schema directly from `schemars` on `I` — no hand-written schema drift.
fn tool_descriptor<I: JsonSchema>(name: &'static str, description: &'static str) -> Tool {
    let schema = schemars::schema_for!(I);
    let schema_json: serde_json::Value =
        serde_json::to_value(&schema).expect("schemars schema must serialise");
    let obj = match schema_json {
        serde_json::Value::Object(m) => m,
        other => {
            let mut m = serde_json::Map::new();
            m.insert("__raw__".to_string(), other);
            m
        }
    };
    Tool::new(name, Cow::Borrowed(description), Arc::new(obj))
}

fn build_tool_list() -> Vec<Tool> {
    vec![
        tool_descriptor::<RecordTransactionInput>(
            TOOL_RECORD_TRANSACTION,
            "Record a Stellar transaction (by hash or simulated envelope) and \
             return the deterministic `Recording` IR.",
        ),
        tool_descriptor::<SynthesizePolicyInput>(
            TOOL_SYNTHESIZE_POLICY,
            "Synthesize the minimum-rights `PolicySpec` (oz-policy-builder/v1) \
             that would permit the given Recording under the chosen `tightness` \
             and `mode`.",
        ),
        tool_descriptor::<SimulatePolicyInput>(
            TOOL_SIMULATE_POLICY,
            "Simulate the given spec against its source recording (permit) and \
             the synthesized + caller-supplied deny vectors. Returns a `SimReport`.",
        ),
        tool_descriptor::<ExportPolicyInput>(
            TOOL_EXPORT_POLICY,
            "Materialize the spec's compiled Track-B WASM, generated Rust source, \
             and/or signed install envelope (per `format`). Returns inline payloads \
             plus `resource://` URIs for the same artefacts.",
        ),
        tool_descriptor::<VerifyInstallInput>(
            TOOL_VERIFY_INSTALL,
            "Verify the on-chain context rule at `(smart_account, context_rule_id)` \
             matches the expected `PolicySpec`. Returns a drift report (empty when \
             matches=true).",
        ),
        tool_descriptor::<SimulateCustomSourceInput>(
            TOOL_SIMULATE_CUSTOM_SOURCE,
            "Playground re-simulate: rebuild the Track-B WASM from a user-edited \
             `lib.rs` (Cargo.toml stays the spec's rendered template) and replay \
             the recording + deny matrix against it. Returns `SimReport`. Pre-flight \
             rejects sources containing forbidden patterns before invoking cargo.",
        ),
    ]
}

/// tool name surface exposed for the smoke tests.
pub fn tool_names() -> &'static [&'static str] {
    TOOL_NAMES
}

// serverHandler impl

impl ServerHandler for PolicyServer {
    /// returns the server's `initialize` response. We advertise:
    ///
    /// * `tools` — five tools enumerated below.
    /// * `prompts` — three wizard templates from `crate::prompts`.
    /// * `resources` — recordings, specs, and artifact bundles from
    ///   `crate::resources`.
    /// * protocol version `2025-11-25`.
    /// * `server_info.name = "oz-policy-mcp"` so mcp clients display the
    ///   canonical name from the registered config.
    /// * `server_info.version = env!("CARGO_PKG_VERSION")` so a deployed
    ///   binary's reported version traces back to a single workspace tag.
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .build();
        // both `ServerInfo` (alias `InitializeResult`) and `Implementation`
        // are `#[non_exhaustive]`, so we construct them through their
        // builder helpers rather than literal struct expressions.
        let server_info = Implementation::new("oz-policy-mcp", env!("CARGO_PKG_VERSION"))
            .with_title("OZ Accounts Policy Builder")
            .with_description(
                "Records a Stellar transaction and synthesizes the minimum \
                 OpenZeppelin smart-account context rule + policies that \
                 would permit exactly that flow.",
            );
        InitializeResult::new(capabilities)
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_server_info(server_info)
            .with_instructions(
                "Use `record_transaction` to capture a Stellar/Soroban call (by \
                 hash or envelope_xdr), then `synthesize_policy` to derive the \
                 minimum-rights policy spec. Always run `simulate_policy` before \
                 exporting. Tools never deploy on-chain; `export_policy` returns \
                 an install envelope your wallet signs.",
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        // `ListToolsResult` is `#[non_exhaustive]` in 1.7.0, so we use
        // struct-update with `..Default::default()` to keep this resilient
        // against future field additions (e.g. SEP-1319 `_meta`).
        Ok(ListToolsResult {
            tools: build_tool_list(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let name = request.name.as_ref();
        let arguments = request.arguments.unwrap_or_default();
        let arguments_value = serde_json::Value::Object(arguments);
        match name {
            TOOL_RECORD_TRANSACTION => {
                let input: RecordTransactionInput = decode_input(arguments_value, name)?;
                let output = record_transaction(&self.store, input).await?;
                Ok(structured_ok(&output))
            }
            TOOL_SYNTHESIZE_POLICY => {
                let input: SynthesizePolicyInput = decode_input(arguments_value, name)?;
                let output = synthesize_policy(&self.store, input).await?;
                Ok(structured_ok(&output))
            }
            TOOL_SIMULATE_POLICY => {
                let input: SimulatePolicyInput = decode_input(arguments_value, name)?;
                let output = simulate_policy(&self.store, input).await?;
                Ok(structured_ok(&output))
            }
            TOOL_EXPORT_POLICY => {
                let input: ExportPolicyInput = decode_input(arguments_value, name)?;
                let output = export_policy(&self.store, input).await?;
                Ok(structured_ok(&output))
            }
            TOOL_VERIFY_INSTALL => {
                let input: VerifyInstallInput = decode_input(arguments_value, name)?;
                let output = verify_install(&self.store, input).await?;
                Ok(structured_ok(&output))
            }
            unknown => Err(McpError::invalid_params(
                format!("unknown tool: {unknown}"),
                Some(serde_json::json!({ "name": unknown })),
            )),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: self.resources().list_resources(),
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        self.resources().read_resource(&request.uri)
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            prompts: self.prompts().list_prompts(),
            ..Default::default()
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        // rmcp's `GetPromptRequestParams::arguments` is `Option<JsonObject>`
        // (a `serde_json::Map<String, Value>`), whereas Stream B's
        // `Prompts::get_prompt` takes `Option<BTreeMap<String, String>>`
        // (per the MCP spec, prompt args are required to be strings).
        // convert by extracting each value's String representation, falling
        // back to the JSON-printed form for non-string values so clients
        // that send e.g. JSON numbers get something sensible.
        let arguments = request.arguments.map(|map| {
            map.into_iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    (k, s)
                })
                .collect::<std::collections::BTreeMap<String, String>>()
        });
        self.prompts().get_prompt(&request.name, arguments)
    }
}

// helpers

/// decode a tool's input JSON into the typed `Input` struct, mapping any
/// serde error to `McpError::invalid_params` with the tool name attached.
/// same shape every tool branch uses.
fn decode_input<I: DeserializeOwned>(
    arguments: serde_json::Value,
    tool_name: &str,
) -> Result<I, McpError> {
    serde_json::from_value(arguments).map_err(|e| {
        McpError::invalid_params(
            format!("{tool_name}: invalid arguments JSON: {e}"),
            Some(serde_json::json!({ "tool": tool_name, "error": e.to_string() })),
        )
    })
}

/// build a `CallToolResult` from a typed output. Uses
/// `CallToolResult::structured` so MCP clients that prefer structured
/// payloads (most do, since 2025-06-18) get the typed JSON directly, and
/// older clients see the same JSON as the unstructured `Content::text`
/// fallback (the `structured` constructor populates both).
///
/// the intermediate `Json` wrapper isn't strictly necessary here — we
/// could call `serde_json::to_value` directly — but going through `Json`
/// keeps the conversion in lockstep with the rmcp-recommended pattern
/// (see `rmcp::handler::server::wrapper::Json`).
fn structured_ok<T: Serialize>(value: &T) -> CallToolResult {
    let _ = Json::<()>; // touch the import so we keep it documented even if rmcp tightens the API later
    let json = serde_json::to_value(value)
        .expect("tool output Serialize impl produces valid JSON (every *Output is JsonSchema)");
    CallToolResult::structured(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_and_default_match() {
        let a = PolicyServer::new();
        let b = PolicyServer::default();
        // two stores; same emptiness invariant. Pointer-equal Arcs would be
        // a stronger check but pointless — `Arc::new` returns fresh handles.
        assert!(a.store.recording_ids().is_empty());
        assert!(b.store.recording_ids().is_empty());
    }

    #[test]
    fn with_store_shares_handle() {
        let store = Arc::new(McpStore::new());
        let s = PolicyServer::with_store(Arc::clone(&store));
        // `s.store` and `store` are the same Arc (same allocation).
        assert!(Arc::ptr_eq(&s.store, &store));
    }

    #[test]
    fn get_info_advertises_2025_11_25() {
        let s = PolicyServer::new();
        let info = s.get_info();
        assert_eq!(info.protocol_version, ProtocolVersion::V_2025_11_25);
    }

    #[test]
    fn get_info_advertises_three_capabilities() {
        let s = PolicyServer::new();
        let caps = s.get_info().capabilities;
        assert!(caps.tools.is_some(), "tools capability must be enabled");
        assert!(caps.prompts.is_some(), "prompts capability must be enabled");
        assert!(
            caps.resources.is_some(),
            "resources capability must be enabled"
        );
    }

    #[test]
    fn get_info_advertises_canonical_name_and_version() {
        let s = PolicyServer::new();
        let info = s.get_info();
        assert_eq!(info.server_info.name, "oz-policy-mcp");
        // the version *must* match the package version verbatim; that's how
        // operators correlate a running binary to a workspace tag.
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
        assert!(info.instructions.is_some());
    }

    #[test]
    fn server_is_clone_and_send() {
        // compile-time + tiny runtime check: PolicyServer must be Send +
        // clone so rmcp's StreamableHttpService factory closure can hand
        // out per-connection handles. Cloning must share the Arc (no deep
        // copy of the store).
        fn assert_send_clone<T: Send + Clone + 'static>() {}
        assert_send_clone::<PolicyServer>();
        let s = PolicyServer::new();
        let s2 = s.clone();
        assert!(Arc::ptr_eq(&s.store, &s2.store));
    }

    /// the fixed tool surface must always be exactly five tools, in the
    /// documented order. Regression guard against accidental additions /
    /// reorderings that would break clients caching the list.
    #[test]
    fn build_tool_list_returns_five_tools_in_canonical_order() {
        let tools = build_tool_list();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(
            names,
            vec![
                TOOL_RECORD_TRANSACTION,
                TOOL_SYNTHESIZE_POLICY,
                TOOL_SIMULATE_POLICY,
                TOOL_EXPORT_POLICY,
                TOOL_VERIFY_INSTALL,
            ],
            "tool order must stay stable across releases"
        );
    }

    /// every tool descriptor carries a non-empty description string — the
    /// JSON-Schema `input_schema` is also populated (the test would fail
    /// to compile if `tool_descriptor` returned anything else, but we keep
    /// the assertion explicit so future contributors don't accidentally
    /// pass `""` as the description for a new tool).
    #[test]
    fn build_tool_list_descriptors_have_descriptions_and_schemas() {
        for tool in build_tool_list() {
            assert!(
                tool.description.as_ref().is_some_and(|d| !d.is_empty()),
                "tool {} missing description",
                tool.name
            );
            assert!(
                !tool.input_schema.is_empty(),
                "tool {} missing input schema",
                tool.name
            );
        }
    }

    #[test]
    fn tool_names_exposed_constant_matches_builder() {
        let from_builder: Vec<_> = build_tool_list()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        let from_constant: Vec<_> = tool_names().iter().map(|s| s.to_string()).collect();
        assert_eq!(from_builder, from_constant);
    }
}
