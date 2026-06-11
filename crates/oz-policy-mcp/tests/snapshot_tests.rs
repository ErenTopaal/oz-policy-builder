//! Integration tests for the playground snapshot store
//! (`tools/snapshot.rs`).
//!
//! these run against real disk via `tempfile::tempdir()` + the
//! `OZ_POLICY_SNAPSHOT_DIR` env-var override. No mocks, no in-memory
//! shimming — the create/get/expire/concurrent-id properties are real
//! serde round-trips against an actual filesystem path. Per the spec's
//! "Mocks are forbidden across the board" rule (playground design §9).

use std::path::PathBuf;
use std::sync::Mutex;

use chrono::{Duration as ChronoDuration, Utc};
use oz_policy_core::arg_value::ArgValue;
use oz_policy_core::recording::{
    AuthEntry, AuthFunction, AuthInvocation, AuthTree, ContractRecord, Credentials,
    IngestSource as RecordingIngestSource, Recording, RECORDING_SCHEMA_URI,
};
use oz_policy_core::spec::{
    ContextRuleSpec, ContextType, ExistingPrimitive, ExistingPrimitiveParams, PolicySlot,
    PolicySpec, RecordingRef, SignerSpec, SynthesisMode, POLICY_SCHEMA_URI,
};
use oz_policy_mcp::tools::snapshot::{
    create_snapshot, get_snapshot, run_gc_once, snapshot_dir, CreateSnapshotInput,
    GetSnapshotInput, SnapshotRecord, SNAPSHOT_DIR_ENV,
};
use oz_policy_mcp::McpStore;
use oz_policy_simhost::run::{PermitResult, SimReport};
use tempfile::TempDir;
use tokio::task::JoinSet;

/// `OZ_POLICY_SNAPSHOT_DIR` is process-wide; all snapshot tests run
/// serially against a single tempdir scope-guard. Without the mutex,
/// `#[tokio::test]` would parallelise them and the second one would
/// either point at the first's tempdir (race) or overwrite the env var
/// mid-flight. Mutex is intentionally `std::sync::Mutex` (not
/// `tokio::sync`) because `set_var` is a sync syscall.
static ENV_GUARD: Mutex<()> = Mutex::new(());

/// scope guard that points `OZ_POLICY_SNAPSHOT_DIR` at a fresh tempdir
/// for the duration of the test. Holds the `TempDir` so it isn't
/// dropped until the test ends. Restores the previous env-var value on
/// drop.
struct DirScope {
    _dir: TempDir,
    prev: Option<String>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl DirScope {
    fn new() -> Self {
        let guard = ENV_GUARD.lock().expect("env guard mutex poisoned");
        let dir = tempfile::tempdir().expect("create tempdir");
        let prev = std::env::var(SNAPSHOT_DIR_ENV).ok();
        // Holding `ENV_GUARD` serialises every test touching this env
        // var, so no other test thread reads or writes it concurrently.
        // Production code (`main.rs`) sets the sibling
        // `OZ_POLICY_MCP_DATA_DIR` once at startup before tasks spawn;
        // we mirror that constraint here.
        std::env::set_var(SNAPSHOT_DIR_ENV, dir.path());
        DirScope {
            _dir: dir,
            prev,
            _guard: guard,
        }
    }

    fn path(&self) -> PathBuf {
        self._dir.path().to_path_buf()
    }
}

impl Drop for DirScope {
    fn drop(&mut self) {
        // restore the previous env. The mutex held in `_guard` is
        // still alive for the duration of Drop's body so this is safe.
        match &self.prev {
            Some(v) => std::env::set_var(SNAPSHOT_DIR_ENV, v),
            None => std::env::remove_var(SNAPSHOT_DIR_ENV),
        }
    }
}

fn sep41_recording() -> Recording {
    Recording {
        schema: RECORDING_SCHEMA_URI.to_string(),
        network_passphrase: "Test SDF Network ; September 2015".to_string(),
        ingest: RecordingIngestSource::Hash {
            hash: "deadbeef".to_string(),
        },
        ledger: Some(1234),
        contracts: vec![ContractRecord {
            address: "CUSDC".to_string(),
            function: "transfer".to_string(),
            args: vec![
                ArgValue::Address("GFROM".to_string()),
                ArgValue::Address("GTO".to_string()),
                ArgValue::I128("5000000".to_string()),
            ],
        }],
        auth_tree: AuthTree {
            roots: vec![AuthEntry {
                credentials: Credentials::Address {
                    signer: "GSIGNER".to_string(),
                    nonce: "1".to_string(),
                    signature_expiration_ledger: 0,
                    signature: ArgValue::Void,
                },
                root_invocation: AuthInvocation {
                    function: AuthFunction::Contract {
                        address: "CUSDC".to_string(),
                        function: "transfer".to_string(),
                        args: vec![],
                    },
                    sub_invocations: vec![],
                },
                source_op_index: 0,
            }],
        },
        state_changes: vec![],
        events: vec![],
    }
}

fn sample_spec(rule_name: &str) -> PolicySpec {
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
        policies: vec![PolicySlot::Existing {
            primitive: ExistingPrimitive::SimpleThreshold,
            params: ExistingPrimitiveParams::SimpleThreshold { threshold: 1 },
        }],
        lifetime_ledgers: None,
        recording_ref: RecordingRef {
            hash: None,
            schema: RECORDING_SCHEMA_URI.to_string(),
        },
    }
}

fn sample_report() -> SimReport {
    SimReport {
        spec_id: "smoke".to_string(),
        permit: PermitResult {
            passed: true,
            error: None,
        },
        deny_results: vec![],
        total_vectors: 0,
        passed: 0,
        timestamp_ledger: 1234,
    }
}

fn warm_store_with_fixture(store: &McpStore) -> (String, String) {
    let rid = store.new_id("rec");
    store.put_recording(&rid, sep41_recording());
    let sid = store.new_id("spec");
    store.put_spec(&sid, sample_spec("rule"));
    (rid, sid)
}

#[tokio::test]
async fn create_then_get_round_trips_content() {
    let _scope = DirScope::new();
    let store = McpStore::new();
    let (rid, sid) = warm_store_with_fixture(&store);

    let input = CreateSnapshotInput {
        recording_id: rid.clone(),
        spec_id: sid.clone(),
        modified_lib_rs: Some("// hello".to_string()),
        report: sample_report(),
    };
    let created = create_snapshot(&store, input)
        .await
        .expect("create_snapshot must succeed");

    assert_eq!(created.snapshot_id.len(), 8, "id must be 8 chars");
    // expires_at is roughly created_at + 30 days; allow a generous tolerance
    // because we don't know the exact created_at here — just bound it.
    let delta = created.expires_at - Utc::now();
    assert!(
        delta > ChronoDuration::days(29) && delta < ChronoDuration::days(31),
        "expires_at must be ~30 days from now, got delta={delta:?}"
    );

    let fetched = get_snapshot(GetSnapshotInput {
        snapshot_id: created.snapshot_id.clone(),
    })
    .await
    .expect("get_snapshot must succeed");

    assert_eq!(fetched.snapshot_id, created.snapshot_id);
    assert_eq!(fetched.modified_lib_rs.as_deref(), Some("// hello"));
    // byte-equal recording + spec round-trip.
    let recording_json_a = serde_json::to_string(&fetched.recording).expect("a");
    let recording_json_b = serde_json::to_string(&sep41_recording()).expect("b");
    assert_eq!(recording_json_a, recording_json_b);
    let spec_json_a = serde_json::to_string(&fetched.spec).expect("c");
    let spec_json_b = serde_json::to_string(&sample_spec("rule")).expect("d");
    assert_eq!(spec_json_a, spec_json_b);
}

#[tokio::test]
async fn gc_removes_expired_snapshot_file() {
    let _scope = DirScope::new();
    let dir = snapshot_dir();

    // hand-construct an expired record on disk (we can't use
    // create_snapshot because it always writes a 30-days-in-the-future
    // expires_at). Use a valid 8-char id from the Crockford alphabet.
    let id = "EXPIRED1";
    let path = dir.join(format!("{id}.json"));
    let expired_record = SnapshotRecord {
        snapshot_id: id.to_string(),
        created_at: Utc::now() - ChronoDuration::days(31),
        expires_at: Utc::now() - ChronoDuration::days(1),
        recording: sep41_recording(),
        spec: sample_spec("rule"),
        modified_lib_rs: None,
        report: sample_report(),
    };
    std::fs::write(&path, serde_json::to_vec(&expired_record).expect("ser"))
        .expect("write expired snapshot");
    assert!(path.exists(), "fixture must be written");

    // run one GC pass synchronously — no waiting on the tokio timer.
    run_gc_once(&dir);

    assert!(
        !path.exists(),
        "expired snapshot file must be unlinked by GC"
    );
}

#[tokio::test]
async fn get_on_unknown_id_returns_snapshot_not_found() {
    let _scope = DirScope::new();

    let err = get_snapshot(GetSnapshotInput {
        snapshot_id: "ABCDEFGH".to_string(),
    })
    .await
    .expect_err("unknown id must error");

    assert_eq!(
        err.code.0, -32111,
        "code must be E_SNAPSHOT_NOT_FOUND (-32111)"
    );
    let data = err.data.expect("data must be populated");
    assert_eq!(
        data.get("error_code").and_then(|v| v.as_str()),
        Some("E_SNAPSHOT_NOT_FOUND")
    );
}

#[tokio::test]
async fn get_on_invalid_id_format_returns_snapshot_not_found() {
    let _scope = DirScope::new();

    // forbidden chars (I/L/O/U), wrong length, lowercase, path traversal.
    for bogus in [
        "../etc/pa", // contains forbidden chars + slash
        "ILOU0000",  // forbidden letters
        "abcdefgh",  // lowercase
        "0000",      // too short
        "000000000", // too long
        "",          // empty
    ] {
        let err = get_snapshot(GetSnapshotInput {
            snapshot_id: bogus.to_string(),
        })
        .await
        .expect_err("invalid id must error");

        assert_eq!(
            err.code.0, -32111,
            "code must be E_SNAPSHOT_NOT_FOUND for {bogus:?}; got {}",
            err.code.0
        );
        let data = err.data.expect("data must be populated");
        assert_eq!(
            data.get("error_code").and_then(|v| v.as_str()),
            Some("E_SNAPSHOT_NOT_FOUND")
        );
        // ensure path-style errors aren't leaking through
        let message = err.message.to_string();
        assert!(
            !message.contains("permission")
                && !message.contains("/var/")
                && !message.contains("ENOENT"),
            "must not leak path-style errors; got {message}"
        );
    }
}

#[tokio::test]
async fn concurrent_creates_produce_distinct_ids() {
    let _scope = DirScope::new();
    let store = McpStore::new();
    let (rid, sid) = warm_store_with_fixture(&store);
    // hand-share the store across tasks via Arc clones.
    let store = std::sync::Arc::new(store);

    let mut set = JoinSet::new();
    for _ in 0..100 {
        let store = std::sync::Arc::clone(&store);
        let rid = rid.clone();
        let sid = sid.clone();
        set.spawn(async move {
            create_snapshot(
                store.as_ref(),
                CreateSnapshotInput {
                    recording_id: rid,
                    spec_id: sid,
                    modified_lib_rs: None,
                    report: sample_report(),
                },
            )
            .await
        });
    }

    let mut ids = std::collections::HashSet::new();
    while let Some(joined) = set.join_next().await {
        let result = joined.expect("task panicked");
        let out = result.expect("create_snapshot must succeed under concurrency");
        assert_eq!(out.snapshot_id.len(), 8, "id must be 8 chars");
        assert!(
            ids.insert(out.snapshot_id.clone()),
            "duplicate id observed: {}",
            out.snapshot_id
        );
    }
    assert_eq!(ids.len(), 100, "expected 100 distinct ids");
}
