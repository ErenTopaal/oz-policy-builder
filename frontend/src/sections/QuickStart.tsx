import { useState } from "react";

type TabId = "cli" | "mcp" | "ts";

const tabs: Array<[TabId, string]> = [
  ["cli", "CLI"],
  ["mcp", "MCP server"],
  ["ts", "TS wallet adapter"],
];

const snippets: Record<TabId, string> = {
  cli: `$ oz-policy record 5a0ccffe…a42db4e --network testnet > rec.json
$ oz-policy synthesize rec.json --mode auto --tightness exact \\
    --lifetime 432000 --rule-name "blend-claim" > spec.json
$ oz-policy codegen spec.json --out ./out
$ oz-policy simulate spec.json rec.json --wasm-dir ./out > sim.json`,
  mcp: `$ cargo build --release -p oz-policy-mcp

# claude_desktop_config.json / cursor / cline / continue
{
  "mcpServers": {
    "oz-policy-builder": {
      "command": "./target/release/oz-policy-mcp",
      "args": ["--stdio"]
    }
  }
}`,
  ts: `import { installPolicy, verifyInstall } from "@oz-policy-builder/wallet-adapter";

const { contextRuleId } = await installPolicy(adapter, {
  smartAccount, spec, wasmDir: "./out",
});

const result = await verifyInstall(smartAccount, contextRuleId);
console.log(result.matches); // true`,
};

const C = {
  ink: "#1d1d1e",
  codeBg: "#161619",
  codeInk: "#f6f6f8",
  codeFaint: "#b2b2b8",
  green: "#1c1c20",
  green2: "#ccccd3",
  kGreen: "#f3f3f8",
  mono: "'JetBrains Mono',monospace",
};

function renderCode(code: string) {
  const lines = code.split("\n");
  return lines.map((ln, i) => {
    const trimmed = ln.trimStart();
    let node: React.ReactNode;
    if (trimmed.indexOf("$") === 0) {
      const idx = ln.indexOf("$");
      node = (
        <>
          <span style={{ color: C.kGreen, fontWeight: 600 }}>$</span>
          <span style={{ color: C.codeInk }}>{ln.slice(idx + 1)}</span>
        </>
      );
    } else if (trimmed.indexOf("#") === 0) {
      node = <span style={{ color: C.codeFaint }}>{ln}</span>;
    } else {
      node = <span style={{ color: C.codeInk }}>{ln || " "}</span>;
    }
    return (
      <div key={i} style={{ minHeight: "1.7em" }}>
        {node}
      </div>
    );
  });
}

export function QuickStart() {
  const [tab, setTab] = useState<TabId>("cli");
  const [copied, setCopied] = useState<string>("");

  const copy = (id: string, text: string) => {
    try {
      void navigator.clipboard.writeText(text);
    } catch {
      // ignore
    }
    setCopied(id);
    window.setTimeout(() => {
      setCopied((c) => (c === id ? "" : c));
    }, 1400);
  };

  const copiedHere = copied === "qs_" + tab;
  const tabLabel = tabs.find((t) => t[0] === tab)?.[1].toLowerCase() ?? "";

  return (
    <section
      id="quickstart"
      style={{
        backgroundColor: "#fbfbfb",
        backgroundImage:
          "linear-gradient(rgba(28,28,33,0.025) 1px,transparent 1px),linear-gradient(90deg,rgba(28,28,33,0.025) 1px,transparent 1px)",
        backgroundSize: "46px 46px",
      }}
    >
      <div
        style={{
          maxWidth: 1180,
          margin: "0 auto",
          padding: "clamp(60px,8vw,100px) 28px",
        }}
      >
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 13,
            marginBottom: 36,
          }}
        >
          <span
            style={{
              fontFamily: C.mono,
              fontSize: 12,
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              color: "#1c1c20",
            }}
          >
            quick start
          </span>
          <h2
            style={{
              margin: 0,
              fontFamily: "'Bricolage Grotesque',sans-serif",
              fontSize: "clamp(23px,2.8vw,34px)",
              fontWeight: 500,
              letterSpacing: "-0.02em",
              color: "#1d1d1e",
            }}
          >
            Run it three ways
          </h2>
          <p
            style={{
              margin: 0,
              maxWidth: "64ch",
              color: "#54545a",
              fontSize: 16.5,
              lineHeight: 1.6,
            }}
          >
            Terminal, MCP client, or programmatically from a wallet. Same Rust core under
            all three.
          </p>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <div
            style={{
              display: "inline-flex",
              gap: 4,
              background: "#dbdbde",
              padding: 5,
              borderRadius: 12,
              alignSelf: "flex-start",
              flexWrap: "wrap",
            }}
          >
            {tabs.map(([v, l]) => {
              const active = tab === v;
              return (
                <button
                  key={v}
                  onClick={() => setTab(v)}
                  style={{
                    background: active ? C.green : "transparent",
                    color: active ? "#f4f4f5" : C.ink,
                    border: "none",
                    fontFamily: C.mono,
                    fontSize: 13,
                    padding: "10px 17px",
                    borderRadius: 9,
                    cursor: "pointer",
                    fontWeight: active ? 600 : 500,
                    transition: "all .2s",
                  }}
                >
                  {l}
                </button>
              );
            })}
          </div>
          <div
            style={{
              borderRadius: 16,
              background: C.codeBg,
              overflow: "hidden",
              boxShadow: "0 16px 38px -22px rgba(18,22,26,0.6)",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "13px 18px",
                background: "rgba(255,255,255,0.04)",
              }}
            >
              <span style={{ fontFamily: C.mono, fontSize: 11.5, color: "#b2b2b7" }}>
                {tabLabel}
              </span>
              <button
                onClick={() => copy("qs_" + tab, snippets[tab])}
                style={{
                  background: copiedHere ? C.green2 : "rgba(255,255,255,0.1)",
                  color: copiedHere ? "#1c1c20" : "#d8d8db",
                  border: "none",
                  fontFamily: C.mono,
                  fontSize: 11,
                  padding: "6px 12px",
                  borderRadius: 8,
                  cursor: "pointer",
                  fontWeight: copiedHere ? 600 : 400,
                }}
              >
                {copiedHere ? "copied ✓" : "copy"}
              </button>
            </div>
            <pre
              style={{
                margin: 0,
                padding: "20px 20px 22px",
                fontFamily: C.mono,
                fontSize: 13,
                lineHeight: 1.75,
                overflowX: "auto",
                whiteSpace: "pre",
              }}
            >
              {renderCode(snippets[tab])}
            </pre>
          </div>
        </div>
      </div>
    </section>
  );
}
