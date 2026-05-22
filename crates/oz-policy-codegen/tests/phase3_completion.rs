//! Phase 3 binary completion gate.
//!
//! Two tests live here, mirroring the structure of the Phase 2 completion
//! gate (`crates/oz-policy-installer/tests/phase2_completion.rs`):
//!
//! 1. [`phase3_render_byte_equal`] — **never `#[ignore]`**. Pure CPU + disk
//!    reads: reads the frozen `walkthroughs/phase3-codegen-fixture/spec.json`,
//!    invokes `render_contract(&spec, 0)`, and asserts the produced
//!    `src_lib_rs` is byte-equal to the committed
//!    `expected/slot_0/source.rs`. This is the Phase 3 completion criterion
//!    — every developer's `cargo nextest run --workspace` runs it.
//!
//! 2. [`phase3_compile_hash_pinned`] — `#[ignore]`. Drives the full
//!    `synthesize_track_b` pipeline (render → sandbox build → `stellar
//!    contract optimize`) and asserts the resulting WASM's SHA-256 matches
//!    the value pinned in `expected/slot_0/wasm_hash.txt`. CI runs this
//!    via `cargo nextest run --workspace -- --include-ignored
//!    phase3_compile_hash_pinned` on hosts with the toolchain + stellar CLI
//!    available. Same ignore pattern as `tests/minimal_compile.rs`.
//!
//! ## Why split the gate?
//!
//! Render is a pure pipeline of askama templates against the input spec —
//! no external tools, no network, no architecture-dependent behaviour. The
//! `#[ignore]`-less render test catches every codegen regression caused by
//! a template edit or a render-context shape change.
//!
//! The compile pass adds rustc, `cargo build`, and `stellar contract
//! optimize` to the equation — each of which has its own version-shift risk
//! that can produce a different WASM hash even when the input source is
//! byte-equal. Keeping the compile-hash assertion out of the default test
//! run isolates that source of friction from the determinism guarantee on
//! render output.

use std::path::PathBuf;

use oz_policy_codegen::render_contract;
use oz_policy_core::spec::PolicySpec;

/// Path from the crate root (where `cargo` runs the test) up to the
/// workspace root, then into the walkthrough directory.
const FIXTURE_DIR: &str = "../../walkthroughs/phase3-codegen-fixture";

/// Index of the (single) `Generated` slot inside `spec.json`. The fixture is
/// hand-authored to have exactly one such slot at index 0; if that changes
/// the test must change too.
const GENERATED_SLOT_INDEX: usize = 0;

fn fixture_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(FIXTURE_DIR);
    p
}

fn load_spec() -> PolicySpec {
    let path = fixture_dir().join("spec.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read spec at {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse spec at {}: {e}", path.display()))
}

/// Phase 3 completion gate (render half).
///
/// Renders the fixture spec's single Generated slot and asserts the output
/// `src_lib_rs` is byte-equal to the frozen
/// `expected/slot_0/source.rs`. Determinism of `render_contract` is what
/// makes this assertion meaningful; a template change that produces
/// semantically-identical-but-textually-different output breaks the gate.
///
/// To refresh the golden after an intentional template change:
///
/// ```sh
/// rm -rf walkthroughs/phase3-codegen-fixture/expected/
/// cargo run -p oz-policy-cli -- codegen \
///     walkthroughs/phase3-codegen-fixture/spec.json \
///     --out walkthroughs/phase3-codegen-fixture/expected/
/// ```
#[test]
fn phase3_render_byte_equal() {
    let spec = load_spec();
    let rendered = render_contract(&spec, GENERATED_SLOT_INDEX)
        .expect("Phase 3 completion: render_contract must succeed on the frozen fixture");

    let expected_path = fixture_dir().join("expected/slot_0/source.rs");
    let expected = std::fs::read_to_string(&expected_path).unwrap_or_else(|e| {
        panic!(
            "Phase 3 completion: read expected source at {}: {e}",
            expected_path.display()
        )
    });

    if rendered.src_lib_rs != expected {
        // Print a unified-ish diff so CI failures are self-diagnosing.
        // We don't pull in a diff crate; line-by-line side-by-side is
        // enough for a binary-completion gate.
        let exp_lines: Vec<&str> = expected.lines().collect();
        let act_lines: Vec<&str> = rendered.src_lib_rs.lines().collect();
        let max = exp_lines.len().max(act_lines.len());
        eprintln!("--- expected (frozen)");
        eprintln!("+++ actual (render_contract output)");
        for i in 0..max {
            let e = exp_lines.get(i).copied().unwrap_or("<EOF>");
            let a = act_lines.get(i).copied().unwrap_or("<EOF>");
            if e != a {
                eprintln!("L{i:>4} -: {e}");
                eprintln!("L{i:>4} +: {a}");
            }
        }
        panic!(
            "Phase 3 completion gate: render_contract output is NOT byte-equal \
             to the frozen expected source at {}. \
             If the templates were intentionally changed, re-run \
             `cargo run -p oz-policy-cli -- codegen \
             walkthroughs/phase3-codegen-fixture/spec.json \
             --out walkthroughs/phase3-codegen-fixture/expected/` \
             and commit the new artifacts.",
            expected_path.display()
        );
    }
}

/// Phase 3 completion gate (compile half).
///
/// Drives the full `synthesize_track_b` pipeline against the fixture spec
/// and asserts the resulting WASM's SHA-256 matches the pinned value in
/// `expected/slot_0/wasm_hash.txt`. `#[ignore]` because the pipeline
/// requires `cargo` + the wasm32 target + `stellar` 25.1.0 + a warm cargo
/// registry — same prerequisites as `minimal_compile.rs`.
///
/// CI invocation (via the verification gate script):
///
/// ```sh
/// cargo nextest run -p oz-policy-codegen --run-ignored only phase3_compile_hash_pinned
/// ```
#[ignore]
#[tokio::test]
async fn phase3_compile_hash_pinned() {
    let spec = load_spec();

    let artifacts = oz_policy_codegen::synthesize_track_b(&spec)
        .await
        .expect("Phase 3 completion: synthesize_track_b must succeed on the frozen fixture");

    assert_eq!(
        artifacts.len(),
        1,
        "fixture has exactly one Generated slot — synthesize_track_b must return exactly one artifact"
    );

    let actual_hex = hex_lower(&artifacts[0].wasm_hash);

    let expected_path = fixture_dir().join("expected/slot_0/wasm_hash.txt");
    let pinned = std::fs::read_to_string(&expected_path).unwrap_or_else(|e| {
        panic!(
            "Phase 3 completion: read pinned hash at {}: {e}",
            expected_path.display()
        )
    });
    let pinned = pinned.trim();

    assert_eq!(
        actual_hex,
        pinned,
        "Phase 3 completion gate: optimized WASM SHA-256 drifted. \
         Expected (pinned in {}): {pinned}; actual: {actual_hex}. \
         If the templates / toolchain / stellar-cli were intentionally \
         bumped, regenerate the fixture and commit the new hash.",
        expected_path.display()
    );
}

/// Lowercase-hex encode a 32-byte digest. Hand-rolled to avoid pulling in
/// a hex dependency for one call site.
fn hex_lower(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}
