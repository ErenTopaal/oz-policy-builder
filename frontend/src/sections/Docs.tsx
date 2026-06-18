const docs: Array<{ tag: string; title: string; desc: string; href: string }> = [
  {
    tag: "01",
    title: "Getting started",
    desc: "Install the toolchain and run the four CLI commands end to end against a real testnet transaction in under a minute.",
    href: "https://docs.policy.erentopal.xyz/docs/getting-started/quickstart",
  },
  {
    tag: "02",
    title: "Concepts",
    desc: "PolicySpec, synthesis modes, the seven constraint primitives, and the three composable OZ primitives.",
    href: "https://docs.policy.erentopal.xyz/docs/concepts/constraints",
  },
  {
    tag: "03",
    title: "CLI reference",
    desc: "Every flag for record, synthesize, codegen, simulate, and prepare-install, with exit-code semantics.",
    href: "https://docs.policy.erentopal.xyz/docs/cli",
  },
  {
    tag: "04",
    title: "MCP tools",
    desc: "Nine tools with full input and output schemas. Transport, auth, snapshot store, and client setup for Claude, Cursor, Cline, Continue.",
    href: "https://docs.policy.erentopal.xyz/docs/mcp/tools",
  },
];

export function Docs() {
  return (
    <section
      id="docs"
      style={{
        background: "#1c1c20",
        backgroundImage:
          "linear-gradient(rgba(255,255,255,0.025) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,0.025) 1px,transparent 1px)",
        backgroundSize: "46px 46px",
        color: "#f4f4f5",
      }}
    >
      <div
        style={{
          maxWidth: 1120,
          margin: "0 auto",
          padding: "clamp(60px,8vw,100px) 28px",
        }}
      >
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "minmax(0,1fr) minmax(0,1.4fr)",
            gap: "clamp(28px,5vw,72px)",
            alignItems: "start",
            marginBottom: 44,
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
            <span
              style={{
                fontFamily: "'JetBrains Mono',monospace",
                fontSize: 12,
                letterSpacing: "0.08em",
                textTransform: "uppercase",
                color: "#9b9ba0",
              }}
            >
              documentation
            </span>
            <h2
              style={{
                margin: 0,
                fontFamily: "'Bricolage Grotesque',sans-serif",
                fontSize: "clamp(26px,3.2vw,40px)",
                fontWeight: 500,
                letterSpacing: "-0.02em",
                color: "#f4f4f5",
                lineHeight: 1.15,
              }}
            >
              Every flag, every schema, every constraint.
            </h2>
          </div>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 16,
              color: "#b8b8be",
              fontSize: 15.5,
              lineHeight: 1.6,
            }}
          >
            <p style={{ margin: 0 }}>
              Reference docs for the CLI, the MCP server, the synthesizer IR, the constraint
              primitives, and the wallet adapter. Verified against the source at every release.
            </p>
            <a
              href="https://docs.policy.erentopal.xyz"
              target="_blank"
              rel="noreferrer"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 9,
                alignSelf: "flex-start",
                textDecoration: "none",
                background: "#f4f4f5",
                color: "#1c1c20",
                fontFamily: "'JetBrains Mono',monospace",
                fontWeight: 500,
                fontSize: 14,
                padding: "13px 22px",
                borderRadius: 11,
                boxShadow: "0 14px 28px -16px rgba(0,0,0,0.6)",
              }}
            >
              Open the docs ↗
            </a>
          </div>
        </div>

        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit,minmax(240px,1fr))",
            gap: 16,
          }}
        >
          {docs.map((d) => (
            <a
              key={d.tag}
              href={d.href}
              target="_blank"
              rel="noreferrer"
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 11,
                padding: "22px 22px 24px",
                borderRadius: 14,
                background: "#26262b",
                color: "inherit",
                textDecoration: "none",
                boxShadow: "inset 0 0 0 1px rgba(255,255,255,0.05)",
                transition: "transform 120ms ease, box-shadow 120ms ease",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.boxShadow =
                  "inset 0 0 0 1px rgba(255,255,255,0.15)";
                e.currentTarget.style.transform = "translateY(-1px)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.boxShadow =
                  "inset 0 0 0 1px rgba(255,255,255,0.05)";
                e.currentTarget.style.transform = "translateY(0)";
              }}
            >
              <span
                style={{
                  fontFamily: "'JetBrains Mono',monospace",
                  fontSize: 10.5,
                  letterSpacing: "0.08em",
                  color: "#7d7d86",
                }}
              >
                {d.tag} ↗
              </span>
              <span
                style={{
                  fontFamily: "'Bricolage Grotesque',sans-serif",
                  fontSize: 18,
                  fontWeight: 500,
                  color: "#f4f4f5",
                  letterSpacing: "-0.01em",
                }}
              >
                {d.title}
              </span>
              <span
                style={{
                  fontSize: 13.5,
                  lineHeight: 1.55,
                  color: "#9b9ba0",
                }}
              >
                {d.desc}
              </span>
            </a>
          ))}
        </div>
      </div>
    </section>
  );
}
