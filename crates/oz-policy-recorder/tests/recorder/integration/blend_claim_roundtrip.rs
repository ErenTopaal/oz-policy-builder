//! Phase 1 binary completion criterion (see `plan.md` § "Phase 1 — Foundations",
//! P1-T4).
//!
//! Drives the recorder against the frozen Blend testnet `claim` fixture in
//! `walkthroughs/01-blend-yield/` and asserts byte-equality between the live
//! `record_by_hash` output and the committed `expected-recording.json`.
//!
//! This test is network-dependent and gated behind `#[ignore]` — Stellar
//! testnet's `getTransaction` retention is ~24 h, so the test will start
//! returning `E_RECORDER_HASH_NOT_FOUND` once the source ledger ages out.
//! That is the expected lifecycle: the test runs on demand during the
//! Phase 1 completion gate and whenever a new fixture is captured. Default
//! CI does **not** run it.
//!
//! Run it explicitly with nextest (note: nextest gates `#[ignore]` via
//! `--run-ignored`, not libtest's `--include-ignored`):
//!
//! ```bash
//! cargo nextest run --workspace --run-ignored all \
//!   recorder::integration::blend_claim_roundtrip
//! ```

use std::fs;
use std::path::Path;

#[derive(serde::Deserialize)]
#[allow(dead_code)] // descriptor fields preserved for diagnostics / future use
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
    // Resolve `walkthroughs/01-blend-yield/` relative to the workspace root.
    // `CARGO_MANIFEST_DIR` for this crate is `crates/oz-policy-recorder`, so
    // two `parent()` hops give us the workspace root.
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
    .expect(
        "recorder must succeed on the frozen Blend hash \
         (testnet retention may have expired — see walkthroughs/01-blend-yield/README.md)",
    );

    let actual =
        serde_json::to_string_pretty(&recording).expect("Recording must round-trip to pretty JSON");

    if actual.trim() != expected.trim() {
        // Drop the divergent output next to `target/` so a developer can run
        // `diff walkthroughs/01-blend-yield/expected-recording.json target/blend_claim_diff.json`
        // to inspect the drift. We deliberately do not auto-overwrite the
        // expected file: the fixture is append-only, and a drift here means
        // either (a) the recorder changed shape (and the schema URI must
        // bump) or (b) the source tx was replaced (and a new walkthrough
        // directory is required).
        let diff_path = workspace_root.join("target/blend_claim_diff.json");
        // Best-effort: target/ should exist post-build, but if it doesn't we
        // still want the panic message to be useful.
        let _ = fs::write(&diff_path, &actual);
        panic!(
            "Recording drift detected vs. walkthroughs/01-blend-yield/expected-recording.json\n\
             actual written to: {}\n\
             expected = {} bytes\n\
             actual   = {} bytes",
            diff_path.display(),
            expected.len(),
            actual.len(),
        );
    }
}
