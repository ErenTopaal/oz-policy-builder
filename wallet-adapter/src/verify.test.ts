// @vitest-environment node

/**
 * Mocked-Vitest tests for `verifyInstall`. Stream C Phase 7.
 *
 * Mock surface (deliberately narrow):
 *  - `child_process.spawn` is replaced with a fake that returns a
 *    `FakeChildProcess` instance — an EventEmitter exposing `stdin`,
 *    `stdout`, `stderr` streams. The fake records every line the
 *    production code writes to stdin and replays scripted responses
 *    on stdout. This means we exercise the REAL `verify.ts` JSON-RPC
 *    framing code — the mock only injects the wire data.
 *
 * Why this is not a "mocks pretending to be real" test: every test
 * asserts specific values pulled from the wire payload (matches=true,
 * matches=false plus an array of drift items with concrete fields).
 * The production parser (`parseVerifyResult` / `validateReport`) is
 * exercised end-to-end — the only thing the mock substitutes is the
 * subprocess transport, which we don't need to spin up cargo for in
 * unit tests.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EventEmitter } from "events";
import { Readable, Writable } from "stream";

// ---- module mocks (must precede module imports under test) ----------

const hoisted = vi.hoisted(() => ({
  spawnMock: vi.fn(),
}));

vi.mock("child_process", async () => {
  const real = await vi.importActual<typeof import("child_process")>(
    "child_process",
  );
  return {
    ...real,
    spawn: hoisted.spawnMock,
  };
});

// ---- module imports (post-mock) -------------------------------------

import {
  VerifyInstallError,
  verifyInstall,
  type VerifyInstallReport,
} from "./verify.js";

const spawnMock = hoisted.spawnMock;

// ---- fake child process ---------------------------------------------

/**
 * Minimal `child_process.ChildProcess`-shaped fake. We expose:
 *  - `stdin`: a Writable that captures every chunk into `writtenLines`,
 *    parses lines, and triggers the scripted replies.
 *  - `stdout` / `stderr`: Readables we push() into.
 *  - The EventEmitter surface (`on('error'/'exit')`).
 *
 * The `script` callback is invoked once per parsed line received on
 * stdin. It returns `{ stdout?, stderr?, exit?, error? }` directives
 * that drive the corresponding events.
 */
class FakeChildProcess extends EventEmitter {
  stdin: Writable;
  stdout: Readable;
  stderr: Readable;
  killed = false;
  writtenLines: string[] = [];

  constructor(
    private readonly script: (line: string) => {
      stdout?: string;
      stderr?: string;
      exit?: { code: number | null; signal: NodeJS.Signals | null };
      error?: Error;
    } | void,
  ) {
    super();
    this.stdout = new Readable({ read() {} });
    this.stderr = new Readable({ read() {} });
    // buffer of partial chunks until we see a newline.
    let buf = "";
    const self = this;
    this.stdin = new Writable({
      write(chunk, _enc, cb) {
        const s =
          typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8");
        buf += s;
        const directives: Array<NonNullable<ReturnType<typeof self.script>>> =
          [];
        let idx: number;
        while ((idx = buf.indexOf("\n")) >= 0) {
          const line = buf.slice(0, idx);
          buf = buf.slice(idx + 1);
          self.writtenLines.push(line);
          const directive = self.script(line);
          if (directive) directives.push(directive);
        }
        // defer the response I/O until AFTER the write Promise resolves
        // so the production code has registered its `recv` listener.
        // `setImmediate` fires after microtasks AND after the writable's
        // internal `nextTick` cb-resolution — which is exactly the moment
        // production code is waiting on `session.recv(...)`.
        setImmediate(() => {
          for (const directive of directives) {
            if (directive.stderr) self.stderr.push(directive.stderr);
            if (directive.stdout) self.stdout.push(directive.stdout);
            if (directive.exit)
              self.emit("exit", directive.exit.code, directive.exit.signal);
            if (directive.error) self.emit("error", directive.error);
          }
        });
        cb();
      },
      final(cb) {
        cb();
      },
    });
  }

  kill() {
    this.killed = true;
    return true;
  }
}

// ---- helpers --------------------------------------------------------

/**
 * Build a script that answers the standard MCP handshake (initialize +
 * notifications/initialized) and then routes the `tools/call
 * verify_install` request to `respondTo`.
 *
 * The response is wrapped in rmcp's `{ content: [{ type:'text', text }] }`
 * envelope — exactly what an end-to-end run against the real server
 * would emit. We add `structuredContent` too so the typed-payload code
 * path is exercised when present.
 */
function makeHappyPathScript(
  respondTo: (request: unknown) => VerifyInstallReport,
): (line: string) => {
  stdout?: string;
  stderr?: string;
  exit?: { code: number | null; signal: NodeJS.Signals | null };
  error?: Error;
} | void {
  return (line: string) => {
    const req = JSON.parse(line);
    if (req.method === "initialize") {
      return {
        stdout:
          JSON.stringify({
            jsonrpc: "2.0",
            id: req.id,
            result: {
              protocolVersion: "2024-11-05",
              capabilities: { tools: {} },
              serverInfo: { name: "oz-policy-mcp", version: "0.0.0" },
            },
          }) + "\n",
      };
    }
    if (req.method === "notifications/initialized") {
      // no reply for notifications.
      return;
    }
    if (req.method === "tools/call" && req.params?.name === "verify_install") {
      const report = respondTo(req.params.arguments);
      const payload = {
        jsonrpc: "2.0",
        id: req.id,
        result: {
          content: [
            { type: "text", text: JSON.stringify(report) },
          ],
          structuredContent: report,
        },
      };
      return { stdout: JSON.stringify(payload) + "\n" };
    }
    // unknown method — emit an error so a future drift in the production
    // code is caught loudly.
    return {
      stdout:
        JSON.stringify({
          jsonrpc: "2.0",
          id: req.id,
          error: { code: -32601, message: `unexpected method ${req.method}` },
        }) + "\n",
    };
  };
}

beforeEach(() => {
  spawnMock.mockReset();
});

afterEach(() => {
  vi.useRealTimers();
});

// happy paths

describe("verifyInstall — happy path", () => {
  it("returns matches=true / drift=[] when the MCP server reports a clean install", async () => {
    let capturedArgs: unknown;
    const script = makeHappyPathScript((args) => {
      capturedArgs = args;
      return { matches: true, drift: [] };
    });
    const child = new FakeChildProcess(script);
    spawnMock.mockReturnValue(child as never);

    const report = await verifyInstall({
      smartAccount: "C".repeat(56),
      contextRuleId: 7,
      network: "testnet",
      rpcUrl: "https://soroban-testnet.stellar.org",
      expectedSpec: { schema: "https://example.test/policy/v1" },
      mcpServerCmd: ["fake-mcp", "--stdio"],
      timeoutMs: 500,
    });

    expect(report).toEqual({ matches: true, drift: [] });
    // spawn was invoked with the requested cmd.
    expect(spawnMock).toHaveBeenCalledTimes(1);
    expect(spawnMock.mock.calls[0]?.[0]).toBe("fake-mcp");
    expect(spawnMock.mock.calls[0]?.[1]).toEqual(["--stdio"]);
    // arguments passed to verify_install are the snake_case translation
    // of the TS input shape, with expected_spec round-tripped.
    expect(capturedArgs).toEqual({
      smart_account: "C".repeat(56),
      context_rule_id: 7,
      network: "testnet",
      rpc_url: "https://soroban-testnet.stellar.org",
      expected_spec: { schema: "https://example.test/policy/v1" },
    });
    // the subprocess was torn down.
    expect(child.killed).toBe(true);
  });

  it("returns drift entries verbatim when the MCP server reports field-level drift", async () => {
    const drifty: VerifyInstallReport = {
      matches: false,
      drift: [
        {
          field: "lifetime_ledgers",
          expected: 432_000,
          actual: 100,
        },
        {
          field: "context_rule.name",
          expected: "blend-yield",
          actual: "rule-abcdef01",
        },
      ],
    };
    const script = makeHappyPathScript(() => drifty);
    const child = new FakeChildProcess(script);
    spawnMock.mockReturnValue(child as never);

    const report = await verifyInstall({
      smartAccount: "C".repeat(56),
      contextRuleId: 99,
      network: "testnet",
      rpcUrl: "https://soroban-testnet.stellar.org",
      mcpServerCmd: ["fake-mcp"],
      timeoutMs: 500,
    });

    expect(report).toEqual(drifty);
    expect(report.matches).toBe(false);
    expect(report.drift).toHaveLength(2);
    expect(report.drift[0]).toMatchObject({
      field: "lifetime_ledgers",
      expected: 432_000,
      actual: 100,
    });
  });

  it("omits expected_spec from the tool arguments when not supplied", async () => {
    let capturedArgs: Record<string, unknown> | undefined;
    const script = makeHappyPathScript((args) => {
      capturedArgs = args as Record<string, unknown>;
      return { matches: false, drift: [{ field: "expected_spec_id", expected: "required", actual: null }] };
    });
    const child = new FakeChildProcess(script);
    spawnMock.mockReturnValue(child as never);

    const report = await verifyInstall({
      smartAccount: "C".repeat(56),
      contextRuleId: 7,
      network: "testnet",
      rpcUrl: "https://soroban-testnet.stellar.org",
      mcpServerCmd: ["fake-mcp"],
      timeoutMs: 500,
    });

    expect(capturedArgs).toBeDefined();
    expect(Object.keys(capturedArgs ?? {})).not.toContain("expected_spec");
    expect(report.matches).toBe(false);
    expect(report.drift[0]?.field).toBe("expected_spec_id");
  });

  it("falls back to text-content JSON when structuredContent is absent", async () => {
    spawnMock.mockReturnValue(
      new FakeChildProcess((line: string) => {
        const req = JSON.parse(line);
        if (req.method === "initialize") {
          return {
            stdout:
              JSON.stringify({
                jsonrpc: "2.0",
                id: req.id,
                result: { protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "x", version: "0" } },
              }) + "\n",
          };
        }
        if (req.method === "notifications/initialized") return;
        // respond WITHOUT structuredContent so the text-fallback parser path is hit.
        return {
          stdout:
            JSON.stringify({
              jsonrpc: "2.0",
              id: req.id,
              result: {
                content: [
                  {
                    type: "text",
                    text: JSON.stringify({ matches: true, drift: [] }),
                  },
                ],
              },
            }) + "\n",
        };
      }) as never,
    );

    const report = await verifyInstall({
      smartAccount: "C".repeat(56),
      contextRuleId: 1,
      network: "testnet",
      rpcUrl: "https://soroban-testnet.stellar.org",
      mcpServerCmd: ["fake-mcp"],
      timeoutMs: 500,
    });
    expect(report).toEqual({ matches: true, drift: [] });
  });
});

// defaults

describe("verifyInstall — defaults", () => {
  it("uses the default cargo cmd when mcpServerCmd is omitted", async () => {
    const child = new FakeChildProcess(makeHappyPathScript(() => ({ matches: true, drift: [] })));
    spawnMock.mockReturnValue(child as never);

    await verifyInstall({
      smartAccount: "C".repeat(56),
      contextRuleId: 1,
      network: "testnet",
      rpcUrl: "https://soroban-testnet.stellar.org",
      timeoutMs: 500,
    });

    expect(spawnMock.mock.calls[0]?.[0]).toBe("cargo");
    expect(spawnMock.mock.calls[0]?.[1]).toEqual([
      "run",
      "-p",
      "oz-policy-mcp",
      "--",
      "--stdio",
    ]);
  });
});

// error branches

describe("verifyInstall — error branches", () => {
  it("rejects with E_VERIFY_SUBPROCESS_SPAWN_FAILED when spawn() throws", async () => {
    spawnMock.mockImplementation(() => {
      throw new Error("ENOENT: no such file or directory, posix_spawnp 'no-such-bin'");
    });
    await expect(
      verifyInstall({
        smartAccount: "C".repeat(56),
        contextRuleId: 1,
        network: "testnet",
        rpcUrl: "https://soroban-testnet.stellar.org",
        mcpServerCmd: ["no-such-bin"],
        timeoutMs: 200,
      }),
    ).rejects.toMatchObject({
      code: "E_VERIFY_SUBPROCESS_SPAWN_FAILED",
      detail: expect.stringContaining("ENOENT"),
    });
  });

  it("rejects with E_VERIFY_SUBPROCESS_SPAWN_FAILED when the child emits 'error' before any reply", async () => {
    const child = new FakeChildProcess((line: string) => {
      // first write triggers an 'error' event.
      void line;
      return { error: new Error("posix_spawn ENOENT") };
    });
    spawnMock.mockReturnValue(child as never);

    await expect(
      verifyInstall({
        smartAccount: "C".repeat(56),
        contextRuleId: 1,
        network: "testnet",
        rpcUrl: "https://soroban-testnet.stellar.org",
        mcpServerCmd: ["fake-mcp"],
        timeoutMs: 500,
      }),
    ).rejects.toMatchObject({
      code: "E_VERIFY_SUBPROCESS_SPAWN_FAILED",
    });
  });

  it("rejects with E_VERIFY_SUBPROCESS_TIMEOUT when no reply arrives", async () => {
    const child = new FakeChildProcess(() => {
      // never reply.
      return undefined;
    });
    spawnMock.mockReturnValue(child as never);
    await expect(
      verifyInstall({
        smartAccount: "C".repeat(56),
        contextRuleId: 1,
        network: "testnet",
        rpcUrl: "https://soroban-testnet.stellar.org",
        mcpServerCmd: ["fake-mcp"],
        timeoutMs: 50,
      }),
    ).rejects.toMatchObject({
      code: "E_VERIFY_SUBPROCESS_TIMEOUT",
    });
  });

  it("rejects with E_VERIFY_SUBPROCESS_CRASHED when the child exits before answering", async () => {
    const child = new FakeChildProcess((line: string) => {
      const req = JSON.parse(line);
      if (req.method === "initialize") {
        return { exit: { code: 1, signal: null } };
      }
      return undefined;
    });
    spawnMock.mockReturnValue(child as never);
    await expect(
      verifyInstall({
        smartAccount: "C".repeat(56),
        contextRuleId: 1,
        network: "testnet",
        rpcUrl: "https://soroban-testnet.stellar.org",
        mcpServerCmd: ["fake-mcp"],
        timeoutMs: 500,
      }),
    ).rejects.toMatchObject({
      code: "E_VERIFY_SUBPROCESS_CRASHED",
    });
  });

  it("rejects with E_VERIFY_TOOL_ERROR when the MCP server returns a JSON-RPC error", async () => {
    const child = new FakeChildProcess((line: string) => {
      const req = JSON.parse(line);
      if (req.method === "initialize") {
        return {
          stdout:
            JSON.stringify({
              jsonrpc: "2.0",
              id: req.id,
              result: { protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "x", version: "0" } },
            }) + "\n",
        };
      }
      if (req.method === "notifications/initialized") return;
      // tool returns an error.
      return {
        stdout:
          JSON.stringify({
            jsonrpc: "2.0",
            id: req.id,
            error: {
              code: -32602,
              message: "verify_install: expected_spec_id 'spec_bogus' not found in store",
            },
          }) + "\n",
      };
    });
    spawnMock.mockReturnValue(child as never);

    await expect(
      verifyInstall({
        smartAccount: "C".repeat(56),
        contextRuleId: 1,
        network: "testnet",
        rpcUrl: "https://soroban-testnet.stellar.org",
        mcpServerCmd: ["fake-mcp"],
        timeoutMs: 500,
      }),
    ).rejects.toMatchObject({
      code: "E_VERIFY_TOOL_ERROR",
      detail: expect.stringContaining("expected_spec_id"),
    });
  });

  it("rejects with E_VERIFY_PROTOCOL_ERROR when the tool returns a malformed report", async () => {
    const child = new FakeChildProcess((line: string) => {
      const req = JSON.parse(line);
      if (req.method === "initialize") {
        return {
          stdout:
            JSON.stringify({
              jsonrpc: "2.0",
              id: req.id,
              result: { protocolVersion: "2024-11-05", capabilities: {}, serverInfo: { name: "x", version: "0" } },
            }) + "\n",
        };
      }
      if (req.method === "notifications/initialized") return;
      // `matches` is a string — invalid.
      return {
        stdout:
          JSON.stringify({
            jsonrpc: "2.0",
            id: req.id,
            result: {
              structuredContent: { matches: "yes", drift: [] },
            },
          }) + "\n",
      };
    });
    spawnMock.mockReturnValue(child as never);

    await expect(
      verifyInstall({
        smartAccount: "C".repeat(56),
        contextRuleId: 1,
        network: "testnet",
        rpcUrl: "https://soroban-testnet.stellar.org",
        mcpServerCmd: ["fake-mcp"],
        timeoutMs: 500,
      }),
    ).rejects.toMatchObject({
      code: "E_VERIFY_PROTOCOL_ERROR",
    });
  });

  it("rejects with E_VERIFY_SUBPROCESS_SPAWN_FAILED when mcpServerCmd is empty", async () => {
    await expect(
      verifyInstall({
        smartAccount: "C".repeat(56),
        contextRuleId: 1,
        network: "testnet",
        rpcUrl: "https://soroban-testnet.stellar.org",
        mcpServerCmd: [],
        timeoutMs: 500,
      }),
    ).rejects.toMatchObject({
      code: "E_VERIFY_SUBPROCESS_SPAWN_FAILED",
      detail: expect.stringContaining("non-empty"),
    });
    expect(spawnMock).not.toHaveBeenCalled();
  });
});

// verifyInstallError shape

describe("VerifyInstallError", () => {
  it("formats its message as [<code>] <detail> and exposes code+detail", () => {
    const err = new VerifyInstallError("E_VERIFY_PROTOCOL_ERROR", "no reply");
    expect(err.message).toBe("[E_VERIFY_PROTOCOL_ERROR] no reply");
    expect(err.code).toBe("E_VERIFY_PROTOCOL_ERROR");
    expect(err.detail).toBe("no reply");
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(VerifyInstallError);
    expect(err.name).toBe("VerifyInstallError");
  });
});
