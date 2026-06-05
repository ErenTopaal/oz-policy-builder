//! network-dependent roundtrip vs `walkthroughs/01-blend-yield/expected-recording.json`.
//! `#[ignore]` because testnet retention is ~24h; run manually with `--run-ignored all`.

use std::fs;
use std::path::Path;

#[derive(serde::Deserialize)]
#[allow(dead_code)] // kept for diagnostics.
struct SourceDescriptor {
    network: String,
    hash: String,
    rpc_url: String,
    network_passphrase: String,
    description: String,
    captured_at: String,
}

#[tokio::test]
#[ignore = "network-dependent: requires Stellar testnet RPC reachable and the frozen hash within retention"]
async fn blend_claim_roundtrip() {
    // two parent() hops from crate manifest to workspace root.
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("manifest dir parent (crates/)")
        .parent()
        .expect("crates/ parent (workspace root)");
    let walkthrough_dir = workspace_root.join("walkthroughs/01-blend-yield");

    let source_path = walkthrough_dir.join("source.json");
    let source: SourceDescriptor = serde_json::from_str(
        &fs::read_to_string(&source_path)
            .unwrap_or_else(|e| panic!("source.json missing at {}: {e}", source_path.display())),
    )
    .expect("source.json malformed");

    let expected_path = walkthrough_dir.join("expected-recording.json");
    let expected = fs::read_to_string(&expected_path).unwrap_or_else(|e| {
        panic!(
            "expected-recording.json missing at {}: {e}",
            expected_path.display()
        )
    });

    let recording = oz_policy_recorder::record_by_hash(
        &source.rpc_url,
        &source.network_passphrase,
        &source.hash,
    )
    .await
    .expect("recorder must succeed on the frozen Blend hash (testnet retention may have expired)");

    let actual =
        serde_json::to_string_pretty(&recording).expect("Recording must round-trip to pretty JSON");

    if actual.trim() != expected.trim() {
        // dump divergent output to target/ for inspection; never auto-overwrite.
        let diff_path = workspace_root.join("target/blend_claim_diff.json");
        // best-effort write — target/ may not exist.
        let _ = fs::write(&diff_path, &actual);
        panic!(
            "recording drift vs walkthroughs/01-blend-yield/expected-recording.json\n\
             actual written to: {}\n\
             expected = {} bytes\n\
             actual   = {} bytes",
            diff_path.display(),
            expected.len(),
            actual.len(),
        );
    }
}
