// numbers below are real counts from the project: 7 rust crates, 363 tests, 5
// mcp tools, 7 constraint templates. update if the project grows.

const STATS: Array<{ value: string; label: string }> = [
  { value: "7", label: "Rust crates" },
  { value: "363", label: "tests passing" },
  { value: "5", label: "MCP tools" },
  { value: "7", label: "constraint templates" },
];

export function Stats() {
  return (
    <section
      style={{
        backgroundColor: "#1c1c20",
        backgroundImage:
          "linear-gradient(rgba(255,255,255,0.03) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,0.03) 1px,transparent 1px)",
        backgroundSize: "42px 42px",
        color: "#ebebec",
      }}
    >
      <div
        style={{
          maxWidth: 1180,
          margin: "0 auto",
          padding: "clamp(50px,6vw,78px) 28px",
        }}
      >
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit,minmax(170px,1fr))",
            gap: 26,
          }}
        >
          {STATS.map((s) => (
            <div
              key={s.label}
              style={{ display: "flex", flexDirection: "column", gap: 8 }}
            >
              <span
                style={{
                  fontFamily: "'Bricolage Grotesque', sans-serif",
                  fontSize: "clamp(40px,5vw,58px)",
                  fontWeight: 600,
                  color: "#fbfbfb",
                  lineHeight: 1,
                }}
              >
                {s.value}
              </span>
              <span
                style={{
                  fontFamily: "'JetBrains Mono', monospace",
                  fontSize: 12.5,
                  color: "#a0a0a8",
                  letterSpacing: "0.02em",
                }}
              >
                {s.label}
              </span>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
