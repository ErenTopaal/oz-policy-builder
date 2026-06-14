// Landing showcase for the /playground route. Replaces the embedded
// Synthesizer widget — the live tool now lives at its own route at /playground,
// so this section's job is to explain the workflow and drive clicks to it.
//
// Matches the existing landing's light-theme system (Bricolage / Hanken / JetBrains)
// and the section pattern background used by other interactive sections.

import { Link } from "react-router-dom";

const STAGES: Array<{
  step: string;
  title: string;
  body: string;
  hint: string;
}> = [
  {
    step: "01",
    title: "Record a transaction",
    body: "Paste a 64-char hash, an unsubmitted envelope XDR, or pick one of the curated testnet presets — Blend yield-claim, SEP-41 transfer, Soroswap swap.",
    hint: "record_transaction · MCP",
  },
  {
    step: "02",
    title: "Inspect the synthesizer's output",
    body: "Read the proposed context rule + policy slots as a structured tree, with reasoning traces explaining which recorded field each constraint came from.",
    hint: "synthesize_policy · get_policy_artifacts",
  },
  {
    step: "03",
    title: "Edit, re-simulate, share",
    body: "Open the generated Rust in Monaco, modify it, and re-run the permit + auto-mutated deny harness server-side. Share any state as a stable, hydratable URL.",
    hint: "simulate_custom_source · create_snapshot",
  },
];

export function PlaygroundShowcase() {
  return (
    <section
      id="synthesize"
      style={{
        backgroundColor: "#dfdfe1",
        backgroundImage:
          "radial-gradient(rgba(28,28,33,0.06) 1px,transparent 1px)",
        backgroundSize: "24px 24px",
      }}
    >
      <div
        style={{
          maxWidth: 1180,
          margin: "0 auto",
          padding: "clamp(60px,8vw,100px) 28px",
          display: "flex",
          flexDirection: "column",
          gap: 56,
        }}
      >
        {/* header row */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "minmax(0,1fr) auto",
            gap: 32,
            alignItems: "end",
            flexWrap: "wrap",
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 14, minWidth: 0 }}>
            <span
              style={{
                fontFamily: "'JetBrains Mono', monospace",
                fontSize: 12,
                letterSpacing: "0.08em",
                textTransform: "uppercase",
                color: "#1c1c20",
              }}
            >
              interactive · /playground
            </span>
            <h2
              style={{
                margin: 0,
                fontFamily: "'Bricolage Grotesque', sans-serif",
                fontSize: "clamp(34px,4.2vw,54px)",
                fontWeight: 500,
                letterSpacing: "-0.02em",
                lineHeight: 1.05,
                color: "#1d1d1e",
              }}
            >
              An interactive review surface for every generated policy.
            </h2>
            <p
              style={{
                margin: 0,
                maxWidth: "62ch",
                color: "#54545a",
                fontSize: 17,
                lineHeight: 1.6,
              }}
            >
              Take a real Stellar transaction. Get back the minimum-rights policy that would
              permit it — and only it. Read the spec, edit the Rust, watch the permit + deny
              harness re-run live, then walk away with a reviewable bundle. No browser-side
              deploys, no mock data.
            </p>
          </div>
          <Link
            to="/playground"
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 10,
              alignSelf: "end",
              textDecoration: "none",
              background: "#1c1c20",
              color: "#fbfbfb",
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 14,
              fontWeight: 600,
              padding: "16px 26px",
              borderRadius: 13,
              boxShadow: "0 14px 36px -16px rgba(22,24,21,0.55)",
              whiteSpace: "nowrap",
            }}
          >
            Open the playground
            <span style={{ fontSize: 16 }}>↗</span>
          </Link>
        </div>

        {/* stage cards */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
            gap: 18,
          }}
        >
          {STAGES.map((s) => (
            <article
              key={s.step}
              style={{
                background: "#fbfbfb",
                borderRadius: 18,
                padding: "26px 26px 22px",
                display: "flex",
                flexDirection: "column",
                gap: 14,
                boxShadow: "0 12px 34px -22px rgba(22,24,21,0.4)",
                minHeight: 230,
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 12,
                }}
              >
                <span
                  style={{
                    fontFamily: "'JetBrains Mono', monospace",
                    fontSize: 12,
                    letterSpacing: "0.04em",
                    color: "#a0a0a8",
                  }}
                >
                  {s.step}
                </span>
                <span
                  style={{
                    fontFamily: "'JetBrains Mono', monospace",
                    fontSize: 10.5,
                    color: "#54545a",
                    background: "rgba(28,28,33,0.06)",
                    padding: "4px 9px",
                    borderRadius: 7,
                  }}
                >
                  {s.hint}
                </span>
              </div>
              <h3
                style={{
                  margin: 0,
                  fontFamily: "'Bricolage Grotesque', sans-serif",
                  fontSize: 22,
                  fontWeight: 500,
                  letterSpacing: "-0.01em",
                  color: "#1d1d1e",
                  lineHeight: 1.2,
                }}
              >
                {s.title}
              </h3>
              <p
                style={{
                  margin: 0,
                  color: "#54545a",
                  fontSize: 14.5,
                  lineHeight: 1.55,
                }}
              >
                {s.body}
              </p>
            </article>
          ))}
        </div>

        {/* preview card */}
        <PreviewMock />
      </div>
    </section>
  );
}

// A stylized, static mock of the /playground UI — pure presentation, no live
// data and not a link (the visible CTA above is the click affordance).
function PreviewMock() {
  return (
    <div
      style={{
        borderRadius: 22,
        overflow: "hidden",
        background: "#1c1c20",
        backgroundImage:
          "linear-gradient(rgba(255,255,255,0.03) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,0.03) 1px,transparent 1px)",
        backgroundSize: "42px 42px",
        boxShadow: "0 30px 80px -40px rgba(22,24,21,0.6)",
        color: "#f4f4f5",
      }}
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "minmax(0,340px) minmax(0,1fr)",
          gap: 20,
          padding: 22,
          minHeight: 320,
        }}
      >
        {/* aside mock */}
        <div
          style={{
            background: "#26262b",
            borderRadius: 14,
            padding: 18,
            display: "flex",
            flexDirection: "column",
            gap: 14,
          }}
        >
          <span
            style={{
              fontFamily: "'Bricolage Grotesque', sans-serif",
              fontSize: 16,
              fontWeight: 600,
              letterSpacing: "-0.01em",
            }}
          >
            Synthesize a policy
          </span>
          <SegMock label="input" options={["hash", "envelope xdr"]} active={0} />
          <div
            style={{
              background: "#2f2f35",
              borderRadius: 11,
              padding: "13px",
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 12.5,
              color: "#cfcfd6",
            }}
          >
            5a0ccffe&hellip;a42db4e
          </div>
          <SegMock label="network" options={["testnet", "mainnet"]} active={0} />
          <SegMock label="tightness" options={["exact", "margin", "loose"]} active={0} />
          <div
            style={{
              background: "#f0f0f3",
              color: "#1c1c20",
              fontFamily: "'JetBrains Mono', monospace",
              fontWeight: 600,
              fontSize: 13,
              padding: "13px",
              borderRadius: 11,
              textAlign: "center",
            }}
          >
            synthesize
          </div>
        </div>
        {/* output mock */}
        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <div
            style={{
              display: "inline-flex",
              gap: 4,
              background: "#2f2f35",
              padding: 5,
              borderRadius: 13,
              alignSelf: "flex-start",
            }}
          >
            {["Spec", "Source", "Simulate", "Bundle"].map((t, i) => (
              <span
                key={t}
                style={{
                  background: i === 1 ? "#34343b" : "transparent",
                  color: i === 1 ? "#f4f4f5" : "#b2b2b8",
                  fontFamily: "'JetBrains Mono', monospace",
                  fontSize: 12,
                  fontWeight: i === 1 ? 600 : 500,
                  padding: "8px 14px",
                  borderRadius: 9,
                }}
              >
                {t}
              </span>
            ))}
          </div>
          <div
            style={{
              background: "#141417",
              borderRadius: 14,
              padding: 16,
              display: "flex",
              flexDirection: "column",
              gap: 10,
              flex: 1,
            }}
          >
            <div
              style={{
                display: "flex",
                gap: 10,
                alignItems: "center",
                fontFamily: "'JetBrains Mono', monospace",
                fontSize: 11.5,
              }}
            >
              <span style={{ color: "#cfcfd6", fontWeight: 600 }}>src/lib.rs</span>
              <span style={{ color: "#7d7d86" }}>editable</span>
            </div>
            <pre
              style={{
                margin: 0,
                fontFamily: "'JetBrains Mono', monospace",
                fontSize: 12,
                lineHeight: 1.6,
                color: "#f6f6f8",
                whiteSpace: "pre",
                overflow: "hidden",
              }}
            >
              {`#![no_std]
use soroban_sdk::{contract, contractimpl, panic_with_error, Env};
use stellar_accounts::smart_account::{Context, ContextRule};

#[contract]
pub struct FunctionAllowlistPolicy;

#[contractimpl]
impl FunctionAllowlistPolicy {
    pub fn enforce(env: Env, ctx: Context, rule: ContextRule) {
        if ctx.fn_name != symbol_short!("claim") {
            panic_with_error!(&env, PolicyError::FunctionNotAllowed);
        }
    }
}`}
            </pre>
          </div>
        </div>
      </div>
    </div>
  );
}

function SegMock({
  label,
  options,
  active,
}: {
  label: string;
  options: string[];
  active: number;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <span
        style={{
          fontFamily: "'JetBrains Mono', monospace",
          fontSize: 10,
          letterSpacing: "0.05em",
          textTransform: "uppercase",
          color: "#8e8e96",
        }}
      >
        {label}
      </span>
      <div
        style={{
          display: "flex",
          background: "#2f2f35",
          padding: 4,
          borderRadius: 11,
          gap: 4,
        }}
      >
        {options.map((o, i) => (
          <span
            key={o}
            style={{
              flex: 1,
              background: i === active ? "#f0f0f3" : "transparent",
              color: i === active ? "#1c1c20" : "#b2b2b8",
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 11.5,
              fontWeight: i === active ? 600 : 500,
              padding: "7px 4px",
              borderRadius: 8,
              textAlign: "center",
            }}
          >
            {o}
          </span>
        ))}
      </div>
    </div>
  );
}
