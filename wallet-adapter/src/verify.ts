/**
 * `verifyInstall` — call the MCP `verify_install` tool via a subprocess
 * and surface its structured drift report.
 *
 * Phase 7 Stream C. The implementation drives a real JSON-RPC session
 * over `child_process.spawn(...)` STDIO — there is no in-process
 * shortcut. This keeps the wire format the same as what
 * `claude_desktop_config.json` / `cursor settings.json` use when they
 * spawn the server themselves, which means the failure modes (server
 * panic, JSON-RPC parse error, malformed tool response) are covered by
 * the test surface here AND by the MCP integration tests
 * (`crates/oz-policy-mcp/tests/stdio_smoke.rs`).
 *
 * Browser flows that want to call the MCP server via the Streamable
 * HTTP transport (Phase 5 Stream C — `POST /mcp` with bearer auth)
 * should add a sibling `verifyInstallHttp` helper in a future commit;
 * the v1 API mirrors `mcpServerCmd` for clarity.
 */

import { spawn, type ChildProcessWithoutNullStreams } from "child_process";

/** Input parameters for {@link verifyInstall}. */
export interface VerifyInstallParams {
  /** Smart-account StrKey `C…` whose context rule we'll inspect. */
  smartAccount: string;
  /** Context rule ID assigned by `add_context_rule` at install time. */
  contextRuleId: number;
  /** Stellar network discriminant. Mirrors `verify_install` input. */
  network: "testnet" | "mainnet";
  /** Soroban RPC URL the MCP server will hit. */
  rpcUrl: string;
  /**
   * Optional `PolicySpec` for diff comparison. When omitted, the MCP
   * server emits a single drift item with field `expected_spec_id` so
   * callers know to provide one for a strict equality check.
   *
   * The shape is opaque here so this package does not need to take a
   * dependency on every internal type. Stream A/B's TypeScript codegen
   * (Phase 6, future) will tighten this to a typed import.
   */
  expectedSpec?: unknown;
  /**
   * Command + args used to spawn the MCP server. Default:
   * `['cargo', 'run', '-p', 'oz-policy-mcp', '--', '--stdio']`.
   *
   * For CI without cargo, pass the path to a precompiled binary, e.g.
   * `['./target/release/oz-policy-mcp', '--stdio']`.
   */
  mcpServerCmd?: string[];
  /**
   * Timeout (ms) for the entire MCP session (initialize +
   * tools/call). Default 60_000 ms. Reduced in tests to keep the
   * negative path fast.
   */
  timeoutMs?: number;
  /**
   * Override env vars the subprocess inherits. Default: inherit
   * `process.env` verbatim.
   */
  env?: NodeJS.ProcessEnv;
}

/** Structured report returned by {@link verifyInstall}. */
export interface VerifyInstallReport {
  /**
   * `true` iff the on-chain rule matches the expected spec field-for-field.
   * Always `false` when `expectedSpec` was not supplied (the MCP server
   * emits a synthetic drift item explaining that).
   */
  matches: boolean;
  /** Per-field drift entries; empty when `matches === true`. */
  drift: VerifyInstallDriftItem[];
}

/** One drift entry between an expected (spec) and actual (on-chain) value. */
export interface VerifyInstallDriftItem {
  /** Dotted field path (e.g. `"context_rule.name"`, `"lifetime_ledgers"`). */
  field: string;
  /** Expected value as a JSON value. */
  expected: unknown;
  /** Actual on-chain value as a JSON value. */
  actual: unknown;
}

/** Canonical error codes emitted by {@link verifyInstall}. */
export type VerifyInstallErrorCode =
  | "E_VERIFY_SUBPROCESS_SPAWN_FAILED"
  | "E_VERIFY_SUBPROCESS_TIMEOUT"
  | "E_VERIFY_SUBPROCESS_CRASHED"
  | "E_VERIFY_PROTOCOL_ERROR"
  | "E_VERIFY_TOOL_ERROR";

/** Typed error thrown by {@link verifyInstall}. */
export class VerifyInstallError extends Error {
  constructor(
    public readonly code: VerifyInstallErrorCode,
    public readonly detail: string,
  ) {
    super(`[${code}] ${detail}`);
    this.name = "VerifyInstallError";
    Object.setPrototypeOf(this, VerifyInstallError.prototype);
  }
}

/** Default MCP-server spawn command. */
const DEFAULT_MCP_SERVER_CMD = [
  "cargo",
  "run",
  "-p",
  "oz-policy-mcp",
  "--",
  "--stdio",
];

/** Default total session timeout (60 s). */
const DEFAULT_TIMEOUT_MS = 60_000;

/** JSON-RPC `initialize` params we send first. Matches MCP 2024-11-05. */
const INITIALIZE_PARAMS = {
  protocolVersion: "2024-11-05",
  capabilities: {},
  clientInfo: {
    name: "@oz-policy-builder/wallet-adapter",
    version: "0.0.0",
  },
};

/**
 * Spawn the MCP server, drive an `initialize` → `tools/call verify_install`
 * session, and return the structured report.
 *
 * @see VerifyInstallParams
 * @see VerifyInstallReport
 * @see VerifyInstallError
 */
export async function verifyInstall(
  params: VerifyInstallParams,
): Promise<VerifyInstallReport> {
  const cmd = params.mcpServerCmd ?? DEFAULT_MCP_SERVER_CMD;
  if (cmd.length === 0) {
    throw new VerifyInstallError(
      "E_VERIFY_SUBPROCESS_SPAWN_FAILED",
      "mcpServerCmd must be a non-empty array (got [])",
    );
  }
  const program = cmd[0]!;
  const args = cmd.slice(1);
  const timeoutMs = params.timeoutMs ?? DEFAULT_TIMEOUT_MS;

  let child: ChildProcessWithoutNullStreams;
  try {
    child = spawn(program, args, {
      stdio: ["pipe", "pipe", "pipe"],
      env: params.env ?? process.env,
    });
  } catch (e) {
    const detail = e instanceof Error ? e.message : "spawn() threw";
    throw new VerifyInstallError(
      "E_VERIFY_SUBPROCESS_SPAWN_FAILED",
      `failed to spawn ${program}: ${detail}`,
    );
  }

  // `spawn` resolves synchronously even when the binary doesn't exist
  // — the error surfaces via the `error` event on the child. We listen
  // for it and route to the same E_VERIFY_SUBPROCESS_SPAWN_FAILED code.
  const session = new McpStdioSession(child);

  try {
    await session.send({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: INITIALIZE_PARAMS,
    });
    const initResp = await session.recv(1, timeoutMs);
    if (initResp.error) {
      throw new VerifyInstallError(
        "E_VERIFY_PROTOCOL_ERROR",
        `initialize returned JSON-RPC error: ${JSON.stringify(initResp.error)}`,
      );
    }

    // MCP requires an `initialized` notification after a successful
    // `initialize` exchange.
    await session.send({
      jsonrpc: "2.0",
      method: "notifications/initialized",
    });

    await session.send({
      jsonrpc: "2.0",
      id: 2,
      method: "tools/call",
      params: {
        name: "verify_install",
        arguments: buildVerifyArguments(params),
      },
    });
    const callResp = await session.recv(2, timeoutMs);
    if (callResp.error) {
      throw new VerifyInstallError(
        "E_VERIFY_TOOL_ERROR",
        `verify_install returned JSON-RPC error: ${JSON.stringify(callResp.error)}`,
      );
    }

    return parseVerifyResult(callResp.result);
  } finally {
    session.dispose();
  }
}

/** Translate `VerifyInstallParams` to the MCP tool's input schema. */
function buildVerifyArguments(
  params: VerifyInstallParams,
): Record<string, unknown> {
  const args: Record<string, unknown> = {
    smart_account: params.smartAccount,
    context_rule_id: params.contextRuleId,
    network: params.network,
    rpc_url: params.rpcUrl,
  };
  if (params.expectedSpec !== undefined) {
    args.expected_spec = params.expectedSpec;
  }
  return args;
}

/**
 * Parse the JSON-RPC `result` payload returned by the MCP server's
 * `tools/call verify_install` invocation.
 *
 * rmcp wraps tool outputs in `{ content: [{ type: 'text', text: '<json>' }],
 * structuredContent?: { ... } }`. We prefer `structuredContent` when
 * present (typed payload, MCP 2025-11-25) and fall back to parsing the
 * `text` JSON for backwards compat.
 */
function parseVerifyResult(result: unknown): VerifyInstallReport {
  if (result === null || typeof result !== "object") {
    throw new VerifyInstallError(
      "E_VERIFY_PROTOCOL_ERROR",
      `tools/call result was not an object: ${typeof result}`,
    );
  }
  const r = result as Record<string, unknown>;

  // Prefer typed `structuredContent` (MCP-2025-11-25).
  if (r.structuredContent && typeof r.structuredContent === "object") {
    return validateReport(r.structuredContent, "structuredContent");
  }

  // Fallback: extract the first `text` content entry and JSON-parse.
  const content = r.content;
  if (Array.isArray(content)) {
    for (const item of content) {
      if (
        item !== null &&
        typeof item === "object" &&
        (item as { type?: unknown }).type === "text" &&
        typeof (item as { text?: unknown }).text === "string"
      ) {
        const text = (item as { text: string }).text;
        let parsed: unknown;
        try {
          parsed = JSON.parse(text);
        } catch (e) {
          const detail = e instanceof Error ? e.message : "JSON parse failed";
          throw new VerifyInstallError(
            "E_VERIFY_PROTOCOL_ERROR",
            `tools/call result.content[].text was not valid JSON: ${detail}`,
          );
        }
        return validateReport(parsed, "content[].text");
      }
    }
  }

  throw new VerifyInstallError(
    "E_VERIFY_PROTOCOL_ERROR",
    `tools/call result carried neither structuredContent nor a text content entry: ${JSON.stringify(r)}`,
  );
}

/** Validate the shape of a `verify_install` payload and narrow its type. */
function validateReport(
  raw: unknown,
  source: string,
): VerifyInstallReport {
  if (raw === null || typeof raw !== "object") {
    throw new VerifyInstallError(
      "E_VERIFY_PROTOCOL_ERROR",
      `verify_install ${source} was not an object: ${typeof raw}`,
    );
  }
  const r = raw as Record<string, unknown>;
  if (typeof r.matches !== "boolean") {
    throw new VerifyInstallError(
      "E_VERIFY_PROTOCOL_ERROR",
      `verify_install ${source}.matches must be boolean; got ${typeof r.matches}`,
    );
  }
  if (!Array.isArray(r.drift)) {
    throw new VerifyInstallError(
      "E_VERIFY_PROTOCOL_ERROR",
      `verify_install ${source}.drift must be an array; got ${typeof r.drift}`,
    );
  }
  const drift: VerifyInstallDriftItem[] = [];
  for (const [i, item] of r.drift.entries()) {
    if (item === null || typeof item !== "object") {
      throw new VerifyInstallError(
        "E_VERIFY_PROTOCOL_ERROR",
        `verify_install ${source}.drift[${i}] was not an object`,
      );
    }
    const d = item as Record<string, unknown>;
    if (typeof d.field !== "string") {
      throw new VerifyInstallError(
        "E_VERIFY_PROTOCOL_ERROR",
        `verify_install ${source}.drift[${i}].field must be string`,
      );
    }
    drift.push({
      field: d.field,
      expected: d.expected,
      actual: d.actual,
    });
  }
  return { matches: r.matches, drift };
}

// =====================================================================
// MCP STDIO session driver.
// =====================================================================

/** JSON-RPC 2.0 request/notification envelope. */
interface JsonRpcRequest {
  jsonrpc: "2.0";
  id?: number | string;
  method: string;
  params?: unknown;
}

/** JSON-RPC 2.0 response envelope (typed loosely; we narrow on use). */
interface JsonRpcResponse {
  jsonrpc: "2.0";
  id: number | string;
  result?: unknown;
  error?: { code: number; message: string; data?: unknown };
}

/**
 * Minimal MCP-STDIO session: one frame per line (JSON object, `\n`
 * terminated). rmcp's STDIO transport speaks the same dialect — see
 * `crates/oz-policy-mcp/src/main.rs::run_stdio_server` and rmcp's
 * `transport::io::stdio` implementation.
 */
class McpStdioSession {
  private buf = "";
  private pendingByteCount = 0;
  private readonly child: ChildProcessWithoutNullStreams;
  private readonly pending: Map<
    number | string,
    {
      resolve: (resp: JsonRpcResponse) => void;
      reject: (err: VerifyInstallError) => void;
    }
  > = new Map();
  private fatal: VerifyInstallError | null = null;
  private stderr = "";
  private disposed = false;

  constructor(child: ChildProcessWithoutNullStreams) {
    this.child = child;
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk: string) => this.onStdoutChunk(chunk));
    child.stderr.on("data", (chunk: string) => {
      // Cap captured stderr so a chatty server can't OOM the runner.
      const max = 16_384;
      if (this.stderr.length < max) {
        this.stderr += chunk.slice(0, max - this.stderr.length);
      }
    });
    child.on("error", (e) => {
      this.fatal = new VerifyInstallError(
        "E_VERIFY_SUBPROCESS_SPAWN_FAILED",
        `child process error event: ${e.message}`,
      );
      this.rejectAllPending(this.fatal);
    });
    child.on("exit", (code, signal) => {
      if (this.disposed) return;
      if (this.pending.size > 0) {
        const detail = `child exited (code=${code}, signal=${signal}) before all replies arrived; stderr=${truncate(this.stderr, 400)}`;
        const err = new VerifyInstallError(
          "E_VERIFY_SUBPROCESS_CRASHED",
          detail,
        );
        this.fatal = err;
        this.rejectAllPending(err);
      }
    });
  }

  /** Write one JSON-RPC frame, newline-terminated, to the child stdin. */
  async send(req: JsonRpcRequest): Promise<void> {
    if (this.fatal) throw this.fatal;
    const frame = JSON.stringify(req) + "\n";
    await new Promise<void>((resolve, reject) => {
      this.child.stdin.write(frame, "utf8", (err) => {
        if (err) {
          reject(
            new VerifyInstallError(
              "E_VERIFY_SUBPROCESS_CRASHED",
              `stdin.write failed: ${err.message}`,
            ),
          );
        } else {
          resolve();
        }
      });
    });
  }

  /**
   * Wait for the response whose `id` matches `id`. Other responses are
   * dropped (verify_install is request/response only, no streaming).
   */
  recv(id: number | string, timeoutMs: number): Promise<JsonRpcResponse> {
    if (this.fatal) return Promise.reject(this.fatal);
    return new Promise<JsonRpcResponse>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(
          new VerifyInstallError(
            "E_VERIFY_SUBPROCESS_TIMEOUT",
            `did not receive response to id=${id} within ${timeoutMs} ms; stderr=${truncate(this.stderr, 400)}`,
          ),
        );
      }, timeoutMs);
      // `unref` lets the node process exit if the timer is the only
      // remaining handle (e.g. when the test asserts on a thrown error
      // and bails fast).
      timer.unref?.();

      this.pending.set(id, {
        resolve: (resp) => {
          clearTimeout(timer);
          resolve(resp);
        },
        reject: (err) => {
          clearTimeout(timer);
          reject(err);
        },
      });
    });
  }

  /** Tear down the child process and release pending waiters. */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    if (this.fatal) this.rejectAllPending(this.fatal);
    try {
      this.child.stdin.end();
    } catch {
      // best-effort
    }
    if (!this.child.killed) {
      try {
        this.child.kill();
      } catch {
        // best-effort
      }
    }
  }

  /** Accumulate stdout, dispatch full lines as JSON-RPC frames. */
  private onStdoutChunk(chunk: string): void {
    this.buf += chunk;
    this.pendingByteCount += chunk.length;
    // Cap the buffer at 4 MiB to avoid runaway memory on a misbehaving server.
    const MAX_BUF = 4 * 1024 * 1024;
    if (this.pendingByteCount > MAX_BUF) {
      const err = new VerifyInstallError(
        "E_VERIFY_PROTOCOL_ERROR",
        `MCP server stdout exceeded ${MAX_BUF} bytes without a newline; aborting`,
      );
      this.fatal = err;
      this.rejectAllPending(err);
      return;
    }
    let idx: number;
    while ((idx = this.buf.indexOf("\n")) >= 0) {
      const line = this.buf.slice(0, idx).trim();
      this.buf = this.buf.slice(idx + 1);
      this.pendingByteCount = this.buf.length;
      if (line.length === 0) continue;
      this.dispatchLine(line);
    }
  }

  /** Parse one JSON-RPC line and route to the matching pending recv. */
  private dispatchLine(line: string): void {
    let parsed: unknown;
    try {
      parsed = JSON.parse(line);
    } catch (e) {
      // Don't fail the whole session on a non-JSON line — rmcp's STDIO
      // transport only writes JSON-RPC frames, but a misbehaving server
      // (panic, log misroute) could emit a stray line. We surface this
      // as a session-level error only if no response ever comes through.
      const err = new VerifyInstallError(
        "E_VERIFY_PROTOCOL_ERROR",
        `non-JSON line on MCP stdout: ${truncate(line, 200)} (${e instanceof Error ? e.message : "unknown"})`,
      );
      this.fatal = err;
      this.rejectAllPending(err);
      return;
    }
    if (parsed === null || typeof parsed !== "object") return;
    const p = parsed as Record<string, unknown>;
    if (typeof p.id !== "number" && typeof p.id !== "string") {
      // Notification or invalid response — ignore.
      return;
    }
    const id = p.id as number | string;
    const pending = this.pending.get(id);
    if (!pending) return;
    this.pending.delete(id);
    pending.resolve({
      jsonrpc: "2.0",
      id,
      result: p.result,
      error: p.error as JsonRpcResponse["error"],
    });
  }

  private rejectAllPending(err: VerifyInstallError): void {
    for (const [, waiter] of this.pending) {
      waiter.reject(err);
    }
    this.pending.clear();
  }
}

function truncate(s: string, max: number): string {
  if (s.length <= max) return s;
  return `${s.slice(0, max)}…[truncated ${s.length - max} bytes]`;
}
