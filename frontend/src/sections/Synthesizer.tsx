import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { McpClient, McpError, readConfig, isLive, describeError } from "../lib/mcp";
import type {
  Network,
  Tightness,
  SynthesisMode,
  Recording,
  PolicySpec,
  SimReport,
} from "../lib/types";
import { Field, FieldHeader, FieldLabel } from "./fields";

// sample tx hash for the "try a sample" button is fetched at mount from
// /sample-hash.txt, which a server-side hourly job refreshes from stellar
// testnet horizon. if the fetch fails, the sample button stays disabled —
// no stale or fabricated hash is ever shown.
const SAMPLE_HASH_URL = "/sample-hash.txt";

type Phase =
  | { kind: "idle" }
  | { kind: "recording" }
  | { kind: "synthesizing" }
  | { kind: "simulating" }
  | {
      kind: "success";
      recording: Recording;
      spec: PolicySpec;
      report: SimReport;
    }
  | { kind: "error"; code: string; detail: string };

type BackendStatus =
  | { kind: "checking" }
  | { kind: "live" }
  | { kind: "down"; reason: string };

const TIGHTNESS_HELP: Record<Tightness, string> = {
  exact: "constraints pin observed values exactly. no slack.",
  small_margin: "numeric ranges scale 1.1×. asset / function sets stay exact.",
  loose: "numeric ranges scale 2×. more agent flexibility, less tight bound.",
};

export function Synthesizer() {
  const [txHash, setTxHash] = useState("");
  const [network, setNetwork] = useState<Network>("testnet");
  const [tightness, setTightness] = useState<Tightness>("exact");
  const [mode, setMode] = useState<SynthesisMode>("auto");
  const [lifetime, setLifetime] = useState(432000);
  const [ruleName, setRuleName] = useState("");

  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [backend, setBackend] = useState<BackendStatus>({ kind: "checking" });
  const [sampleHash, setSampleHash] = useState<string | null>(null);

  const cfg = useMemo(() => readConfig(), []);
  const cancelRef = useRef<AbortController | null>(null);

  // health-check on mount. real, honest. no caching.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      if (!cfg.endpoint) {
        if (!cancelled)
          setBackend({
            kind: "down",
            reason:
              "mcp backend endpoint isn't configured. set VITE_MCP_ENDPOINT (and VITE_MCP_TOKEN if your endpoint requires bearer auth) and reload.",
          });
        return;
      }
      const ok = await isLive(cfg);
      if (cancelled) return;
      setBackend(
        ok
          ? { kind: "live" }
          : {
              kind: "down",
              reason:
                "mcp backend is not reachable. the live synthesizer is disabled until the endpoint responds.",
            }
      );
    })();
    return () => {
      cancelled = true;
    };
  }, [cfg]);

  // fetch the current sample hash. file is refreshed hourly server-side
  // from horizon. on failure (404, network error, invalid format) leave
  // sampleHash null so the button stays disabled.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const r = await fetch(SAMPLE_HASH_URL, { cache: "no-store" });
        if (!r.ok) return;
        const text = (await r.text()).trim();
        if (!/^[0-9a-f]{64}$/.test(text)) return;
        if (!cancelled) setSampleHash(text);
      } catch {
        // network failure — leave button disabled, no fallback
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const isBusy =
    phase.kind === "recording" || phase.kind === "synthesizing" || phase.kind === "simulating";
  const isFormDisabled = backend.kind !== "live" || isBusy;
  const isSubmitDisabled =
    isFormDisabled || !/^[0-9a-fA-F]{64}$/.test(txHash.trim());

  const sampleDisabled = isFormDisabled || !sampleHash;
  const loadSample = useCallback(() => {
    if (sampleHash) setTxHash(sampleHash);
  }, [sampleHash]);

  const cancel = useCallback(() => {
    cancelRef.current?.abort();
    setPhase({ kind: "idle" });
  }, []);

  const doSynth = useCallback(async () => {
    if (!cfg.endpoint) return;
    cancelRef.current?.abort();
    cancelRef.current = new AbortController();

    const client = new McpClient(cfg);
    try {
      setPhase({ kind: "recording" });
      const rec = await client.recordTransaction({ network, hash: txHash.trim().toLowerCase() });

      setPhase({ kind: "synthesizing" });
      const synth = await client.synthesizePolicy({
        recording_id: rec.recording_id,
        tightness,
        mode,
        lifetime_ledgers: lifetime,
        rule_name: ruleName.trim() || undefined,
      });

      setPhase({ kind: "simulating" });
      const report = await client.simulatePolicy({
        spec_id: synth.spec_id,
        recording_id: rec.recording_id,
      });

      setPhase({ kind: "success", recording: rec.recording, spec: synth.spec, report });
    } catch (e: unknown) {
      const code = e instanceof McpError ? e.code : "E_UNKNOWN";
      const detail =
        e instanceof McpError
          ? e.detail
          : e instanceof Error
          ? e.message
          : "unknown error";
      setPhase({ kind: "error", code, detail });
    }
  }, [cfg, network, txHash, tightness, mode, lifetime, ruleName]);

  const submitLabel = (() => {
    if (backend.kind === "checking") return "checking backend…";
    if (backend.kind === "down") return "live mode unavailable";
    switch (phase.kind) {
      case "recording":
        return "recording transaction…";
      case "synthesizing":
        return "synthesizing policy…";
      case "simulating":
        return "simulating permit + deny…";
      default:
        return "synthesize";
    }
  })();

  return (
    <section
      id="synthesize"
      style={{
        backgroundColor: "#dfdfe1",
        backgroundImage: "radial-gradient(rgba(28,28,33,0.06) 1px,transparent 1px)",
        backgroundSize: "24px 24px",
      }}
    >
      <div style={{ maxWidth: 1180, margin: "0 auto", padding: "clamp(60px,8vw,100px) 28px" }}>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 20,
            alignItems: "flex-end",
            justifyContent: "space-between",
            marginBottom: 38,
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
            <span
              style={{
                fontFamily: "'JetBrains Mono', monospace",
                fontSize: 12,
                letterSpacing: "0.08em",
                textTransform: "uppercase",
                color: "#1c1c20",
              }}
            >
              interactive
            </span>
            <h2
              style={{
                margin: 0,
                fontFamily: "'Bricolage Grotesque', sans-serif",
                fontSize: "clamp(28px,3.5vw,46px)",
                fontWeight: 500,
                letterSpacing: "-0.02em",
                color: "#1d1d1e",
              }}
            >
              Synthesize a policy from a transaction
            </h2>
            <p
              style={{
                margin: 0,
                maxWidth: "60ch",
                color: "#54545a",
                fontSize: 16.5,
                lineHeight: 1.6,
              }}
            >
              Enter a transaction hash and synthesize. You'll get the recorded transaction, the
              proposed policy, and a simulation report with permit and deny vectors.
            </p>
          </div>
          <BackendBadge status={backend} />
        </div>

        <div style={{ display: "flex", flexWrap: "wrap", gap: 20, alignItems: "flex-start" }}>
          <div
            style={{
              flex: "1.05 1 420px",
              minWidth: 360,
              alignSelf: "flex-start",
              position: "sticky",
              top: 90,
              background: "#fbfbfb",
              borderRadius: 16,
              padding: 26,
              display: "flex",
              flexDirection: "column",
              gap: 18,
              boxShadow: "0 12px 34px -20px rgba(22,24,21,0.35)",
            }}
          >
            <Field>
              <FieldHeader>
                <FieldLabel>transaction hash</FieldLabel>
                <button
                  onClick={loadSample}
                  disabled={sampleDisabled}
                  title={!sampleHash ? "sample hash unavailable" : "load a fresh testnet hash"}
                  style={{
                    background: "rgba(28,28,33,0.06)",
                    border: "none",
                    cursor: sampleDisabled ? "not-allowed" : "pointer",
                    fontFamily: "'JetBrains Mono', monospace",
                    fontSize: 10.5,
                    color: "#1c1c20",
                    padding: "5px 9px",
                    borderRadius: 7,
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 5,
                    opacity: sampleDisabled ? 0.5 : 1,
                  }}
                >
                  ↺ try a sample
                </button>
              </FieldHeader>
              <input
                value={txHash}
                onChange={(e) => setTxHash(e.target.value)}
                disabled={isFormDisabled}
                placeholder="64-char hex transaction hash"
                className="input-focus"
                style={inputBig}
              />
            </Field>

            <Field>
              <FieldLabel>network</FieldLabel>
              <Segments
                options={[
                  { value: "testnet", label: "testnet" },
                  { value: "mainnet", label: "mainnet" },
                ]}
                value={network}
                onChange={(v) => setNetwork(v as Network)}
                disabled={isFormDisabled}
              />
            </Field>

            <div style={{ height: 1, background: "rgba(28,28,33,0.08)" }} />

            <Field>
              <FieldLabel>
                tightness
                <span style={{ color: "#a0a0a6", textTransform: "none", letterSpacing: 0 }}>
                  {" "}
                  · how tightly to bound
                </span>
              </FieldLabel>
              <Segments
                options={[
                  { value: "exact", label: "exact" },
                  { value: "small_margin", label: "small margin" },
                  { value: "loose", label: "loose" },
                ]}
                value={tightness}
                onChange={(v) => setTightness(v as Tightness)}
                disabled={isFormDisabled}
              />
              <span
                style={{
                  fontSize: 11.5,
                  color: "#797980",
                  lineHeight: 1.45,
                  fontFamily: "'JetBrains Mono', monospace",
                }}
              >
                {TIGHTNESS_HELP[tightness]}
              </span>
            </Field>

            <Field>
              <FieldLabel>synthesis mode</FieldLabel>
              <Segments
                options={[
                  { value: "auto", label: "auto" },
                  { value: "compose_only", label: "compose only" },
                  { value: "codegen_only", label: "codegen only" },
                ]}
                value={mode}
                onChange={(v) => setMode(v as SynthesisMode)}
                disabled={isFormDisabled}
              />
            </Field>

            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
              <label style={{ display: "flex", flexDirection: "column", gap: 7 }}>
                <FieldLabel>lifetime · ledgers</FieldLabel>
                <input
                  type="number"
                  value={lifetime}
                  onChange={(e) => setLifetime(Number(e.target.value) || 0)}
                  disabled={isFormDisabled}
                  className="input-focus"
                  style={inputSmall}
                />
              </label>
              <label style={{ display: "flex", flexDirection: "column", gap: 7 }}>
                <FieldLabel>
                  rule name<span style={{ color: "#99999e", textTransform: "none" }}> · opt</span>
                </FieldLabel>
                <input
                  value={ruleName}
                  onChange={(e) => setRuleName(e.target.value)}
                  disabled={isFormDisabled}
                  placeholder="auto"
                  className="input-focus"
                  style={inputSmall}
                />
              </label>
            </div>

            {isBusy ? (
              <button onClick={cancel} className="btn-dark" style={submitBtn}>
                cancel
              </button>
            ) : (
              <button
                onClick={doSynth}
                disabled={isSubmitDisabled}
                className="btn-dark"
                style={{
                  ...submitBtn,
                  opacity: isSubmitDisabled ? 0.5 : 1,
                  cursor: isSubmitDisabled ? "not-allowed" : "pointer",
                }}
              >
                {submitLabel}
              </button>
            )}
            <div
              style={{
                fontFamily: "'JetBrains Mono', monospace",
                fontSize: 10.5,
                color: "#99999e",
                lineHeight: 1.5,
                textAlign: "center",
              }}
            >
              code-first · deployment is always a separate, explicit step you take
            </div>
            <a
              href="/playground"
              style={{
                fontFamily: "'Hanken Grotesk', sans-serif",
                fontSize: 12.5,
                color: "#1c1c20",
                opacity: 0.7,
                textAlign: "center",
                textDecoration: "none",
                letterSpacing: "0.01em",
              }}
            >
              → open full playground
            </a>
          </div>

          <div style={{ flex: "1.15 1 440px", minWidth: 340 }}>
            <OutputArea phase={phase} backend={backend} />
          </div>
        </div>
      </div>
    </section>
  );
}

// ─── sub-components ────────────────────────────────────────────────────────────

function Segments<T extends string>({
  options,
  value,
  onChange,
  disabled,
}: {
  options: Array<{ value: T; label: string }>;
  value: T;
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  return (
    <div
      style={{
        display: "flex",
        gap: 6,
        background: "rgba(28,28,33,0.06)",
        padding: 4,
        borderRadius: 10,
      }}
    >
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            onClick={() => onChange(o.value)}
            disabled={disabled}
            style={{
              flex: 1,
              border: "none",
              background: active ? "#1c1c20" : "transparent",
              color: active ? "#f4f4f5" : "#54545a",
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 11.5,
              padding: "9px 8px",
              borderRadius: 7,
              cursor: disabled ? "not-allowed" : "pointer",
              opacity: disabled ? 0.55 : 1,
              transition: "background 0.15s, color 0.15s",
              letterSpacing: "0.02em",
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

function BackendBadge({ status }: { status: BackendStatus }) {
  const tone = (() => {
    if (status.kind === "live")
      return {
        bg: "rgba(40,165,90,0.12)",
        fg: "#197a40",
        dot: "#23a35a",
        label: "backend live",
      };
    if (status.kind === "checking")
      return {
        bg: "rgba(28,28,33,0.08)",
        fg: "#55555b",
        dot: "#a0a0a6",
        label: "checking backend…",
      };
    return {
      bg: "rgba(170,80,60,0.12)",
      fg: "#9c4a36",
      dot: "#c66448",
      label: "backend offline",
    };
  })();
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 7,
        background: tone.bg,
        color: tone.fg,
        padding: "5px 11px",
        borderRadius: 99,
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 11,
        letterSpacing: "0.03em",
      }}
    >
      <span style={{ width: 7, height: 7, borderRadius: "50%", background: tone.dot }} />
      {tone.label}
    </span>
  );
}

function OutputArea({ phase, backend }: { phase: Phase; backend: BackendStatus }) {
  if (backend.kind === "down") return <DisabledPanel reason={backend.reason} />;
  if (backend.kind === "checking") return <CheckingPanel />;

  switch (phase.kind) {
    case "idle":
      return <IdlePanel />;
    case "recording":
    case "synthesizing":
    case "simulating":
      return <LoadingPanel phase={phase.kind} />;
    case "error":
      return <ErrorPanel code={phase.code} detail={phase.detail} />;
    case "success":
      return (
        <SuccessPanel recording={phase.recording} spec={phase.spec} report={phase.report} />
      );
  }
}

function DisabledPanel({ reason }: { reason: string }) {
  return (
    <Panel>
      <PanelHeader>live mode unavailable</PanelHeader>
      <p style={{ margin: 0, color: "#54545a", fontSize: 14, lineHeight: 1.6 }}>{reason}</p>
      <p style={{ margin: 0, color: "#797980", fontSize: 12.5, lineHeight: 1.6 }}>
        scroll up to read the curated examples — they are real frozen artifacts from a real
        testnet run, displayed verbatim, not fake data.
      </p>
    </Panel>
  );
}

function CheckingPanel() {
  return (
    <Panel>
      <PanelHeader>checking backend…</PanelHeader>
      <Spinner />
    </Panel>
  );
}

function IdlePanel() {
  return (
    <Panel>
      <PanelHeader>output</PanelHeader>
      <p style={{ margin: 0, color: "#797980", fontSize: 13, lineHeight: 1.6 }}>
        results render here once you submit. you'll see the recorded transaction, the synthesized
        policy, and the simulation report (permit + auto-generated deny vectors).
      </p>
    </Panel>
  );
}

function LoadingPanel({ phase }: { phase: "recording" | "synthesizing" | "simulating" }) {
  const steps: Array<{ key: typeof phase; label: string }> = [
    { key: "recording", label: "recording transaction" },
    { key: "synthesizing", label: "synthesizing policy" },
    { key: "simulating", label: "simulating permit + deny" },
  ];
  const idx = steps.findIndex((s) => s.key === phase);
  return (
    <Panel>
      <PanelHeader>running…</PanelHeader>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {steps.map((s, i) => {
          const state = i < idx ? "done" : i === idx ? "active" : "pending";
          return (
            <div key={s.key} style={{ display: "flex", alignItems: "center", gap: 11 }}>
              <span
                style={{
                  width: 18,
                  height: 18,
                  borderRadius: "50%",
                  background:
                    state === "done"
                      ? "#1c1c20"
                      : state === "active"
                      ? "transparent"
                      : "rgba(28,28,33,0.08)",
                  border: state === "active" ? "2px solid #1c1c20" : "none",
                  borderTopColor: state === "active" ? "transparent" : undefined,
                  display: "inline-block",
                  animation: state === "active" ? "spin 0.8s linear infinite" : undefined,
                }}
              />
              <span
                style={{
                  fontFamily: "'JetBrains Mono', monospace",
                  fontSize: 13,
                  color: state === "pending" ? "#a0a0a6" : "#1d1d1e",
                }}
              >
                {s.label}
                {state === "active" ? "…" : ""}
              </span>
            </div>
          );
        })}
      </div>
    </Panel>
  );
}

function ErrorPanel({ code, detail }: { code: string; detail: string }) {
  const desc = describeError(code);
  return (
    <Panel>
      <PanelHeader>
        <span style={{ color: "#c0533a" }}>error</span>
      </PanelHeader>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        <span
          style={{
            display: "inline-flex",
            alignSelf: "flex-start",
            background: "rgba(192,83,58,0.12)",
            color: "#9c4a36",
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 11.5,
            padding: "5px 10px",
            borderRadius: 7,
            letterSpacing: "0.04em",
          }}
        >
          {code}
        </span>
        <p style={{ margin: 0, color: "#1d1d1e", fontSize: 14.5, lineHeight: 1.55 }}>{desc}</p>
        <pre
          style={{
            margin: 0,
            background: "#f3f3f4",
            color: "#54545a",
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 11.5,
            padding: 12,
            borderRadius: 9,
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            lineHeight: 1.45,
          }}
        >
          {detail}
        </pre>
      </div>
    </Panel>
  );
}

function SuccessPanel({
  recording,
  spec,
  report,
}: {
  recording: Recording;
  spec: PolicySpec;
  report: SimReport;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
      <ResultPanel
        title="recorded transaction"
        summary={summarizeRecording(recording)}
        json={recording}
      />
      <ResultPanel title="synthesized policy" summary={summarizeSpec(spec)} json={spec} />
      <ResultPanel
        title="simulation report"
        summary={summarizeReport(report)}
        json={report}
        report={report}
      />
    </div>
  );
}

function ResultPanel({
  title,
  summary,
  json,
  report,
}: {
  title: string;
  summary: string;
  json: unknown;
  report?: SimReport;
}) {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const text = useMemo(() => JSON.stringify(json, null, 2), [json]);
  const copy = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    });
  }, [text]);

  return (
    <div
      style={{
        background: "#fbfbfb",
        borderRadius: 14,
        padding: 18,
        boxShadow: "0 6px 18px -12px rgba(22,24,21,0.3)",
      }}
    >
      <button
        onClick={() => setOpen((o) => !o)}
        style={{
          all: "unset",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          width: "100%",
          cursor: "pointer",
          gap: 10,
        }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          <span
            style={{
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 10.5,
              letterSpacing: "0.06em",
              color: "#797980",
              textTransform: "uppercase",
            }}
          >
            {title}
          </span>
          <span
            style={{
              fontFamily: "'Bricolage Grotesque', sans-serif",
              fontSize: 16,
              color: "#1d1d1e",
              lineHeight: 1.3,
            }}
          >
            {summary}
          </span>
        </div>
        <span
          style={{
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 12,
            color: "#1c1c20",
            background: "rgba(28,28,33,0.06)",
            padding: "5px 9px",
            borderRadius: 6,
          }}
        >
          {open ? "hide" : "json"}
        </span>
      </button>

      {report && <DenyVectors report={report} />}

      {open && (
        <div style={{ marginTop: 12 }}>
          <div style={{ display: "flex", justifyContent: "flex-end", marginBottom: 6 }}>
            <button
              onClick={copy}
              style={{
                background: "rgba(28,28,33,0.06)",
                border: "none",
                cursor: "pointer",
                fontFamily: "'JetBrains Mono', monospace",
                fontSize: 10.5,
                color: "#1c1c20",
                padding: "4px 9px",
                borderRadius: 6,
              }}
            >
              {copied ? "copied" : "copy json"}
            </button>
          </div>
          <pre
            style={{
              margin: 0,
              background: "#1c1c20",
              color: "#e8e8ec",
              padding: 14,
              borderRadius: 10,
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 11.5,
              lineHeight: 1.55,
              overflowX: "auto",
              maxHeight: 320,
            }}
          >
            {text}
          </pre>
        </div>
      )}
    </div>
  );
}

function DenyVectors({ report }: { report: SimReport }) {
  return (
    <div style={{ marginTop: 14, display: "flex", flexDirection: "column", gap: 8 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "9px 12px",
          background: report.permit.passed ? "rgba(40,165,90,0.1)" : "rgba(192,83,58,0.12)",
          color: report.permit.passed ? "#197a40" : "#9c4a36",
          borderRadius: 8,
          fontFamily: "'JetBrains Mono', monospace",
          fontSize: 12,
        }}
      >
        <span style={{ fontWeight: 600 }}>
          {report.permit.passed ? "permit ✓" : "permit ✗"}
        </span>
        <span style={{ opacity: 0.8 }}>
          {report.permit.passed
            ? "recorded transaction would be authorized"
            : report.permit.error ?? "policy rejected the recorded transaction"}
        </span>
      </div>
      {report.deny_results.map((d) => (
        <div
          key={d.name}
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 10,
            padding: "8px 12px",
            background: d.passed ? "rgba(40,165,90,0.07)" : "rgba(192,83,58,0.1)",
            color: "#1d1d1e",
            borderRadius: 8,
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 11.5,
            lineHeight: 1.5,
          }}
        >
          <span>{humanizeDenyName(d.name)}</span>
          <span style={{ color: d.passed ? "#197a40" : "#9c4a36" }}>
            {d.passed ? `✓ ${d.expected_error_code}` : `✗ got ${d.actual_error_code ?? "none"}`}
          </span>
        </div>
      ))}
    </div>
  );
}

// ─── ui primitives ─────────────────────────────────────────────────────────────

function Panel({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        background: "#fbfbfb",
        borderRadius: 16,
        padding: 22,
        display: "flex",
        flexDirection: "column",
        gap: 14,
        boxShadow: "0 8px 24px -16px rgba(22,24,21,0.3)",
        minHeight: 180,
      }}
    >
      {children}
    </div>
  );
}

function PanelHeader({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 11,
        letterSpacing: "0.06em",
        color: "#797980",
        textTransform: "uppercase",
      }}
    >
      {children}
    </span>
  );
}

function Spinner() {
  return (
    <span
      style={{
        width: 22,
        height: 22,
        borderRadius: "50%",
        border: "2px solid rgba(28,28,33,0.15)",
        borderTopColor: "#1c1c20",
        display: "inline-block",
        animation: "spin 0.8s linear infinite",
      }}
    />
  );
}

// ─── summarizers ───────────────────────────────────────────────────────────────

function summarizeRecording(r: Recording): string {
  const contracts = r.contracts ?? [];
  if (!contracts.length) return "0 contract invocations";
  const fn = contracts[0].function;
  const addr = shortAddr(contracts[0].address);
  return `${contracts.length} contract ${
    contracts.length === 1 ? "call" : "calls"
  } — ${fn} on ${addr}`;
}

function summarizeSpec(s: PolicySpec): string {
  const ctx =
    s.context_rule.context_type.kind === "call_contract"
      ? `CallContract ${shortAddr(s.context_rule.context_type.address)}`
      : "Default";
  const slots = s.policies?.length ?? 0;
  return `rule "${s.context_rule.name}" · ${ctx} · ${slots} ${slots === 1 ? "slot" : "slots"}`;
}

function summarizeReport(r: SimReport): string {
  return `permit ${r.permit.passed ? "✓" : "✗"} · ${r.passed}/${r.total_vectors} deny vectors pass`;
}

function shortAddr(a: string): string {
  if (a.length <= 12) return a;
  return `${a.slice(0, 5)}…${a.slice(-4)}`;
}

function humanizeDenyName(n: string): string {
  return n
    .replace(/^slot\d+_c\d+_/, "")
    .replace(/_/g, " ");
}

// ─── shared style objects ──────────────────────────────────────────────────────

const inputBig: React.CSSProperties = {
  background: "#ebebec",
  border: "none",
  borderRadius: 11,
  padding: 14,
  color: "#1d1d1e",
  fontFamily: "'JetBrains Mono', monospace",
  fontSize: 13,
  outline: "none",
  width: "100%",
  textOverflow: "ellipsis",
};

const inputSmall: React.CSSProperties = {
  background: "#ebebec",
  border: "none",
  borderRadius: 10,
  padding: "11px 12px",
  color: "#1d1d1e",
  fontFamily: "'JetBrains Mono', monospace",
  fontSize: 12.5,
  outline: "none",
  width: "100%",
};

const submitBtn: React.CSSProperties = {
  marginTop: 2,
  width: "100%",
  background: "#1c1c20",
  color: "#f4f4f5",
  fontFamily: "'JetBrains Mono', monospace",
  fontWeight: 600,
  fontSize: 14,
  border: "none",
  borderRadius: 11,
  padding: 15,
  letterSpacing: "0.02em",
};
