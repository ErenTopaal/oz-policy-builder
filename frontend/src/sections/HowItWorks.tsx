const steps: Array<{ n: string; title: string; body: string }> = [
  {
    n: "01",
    title: "record",
    body: "Pull a transaction by hash, or simulate one, and decode every contract call, argument, asset movement and state change into a typed Recording.",
  },
  {
    n: "02",
    title: "synthesize",
    body: "Derive the tightest context rule plus the minimum set of policies that permit exactly that flow. Bias toward least privilege, never a function or asset that wasn't observed.",
  },
  {
    n: "03",
    title: "codegen",
    body: "Compose existing OZ primitives first; emit real, compilable Soroban policy contracts from audit-gated templates only where a primitive can't express the constraint.",
  },
  {
    n: "04",
    title: "simulate",
    body: "Replay the recording as a permit case, then generate deny vectors, wrong function, wrong asset, over-limit amount, out-of-window timing, that the policy must reject.",
  },
  {
    n: "05",
    title: "export",
    body: "Produce a wallet-signable install envelope as XDR. The tool builds and preflights it, it never submits anything on your behalf.",
  },
  {
    n: "06",
    title: "install",
    body: "Sign with your wallet, install the context rule on your smart account, then read it back from chain and verify it matches what you reviewed.",
  },
];

export function HowItWorks() {
  return (
    <section
      id="how"
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
            marginBottom: 52,
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
            how it works
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
            From a transaction to an installed policy
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
            Six deterministic stages. Each has a public command and a typed JSON output,
            nothing happens on-chain until the final, explicit install step.
          </p>
        </div>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "clamp(32px,5vw,60px)",
            alignItems: "flex-start",
          }}
        >
          <div style={{ position: "relative", flex: "1 1 440px", minWidth: 300 }}>
            <div
              style={{
                position: "absolute",
                left: 23,
                top: 30,
                bottom: 30,
                width: 2,
                background: "rgba(28,28,33,0.13)",
                borderRadius: 2,
              }}
            />
            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              {steps.map((s) => (
                <div
                  key={s.n}
                  className="howstep"
                  style={{
                    display: "grid",
                    gridTemplateColumns: "48px 1fr",
                    gap: 24,
                    alignItems: "start",
                    padding: "16px 0",
                  }}
                >
                  <div
                    style={{
                      position: "relative",
                      zIndex: 1,
                      width: 48,
                      height: 48,
                      borderRadius: "50%",
                      background: "#1c1c20",
                      color: "#f4f4f5",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      fontFamily: "'JetBrains Mono',monospace",
                      fontSize: 13,
                      fontWeight: 600,
                      boxShadow: "0 6px 16px -8px rgba(28,28,33,0.8)",
                    }}
                  >
                    {s.n}
                  </div>
                  <div style={{ paddingTop: 4 }}>
                    <div
                      style={{
                        fontFamily: "'JetBrains Mono',monospace",
                        fontSize: 16,
                        color: "#1d1d1e",
                        fontWeight: 600,
                      }}
                    >
                      {s.title}
                    </div>
                    <div
                      style={{
                        marginTop: 5,
                        color: "#54545a",
                        fontSize: 14.5,
                        lineHeight: 1.55,
                        maxWidth: "58ch",
                      }}
                    >
                      {s.body}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
          <div style={{ flex: "1 1 320px", minWidth: 280, alignSelf: "stretch" }}>
            <div
              style={{
                position: "sticky",
                top: 96,
                background: "#1c1c20",
                borderRadius: 18,
                padding: "32px 28px",
                boxShadow: "0 16px 40px -22px rgba(28,28,33,0.6)",
              }}
            >
              <div
                style={{
                  fontFamily: "'Bricolage Grotesque',sans-serif",
                  fontSize: "clamp(28px,3vw,36px)",
                  lineHeight: 1.05,
                  color: "#f3f3f8",
                  fontWeight: 600,
                  letterSpacing: "-0.025em",
                }}
              >
                No black box.
              </div>
              <div
                style={{
                  marginTop: 14,
                  color: "#b2b2b8",
                  fontSize: 15,
                  lineHeight: 1.55,
                }}
              >
                Every stage prints typed JSON you can read and verify.
              </div>
              <div
                style={{
                  marginTop: 30,
                  display: "flex",
                  flexDirection: "column",
                  gap: 22,
                }}
              >
                {[
                  ["record", "what happened"],
                  ["synthesize", "the policy"],
                  ["simulate", "the proof"],
                ].map(([k, v]) => (
                  <div
                    key={k}
                    style={{
                      display: "flex",
                      alignItems: "baseline",
                      gap: 16,
                    }}
                  >
                    <span
                      style={{
                        fontFamily: "'JetBrains Mono',monospace",
                        fontSize: 11,
                        color: "#6e6e76",
                        fontWeight: 600,
                        width: 80,
                        flexShrink: 0,
                        textTransform: "uppercase",
                        letterSpacing: "0.05em",
                      }}
                    >
                      {k}
                    </span>
                    <span
                      style={{
                        color: "#f3f3f8",
                        fontSize: 19,
                        lineHeight: 1.25,
                        fontWeight: 500,
                      }}
                    >
                      {v}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
