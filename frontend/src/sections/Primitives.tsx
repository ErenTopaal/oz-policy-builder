import { useState } from "react";

const items: Array<[string, string, string]> = [
  ["function_allowlist", "Restrict to a named set of functions.", 'functions: ["claim"]'],
  [
    "argument_pattern",
    "Pin specific argument slots to exact typed values.",
    "slot[0] == Address(GATJIJRQ…)",
  ],
  [
    "amount_range",
    "Clamp an i128 amount to an observed range.",
    "0 ≤ amount ≤ 50_000000",
  ],
  [
    "asset_allowlist",
    "Whitelist the contract addresses that may be touched.",
    "asset ∈ { USDC }",
  ],
  [
    "time_window",
    "Permit only within a ledger-sequence window.",
    "ledger ∈ [2572326, 3004326]",
  ],
  [
    "call_frequency",
    "Stateful rate limit, N calls per window.",
    "≤ 1 call / 17280 ledgers",
  ],
  [
    "sequence_ordering",
    "Enforce a phased order of operations.",
    "claim → swap → transfer",
  ],
];

export function Primitives() {
  const [open, setOpen] = useState(-1);
  return (
    <section
      id="primitives"
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
            marginBottom: 34,
            maxWidth: "66ch",
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
            constraint primitives
          </span>
          <h2
            style={{
              margin: 0,
              fontFamily: "'Bricolage Grotesque',sans-serif",
              fontSize: "clamp(28px,3.5vw,46px)",
              fontWeight: 500,
              letterSpacing: "-0.02em",
              color: "#1d1d1e",
            }}
          >
            Seven ways to bound a call
          </h2>
          <p
            style={{
              margin: 0,
              color: "#54545a",
              fontSize: 16.5,
              lineHeight: 1.6,
            }}
          >
            When a standard OZ primitive can't express the constraint, the synthesizer
            emits one of these audit-gated templates, composing them to permit exactly the
            observed flow. Tap any to see what it pins down.
          </p>
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill,minmax(248px,1fr))",
            gap: 14,
          }}
        >
          {items.map(([name, desc, code], i) => {
            const isOpen = open === i;
            return (
              <div
                key={name}
                onClick={() => setOpen(isOpen ? -1 : i)}
                style={{
                  background: "#eceef1",
                  borderRadius: 14,
                  padding: 18,
                  cursor: "pointer",
                  boxShadow: isOpen
                    ? "0 14px 30px -16px rgba(28,28,33,0.4)"
                    : "0 3px 12px -7px rgba(22,24,21,0.22)",
                  transition: "box-shadow .25s, transform .25s",
                  transform: isOpen ? "translateY(-2px)" : "none",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 8,
                  }}
                >
                  <span
                    style={{
                      fontFamily: "'JetBrains Mono',monospace",
                      fontSize: 13.5,
                      color: "#1d1d1e",
                      fontWeight: 600,
                    }}
                  >
                    {name}
                  </span>
                  <span
                    style={{
                      fontFamily: "'JetBrains Mono',monospace",
                      fontSize: 15,
                      color: "#797980",
                      transition: "transform .25s",
                      transform: isOpen ? "rotate(45deg)" : "none",
                    }}
                  >
                    +
                  </span>
                </div>
                <div
                  style={{
                    marginTop: 7,
                    color: "#54545a",
                    fontSize: 13.5,
                    lineHeight: 1.5,
                  }}
                >
                  {desc}
                </div>
                <div
                  style={{
                    maxHeight: isOpen ? 70 : 0,
                    opacity: isOpen ? 1 : 0,
                    overflow: "hidden",
                    transition: "max-height .35s ease, opacity .3s, margin-top .35s",
                    marginTop: isOpen ? 12 : 0,
                  }}
                >
                  <div
                    style={{
                      background: "#161619",
                      color: "#f6f6f8",
                      borderRadius: 9,
                      padding: "10px 12px",
                      fontFamily: "'JetBrains Mono',monospace",
                      fontSize: 12,
                    }}
                  >
                    {code}
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
