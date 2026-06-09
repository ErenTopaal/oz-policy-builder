import { useState } from "react";

const qs: Array<[string, string]> = [
  [
    "Does this deploy policies for me?",
    "No. It produces reviewable policy code; deployment is always a separate, explicit step you (or an agent acting under existing permissions) take.",
  ],
  [
    "Is the generated code real Soroban?",
    "Yes, compilable Rust implementing the Policy trait, gated by five audit lints and compiled in a sandbox before you ever see a WASM hash.",
  ],
  [
    "Does it reuse existing OZ primitives?",
    "It composes simple_threshold, weighted_threshold and spending_limit first, and only generates a fresh policy contract when a constraint can’t be expressed by composition.",
  ],
  [
    "What if the synthesized policy is wrong?",
    "The dry-run harness replays the original transaction as a permit case and generates deny vectors, wrong function, asset, amount, timing, so you see pass/fail before installing.",
  ],
  [
    "Which wallets are supported?",
    "SEP-43 wallets through the TypeScript adapter, Freighter and passkey-kit today, with installPolicy + verifyInstall orchestration.",
  ],
  [
    "Is it open source?",
    "Yes, Apache-2.0 across the Rust workspace, MCP server, agent skill and wallet adapter.",
  ],
];

export function Faq() {
  const [open, setOpen] = useState(-1);
  return (
    <section
      id="faq"
      style={{
        backgroundColor: "#fbfbfb",
        backgroundImage:
          "linear-gradient(rgba(28,28,33,0.025) 1px,transparent 1px),linear-gradient(90deg,rgba(28,28,33,0.025) 1px,transparent 1px)",
        backgroundSize: "46px 46px",
      }}
    >
      <div
        style={{
          maxWidth: 880,
          margin: "0 auto",
          padding: "clamp(60px,8vw,100px) 28px",
        }}
      >
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 13,
            marginBottom: 34,
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
            questions
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
            What it does, and doesn't, do
          </h2>
        </div>
        <div>
          {qs.map(([q, a], i) => {
            const isOpen = open === i;
            return (
              <div
                key={q}
                style={{ borderBottom: "1px solid rgba(28,28,33,0.1)" }}
              >
                <button
                  onClick={() => setOpen(isOpen ? -1 : i)}
                  style={{
                    width: "100%",
                    background: "transparent",
                    border: "none",
                    cursor: "pointer",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 14,
                    padding: "20px 4px",
                    textAlign: "left",
                  }}
                >
                  <span
                    style={{
                      fontFamily: "'Bricolage Grotesque',sans-serif",
                      fontSize: 17,
                      color: "#1d1d1e",
                      fontWeight: 500,
                    }}
                  >
                    {q}
                  </span>
                  <span
                    style={{
                      fontFamily: "'JetBrains Mono',monospace",
                      fontSize: 18,
                      color: "#797980",
                      transition: "transform .25s",
                      transform: isOpen ? "rotate(45deg)" : "none",
                      flexShrink: 0,
                    }}
                  >
                    +
                  </span>
                </button>
                <div
                  style={{
                    maxHeight: isOpen ? 200 : 0,
                    opacity: isOpen ? 1 : 0,
                    overflow: "hidden",
                    transition: "max-height .4s ease, opacity .3s, padding .4s",
                    padding: isOpen ? "0 4px 22px" : "0 4px",
                  }}
                >
                  <div
                    style={{
                      color: "#54545a",
                      fontSize: 15,
                      lineHeight: 1.6,
                      maxWidth: "70ch",
                    }}
                  >
                    {a}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
