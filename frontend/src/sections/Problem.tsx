const fullKeyTags = [
  "transfer",
  "approve",
  "swap",
  "claim",
  "withdraw",
  "burn",
  "mint",
  "set_admin",
  "upgrade",
];

const scopedTags: Array<{ label: string; highlight: boolean }> = [
  { label: "transfer", highlight: false },
  { label: "approve", highlight: false },
  { label: "swap", highlight: false },
  { label: "claim", highlight: true },
  { label: "withdraw", highlight: false },
  { label: "burn", highlight: false },
  { label: "mint", highlight: false },
  { label: "set_admin", highlight: false },
  { label: "upgrade", highlight: false },
];

export function Problem() {
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
          padding: "clamp(60px,8vw,100px) 28px",
        }}
      >
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 13,
            marginBottom: 44,
            maxWidth: "66ch",
          }}
        >
          <span
            style={{
              fontFamily: "'JetBrains Mono',monospace",
              fontSize: 12,
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              color: "#a0a0a8",
            }}
          >
            why scoped, not keys
          </span>
          <h2
            style={{
              margin: 0,
              fontFamily: "'Bricolage Grotesque',sans-serif",
              fontSize: "clamp(30px,4.2vw,54px)",
              fontWeight: 500,
              letterSpacing: "-0.025em",
              color: "#fbfbfb",
            }}
          >
            An agent should hold a permission, not your account
          </h2>
          <p
            style={{
              margin: 0,
              color: "#b6b6bd",
              fontSize: 16.5,
              lineHeight: 1.6,
            }}
          >
            Hand an agent your keys and it inherits everything you can do. Hand it a
            synthesized policy and it inherits exactly one flow, the one you recorded,
            under tight bounds.
          </p>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit,minmax(300px,1fr))",
            gap: 18,
          }}
        >
          <div style={{ background: "#26262b", borderRadius: 16, padding: 26 }}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 10,
                marginBottom: 8,
              }}
            >
              <span
                style={{
                  fontFamily: "'JetBrains Mono',monospace",
                  fontSize: 12,
                  color: "#a0a0a8",
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                }}
              >
                full key access
              </span>
              <span
                style={{
                  fontFamily: "'JetBrains Mono',monospace",
                  fontSize: 11,
                  color: "#e08a72",
                  border: "1px solid rgba(224,138,114,0.4)",
                  padding: "2px 8px",
                  borderRadius: 20,
                }}
              >
                unbounded
              </span>
            </div>
            <div
              style={{
                fontFamily: "'Bricolage Grotesque',sans-serif",
                fontSize: 23,
                color: "#fbfbfb",
                fontWeight: 500,
                marginBottom: 18,
              }}
            >
              The agent can do anything
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
              {fullKeyTags.map((t) => (
                <span
                  key={t}
                  style={{
                    fontFamily: "'JetBrains Mono',monospace",
                    fontSize: 12,
                    color: "#e8e8ee",
                    background: "rgba(255,255,255,0.06)",
                    border: "1px solid rgba(255,255,255,0.18)",
                    padding: "6px 11px",
                    borderRadius: 8,
                  }}
                >
                  {t}
                </span>
              ))}
            </div>
            <div
              style={{
                marginTop: 18,
                fontFamily: "'JetBrains Mono',monospace",
                fontSize: 11.5,
                color: "#8a8a92",
                lineHeight: 1.5,
              }}
            >
              your entire account surface is reachable
            </div>
          </div>
          <div
            style={{
              background: "#26262b",
              borderRadius: 16,
              padding: 26,
              position: "relative",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 10,
                marginBottom: 8,
              }}
            >
              <span
                style={{
                  fontFamily: "'JetBrains Mono',monospace",
                  fontSize: 12,
                  color: "#a0a0a8",
                  textTransform: "uppercase",
                  letterSpacing: "0.05em",
                }}
              >
                scoped policy
              </span>
              <span
                style={{
                  fontFamily: "'JetBrains Mono',monospace",
                  fontSize: 11,
                  color: "#1c1c20",
                  background: "#e8e8ee",
                  padding: "2px 9px",
                  borderRadius: 20,
                  fontWeight: 600,
                }}
              >
                function_allowlist[claim]
              </span>
            </div>
            <div
              style={{
                fontFamily: "'Bricolage Grotesque',sans-serif",
                fontSize: 23,
                color: "#fbfbfb",
                fontWeight: 500,
                marginBottom: 18,
              }}
            >
              The agent can do one thing
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
              {scopedTags.map((t) =>
                t.highlight ? (
                  <span
                    key={t.label}
                    style={{
                      fontFamily: "'JetBrains Mono',monospace",
                      fontSize: 12,
                      color: "#1c1c20",
                      background: "#f3f3f8",
                      border: "1px solid #f3f3f8",
                      padding: "6px 11px",
                      borderRadius: 8,
                      fontWeight: 600,
                      animation: "pulseSoft 2.4s ease-in-out infinite",
                    }}
                  >
                    {t.label}
                  </span>
                ) : (
                  <span
                    key={t.label}
                    style={{
                      fontFamily: "'JetBrains Mono',monospace",
                      fontSize: 12,
                      color: "#5c5c63",
                      border: "1px solid rgba(255,255,255,0.07)",
                      padding: "6px 11px",
                      borderRadius: 8,
                      textDecoration: "line-through",
                    }}
                  >
                    {t.label}
                  </span>
                ),
              )}
            </div>
            <div
              style={{
                marginTop: 18,
                fontFamily: "'JetBrains Mono',monospace",
                fontSize: 11.5,
                color: "#8a8a92",
                lineHeight: 1.5,
              }}
            >
              every other call is denied at enforce-time
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
