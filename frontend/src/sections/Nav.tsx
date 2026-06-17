import { Link } from "react-router-dom";

export function Nav() {
  return (
    <nav
      style={{
        position: "sticky",
        top: 0,
        zIndex: 50,
        backdropFilter: "blur(13px)",
        WebkitBackdropFilter: "blur(13px)",
        background: "rgba(223,224,226,0.84)",
        boxShadow: "0 1px 0 rgba(22,24,21,0.07)",
      }}
    >
      <div
        style={{
          maxWidth: 1180,
          margin: "0 auto",
          padding: "15px 28px",
          display: "flex",
          alignItems: "center",
          gap: 16,
          flexWrap: "wrap",
        }}
      >
        <a
          href="#top"
          style={{ display: "flex", alignItems: "center", gap: 11, textDecoration: "none" }}
        >
          <span
            style={{
              width: 22,
              height: 22,
              borderRadius: 7,
              background: "#1c1c20",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <span
              style={{ width: 8, height: 8, borderRadius: "50%", background: "#dbdbde" }}
            />
          </span>
          <span
            style={{
              fontFamily: "'Bricolage Grotesque',sans-serif",
              fontSize: 16,
              color: "#1d1d1e",
              fontWeight: 600,
              letterSpacing: "-0.01em",
            }}
          >
            OZ Policy Builder
          </span>
        </a>
        <span
          style={{
            fontFamily: "'JetBrains Mono',monospace",
            fontSize: 10.5,
            color: "#606066",
            background: "rgba(28,28,33,0.09)",
            padding: "3px 8px",
            borderRadius: 6,
          }}
        >
          Apache-2.0
        </span>
        <div style={{ flex: 1 }} />
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 28,
            flexWrap: "wrap",
          }}
        >
          <a
            href="#how"
            className="nav-link"
            style={{ textDecoration: "none", color: "#55555b", fontSize: 14.5, fontWeight: 500 }}
          >
            How it works
          </a>
          <Link
            to="/playground"
            className="nav-link"
            style={{ textDecoration: "none", color: "#55555b", fontSize: 14.5, fontWeight: 500 }}
          >
            Playground
          </Link>
          <a
            href="#quickstart"
            className="nav-link"
            style={{ textDecoration: "none", color: "#55555b", fontSize: 14.5, fontWeight: 500 }}
          >
            Quick start
          </a>
          <a
            href="#architecture"
            className="nav-link"
            style={{ textDecoration: "none", color: "#55555b", fontSize: 14.5, fontWeight: 500 }}
          >
            Architecture
          </a>
          <a
            href="https://github.com/ErenTopaal/oz-policy-builder"
            target="_blank"
            rel="noreferrer"
            className="btn-dark"
            style={{
              textDecoration: "none",
              color: "#f4f4f5",
              fontSize: 13,
              fontFamily: "'JetBrains Mono',monospace",
              background: "#1c1c20",
              padding: "9px 16px",
              borderRadius: 9,
            }}
          >
            GitHub ↗
          </a>
        </div>
      </div>
    </nav>
  );
}
