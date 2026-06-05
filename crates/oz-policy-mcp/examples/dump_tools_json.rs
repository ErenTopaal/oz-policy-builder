//! phase 6 helper: emit the five MCP tools' JSON Schemas in
//! openAI-compatible function-calling shape so the Anthropic Agent Skills
//! flat-file twin (`skills/oz-policy-builder/flat/tools.json`) is generated
//! from the real `schemars::schema_for!` output instead of hand-written
//! JSON Schema.
//!
//! run from the workspace root:
//!
//! ```text
//! cargo run -p oz-policy-mcp --example dump_tools_json \
//!   > skills/oz-policy-builder/flat/tools.json
//! ```
//!
//! output is a pretty-printed JSON array of `{name, description, parameters}`
//! objects, one per tool, in the same order `tools/list` returns.

use oz_policy_mcp::{
    ExportPolicyInput, RecordTransactionInput, SimulatePolicyInput, SynthesizePolicyInput,
    VerifyInstallInput,
};
use schemars::schema_for;
use serde_json::{json, Value};

fn tool_entry<T: schemars::JsonSchema>(name: &str, description: &str) -> Value {
    let schema = schema_for!(T);
    let params = serde_json::to_value(&schema).expect("schema must serialise");
    json!({
        "name": name,
        "description": description,
        "parameters": params,
    })
}

fn main() {
    let tools = json!([
        tool_entry::<RecordTransactionInput>(
            "record_transaction",
            "Record a Stellar transaction (by hash or simulated envelope) and return the \
             deterministic `Recording` IR. Pass exactly one of `hash` or \
             `envelope_xdr_base64`."
        ),
        tool_entry::<SynthesizePolicyInput>(
            "synthesize_policy",
            "Synthesize the minimum-rights `PolicySpec` (oz-policy-builder/v1) that would \
             permit the given Recording under the chosen `tightness` and `mode`."
        ),
        tool_entry::<SimulatePolicyInput>(
            "simulate_policy",
            "Simulate the given spec against its source recording (permit) and the \
             synthesized + caller-supplied deny vectors. Returns a `SimReport`. Always run \
             this before exporting."
        ),
        tool_entry::<ExportPolicyInput>(
            "export_policy",
            "Materialize the spec's compiled Track-B WASM, generated Rust source, and/or \
             signed install envelope (per `format`). Returns inline payloads plus \
             `resource://` URIs."
        ),
        tool_entry::<VerifyInstallInput>(
            "verify_install",
            "Verify the on-chain context rule at `(smart_account, context_rule_id)` matches \
             the expected `PolicySpec`. Returns a drift report (empty when matches=true)."
        ),
    ]);

    let s = serde_json::to_string_pretty(&tools).expect("serialize tools array");
    println!("{s}");
}
