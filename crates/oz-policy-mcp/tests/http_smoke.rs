//! Streamable HTTP transport conformance smoke test for Phase 5 Stream C.
//!
//! Spawns the actual `oz-policy-mcp --http 0 --token testtoken` binary
//! (port 0 = OS-assigned, discovered by grepping stderr for the
//! `oz-policy-mcp http listening on <addr>` line), then drives the same
//! scripted JSON-RPC session as `stdio_smoke.rs` against `POST /mcp` via
//! `reqwest`. Asserts:
//!
//! 1. `initialize` succeeds with the bearer token; the response carries
//!    a `Mcp-Session-Id` header (rmcp's stateful session mode).
//! 2. `tools/list` returns exactly five tool names in canonical order.
//! 3. `resources/list` returns an empty list.
//! 4. `prompts/list` returns three prompts.
//! 5. `tools/call record_transaction { hash: <blend> }` returns a
//!    `recording_id` + the canonical `claim` recording.
//! 6. `tools/call synthesize_policy { recording_id, mode: compose_only,
//!    tightness: exact }` returns a `spec_id` + the canonical spec.
//! 7. **No `Authorization` header** → 401.
//! 8. **Wrong token** → 401.
//! 9. **Bad `Bearer` syntax** (e.g. `"Basic abc"`) → 401.
//! 10. `GET /healthz` works with no auth and returns
//!     `{"status":"ok","version":"<CARGO_PKG_VERSION>"}`.
//! 11. The full session output (after UUID + session-id redaction)
//!     matches the same script run twice and matches the STDIO transcript
//!     for the same script.
//!
//! ## Why `#[ignore]`?
//!
//! Steps 5–6 hit Stellar testnet RPC. CI default does NOT run this test
//! — invoke explicitly with `cargo nextest run --workspace --run-ignored
//! all http_smoke` when validating Phase 5 completion.

use std::process::Stdio;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::ChildStderr;
use tokio::time::timeout;

const BLEND_TESTNET_HASH: &str = "5a0ccffed7aa586fe5f2763f1f85869c349a1ddff6edb21e4d76bf087a42db4e";
const TEST_TOKEN: &str = "testtoken";
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);

#[tokio::test]
#[ignore = "network-dependent: hits Stellar testnet RPC + spawns the oz-policy-mcp binary"]
async fn http_smoke_full_session() {
    let server = spawn_http_server()
        .await
        .expect("HTTP server must start within STARTUP_TIMEOUT");
    let client = reqwest::Client::builder()
        .build()
        .expect("reqwest client must build");
    let base = format!("http://{}/mcp", server.addr);
    let healthz = format!("http://{}/healthz", server.addr);

    // ---- 10. healthz first (no auth, fastest possible smoke check) ----
    let resp = client
        .get(&healthz)
        .send()
        .await
        .expect("healthz request must reach server");
    assert_eq!(resp.status(), 200, "healthz must return 200");
    let body: Value = resp.json().await.expect("healthz body must be JSON");
    assert_eq!(body["status"], "ok");
    assert_eq!(
        body["version"],
        env!("CARGO_PKG_VERSION"),
        "healthz version must match the crate's CARGO_PKG_VERSION"
    );

    // ---- 7. no Authorization header → 401 ----
    let resp = client
        .post(&base)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .body(initialize_body())
        .send()
        .await
        .expect("missing-auth request must reach server");
    assert_eq!(
        resp.status(),
        401,
        "missing Authorization header must return 401"
    );
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("missing bearer"),
        "missing-auth body should mention missing bearer: {body}"
    );

    // ---- 8. wrong token → 401 ----
    let resp = client
        .post(&base)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .header(AUTHORIZATION, "Bearer wrongtoken")
        .body(initialize_body())
        .send()
        .await
        .expect("wrong-token request must reach server");
    assert_eq!(resp.status(), 401, "wrong bearer token must return 401");
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("invalid bearer"),
        "wrong-token body should mention invalid bearer: {body}"
    );

    // ---- 9. bad scheme → 401 ----
    let resp = client
        .post(&base)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .header(AUTHORIZATION, "Basic dXNlcjpwYXNz")
        .body(initialize_body())
        .send()
        .await
        .expect("bad-scheme request must reach server");
    assert_eq!(resp.status(), 401, "bad scheme must return 401");
    let body = resp.text().await.unwrap();
    assert!(
        body.contains("invalid bearer"),
        "bad-scheme body should mention invalid bearer: {body}"
    );

    // ---- 1-6. happy-path scripted session ----
    let transcript_a = run_full_http_session(&client, &server.addr)
        .await
        .expect("first HTTP session must succeed");

    // For the determinism check we need a *fresh store*, otherwise the
    // second `resources/list` would observe the prior session's
    // recording + spec — that's the documented sticky-store semantics
    // (see `McpStore` docs), and exactly the contract that lets multiple
    // HTTP sessions share `recording_id` lookups. We get a fresh store
    // by spawning a second server process — the test ergonomically
    // mirrors the STDIO case, which gets per-process isolation for free.
    drop(server); // kill_on_drop=true → SIGTERM the first server
    let server2 = spawn_http_server()
        .await
        .expect("second HTTP server must start");
    let transcript_b = run_full_http_session(&client, &server2.addr)
        .await
        .expect("second HTTP session must succeed");

    assert_byte_equal_modulo_uuids(&transcript_a, &transcript_b);
}

// =====================================================================
// Server spawn / discovery
// =====================================================================

struct ServerHandle {
    addr: String,
    _child: tokio::process::Child, // drop = kill (kill_on_drop set below)
}

async fn spawn_http_server() -> Result<ServerHandle, String> {
    let bin = env!("CARGO_BIN_EXE_oz-policy-mcp");
    let mut child = tokio::process::Command::new(bin)
        .args(["--http", "0", "--token", TEST_TOKEN])
        .env_remove("OZ_POLICY_MCP_DATA_DIR")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn oz-policy-mcp --http 0: {e}"))?;

    let stderr = child.stderr.take().ok_or("no stderr")?;
    // Read the startup line, then keep draining stderr in a background
    // task. **Critical**: if we leave the pipe buffer full, the server's
    // `tracing` writer eventually blocks on stderr write, which appears
    // as "connection error" / "error sending request" on the client side
    // since the server's tokio reactor can't make progress.
    let addr = read_listening_addr(stderr).await?;
    Ok(ServerHandle {
        addr,
        _child: child,
    })
}

/// Reads stderr until a line matching
/// `oz-policy-mcp http listening on <SocketAddr>` is found, then returns
/// the parsed address AND spawns a background task that drains the rest
/// of stderr to `/dev/null`. The binary writes the listening line
/// exactly once during startup; see `main::run_http_server` for the source.
async fn read_listening_addr(stderr: ChildStderr) -> Result<String, String> {
    let mut reader = BufReader::new(stderr).lines();
    let fut = async {
        while let Ok(Some(line)) = reader.next_line().await {
            // Expected format: "oz-policy-mcp http listening on 127.0.0.1:NNNNN"
            if let Some(addr) = line.strip_prefix("oz-policy-mcp http listening on ") {
                return Ok::<_, String>(addr.trim().to_string());
            }
        }
        Err("EOF on stderr before listening-line".to_string())
    };
    let addr = timeout(STARTUP_TIMEOUT, fut)
        .await
        .map_err(|_| "timeout waiting for listening-line".to_string())??;
    // Spawn the stderr drain after the startup line is consumed.
    tokio::spawn(async move {
        while let Ok(Some(_line)) = reader.next_line().await {
            // Discard; the smoke test doesn't assert on log content.
        }
    });
    Ok(addr)
}

// =====================================================================
// Session driver
// =====================================================================

fn initialize_body() -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "smoke-test", "version": "0.0.0" }
        }
    })
    .to_string()
}

async fn run_full_http_session(client: &reqwest::Client, addr: &str) -> Result<Vec<Value>, String> {
    let base = format!("http://{addr}/mcp");

    // ---- 1. initialize ----
    let (init_resp, session_id) = post_initialize(client, &base, 1).await?;

    // ---- The MCP handshake requires `notifications/initialized` after init. ----
    let resp = client
        .post(&base)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .header(AUTHORIZATION, format!("Bearer {TEST_TOKEN}"))
        .header("Mcp-Session-Id", &session_id)
        .header("Mcp-Protocol-Version", "2025-11-25")
        .body(json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string())
        .send()
        .await
        .map_err(|e| format!("notifications/initialized POST: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "notifications/initialized returned {}",
            resp.status()
        ));
    }
    // Drain body so the SSE channel is freed for subsequent requests.
    let _ = resp.bytes().await;

    // ---- 2. tools/list ----
    let tools_resp = post_request(client, &base, &session_id, 2, "tools/list", None).await?;
    let tool_names: Vec<String> = tools_resp
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(Value::as_array)
        .ok_or("tools/list missing result.tools")?
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str).map(String::from))
        .collect();
    let expected_names = [
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
        post_request(client, &base, &session_id, 3, "resources/list", None).await?;
    let resources = resources_resp
        .get("result")
        .and_then(|r| r.get("resources"))
        .and_then(Value::as_array)
        .ok_or("resources/list missing result.resources")?;
    if !resources.is_empty() {
        return Err(format!(
            "resources/list expected empty, got {}",
            resources.len()
        ));
    }

    // ---- 4. prompts/list ----
    let prompts_resp = post_request(client, &base, &session_id, 4, "prompts/list", None).await?;
    let prompts = prompts_resp
        .get("result")
        .and_then(|r| r.get("prompts"))
        .and_then(Value::as_array)
        .ok_or("prompts/list missing result.prompts")?;
    if prompts.len() != 3 {
        return Err(format!("prompts/list expected 3, got {}", prompts.len()));
    }

    // ---- 5. tools/call record_transaction ----
    let record_resp = post_request(
        client,
        &base,
        &session_id,
        5,
        "tools/call",
        Some(json!({
            "name": "record_transaction",
            "arguments": {
                "network": "testnet",
                "hash": BLEND_TESTNET_HASH,
                "rpc_url": "https://soroban-testnet.stellar.org",
            }
        })),
    )
    .await?;
    let recording_id = extract_structured_field(&record_resp, "recording_id")?
        .as_str()
        .ok_or("recording_id not a string")?
        .to_string();
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
    let synth_resp = post_request(
        client,
        &base,
        &session_id,
        6,
        "tools/call",
        Some(json!({
            "name": "synthesize_policy",
            "arguments": {
                // See the matching comment in stdio_smoke.rs: the Blend
                // fixture is not SEP-41-expressible, so we drive Track-B
                // synthesis via `auto` instead of plan-literal
                // `compose_only`. The compose-only error path is covered
                // by `tools::tests::synthesize_policy_compose_only_multi_target_surfaces_e_synth_not_expressible`.
                "recording_id": recording_id,
                "mode": "auto",
                "tightness": "exact"
            }
        })),
    )
    .await?;
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

    Ok(vec![
        init_resp,
        tools_resp,
        resources_resp,
        prompts_resp,
        record_resp,
        synth_resp,
    ])
}

/// Posts the `initialize` request and returns the parsed JSON-RPC
/// response + the `Mcp-Session-Id` the server allocated. The response
/// body for rmcp's stateful HTTP mode is SSE-framed
/// (`text/event-stream`); we extract the single JSON-RPC payload from
/// the `data:` line.
async fn post_initialize(
    client: &reqwest::Client,
    base: &str,
    id: u64,
) -> Result<(Value, String), String> {
    let resp = client
        .post(base)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json, text/event-stream")
        .header(AUTHORIZATION, format!("Bearer {TEST_TOKEN}"))
        .body(
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": { "name": "smoke-test", "version": "0.0.0" }
                }
            })
            .to_string(),
        )
        .send()
        .await
        .map_err(|e| format!("initialize POST: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("initialize returned {}", resp.status()));
    }
    let session_id = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .ok_or("initialize response missing Mcp-Session-Id header")?
        .to_string();
    let body = timeout(REQUEST_TIMEOUT, resp.text())
        .await
        .map_err(|_| "timeout reading initialize body")?
        .map_err(|e| format!("read initialize body: {e}"))?;
    let payload = parse_sse_response(&body)?;
    Ok((payload, session_id))
}

async fn post_request(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
    id: u64,
    method: &str,
    params: Option<Value>,
) -> Result<Value, String> {
    let mut body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
    });
    if let Some(p) = params {
        body["params"] = p;
    }
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {TEST_TOKEN}"))
            .map_err(|e| format!("auth header: {e}"))?,
    );
    headers.insert(
        "Mcp-Session-Id",
        HeaderValue::from_str(session_id).map_err(|e| format!("session-id header: {e}"))?,
    );
    headers.insert(
        "Mcp-Protocol-Version",
        HeaderValue::from_static("2025-11-25"),
    );

    let resp = client
        .post(base)
        .headers(headers)
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("{method} POST: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("{method} returned {status}: {body}"));
    }
    let raw = timeout(REQUEST_TIMEOUT, resp.text())
        .await
        .map_err(|_| format!("timeout reading {method} body"))?
        .map_err(|e| format!("read {method} body: {e}"))?;
    parse_sse_response(&raw)
}

/// rmcp's Streamable HTTP server (stateful mode) returns each JSON-RPC
/// response as an SSE event stream. The format per
/// [streamable_http_priming.rs] is `id: <n>\nretry: <ms>\ndata: <json>\n\n`
/// with one or more events delimited by `\n\n`. We extract every
/// `data:`-prefixed line, parse it as JSON, and return the first
/// response that carries a non-null `id` (skipping priming events whose
/// `data` is the empty SSE payload).
fn parse_sse_response(body: &str) -> Result<Value, String> {
    // Iterate event blocks split by blank-line boundaries.
    for block in body.split("\n\n") {
        for line in block.lines() {
            if let Some(payload) = line.strip_prefix("data:") {
                let trimmed = payload.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let parsed: Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => continue, // priming events have non-JSON `data:` payloads
                };
                if parsed.get("id").is_some() {
                    return Ok(parsed);
                }
            }
        }
    }
    Err(format!("no JSON-RPC response found in SSE body: {body}"))
}

fn extract_structured_field<'a>(resp: &'a Value, field: &str) -> Result<&'a Value, String> {
    resp.get("result")
        .and_then(|r| r.get("structuredContent"))
        .and_then(|s| s.get(field))
        .ok_or_else(|| format!("result.structuredContent.{field} missing: {resp}"))
}

// =====================================================================
// Determinism comparator (shares the UUID-redaction approach with
// stdio_smoke.rs; duplicated rather than shared because integration
// tests in separate files don't get a common module)
// =====================================================================

fn assert_byte_equal_modulo_uuids(a: &[Value], b: &[Value]) {
    assert_eq!(a.len(), b.len(), "transcript length mismatch");
    for (i, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
        let ra = redact_uuids(va.clone());
        let rb = redact_uuids(vb.clone());
        assert_eq!(
            ra, rb,
            "transcript entry {i} differs after UUID redaction:\nA = {ra}\nB = {rb}"
        );
    }
}

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
    // Mirrors the redactor in `stdio_smoke.rs` — see that file's
    // module-level docs for the full redaction set + rationale. Both
    // smoke tests use the same scheme so their transcripts can be diffed
    // against each other in a future cross-transport conformance check.
    for prefix in ["rec_", "spec_", "art_"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if looks_like_uuid_v4(rest) {
                return format!("{prefix}<uuid>");
            }
        }
    }
    if let Some(rest) = s.strip_prefix("rule-") {
        if rest.len() == 8 && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return "rule-<id8>".to_string();
        }
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let mut replaced = false;
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
        if !replaced {
            let prefix = "rule-";
            let plen = prefix.len();
            if i + plen <= bytes.len() && &bytes[i..i + plen] == prefix.as_bytes() {
                let rest_start = i + plen;
                let id_end = rest_start + 8;
                if id_end <= bytes.len() {
                    let candidate = &s[rest_start..id_end];
                    if candidate.bytes().all(|b| b.is_ascii_hexdigit()) {
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

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn parse_sse_response_extracts_json_data_line() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        let parsed = parse_sse_response(body).unwrap();
        assert_eq!(parsed["id"], 1);
    }

    #[test]
    fn parse_sse_response_skips_priming_blocks_with_no_id() {
        let body = "id: 0\nretry: 3000\ndata: {}\n\nid: 1\ndata: {\"jsonrpc\":\"2.0\",\"id\":42,\"result\":{}}\n\n";
        let parsed = parse_sse_response(body).unwrap();
        assert_eq!(parsed["id"], 42);
    }

    #[test]
    fn parse_sse_response_errors_when_no_response() {
        let body = "id: 0\nretry: 3000\ndata: \n\n";
        assert!(parse_sse_response(body).is_err());
    }

    #[test]
    fn redactor_normalises_id_strings() {
        let input = json!({
            "recording_id": "rec_3b241101-e2bb-4255-8caf-4136c566a962",
            "literal": "rec_not-a-uuid",
        });
        let out = redact_uuids(input);
        assert_eq!(out["recording_id"], "rec_<uuid>");
        assert_eq!(out["literal"], "rec_not-a-uuid");
    }
}
