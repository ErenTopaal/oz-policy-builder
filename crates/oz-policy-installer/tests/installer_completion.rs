//! installer binary completion gate.
//!
//! this test is the **literal** completion criterion for the
//! recording → spec leg of the OZ accounts Policy Builder. It must run
//! in the default `cargo nextest run --workspace` invocation (no
//! `#[ignore]` gate, no network calls): the deliverable is "decision
//! tree turns a frozen Recording into a frozen PolicySpec, byte-equal
//! forever".
//!
//! ## What it does (deterministically, offline)
//!
//! 1. Reads the frozen `Recording` JSON for the SEP-41 testnet fixture
//!    (`walkthroughs/02-sep41-subscription/recording.json`).
//! 2. Calls `oz_policy_core::decision_tree::synthesize(&recording, &opts)`
//!    with the canonical options:
//!    `mode=ComposeOnly, tightness=Exact, lifetime_ledgers=Some(432_000),
//!    context_rule_name="sep41-subscription"`.
//! 3. Reads the frozen expected `PolicySpec` JSON
//!    (`walkthroughs/02-sep41-subscription/expected-spec-track-a.json`).
//! 4. Asserts `serde_json::to_string_pretty(&actual_spec) ==
//!    expected_spec_string` — byte-equal, including key insertion order.
//!
//! both files are produced by re-running the CLI; see the walkthrough
//! README for the recipe. If the decision tree ever produces a different
//! shape for this exact input, this test breaks loudly — which is the
//! point of the gate.
//!
//! ## Why this test lives in `oz-policy-installer/tests/`
//!
//! the completion gate naturally lives next to the envelope-shape gate
//! (`envelope_structure.rs`). The installer already depends on
//! `oz-policy-core`, so the decision-tree call is a direct path import,
//! no extra dev-dep needed beyond `serde_json` (added in `Cargo.toml`
//! for this test).

use oz_policy_core::decision_tree::{synthesize, SynthesisOptions, Tightness};
use oz_policy_core::recording::Recording;
use oz_policy_core::spec::SynthesisMode;

/// path from the crate root (where `cargo` runs the test) up to the
/// workspace root, then into the walkthroughs directory. `CARGO_MANIFEST_DIR`
/// is set by cargo to the absolute path of the crate's `Cargo.toml`
/// directory — `crates/oz-policy-installer/` — so two `..` segments climb to
/// the workspace root.
const WALKTHROUGH_DIR: &str = "../../walkthroughs/02-sep41-subscription";

#[test]
fn installer_completion() {
    installer_completion_gate_byte_equal_synth();
}

/// internal helper. The outer wrapper is named so the verification gate's
/// positional filter `cargo nextest run --workspace installer_completion`
/// matches (nextest's positional substring match is over the test name).
fn installer_completion_gate_byte_equal_synth() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let recording_path = format!("{manifest_dir}/{WALKTHROUGH_DIR}/recording.json");
    let expected_path = format!("{manifest_dir}/{WALKTHROUGH_DIR}/expected-spec-track-a.json");

    let recording_raw = std::fs::read_to_string(&recording_path)
        .unwrap_or_else(|e| panic!("read recording at {recording_path}: {e}"));
    let recording: Recording = serde_json::from_str(&recording_raw)
        .unwrap_or_else(|e| panic!("parse recording at {recording_path}: {e}"));

    let opts = SynthesisOptions {
        mode: SynthesisMode::ComposeOnly,
        tightness: Tightness::Exact,
        lifetime_ledgers: Some(432_000),
        delegated_signer: None,
        context_rule_name: "sep41-subscription".to_string(),
    };

    let actual_spec = synthesize(&recording, &opts)
        .expect("Phase 2 completion: synthesize must succeed on the frozen SEP-41 recording");

    let actual_json = serde_json::to_string_pretty(&actual_spec)
        .expect("Phase 2 completion: PolicySpec must serialize to pretty JSON");

    let expected_json = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("read expected spec at {expected_path}: {e}"));

    // `read_to_string` on a freshly-emitted `serde_json::to_string_pretty`
    // file includes a trailing newline iff the file was created with one
    // (most editors add it). `to_string_pretty` itself never appends a
    // trailing newline. Normalise both sides by trimming a single trailing
    // newline if present — this is the only whitespace divergence we
    // tolerate; everything else must be byte-equal.
    let expected_trimmed = expected_json.strip_suffix('\n').unwrap_or(&expected_json);
    let actual_trimmed = actual_json.strip_suffix('\n').unwrap_or(&actual_json);

    if expected_trimmed != actual_trimmed {
        // print a unified-ish diff to make CI failures self-diagnosing.
        // we don't pull in a diff crate; a line-by-line side-by-side print
        // is enough for the binary-completion gate.
        let exp_lines: Vec<&str> = expected_trimmed.lines().collect();
        let act_lines: Vec<&str> = actual_trimmed.lines().collect();
        let max = exp_lines.len().max(act_lines.len());
        eprintln!("--- expected (frozen)");
        eprintln!("+++ actual (synthesize output)");
        for i in 0..max {
            let e = exp_lines.get(i).copied().unwrap_or("<EOF>");
            let a = act_lines.get(i).copied().unwrap_or("<EOF>");
            if e != a {
                eprintln!("L{i:>4} -: {e}");
                eprintln!("L{i:>4} +: {a}");
            }
        }
        panic!(
            "Phase 2 completion gate: synthesize output is NOT byte-equal to \
             the frozen expected spec at {expected_path}. \
             If the decision tree was intentionally changed, re-run \
             `oz-policy-cli synthesize {WALKTHROUGH_DIR}/recording.json \
             --mode compose-only --tightness exact --lifetime 432000 \
             --rule-name sep41-subscription \
             > {WALKTHROUGH_DIR}/expected-spec-track-a.json` and commit the new file."
        );
    }
}
