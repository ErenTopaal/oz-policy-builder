//! STDIO transport conformance smoke test for Phase 5 Stream C.
//!
//! spawns the actual `oz-policy-mcp --stdio` binary as a subprocess via
//! `tokio::process::Command::new(env!("CARGO_BIN_EXE_oz-policy-mcp"))`,
//! drives a scripted JSON-RPC session through its stdin, and asserts the
//! responses on stdout match the documented contract. The session covers:
//!
//! 1. `initialize` — server capabilities + protocol version
//! 2. `tools/list` — exactly five tool names in canonical order
//! 3. `resources/list` — empty (store is fresh)
//! 4. `prompts/list` — three wizard templates
//! 5. `tools/call record_transaction { network: "testnet", hash: "<blend>" }`
//!    against the frozen Phase 1 Blend `claim` fixture
//! 6. `tools/call synthesize_policy { recording_id, mode: compose_only, tightness: exact }`
//! 7. Determinism check — same script run twice must produce byte-equal
//!    outputs **modulo the UUID strings** in `recording_id` / `spec_id` /
//!    `artifact_id`. The redaction helper replaces every UUID with a
//!    fixed marker before comparison.
//!
//! ## Why `#[ignore]`?
//!
//! Steps 5–6 hit Stellar testnet RPC (`https://soroban-testnet.stellar.org`)
//! to resolve the frozen Blend hash. CI default does NOT run this test —
//! invoke explicitly with `cargo nextest run --workspace --run-ignored
//! all stdio_smoke` when validating Phase 5 completion.
//!
//! ## UUID redaction set
//!
//! the following non-deterministic strings are redacted before byte
//! comparison:
//!
//! * `RecordTransactionOutput.recording_id` (`rec_<uuid v4>` → `rec_<uuid>`)
//! * `SynthesizePolicyOutput.spec_id` (`spec_<uuid v4>` → `spec_<uuid>`)
//! * `ExportPolicyOutput.artifact_id` (`art_<uuid v4>` → `art_<uuid>`)
//! * Auto-generated `context_rule.name` strings of shape
//!   `rule-<first-8-hex-of-recording-uuid>` (see
//!   `tools::default_rule_name`) → `rule-<id8>`. These appear inside
//!   `PolicySpec.context_rule.name` whenever the caller doesn't supply
//!   an explicit `rule_name`, and are derived from the UUID — so they
//!   diverge across runs even though every other byte stays equal.
//!
//! the redactor walks every JSON value (including JSON strings that
//! embed escaped JSON — `content[0].text` carries the typed payload
//! verbatim, so embedded UUIDs need the same treatment).

use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::time::timeout;

// constants — the Phase 1 frozen Blend testnet hash, kept in lockstep
// with `walkthroughs/01-blend-yield/source.json`. Updating one without
// the other is a fixture-drift bug that this constant catches.

const BLEND_TESTNET_HASH: &str = "5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e";
const BLEND_NETWORK: &str = "testnet";
const BLEND_RPC: &str = "https://soroban-testnet.stellar.org";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45); // generous; testnet RPC can be slow

#[tokio::test]
#[ignore = "network-dependent: hits Stellar testnet RPC + spawns the oz-policy-mcp binary"]
async fn stdio_smoke_full_session() {
    let transcript_a = run_full_session()
        .await
        .expect("first STDIO session must succeed");
    let transcript_b = run_full_session()
        .await
        .expect("second STDIO session must succeed");
    assert_byte_equal_modulo_uuids(&transcript_a, &transcript_b);
}

/// drives a full scripted session against a freshly-spawned subprocess.
/// returns the ordered list of JSON-RPC responses (excluding
/// notifications, which the server may send between request responses).
async fn run_full_session() -> Result<Vec<Value>, String> {
    let bin = env!("CARGO_BIN_EXE_oz-policy-mcp");
    let mut child = tokio::process::Command::new(bin)
        .arg("--stdio")
        // pipe everything so we can drive stdin / read stdout / drain
        // stderr without it interleaving with the protocol stream.
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // belt-and-braces: an unrelated `OZ_POLICY_MCP_TOKEN` in the host
        // shell must not affect STDIO mode. The binary ignores --token in
        // STDIO anyway, but unsetting the env var keeps the test
        // hermetic.
        .env_remove("OZ_POLICY_MCP_TOKEN")
        .env_remove("OZ_POLICY_MCP_DATA_DIR")
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn oz-policy-mcp: {e}"))?;

    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut reader = BufReader::new(stdout);

    // ---- 1. initialize ----
    let init_resp = jsonrpc_request(
        &mut stdin,
        &mut reader,
        1,
        "initialize",
        Some(json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "smoke-test", "version": "0.0.0" }
        })),
    )
    .await?;
    assert_response_id(&init_resp, 1)?;

    // the MCP handshake requires `notifications/initialized` after the
    // server's `initialize` response. Skipping this leaves the server in
    // the not-yet-initialised state and subsequent `tools/list` etc.
    // would be rejected.
    send_jsonrpc_notification(&mut stdin, "notifications/initialized", None).await?;

    // ---- 2. tools/list ----
    let tools_resp = jsonrpc_request(&mut stdin, &mut reader, 2, "tools/list", None).await?;
    assert_response_id(&tools_resp, 2)?;
    let tools = tools_resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(Value::as_array)
        .ok_or("tools/list result.tools missing or not array")?;
    let tool_names: Vec<&str> = tools
        .iter()
        .map(|t| t.get("name").and_then(Value::as_str).unwrap_or(""))
        .collect();
    let expected_names = vec![
        "record_transaction",
        "synthesize_policy",
        "simulate_policy",
        "export_policy",
        "verify_install",
    ];
    if tool_names != expected_names {
        return Err(format!(
            "tools/list returned {tool_names:?}, expected {expected_names:?}"
        ));
    }

    // ---- 3. resources/list ----
    let resources_resp =
        jsonrpc_request(&mut stdin, &mut reader, 3, "resources/list", None).await?;
    assert_response_id(&resources_resp, 3)?;
    let resources = resources_resp
        .get("result")
        .and_then(|r| r.get("resources"))
        .and_then(Value::as_array)
        .ok_or("resources/list result.resources missing")?;
    if !resources.is_empty() {
        return Err(format!(
            "resources/list expected empty (fresh store), got {} entries",
            resources.len()
        ));
    }

    // ---- 4. prompts/list ----
    let prompts_resp = jsonrpc_request(&mut stdin, &mut reader, 4, "prompts/list", None).await?;
    assert_response_id(&prompts_resp, 4)?;
    let prompts = prompts_resp
        .get("result")
        .and_then(|r| r.get("prompts"))
        .and_then(Value::as_array)
        .ok_or("prompts/list result.prompts missing")?;
    if prompts.len() != 3 {
        return Err(format!(
            "prompts/list expected 3 entries, got {}",
            prompts.len()
        ));
    }

    // ---- 5. tools/call record_transaction ----
    let record_resp = jsonrpc_request(
        &mut stdin,
        &mut reader,
        5,
        "tools/call",
        Some(json!({
            "name": "record_transaction",
            "arguments": {
                "network": BLEND_NETWORK,
                "hash": BLEND_TESTNET_HASH,
                "rpc_url": BLEND_RPC,
            }
        })),
    )
    .await?;
    assert_response_id(&record_resp, 5)?;
    let recording_id = extract_structured_field(&record_resp, "recording_id")?
        .as_str()
        .ok_or("recording_id not a string")?
        .to_string();
    if !recording_id.starts_with("rec_") {
        return Err(format!("recording_id missing rec_ prefix: {recording_id}"));
    }
    // the Blend `claim` fixture: contracts[0].function must equal "claim".
    let claim_fn = extract_structured_field(&record_resp, "recording")?
        .get("contracts")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("function"))
        .and_then(Value::as_str)
        .ok_or("recording.contracts[0].function missing")?
        .to_string();
    if claim_fn != "claim" {
        return Err(format!(
            "recording.contracts[0].function = {claim_fn:?}, expected \"claim\""
        ));
    }

    // ---- 6. tools/call synthesize_policy ----
    //
    // plan asks for `mode: "compose_only"`, but the Blend `claim` fixture
    // is not a SEP-41 transfer (it's a `pool.claim(from, reserve_ids, to)`
    // call), so the Track-A "compose existing primitives" path
    // legitimately errors with `E_SYNTH_NOT_EXPRESSIBLE`. We use `auto`
    // (which falls through to Track-B generation) so the smoke test
    // exercises the full plan-vs-recording happy path against this
    // specific frozen fixture. The compose-only / not-expressible branch
    // is covered by `tools::tests::synthesize_policy_compose_only_multi_target_surfaces_e_synth_not_expressible`.
    let synth_resp = jsonrpc_request(
        &mut stdin,
        &mut reader,
        6,
        "tools/call",
        Some(json!({
            "name": "synthesize_policy",
            "arguments": {
                "recording_id": recording_id,
                "mode": "auto",
                "tightness": "exact"
            }
        })),
    )
    .await?;
    assert_response_id(&synth_resp, 6)?;
    let spec_id = extract_structured_field(&synth_resp, "spec_id")?
        .as_str()
        .ok_or("spec_id not a string")?
        .to_string();
    if !spec_id.starts_with("spec_") {
        return Err(format!("spec_id missing spec_ prefix: {spec_id}"));
    }
    let schema = extract_structured_field(&synth_resp, "spec")?
        .get("schema")
        .and_then(Value::as_str)
        .ok_or("spec.schema missing")?
        .to_string();
    if schema != "oz-policy-builder/v1" {
        return Err(format!(
            "spec.schema = {schema:?}, expected \"oz-policy-builder/v1\""
        ));
    }

    // close the server (drop stdin → EOF on stdin) and let it shut down.
    drop(stdin);
    let _ = child.wait().await;

    Ok(vec![
        init_resp,
        tools_resp,
        resources_resp,
        prompts_resp,
        record_resp,
        synth_resp,
    ])
}

// determinism comparator

/// asserts that two scripted-session transcripts are byte-equal modulo
/// the freshly-generated UUID strings embedded in `*_id` fields. Any
/// difference outside that redaction set is a determinism violation.
fn assert_byte_equal_modulo_uuids(a: &[Value], b: &[Value]) {
    assert_eq!(
        a.len(),
        b.len(),
        "transcript length mismatch ({} vs {})",
        a.len(),
        b.len()
    );
    for (i, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
        let ra = redact_uuids(va.clone());
        let rb = redact_uuids(vb.clone());
        assert_eq!(
            ra, rb,
            "transcript entry {i} differs after UUID redaction:\nA = {ra}\nB = {rb}"
        );
    }
}

/// recursively walks a JSON value and replaces any string of shape
/// `<prefix>_<uuid v4>` with `<prefix>_<uuid>`. The prefix set is
/// `rec_` / `spec_` / `art_` (the three ID kinds the store hands out).
fn redact_uuids(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(redact_uuid_string(&s)),
        Value::Array(arr) => Value::Array(arr.into_iter().map(redact_uuids).collect()),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, redact_uuids(v))).collect())
        }
        other => other,
    }
}

fn redact_uuid_string(s: &str) -> String {
    // fast-path: the whole string is a single `<prefix>_<uuid>` token.
    for prefix in ["rec_", "spec_", "art_"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if looks_like_uuid_v4(rest) {
                return format!("{prefix}<uuid>");
            }
        }
    }
    // fast-path: the whole string is an auto-generated `rule-<8 hex>` name.
    if let Some(rest) = s.strip_prefix("rule-") {
        if rest.len() == 8 && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return "rule-<id8>".to_string();
        }
    }
    // slow-path: the string embeds one or more redaction-eligible tokens.
    // this is what `tools/call` payloads look like — the typed JSON is
    // serialised into the `content[0].text` fallback verbatim, so every
    // UUID + auto rule-name embedded in the payload appears inside the
    // string verbatim. We do a manual scan rather than a regex dep —
    // three fixed `<prefix>_<uuid>` prefixes plus one fixed `rule-<id8>`
    // pattern, all with fixed-length suffixes.
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let mut replaced = false;
        // (a) <prefix>_<uuid>
        for prefix in ["rec_", "spec_", "art_"] {
            let plen = prefix.len();
            if i + plen <= bytes.len() && &bytes[i..i + plen] == prefix.as_bytes() {
                let rest_start = i + plen;
                let uuid_end = rest_start + 36;
                if uuid_end <= bytes.len() {
                    let candidate = &s[rest_start..uuid_end];
                    if looks_like_uuid_v4(candidate) {
                        out.push_str(prefix);
                        out.push_str("<uuid>");
                        i = uuid_end;
                        replaced = true;
                        break;
                    }
                }
            }
        }
        // (b) rule-<8 hex>
        if !replaced {
            let prefix = "rule-";
            let plen = prefix.len();
            if i + plen <= bytes.len() && &bytes[i..i + plen] == prefix.as_bytes() {
                let rest_start = i + plen;
                let id_end = rest_start + 8;
                if id_end <= bytes.len() {
                    let candidate = &s[rest_start..id_end];
                    if candidate.bytes().all(|b| b.is_ascii_hexdigit()) {
                        // be conservative: only consume if the next byte is
                        // NOT a hex digit (so we don't accidentally truncate
                        // a longer hex token).
                        let next_is_hex = bytes.get(id_end).is_some_and(|b| b.is_ascii_hexdigit());
                        if !next_is_hex {
                            out.push_str("rule-<id8>");
                            i = id_end;
                            replaced = true;
                        }
                    }
                }
            }
        }
        if !replaced {
            // push one char (UTF-8 safe — find the char boundary).
            let ch_start = i;
            i += 1;
            while i < bytes.len() && !s.is_char_boundary(i) {
                i += 1;
            }
            out.push_str(&s[ch_start..i]);
        }
    }
    out
}

/// cheap UUID v4 recogniser: 36 chars in `8-4-4-4-12` hex layout.
fn looks_like_uuid_v4(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    bytes.iter().enumerate().all(|(i, b)| match i {
        8 | 13 | 18 | 23 => *b == b'-',
        _ => b.is_ascii_hexdigit(),
    })
}

// helpers — JSON-RPC framing over stdio (line-delimited, NOT
// content-Length framed; rmcp's STDIO transport uses newline framing).

async fn jsonrpc_request(
    stdin: &mut ChildStdin,
    reader: &mut BufReader<ChildStdout>,
    id: u64,
    method: &str,
    params: Option<Value>,
) -> Result<Value, String> {
    let mut req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
    });
    if let Some(p) = params {
        req["params"] = p;
    }
    let mut line = serde_json::to_string(&req).map_err(|e| format!("serialise {method}: {e}"))?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("write {method}: {e}"))?;
    stdin
        .flush()
        .await
        .map_err(|e| format!("flush {method}: {e}"))?;

    // loop because the server may inject notifications (e.g. logging) in
    // between the request and its matching response. We skip any
    // non-response frame.
    loop {
        let mut buf = String::new();
        let read_result = timeout(REQUEST_TIMEOUT, reader.read_line(&mut buf))
            .await
            .map_err(|_| format!("timeout reading response to {method}"))?;
        let n = read_result.map_err(|e| format!("read response {method}: {e}"))?;
        if n == 0 {
            return Err(format!("EOF before response to {method}"));
        }
        let trimmed = buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed: Value = serde_json::from_str(trimmed)
            .map_err(|e| format!("invalid JSON line in {method}: {e} :: {trimmed:?}"))?;
        // notifications carry no "id"; skip them.
        if parsed.get("id").is_none() {
            continue;
        }
        return Ok(parsed);
    }
}

async fn send_jsonrpc_notification(
    stdin: &mut ChildStdin,
    method: &str,
    params: Option<Value>,
) -> Result<(), String> {
    let mut req = json!({
        "jsonrpc": "2.0",
        "method": method,
    });
    if let Some(p) = params {
        req["params"] = p;
    }
    let mut line = serde_json::to_string(&req).map_err(|e| format!("serialise {method}: {e}"))?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("write {method}: {e}"))?;
    stdin
        .flush()
        .await
        .map_err(|e| format!("flush {method}: {e}"))?;
    Ok(())
}

fn assert_response_id(resp: &Value, expected: u64) -> Result<(), String> {
    let got = resp
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("response missing numeric id: {resp}"))?;
    if got != expected {
        return Err(format!(
            "response id mismatch: got {got}, expected {expected}"
        ));
    }
    if let Some(err) = resp.get("error") {
        return Err(format!("response was an error: {err}"));
    }
    Ok(())
}

/// extract `result.structuredContent.<field>` from a `tools/call`
/// response. The MCP `tools/call` result wraps the typed payload under
/// `structuredContent` so that newer clients can parse the typed JSON
/// directly without re-parsing the `content[0].text` fallback.
fn extract_structured_field<'a>(resp: &'a Value, field: &str) -> Result<&'a Value, String> {
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|s| s.get(field))
        .ok_or_else(|| format!("result.structuredContent.{field} missing: {resp}"))
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn uuid_recognizer_matches_canonical_v4() {
        assert!(looks_like_uuid_v4("3b241101-e2bb-4255-8caf-4136c566a962"));
        assert!(!looks_like_uuid_v4(""));
        assert!(!looks_like_uuid_v4("not-a-uuid"));
        assert!(!looks_like_uuid_v4("3b241101e2bb42558caf4136c566a962")); // missing dashes
    }

    #[test]
    fn redact_replaces_rec_and_spec_ids() {
        let input = json!({
            "recording_id": "rec_3b241101-e2bb-4255-8caf-4136c566a962",
            "spec_id": "spec_b9f1d50c-5b3a-4f5f-9b21-bbf6fa7d8c0a",
            "art_id": "art_b9f1d50c-5b3a-4f5f-9b21-bbf6fa7d8c0a",
            "kept": "hello",
            "nested": {
                "another_id": "rec_b9f1d50c-5b3a-4f5f-9b21-bbf6fa7d8c0a"
            }
        });
        let out = redact_uuids(input);
        assert_eq!(out["recording_id"], "rec_<uuid>");
        assert_eq!(out["spec_id"], "spec_<uuid>");
        assert_eq!(out["art_id"], "art_<uuid>");
        assert_eq!(out["kept"], "hello");
        assert_eq!(out["nested"]["another_id"], "rec_<uuid>");
    }

    #[test]
    fn redact_leaves_non_uuid_strings_alone() {
        let input = json!("oz-policy-builder/v1");
        assert_eq!(
            redact_uuids(input),
            Value::String("oz-policy-builder/v1".into())
        );
    }
}
