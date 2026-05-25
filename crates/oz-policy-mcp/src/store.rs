//! In-memory store backing the MCP server's tools, resources, and prompts.
//!
//! Phase 5 Stream B (P5-Stream-B) lays this down as the single concurrent
//! source of truth shared across all three streams:
//!
//! * Stream A (tool handlers) puts entries via [`McpStore::put_recording`]
//!   etc. when a tool produces a new artefact.
//! * Stream B (this stream's [`crate::resources`] module) reads entries when
//!   serving `resources/list` and `resources/read`.
//! * Stream C (transport) constructs the [`McpStore`] inside the MCP server
//!   service and clones it into the per-connection handler.
//!
//! All values are `Clone` so handler routes can take an owned snapshot
//! without holding the dashmap shard lock across the await point.
//!
//! ## Disk persistence (best-effort)
//!
//! If the env var `OZ_POLICY_MCP_DATA_DIR` is set, OR
//! `$XDG_DATA_HOME/oz-policy-mcp` exists on disk, every `put_*` call is
//! mirrored to disk and every `get_*` miss tries to load from disk before
//! returning `None`. Disk I/O is best-effort: any error becomes a
//! `tracing::warn!` and the operation continues. This is exactly the
//! contract documented in plan.md Phase 5 Implementation → Resources
//! ("backing store: an in-memory `dashmap` keyed by ID, with an optional
//! disk-backing under `${OZ_POLICY_MCP_DATA_DIR:-$XDG_DATA_HOME/oz-policy-mcp}`
//! for persistence across STDIO sessions").

use std::path::PathBuf;
use std::sync::Arc;

use dashmap::DashMap;
use oz_policy_core::{recording::Recording, spec::PolicySpec};
use serde::{Deserialize, Serialize};

/// Concurrent in-memory store shared across the MCP server's tool, resource,
/// and prompt handlers. Cheap to `Clone` — wraps an `Arc<StoreInner>`.
#[derive(Clone, Default, Debug)]
pub struct McpStore {
    inner: Arc<StoreInner>,
}

#[derive(Default, Debug)]
struct StoreInner {
    recordings: DashMap<String, Recording>,
    specs: DashMap<String, PolicySpec>,
    artifacts: DashMap<String, ArtifactBundle>,
}

/// The triple of artefacts an `export_policy` tool call hands back: the
/// generated Rust source (when Track B ran), the compiled WASM bytes, and
/// the install envelope XDR. All three are optional because the export tool
/// can be called with `format: "wasm"` (etc.) and only some fields will be
/// populated.
///
/// `wasm` is serialised as a base64 JSON string via [`base64_bytes`] so the
/// whole bundle round-trips through any JSON-only transport (e.g., the disk
/// persistence below) without `Vec<u8>`'s JSON-array-of-numbers default.
#[derive(Clone, Default, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArtifactBundle {
    /// Track-B generated Rust source. `None` for Track-A-only specs.
    pub source: Option<String>,
    /// Compiled policy WASM bytes. `None` until codegen + sandbox build run.
    ///
    /// `serde(with = ...)` routes ser/de through the in-file `base64_bytes`
    /// adapter; `schemars(with = "Option<String>")` overrides the JSON
    /// Schema lookup so schemars does NOT try to resolve a `JsonSchema`
    /// impl on the `base64_bytes` module path (it has none). Without the
    /// schemars override, `#[derive(JsonSchema)]` errors with
    /// `expected type, found module 'crate::store::base64_bytes'` —
    /// schemars 1.x reads the same `with = "..."` attribute serde does and
    /// treats it as a type substitution. Adding the dedicated
    /// `schemars(...)` attribute is the documented schemars workaround.
    #[serde(with = "crate::store::base64_bytes", default)]
    #[schemars(with = "Option<String>")]
    pub wasm: Option<Vec<u8>>,
    /// Base64-encoded install envelope XDR. `None` until the installer runs.
    pub install_envelope_xdr: Option<String>,
}

/// Discriminator for the three persisted artefact kinds. Used by
/// [`McpStore::try_persist`] / [`McpStore::try_load`] to route to the
/// correct on-disk directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorePersistKind {
    Recording,
    Spec,
    Artifact,
}

impl StorePersistKind {
    /// Subdirectory name under the data dir. Lowercase, single word — these
    /// strings become path segments so they must stay portable across OSes.
    fn subdir(self) -> &'static str {
        match self {
            StorePersistKind::Recording => "recordings",
            StorePersistKind::Spec => "specs",
            StorePersistKind::Artifact => "artifacts",
        }
    }
}

impl McpStore {
    /// Constructs an empty store. Equivalent to [`McpStore::default`] but
    /// kept as a free function because plan.md spells it out explicitly.
    pub fn new() -> Self {
        Self::default()
    }

    /// Generates a fresh `<prefix>_<uuid v4>` ID for a new entry. The prefix
    /// is *not* stripped on lookup — callers must reuse the full string they
    /// receive here when fetching the entry back. Using a v4 UUID gives us
    /// 122 bits of entropy, which is more than enough to make collisions
    /// unobservable across the lifetime of a single MCP server process.
    pub fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}_{}", uuid::Uuid::new_v4())
    }

    // ------------------------------------------------------------------
    // Recording
    // ------------------------------------------------------------------

    /// Inserts a [`Recording`] under `id`, replacing any prior entry.
    /// Best-effort disk persistence runs after the in-memory insert.
    pub fn put_recording(&self, id: &str, rec: Recording) {
        self.inner.recordings.insert(id.to_string(), rec);
        self.try_persist(id, StorePersistKind::Recording);
    }

    /// Returns a clone of the [`Recording`] stored under `id`, or `None`.
    /// On miss attempts a best-effort disk load and re-populates the cache.
    pub fn get_recording(&self, id: &str) -> Option<Recording> {
        if let Some(v) = self.inner.recordings.get(id) {
            return Some(v.clone());
        }
        if self.try_load(id, StorePersistKind::Recording).is_some() {
            return self.inner.recordings.get(id).map(|v| v.clone());
        }
        None
    }

    // ------------------------------------------------------------------
    // PolicySpec
    // ------------------------------------------------------------------

    /// Inserts a [`PolicySpec`] under `id`. See [`McpStore::put_recording`]
    /// for disk-persistence semantics.
    pub fn put_spec(&self, id: &str, spec: PolicySpec) {
        self.inner.specs.insert(id.to_string(), spec);
        self.try_persist(id, StorePersistKind::Spec);
    }

    /// Returns a clone of the [`PolicySpec`] stored under `id`, or `None`.
    pub fn get_spec(&self, id: &str) -> Option<PolicySpec> {
        if let Some(v) = self.inner.specs.get(id) {
            return Some(v.clone());
        }
        if self.try_load(id, StorePersistKind::Spec).is_some() {
            return self.inner.specs.get(id).map(|v| v.clone());
        }
        None
    }

    // ------------------------------------------------------------------
    // Artifact bundle
    // ------------------------------------------------------------------

    /// Inserts an [`ArtifactBundle`] under `id`.
    pub fn put_artifact(&self, id: &str, bundle: ArtifactBundle) {
        self.inner.artifacts.insert(id.to_string(), bundle);
        self.try_persist(id, StorePersistKind::Artifact);
    }

    /// Returns a clone of the [`ArtifactBundle`] stored under `id`, or `None`.
    pub fn get_artifact(&self, id: &str) -> Option<ArtifactBundle> {
        if let Some(v) = self.inner.artifacts.get(id) {
            return Some(v.clone());
        }
        if self.try_load(id, StorePersistKind::Artifact).is_some() {
            return self.inner.artifacts.get(id).map(|v| v.clone());
        }
        None
    }

    // ------------------------------------------------------------------
    // Listing helpers (used by resources::list_resources)
    // ------------------------------------------------------------------

    /// Returns every recording ID currently in memory. (Disk-only IDs are
    /// *not* enumerated — `resources/list` is allowed to be eventually
    /// consistent per the MCP spec; clients always observe a strict subset
    /// of the persisted entries.)
    pub fn recording_ids(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .inner
            .recordings
            .iter()
            .map(|e| e.key().clone())
            .collect();
        v.sort(); // deterministic ordering for test stability
        v
    }

    /// Returns every spec ID currently in memory.
    pub fn spec_ids(&self) -> Vec<String> {
        let mut v: Vec<String> = self.inner.specs.iter().map(|e| e.key().clone()).collect();
        v.sort();
        v
    }

    /// Returns every artifact-bundle ID currently in memory.
    pub fn artifact_ids(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .inner
            .artifacts
            .iter()
            .map(|e| e.key().clone())
            .collect();
        v.sort();
        v
    }

    // ------------------------------------------------------------------
    // Best-effort disk persistence
    // ------------------------------------------------------------------

    /// Mirrors the in-memory entry under `id` to disk if a data dir is
    /// configured. Failures are logged at `warn` level and swallowed — the
    /// in-memory store remains authoritative.
    pub fn try_persist(&self, id: &str, kind: StorePersistKind) {
        let Some(dir) = resolve_data_dir() else {
            return;
        };
        let kind_dir = dir.join(kind.subdir());
        if let Err(e) = std::fs::create_dir_all(&kind_dir) {
            tracing::warn!(?kind_dir, error=?e, "oz-policy-mcp: failed to create persistence directory");
            return;
        }
        let path = kind_dir.join(format!("{}.json", sanitize_id(id)));
        let json = match kind {
            StorePersistKind::Recording => self
                .inner
                .recordings
                .get(id)
                .and_then(|v| serde_json::to_vec_pretty(&*v).ok()),
            StorePersistKind::Spec => self
                .inner
                .specs
                .get(id)
                .and_then(|v| serde_json::to_vec_pretty(&*v).ok()),
            StorePersistKind::Artifact => self
                .inner
                .artifacts
                .get(id)
                .and_then(|v| serde_json::to_vec_pretty(&*v).ok()),
        };
        let Some(bytes) = json else {
            // The entry vanished between insert and persist (extremely
            // unlikely) — nothing to do.
            return;
        };
        if let Err(e) = atomic_write(&path, &bytes) {
            tracing::warn!(?path, error=?e, "oz-policy-mcp: failed to persist entry to disk");
        }
    }

    /// Tries to load the entry under `id` from disk if a data dir is
    /// configured and returns `Some(())` if it succeeded (the value is
    /// re-inserted into the in-memory map for subsequent reads). Returns
    /// `None` on any error or absence.
    pub fn try_load(&self, id: &str, kind: StorePersistKind) -> Option<()> {
        let dir = resolve_data_dir()?;
        let path = dir
            .join(kind.subdir())
            .join(format!("{}.json", sanitize_id(id)));
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::warn!(?path, error=?e, "oz-policy-mcp: failed to read persisted entry");
                return None;
            }
        };
        match kind {
            StorePersistKind::Recording => match serde_json::from_slice::<Recording>(&bytes) {
                Ok(v) => {
                    self.inner.recordings.insert(id.to_string(), v);
                    Some(())
                }
                Err(e) => {
                    tracing::warn!(?path, error=?e, "oz-policy-mcp: failed to deserialize persisted Recording");
                    None
                }
            },
            StorePersistKind::Spec => match serde_json::from_slice::<PolicySpec>(&bytes) {
                Ok(v) => {
                    self.inner.specs.insert(id.to_string(), v);
                    Some(())
                }
                Err(e) => {
                    tracing::warn!(?path, error=?e, "oz-policy-mcp: failed to deserialize persisted PolicySpec");
                    None
                }
            },
            StorePersistKind::Artifact => match serde_json::from_slice::<ArtifactBundle>(&bytes) {
                Ok(v) => {
                    self.inner.artifacts.insert(id.to_string(), v);
                    Some(())
                }
                Err(e) => {
                    tracing::warn!(?path, error=?e, "oz-policy-mcp: failed to deserialize persisted ArtifactBundle");
                    None
                }
            },
        }
    }
}

// ----------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------

/// Resolves the on-disk data directory per the plan.md convention:
/// 1. `OZ_POLICY_MCP_DATA_DIR` env var, if set (used unconditionally — the
///    caller is asserting they want persistence here).
/// 2. Otherwise, `$XDG_DATA_HOME/oz-policy-mcp` *if that path already
///    exists* (we don't auto-create it; the user must opt in by creating
///    the directory).
///
/// Returns `None` when neither condition is satisfied — i.e. persistence
/// is disabled and the store is purely in-memory.
fn resolve_data_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("OZ_POLICY_MCP_DATA_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        let candidate = PathBuf::from(xdg).join("oz-policy-mcp");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Atomic write via tempfile-in-same-dir + rename. The same-directory rename
/// guarantee is what makes the rename atomic on POSIX (cross-mount renames
/// fall back to copy+unlink which is not atomic).
fn atomic_write(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "persistence path has no parent directory",
        )
    })?;
    // Manually construct the temp filename (we don't pull in the `tempfile`
    // crate as a non-dev dep just for this — the persistence path is
    // best-effort and a process-unique nonce is sufficient).
    let nonce = uuid::Uuid::new_v4();
    let tmp = dir.join(format!(
        ".{}.{nonce}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("entry")
    ));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    // `rename` is atomic on same filesystem on POSIX, and Windows since
    // Vista honours `MOVEFILE_REPLACE_EXISTING` semantics.
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Best-effort cleanup so we don't leak the tempfile on failure.
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Strips path separators and other troublesome characters so a caller can't
/// inject `../` traversal via the ID. IDs we generate ourselves are
/// `<prefix>_<uuid>` and are already safe, but `put_*` is `pub` so an
/// upstream caller could in principle pass an arbitrary string.
fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '_',
        })
        .collect()
}

// ----------------------------------------------------------------------
// Serde adapter: serialise `Option<Vec<u8>>` as base64 JSON strings
// ----------------------------------------------------------------------

/// Serialises an `Option<Vec<u8>>` as either `null` or a base64-encoded JSON
/// string. The default serde behaviour for `Vec<u8>` is a JSON array of
/// numbers, which is unreadable, bandwidth-heavy, and incompatible with the
/// MCP `BlobResourceContents.blob` field (which is required to be base64).
///
/// Pairs with [`base64_bytes::deserialize`] for round-trip byte-equality
/// (verified by the unit test below).
pub mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serializer half of the adapter. `None` becomes JSON `null`; `Some(v)`
    /// becomes a base64-encoded string.
    pub fn serialize<S>(value: &Option<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            None => serializer.serialize_none(),
            Some(bytes) => serializer.serialize_str(&STANDARD.encode(bytes)),
        }
    }

    /// Deserializer half of the adapter. Accepts JSON `null` (→ `None`) or a
    /// base64-encoded string (→ `Some`). Any non-base64 string produces a
    /// serde error so corrupt persisted bundles surface loudly.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = <Option<String>>::deserialize(deserializer)?;
        match opt {
            None => Ok(None),
            Some(s) => STANDARD
                .decode(s.as_bytes())
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }
}

// ----------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------

#[cfg(test)]
// Test scaffolding mutates process-wide env vars via `EnvGuard`, which
// requires the Rust 2024 `unsafe { std::env::set_var(...) }` block. The
// hazard (concurrent env-var races) is documented inline on each call
// site; nextest isolates tests per-binary so the racy window cannot be
// observed in practice. The crate-level `#![deny(unsafe_code)]` (lib.rs)
// is relaxed here ONLY for the test module — production paths in this
// file remain unsafe-free.
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use oz_policy_core::recording::{AuthTree, IngestSource, Recording, RECORDING_SCHEMA_URI};

    fn sample_recording() -> Recording {
        Recording {
            schema: RECORDING_SCHEMA_URI.to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            ingest: IngestSource::Hash {
                hash: "deadbeef".to_string(),
            },
            ledger: Some(42),
            contracts: vec![],
            auth_tree: AuthTree { roots: vec![] },
            state_changes: vec![],
            events: vec![],
        }
    }

    fn sample_spec() -> PolicySpec {
        use oz_policy_core::spec::{
            ContextRuleSpec, ContextType, PolicySpec, RecordingRef, SynthesisMode,
            POLICY_SCHEMA_URI,
        };
        PolicySpec {
            schema: POLICY_SCHEMA_URI.to_string(),
            synthesis_mode: SynthesisMode::Auto,
            context_rule: ContextRuleSpec {
                name: "test".to_string(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: vec![],
            policies: vec![],
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: Some("deadbeef".to_string()),
                schema: oz_policy_core::recording::RECORDING_SCHEMA_URI.to_string(),
            },
        }
    }

    /// Smoke-test the in-memory put/get round trip — the simplest contract
    /// the store has to satisfy.
    #[test]
    fn put_get_recording_roundtrip() {
        let s = McpStore::new();
        let id = s.new_id("rec");
        assert!(id.starts_with("rec_"));
        s.put_recording(&id, sample_recording());
        let got = s.get_recording(&id).expect("recording must be present");
        assert_eq!(got.network_passphrase, "Test SDF Network ; September 2015");
    }

    /// Get-miss returns `None` cleanly when no persistence is configured.
    #[test]
    fn get_miss_returns_none() {
        // Use a *guaranteed* miss key (UUID) and disable persistence by
        // unsetting both env vars for this test.
        // Safety: the env-var mutation is scoped to this single-threaded
        // test; we restore via the `RAII` pattern using a guard struct.
        let _g = EnvGuard::clear(&["OZ_POLICY_MCP_DATA_DIR", "XDG_DATA_HOME"]);
        let s = McpStore::new();
        assert!(s.get_recording("nonexistent").is_none());
        assert!(s.get_spec("nonexistent").is_none());
        assert!(s.get_artifact("nonexistent").is_none());
    }

    /// IDs returned by `recording_ids` / `spec_ids` / `artifact_ids` must be
    /// deterministically ordered. Listing depends on this for the resources
    /// surface to produce byte-equal output across runs.
    #[test]
    fn ids_are_sorted() {
        let s = McpStore::new();
        let _g = EnvGuard::clear(&["OZ_POLICY_MCP_DATA_DIR", "XDG_DATA_HOME"]);
        s.put_recording("rec_zzz", sample_recording());
        s.put_recording("rec_aaa", sample_recording());
        s.put_recording("rec_mmm", sample_recording());
        assert_eq!(
            s.recording_ids(),
            vec![
                "rec_aaa".to_string(),
                "rec_mmm".to_string(),
                "rec_zzz".to_string()
            ]
        );
    }

    /// PolicySpec round-trips through the store too.
    #[test]
    fn put_get_spec_roundtrip() {
        let _g = EnvGuard::clear(&["OZ_POLICY_MCP_DATA_DIR", "XDG_DATA_HOME"]);
        let s = McpStore::new();
        let id = s.new_id("spec");
        s.put_spec(&id, sample_spec());
        let got = s.get_spec(&id).expect("spec must be present");
        assert_eq!(got.context_rule.name, "test");
    }

    /// ArtifactBundle round-trip preserves WASM bytes byte-equal through the
    /// base64 serde adapter. This is the load-bearing assertion plan.md
    /// pins as a Hard Constraint ("verify byte-equality round-trips through
    /// JSON").
    #[test]
    fn artifact_wasm_bytes_round_trip_json() {
        let raw: Vec<u8> = (0u8..=255).collect();
        let bundle = ArtifactBundle {
            source: Some("// hello".to_string()),
            wasm: Some(raw.clone()),
            install_envelope_xdr: Some("AAAA".to_string()),
        };
        let json = serde_json::to_string(&bundle).expect("serialize bundle");
        // The WASM field must be a JSON string (base64), not an array of numbers.
        assert!(
            json.contains("\"wasm\":\""),
            "expected base64 string, got: {json}"
        );
        let back: ArtifactBundle = serde_json::from_str(&json).expect("deserialize bundle");
        assert_eq!(
            back.wasm,
            Some(raw),
            "wasm bytes must round-trip byte-equal"
        );
        assert_eq!(back.source.as_deref(), Some("// hello"));
        assert_eq!(back.install_envelope_xdr.as_deref(), Some("AAAA"));
    }

    /// `Option<Vec<u8>>::None` round-trips as JSON `null` through the adapter
    /// (not as an empty string), which is what `serde(skip_serializing_if = "Option::is_none")`
    /// callers rely on if we ever add that attribute downstream.
    #[test]
    fn artifact_wasm_none_round_trip() {
        let bundle = ArtifactBundle::default();
        let json = serde_json::to_string(&bundle).expect("serialize empty bundle");
        let back: ArtifactBundle = serde_json::from_str(&json).expect("deserialize empty bundle");
        assert!(back.wasm.is_none());
    }

    /// Disk-persistence happy path: setting `OZ_POLICY_MCP_DATA_DIR` causes
    /// puts to land on disk, and a *fresh* `McpStore` reading from the same
    /// directory recovers them via `get_*`.
    #[test]
    fn persistence_round_trip_via_env_var() {
        let dir = tempfile::tempdir().expect("create tempdir");
        let _g = EnvGuard::set("OZ_POLICY_MCP_DATA_DIR", dir.path().to_str().unwrap());

        let s1 = McpStore::new();
        let id = s1.new_id("rec");
        s1.put_recording(&id, sample_recording());

        // A brand-new store with no in-memory state must still find the entry
        // via the on-disk file.
        let s2 = McpStore::new();
        let got = s2
            .get_recording(&id)
            .expect("entry must be loadable from disk");
        assert_eq!(got.network_passphrase, "Test SDF Network ; September 2015");
    }

    /// Persistence directory creation failure does not panic. Pointing the
    /// data dir at a non-writable location (a path under a read-only file)
    /// should cause `put_*` to log a warning and continue.
    #[test]
    fn persistence_failure_is_silent_warn() {
        // Use a path that's structurally impossible to create as a directory
        // (root has a file in the way). The `/dev/null` device is a regular
        // file on macOS and Linux; mkdir under it fails with ENOTDIR.
        let _g = EnvGuard::set("OZ_POLICY_MCP_DATA_DIR", "/dev/null/oz-policy-mcp-test");
        let s = McpStore::new();
        let id = s.new_id("rec");
        // Must not panic.
        s.put_recording(&id, sample_recording());
        // In-memory entry is still available even though disk write failed.
        assert!(s.get_recording(&id).is_some());
    }

    /// `sanitize_id` strips path separators so a malicious ID can't escape
    /// the data dir. Regression guard against future call sites that don't
    /// vet user-provided IDs. `.` is intentionally preserved (legitimate
    /// IDs may contain it; e.g. the persisted filename gets a `.json`
    /// suffix appended later) — what matters is that no `/` (or platform
    /// path separator) survives, which is what prevents directory escape.
    #[test]
    fn sanitize_blocks_path_traversal() {
        let sanitised = sanitize_id("../../etc/passwd");
        assert_eq!(sanitised, ".._.._etc_passwd");
        // Crucial property: result contains no path separators, so
        // joining it onto a kind dir cannot escape the data dir.
        assert!(!sanitised.contains('/'));
        assert!(!sanitised.contains(std::path::MAIN_SEPARATOR));
        assert_eq!(sanitize_id("normal_id.123"), "normal_id.123");
    }

    // ------------------------------------------------------------------
    // Test-only env-var guard. Mutating process env from tests requires
    // a `cargo test --test-threads=1` discipline or RAII restoration —
    // we use RAII here so the rest of the suite can stay parallel.
    // ------------------------------------------------------------------

    /// All env-mutating tests grab this Mutex first so the underlying
    /// `set_var` / `remove_var` calls are serialised across nextest's
    /// per-binary thread pool. Rust 2024 edition makes those functions
    /// `unsafe` because cross-thread env mutation is a race; staying on
    /// 2021 lets us call them safely but we still need the lock to keep
    /// observed environment state deterministic across tests in this
    /// binary.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvGuard {
        prior: Vec<(String, Option<String>)>,
        // Held for the lifetime of the guard so concurrent env-mutating
        // tests block on each other instead of racing.
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl EnvGuard {
        fn clear(keys: &[&str]) -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let prior = keys
                .iter()
                .map(|k| (k.to_string(), std::env::var(k).ok()))
                .collect();
            for k in keys {
                std::env::remove_var(k);
            }
            Self { prior, _lock: lock }
        }
        fn set(key: &str, val: &str) -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let prior = vec![(key.to_string(), std::env::var(key).ok())];
            std::env::set_var(key, val);
            Self { prior, _lock: lock }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (k, v) in &self.prior {
                match v {
                    Some(val) => std::env::set_var(k, val),
                    None => std::env::remove_var(k),
                }
            }
        }
    }
}
