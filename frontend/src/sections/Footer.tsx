export function Footer() {
  return (
    <footer
      style={{
        backgroundColor: "#1c1c20",
        color: "#c9c9cd",
        padding: "50px 28px 56px",
        backgroundImage:
          "linear-gradient(rgba(255,255,255,0.03) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,0.03) 1px,transparent 1px)",
        backgroundSize: "42px 42px",
      }}
    >
      <div
        style={{
          maxWidth: 1180,
          margin: "0 auto",
          display: "flex",
          flexWrap: "wrap",
          gap: 28,
          justifyContent: "space-between",
          alignItems: "flex-start",
        }}
      >
        <div style={{ maxWidth: "42ch" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 11 }}>
            <span
              style={{
                width: 21,
                height: 21,
                borderRadius: 6,
                background: "#ccccd3",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <span
                style={{
                  width: 7,
                  height: 7,
                  borderRadius: "50%",
                  background: "#1c1c20",
                }}
              />
            </span>
            <span
              style={{
                fontFamily: "'Bricolage Grotesque',sans-serif",
                fontSize: 16,
                color: "#f5f5f6",
                fontWeight: 600,
              }}
            >
              OZ Policy Builder
            </span>
          </div>
          <p
            style={{
              margin: "14px 0 0",
              color: "#b2b2b7",
              fontSize: 13.5,
              lineHeight: 1.6,
            }}
          >
            Records a Stellar transaction and synthesizes the smallest OpenZeppelin
            smart-account policy that would permit exactly that transaction, and nothing
            more. Open source, Apache-2.0.
          </p>
        </div>
        <div style={{ display: "flex", gap: 52, flexWrap: "wrap" }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 11 }}>
            <span
              style={{
                fontFamily: "'JetBrains Mono',monospace",
                fontSize: 10.5,
                letterSpacing: "0.06em",
                textTransform: "uppercase",
                color: "#717177",
              }}
            >
              project
            </span>
            <a
              href="https://github.com/ErenTopaal/oz-policy-builder"
              target="_blank"
              rel="noreferrer"
              className="footer-link"
              style={{ textDecoration: "none", color: "#cfcfd1", fontSize: 13.5 }}
            >
              GitHub ↗
            </a>
            <a
              href="https://docs.policy.erentopal.xyz"
              target="_blank"
              rel="noreferrer"
              className="footer-link"
              style={{ textDecoration: "none", color: "#cfcfd1", fontSize: 13.5 }}
            >
              Documentation ↗
            </a>
            <a
              href="https://github.com/ErenTopaal/oz-policy-builder/blob/main/LICENSE-APACHE"
              target="_blank"
              rel="noreferrer"
              className="footer-link"
              style={{ textDecoration: "none", color: "#cfcfd1", fontSize: 13.5 }}
            >
              License · Apache-2.0
            </a>
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 11 }}>
            <span
              style={{
                fontFamily: "'JetBrains Mono',monospace",
                fontSize: 10.5,
                letterSpacing: "0.06em",
                textTransform: "uppercase",
                color: "#717177",
              }}
            >
              built on
            </span>
            <a
              href="#"
              className="footer-link"
              style={{ textDecoration: "none", color: "#cfcfd1", fontSize: 13.5 }}
            >
              Stellar ↗
            </a>
            <a
              href="#"
              className="footer-link"
              style={{ textDecoration: "none", color: "#cfcfd1", fontSize: 13.5 }}
            >
              Soroban ↗
            </a>
            <a
              href="#"
              className="footer-link"
              style={{ textDecoration: "none", color: "#cfcfd1", fontSize: 13.5 }}
            >
              OpenZeppelin smart accounts ↗
            </a>
          </div>
        </div>
      </div>
    </footer>
  );
}
