export function Hero() {
  return (
    <section
      id="top"
      style={{ position: "relative", overflow: "hidden", background: "#dfdfe1" }}
    >
      <div
        style={{
          position: "relative",
          maxWidth: 1180,
          margin: "0 auto",
          padding: "clamp(50px,7vw,100px) 28px",
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit,minmax(335px,1fr))",
          gap: "clamp(40px,5vw,64px)",
          alignItems: "center",
        }}
      >
        <div>
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 9,
              fontFamily: "'JetBrains Mono',monospace",
              fontSize: 11.5,
              letterSpacing: "0.04em",
              textTransform: "uppercase",
              color: "#1c1c20",
              background: "rgba(28,28,33,0.1)",
              padding: "6px 13px",
              borderRadius: 30,
            }}
          >
            <span
              style={{ width: 6, height: 6, borderRadius: "50%", background: "#1c1c20" }}
            />
            stellar · soroban · smart accounts
          </span>
          <h1
            style={{
              fontFamily: "'Bricolage Grotesque',sans-serif",
              fontSize: "clamp(36px,5.2vw,64px)",
              lineHeight: 1.04,
              letterSpacing: "-0.02em",
              margin: "22px 0 0",
              fontWeight: 500,
              color: "#1d1d1e",
            }}
          >
            Record a transaction. Get the policy that permits{" "}
            <em style={{ fontStyle: "italic", color: "#1c1c20" }}>exactly that</em>
            <span style={{ color: "#8c8c92" }}>, nothing more.</span>
          </h1>
          <p
            style={{
              margin: "24px 0 0",
              fontSize: "clamp(15px,1.5vw,18.5px)",
              lineHeight: 1.62,
              color: "#54545a",
              maxWidth: "50ch",
            }}
          >
            A developer tool that records a real Soroban transaction and synthesizes the
            minimum OpenZeppelin smart-account policy that would authorize it. Scoped,
            time-bounded authority for agents and dapps, without handing over your keys.
          </p>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 11, marginTop: 32 }}>
            <a
              href="#"
              className="btn-dark"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 9,
                textDecoration: "none",
                background: "#1c1c20",
                color: "#f4f4f5",
                fontFamily: "'JetBrains Mono',monospace",
                fontWeight: 500,
                fontSize: 14,
                padding: "14px 23px",
                borderRadius: 11,
                boxShadow: "0 16px 30px -16px rgba(28,28,33,0.7)",
              }}
            >
              View on GitHub ↗
            </a>
            <a
              href="#synthesize"
              className="btn-light"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 9,
                textDecoration: "none",
                background: "#fbfbfb",
                color: "#1c1c20",
                fontFamily: "'JetBrains Mono',monospace",
                fontSize: 14,
                padding: "14px 23px",
                borderRadius: 11,
                boxShadow: "0 2px 8px -3px rgba(22,24,21,0.18)",
              }}
            >
              Try the synthesizer ↓
            </a>
          </div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginTop: 36 }}>
            {["Rust CLI", "MCP server", "TS wallet adapter", "deterministic output"].map(
              (t) => (
                <span
                  key={t}
                  style={{
                    fontFamily: "'JetBrains Mono',monospace",
                    fontSize: 11,
                    color: "#55555b",
                    background: "rgba(28,28,33,0.07)",
                    padding: "6px 12px",
                    borderRadius: 8,
                  }}
                >
                  {t}
                </span>
              ),
            )}
          </div>
        </div>
        <div>
          <HeroVisual />
        </div>
      </div>
    </section>
  );
}

function HeroVisual() {
  const C = {
    codeBg: "#161619",
    green2: "#ccccd3",
    kGreen: "#f3f3f8",
    mono: "'JetBrains Mono',monospace",
  };
  const rows: Array<[string, string]> = [
    ["schema", "oz-policy-builder/v1"],
    ["rule", "blend-claim"],
    ["context", "call_contract → CCEBVDYM…44HGF"],
    ["policy[0]", "generated · function_allowlist"],
    ["constraint", 'functions ["claim"]'],
    ["lifetime", "432000 ledgers"],
    ["signers", "none"],
  ];
  return (
    <div
      style={{
        background: C.codeBg,
        borderRadius: 16,
        padding: 22,
        boxShadow: "0 30px 60px -28px rgba(18,22,26,0.6)",
        animation: "floaty 7s ease-in-out infinite",
        minHeight: 372,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          paddingBottom: 14,
          marginBottom: 10,
          borderBottom: "1px solid rgba(255,255,255,0.08)",
        }}
      >
        <span
          style={{ width: 8, height: 8, borderRadius: "50%", background: C.green2 }}
        />
        <span
          style={{ fontFamily: C.mono, fontSize: 12, color: "#b2b2b7", fontWeight: 600 }}
        >
          PolicySpec
        </span>
        <span
          style={{
            fontFamily: C.mono,
            fontSize: 11,
            color: "#8c8c93",
            marginLeft: "auto",
          }}
        >
          synthesized · deterministic
        </span>
      </div>
      <div>
        {rows.map(([k, v]) => (
          <div
            key={k}
            style={{
              display: "flex",
              gap: 10,
              padding: "7px 0",
              alignItems: "baseline",
            }}
          >
            <span
              style={{
                fontFamily: C.mono,
                fontSize: 11.5,
                color: C.kGreen,
                minWidth: 92,
              }}
            >
              {k}
            </span>
            <span style={{ fontFamily: C.mono, fontSize: 12, color: "#dedee0" }}>{v}</span>
          </div>
        ))}
      </div>
      <div
        style={{
          marginTop: 10,
          paddingTop: 12,
          borderTop: "1px solid rgba(255,255,255,0.08)",
        }}
      >
        <div
          style={{
            fontFamily: C.mono,
            fontSize: 11,
            color: "#8c8c93",
            marginBottom: 8,
          }}
        >
          ── simulate ──
        </div>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <span
            style={{
              fontFamily: C.mono,
              fontSize: 11.5,
              color: "#1c1c20",
              background: C.green2,
              padding: "4px 10px",
              borderRadius: 6,
              fontWeight: 600,
            }}
          >
            permit ✓
          </span>
          <span
            style={{
              fontFamily: C.mono,
              fontSize: 11.5,
              color: "#1c1c20",
              background: C.green2,
              padding: "4px 10px",
              borderRadius: 6,
              fontWeight: 600,
            }}
          >
            deny 6/6 ✓
          </span>
          <span
            style={{
              fontFamily: C.mono,
              fontSize: 11.5,
              color: C.kGreen,
              padding: "4px 10px",
            }}
          >
            matches = true
          </span>
        </div>
      </div>
    </div>
  );
}
