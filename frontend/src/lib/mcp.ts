// real mcp client: streamable http transport, json-rpc 2.0, bearer auth.
// no mocks, no fallbacks. if the endpoint isn't configured or the server's
// /healthz isn't 200, isLive() returns false and the synthesizer disables
// itself with an honest message.

import {
  type RecordTransactionInput,
  type RecordTransactionOutput,
  type SynthesizePolicyInput,
  type SynthesizePolicyOutput,
  type SimulatePolicyInput,
  type SimReport,
  McpError,
} from "./types";

export { McpError } from "./types";

const MCP_PROTOCOL_VERSION = "2025-11-25";
const DEFAULT_TIMEOUT_MS = 30_000;

export interface McpConfig {
  endpoint: string | null; // null = not configured
  token: string | null;
}

export function readConfig(): McpConfig {
  const ep = (import.meta.env.VITE_MCP_ENDPOINT as string | undefined) ?? "";
  const tk = (import.meta.env.VITE_MCP_TOKEN as string | undefined) ?? "";
  return {
    endpoint: ep.trim() || null,
    token: tk.trim() || null,
  };
}

/**
 * checks whether the configured backend is reachable. real http call, no
 * caching. returns true only if /healthz responds 200 within the timeout.
 */
export async function isLive(cfg: McpConfig, timeoutMs = 5_000): Promise<boolean> {
  if (!cfg.endpoint) return false;
  const healthUrl = healthzUrlFor(cfg.endpoint);
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const r = await fetch(healthUrl, { method: "GET", signal: ctrl.signal });
    return r.ok;
  } catch {
    return false;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * mcp client. one instance per page load. lazily handles the initialize
 * handshake on first tool call.
 */
export class McpClient {
  readonly cfg: McpConfig;
  private nextId = 1;
  private initialized = false;

  constructor(cfg: McpConfig) {
    if (!cfg.endpoint) {
      throw new McpError(
        "CLIENT_NOT_CONFIGURED",
        "mcp endpoint env var is not set",
        -32099
      );
    }
    this.cfg = cfg;
  }

  async recordTransaction(input: RecordTransactionInput): Promise<RecordTransactionOutput> {
    return this.callTool<RecordTransactionOutput>("record_transaction", input);
  }

  async synthesizePolicy(input: SynthesizePolicyInput): Promise<SynthesizePolicyOutput> {
    return this.callTool<SynthesizePolicyOutput>("synthesize_policy", input);
  }

  async simulatePolicy(input: SimulatePolicyInput): Promise<SimReport> {
    return this.callTool<SimReport>("simulate_policy", input);
  }

  private async callTool<T>(name: string, args: unknown): Promise<T> {
    await this.ensureInitialized();
    const result = await this.rpc("tools/call", { name, arguments: args });
    // mcp tool call result: { content, structuredContent, isError }
    const r = result as {
      structuredContent?: T;
      content?: Array<{ type: string; text?: string }>;
      isError?: boolean;
    };
    if (r.isError) {
      // tool surfaced its own error. backend's error_mapping returns a
      // json-rpc error envelope on these, but if the server packed it
      // into the tool result instead, fish out the text payload.
      const msg = r.content?.find((c) => c.type === "text")?.text ?? "tool reported isError";
      throw new McpError("E_TOOL_ERROR", msg, -32000);
    }
    if (r.structuredContent == null) {
      throw new McpError(
        "E_MALFORMED_RESPONSE",
        `mcp tool ${name} returned no structuredContent`,
        -32603
      );
    }
    return r.structuredContent;
  }

  private async ensureInitialized(): Promise<void> {
    if (this.initialized) return;
    await this.rpc("initialize", {
      protocolVersion: MCP_PROTOCOL_VERSION,
      capabilities: {},
      clientInfo: { name: "oz-policy-builder-web", version: "0.0.0" },
    });
    // fire-and-forget notification per mcp spec
    await this.notify("notifications/initialized", {});
    this.initialized = true;
  }

  private async rpc(method: string, params: unknown): Promise<unknown> {
    const id = this.nextId++;
    const body = JSON.stringify({ jsonrpc: "2.0", id, method, params });
    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(), DEFAULT_TIMEOUT_MS);
    let r: Response;
    try {
      r = await fetch(this.cfg.endpoint!, {
        method: "POST",
        headers: this.headers(),
        body,
        signal: ctrl.signal,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      throw new McpError("E_NETWORK", msg, -32099);
    } finally {
      clearTimeout(timer);
    }

    if (!r.ok) {
      // bearer auth fail or 5xx — read body if any, surface honestly
      const text = await r.text().catch(() => "");
      throw new McpError(
        r.status === 401 ? "E_UNAUTHORIZED" : "E_HTTP",
        `${r.status} ${r.statusText}${text ? `: ${text.slice(0, 200)}` : ""}`,
        -32099
      );
    }

    const envelope = (await r.json()) as {
      jsonrpc?: string;
      id?: number;
      result?: unknown;
      error?: { code: number; message: string; data?: { error_code?: string; details?: unknown } };
    };

    if (envelope.error) {
      const ec = envelope.error.data?.error_code ?? "E_UNKNOWN";
      throw new McpError(ec, envelope.error.message, envelope.error.code);
    }
    return envelope.result;
  }

  private async notify(method: string, params: unknown): Promise<void> {
    const body = JSON.stringify({ jsonrpc: "2.0", method, params });
    await fetch(this.cfg.endpoint!, {
      method: "POST",
      headers: this.headers(),
      body,
    }).catch(() => {
      // notifications are fire-and-forget; swallow transport errors.
    });
  }

  private headers(): Record<string, string> {
    const h: Record<string, string> = { "Content-Type": "application/json" };
    if (this.cfg.token) h["Authorization"] = `Bearer ${this.cfg.token}`;
    return h;
  }
}

function healthzUrlFor(endpoint: string): string {
  // endpoint is typically https://host/mcp ; healthz is at the same host /healthz
  try {
    const u = new URL(endpoint);
    u.pathname = "/healthz";
    u.search = "";
    return u.toString();
  } catch {
    // if env var isn't a full URL, just append
    return endpoint.replace(/\/mcp\/?$/, "") + "/healthz";
  }
}

// one-line human description per error code, for ui rendering.
export function describeError(code: string): string {
  const map: Record<string, string> = {
    E_RECORDER_HASH_NOT_FOUND: "transaction not found on this network — check the hash and try again",
    E_RECORDER_SIM_FAILED: "couldn't fetch this transaction from the network — rpc may be down",
    E_RECORDER_XDR_DECODE_FAILED: "this transaction's data failed to decode",
    E_SYNTH_NOT_EXPRESSIBLE: "this transaction shape can't yet be expressed as a policy",
    E_CODEGEN_COMPILE_FAILED: "policy code generation failed",
    E_SIM_PERMIT_DENIED: "generated policy unexpectedly rejected the recorded transaction (bug)",
    E_SIM_DENY_PASSED: "generated policy permitted something it shouldn't (bug)",
    E_VERIFY_DRIFT: "on-chain rule differs from the spec",
    E_WALLET_REJECTED: "wallet declined the signature",
    E_INSTALL_PREFLIGHT_FAILED: "install preflight check failed",
    E_NETWORK: "couldn't reach the mcp backend",
    E_UNAUTHORIZED: "mcp backend rejected the bearer token",
    E_HTTP: "mcp backend returned an unexpected http status",
    E_TOOL_ERROR: "mcp tool reported an error",
    E_MALFORMED_RESPONSE: "mcp response was malformed",
    CLIENT_NOT_CONFIGURED: "mcp endpoint isn't set",
  };
  return map[code] ?? "unexpected error";
}
