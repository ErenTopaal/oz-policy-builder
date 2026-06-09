type Crate = { tag: "rust" | "typescript"; name: string; body: string };

const crates: Crate[] = [
  {
    tag: "rust",
    name: "oz-policy-core",
    body: "PolicySpec IR, decision tree, SEP-41 detection, Recording types.",
  },
  {
    tag: "rust",
    name: "oz-policy-recorder",
    body: "Soroban RPC client plus XDR decoder, hash or simulation to Recording.",
  },
  {
    tag: "rust",
    name: "oz-policy-codegen",
    body: "Askama templates, sandboxed compile, and five audit lint rules.",
  },
  {
    tag: "rust",
    name: "oz-policy-simhost",
    body: "In-process soroban-env-host harness with a proptest deny generator.",
  },
  {
    tag: "rust",
    name: "oz-policy-installer",
    body: "Install envelope builder with preflight and address registry, no submit.",
  },
  {
    tag: "rust",
    name: "oz-policy-mcp",
    body: "rmcp server, 5 tools, STDIO + HTTP, real on-chain readback.",
  },
  {
    tag: "rust",
    name: "oz-policy-cli",
    body: "Thin CLI over every crate: record / synthesize / codegen / simulate.",
  },
  {
    tag: "typescript",
    name: "@oz-policy-builder/wallet-adapter",
    body: "SEP-43 types, Freighter + passkey adapters, installPolicy + verifyInstall.",
  },
];

export function Architecture() {
  return (
    <section
      id="architecture"
      style={{
        backgroundColor: "#dbdbde",
        backgroundImage:
          "radial-gradient(rgba(28,28,33,0.06) 1px,transparent 1px)",
        backgroundSize: "24px 24px",
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
              fontFamily: "'JetBrains Mono',monospace",
              fontSize: 12,
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              color: "#1c1c20",
            }}
          >
            architecture
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
            Seven Rust crates, one TypeScript package
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
            A virtual workspace over one core IR, with four interfaces, CLI, MCP server,
            agent skill, and wallet adapter.
          </p>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit,minmax(255px,1fr))",
            gap: 14,
          }}
        >
          {crates.map((c) =>
            c.tag === "typescript" ? (
              <div
                key={c.name}
                className="arch-card-dark"
                style={{
                  borderRadius: 14,
                  background: "#1c1c20",
                  padding: 20,
                  boxShadow: "0 12px 26px -14px rgba(28,28,33,0.6)",
                }}
              >
                <span
                  style={{
                    fontFamily: "'JetBrains Mono',monospace",
                    fontSize: 9.5,
                    letterSpacing: "0.06em",
                    textTransform: "uppercase",
                    color: "#d5d5d8",
                    background: "rgba(255,255,255,0.12)",
                    padding: "3px 8px",
                    borderRadius: 6,
                  }}
                >
                  typescript
                </span>
                <div
                  style={{
                    fontFamily: "'JetBrains Mono',monospace",
                    fontSize: 13.5,
                    color: "#f5f5f6",
                    fontWeight: 600,
                    marginTop: 14,
                  }}
                >
                  {c.name}
                </div>
                <div
                  style={{
                    marginTop: 6,
                    color: "#cfcfd1",
                    fontSize: 13.5,
                    lineHeight: 1.5,
                  }}
                >
                  {c.body}
                </div>
              </div>
            ) : (
              <div
                key={c.name}
                className="arch-card"
                style={{
                  borderRadius: 14,
                  background: "#fbfbfb",
                  padding: 20,
                  boxShadow: "0 3px 12px -7px rgba(22,24,21,0.22)",
                }}
              >
                <span
                  style={{
                    fontFamily: "'JetBrains Mono',monospace",
                    fontSize: 9.5,
                    letterSpacing: "0.06em",
                    textTransform: "uppercase",
                    color: "#1c1c20",
                    background: "rgba(28,28,33,0.1)",
                    padding: "3px 8px",
                    borderRadius: 6,
                  }}
                >
                  rust
                </span>
                <div
                  style={{
                    fontFamily: "'JetBrains Mono',monospace",
                    fontSize: 14,
                    color: "#1d1d1e",
                    fontWeight: 600,
                    marginTop: 14,
                  }}
                >
                  {c.name}
                </div>
                <div
                  style={{
                    marginTop: 6,
                    color: "#54545a",
                    fontSize: 13.5,
                    lineHeight: 1.5,
                  }}
                >
                  {c.body}
                </div>
              </div>
            ),
          )}
        </div>
      </div>
    </section>
  );
}
