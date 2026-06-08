const links: Array<{ label: string; short: string; href: string }> = [
  {
    label: "install transaction",
    short: "038583fa…ce90bb ↗",
    href: "https://stellar.expert/explorer/testnet/tx/038583fa4c95654c9a26323702b86729e084357d47ab169fa22a77d821ce90bb",
  },
  {
    label: "smart account · c-addr",
    short: "CAQGYWVE…SNFKCBN3A ↗",
    href: "https://stellar.expert/explorer/testnet/contract/CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A",
  },
  {
    label: "policy contract · c-addr",
    short: "CDBE67MN…OLY7CWAR ↗",
    href: "https://stellar.expert/explorer/testnet/contract/CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR",
  },
];

export function ProofPoint() {
  return (
    <section
      style={{
        backgroundColor: "#1c1c20",
        color: "#ebebec",
        backgroundImage:
          "linear-gradient(rgba(255,255,255,0.03) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,0.03) 1px,transparent 1px)",
        backgroundSize: "42px 42px",
      }}
    >
      <div
        style={{
          maxWidth: 1180,
          margin: "0 auto",
          padding: "clamp(60px,8vw,94px) 28px",
        }}
      >
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 13,
            marginBottom: 38,
          }}
        >
          <span
            style={{
              fontFamily: "'JetBrains Mono',monospace",
              fontSize: 12,
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              color: "#ccccd3",
            }}
          >
            on-chain proof
          </span>
          <h2
            style={{
              margin: 0,
              fontFamily: "'Bricolage Grotesque',sans-serif",
              fontSize: "clamp(28px,3.5vw,46px)",
              fontWeight: 500,
              letterSpacing: "-0.02em",
              color: "#f5f5f6",
            }}
          >
            The full pipeline, closed on testnet
          </h2>
          <p
            style={{
              margin: 0,
              maxWidth: "64ch",
              color: "#c9c9cd",
              fontSize: 16.5,
              lineHeight: 1.6,
            }}
          >
            A real record → generate → simulate → sign → install → verify roundtrip. Not a
            screenshot, a live context rule you can inspect in the explorer.
          </p>
        </div>
        <div
          style={{
            borderRadius: 18,
            background: "#fbfbfb",
            overflow: "hidden",
            boxShadow: "0 16px 44px -24px rgba(22,24,21,0.4)",
          }}
        >
          <div
            style={{
              padding: 28,
              display: "flex",
              flexWrap: "wrap",
              gap: 20,
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 18 }}>
              <div
                style={{
                  width: 48,
                  height: 48,
                  borderRadius: "50%",
                  background: "#1c1c20",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  flexShrink: 0,
                  fontSize: 21,
                  color: "#f4f4f5",
                }}
              >
                ✓
              </div>
              <div>
                <div
                  style={{
                    fontFamily: "'JetBrains Mono',monospace",
                    fontSize: "clamp(18px,2.3vw,23px)",
                    color: "#1d1d1e",
                    fontWeight: 600,
                    letterSpacing: "-0.01em",
                  }}
                >
                  verifyInstall · matches = true
                </div>
                <div
                  style={{
                    marginTop: 6,
                    color: "#54545a",
                    fontSize: 14,
                    lineHeight: 1.5,
                    maxWidth: "52ch",
                  }}
                >
                  The context rule installed on chain is byte-for-byte the policy you
                  reviewed.
                </div>
              </div>
            </div>
            <span
              style={{
                fontFamily: "'JetBrains Mono',monospace",
                fontSize: 11.5,
                color: "#54545a",
                background: "#e9e9eb",
                padding: "8px 14px",
                borderRadius: 22,
                whiteSpace: "nowrap",
              }}
            >
              stellar testnet
            </span>
          </div>
          {links.map((l) => (
            <a
              key={l.label}
              href={l.href}
              target="_blank"
              rel="noreferrer"
              className="row-hover"
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 14,
                padding: "17px 28px",
                borderTop: "1px solid rgba(22,24,21,0.08)",
                textDecoration: "none",
              }}
            >
              <span
                style={{
                  fontFamily: "'JetBrains Mono',monospace",
                  fontSize: 10.5,
                  letterSpacing: "0.05em",
                  textTransform: "uppercase",
                  color: "#3f3f45",
                  fontWeight: 600,
                }}
              >
                {l.label}
              </span>
              <span
                style={{
                  fontFamily: "'JetBrains Mono',monospace",
                  fontSize: 13,
                  color: "#1c1c20",
                  fontWeight: 500,
                }}
              >
                {l.short}
              </span>
            </a>
          ))}
          <div
            style={{
              padding: "17px 28px",
              borderTop: "1px solid rgba(22,24,21,0.08)",
              background: "#f4f4f5",
              display: "flex",
              flexWrap: "wrap",
              gap: "8px 16px",
              fontFamily: "'JetBrains Mono',monospace",
              fontSize: 11.5,
              color: "#54545a",
              alignItems: "center",
            }}
          >
            <span>ledger 2617998</span>
            <span style={{ color: "#cfcfd2" }}>·</span>
            <span>context_rule_id 4</span>
            <span style={{ color: "#cfcfd2" }}>·</span>
            <span>Blend yield-claim</span>
            <span style={{ color: "#cfcfd2" }}>·</span>
            <span style={{ color: "#1c1c20", fontWeight: 600 }}>
              function_allowlist[claim]
            </span>
          </div>
        </div>
      </div>
    </section>
  );
}
