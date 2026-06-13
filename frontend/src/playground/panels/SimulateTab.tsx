// Simulate tab — permit row + deny vector matrix + re-simulate button.
// Spec: docs/superpowers/specs/2026-06-14-playground-design.md §§4.1, 5, 7, 8.
//
// Theme parity with frontend/src/sections/Synthesizer.tsx — inline styles only,
// Hanken Grotesk for body, JetBrains Mono for error codes / vector names,
// #16a34a pass, #dc2626 fail, panel shadow 0 12px 34px -20px rgba(22,24,21,0.35).
//
// No mock data: when report === null we render an explicit empty-state marker,
// not fabricated rows. When resimError is null we render nothing — never a
// placeholder "fake" error. Honors feedback-no-mock-fallback +
// feedback-honesty-no-fakes from the user's memory.

import type { SimReport, DenyResult } from "../../lib/types";
import { describeError } from "../../lib/mcp";

// PLACEHOLDER: real org/repo URL must replace this before shipping. The user's
const BUG_REPORT_URL = "https://github.com/ErenTopaal/oz-policy-builder/issues/new";

const COLOR_PASS = "#16a34a";
const COLOR_FAIL = "#dc2626";
const COLOR_INK = "#1c1c20";
const COLOR_MUTED = "#54545a";
const COLOR_SUBTLE = "#a0a0a8";
const COLOR_BORDER = "#e4e4e7";
const COLOR_SURFACE = "#fbfbfb";
const COLOR_SURFACE_ALT = "#fafafa";
const PANEL_SHADOW = "0 12px 34px -20px rgba(22,24,21,0.35)";
const MONO = "'JetBrains Mono', monospace";
const BODY = "'Hanken Grotesk', sans-serif";

// known soroban policy error codes — used to give a one-line hint when a deny
// vector that *should* be denied with code N was not. List is intentionally
// conservative; unknown codes fall back to a neutral phrase.
const DENY_CODE_HINTS: Record<number, string> = {
  1010: "function not in allowlist",
  1011: "amount over cap",
  1012: "amount under floor",
  1013: "argument pattern mismatch",
  1014: "asset not in allowlist",
  1015: "outside time window",
  1016: "call frequency exceeded",
  1017: "sequence ordering violation",
};

function hintForExpectedCode(code: number): string {
  return DENY_CODE_HINTS[code] ?? "expected policy rejection did not occur";
}

export interface SimulateTabProps {
  report: SimReport | null;
  modified: boolean;
  onReSimulate: () => void;
  busy: boolean;
  resimError: { code: string; detail: string } | null;
}

// All props have safe defaults so PlaygroundPage's prop-less mount renders the
// empty state without crashing. The defaults are honest (null/false/noop),
// never fabricated data.
const DEFAULT_PROPS: SimulateTabProps = {
  report: null,
  modified: false,
  onReSimulate: () => {},
  busy: false,
  resimError: null,
};

export function SimulateTab(props: Partial<SimulateTabProps> = {}) {
  const { report, modified, onReSimulate, busy, resimError } = { ...DEFAULT_PROPS, ...props };

  if (report === null) {
    return (
      <div
        style={{
          padding: 24,
          fontFamily: BODY,
          color: COLOR_SUBTLE,
          minHeight: 320,
        }}
      >
        <span data-testid="simulate-empty">no simulation yet — synthesize first</span>
      </div>
    );
  }

  const anyDenyFailed = report.deny_results.some((d) => !d.passed);
  const allPassed = report.permit.passed && report.passed === report.total_vectors && !anyDenyFailed;
  const synthesizerBug =
    report.permit.passed === false ||
    report.deny_results.some((d) => !d.passed && d.actual_error_code === null);

  return (
    <div
      style={{
        padding: 22,
        fontFamily: BODY,
        color: COLOR_INK,
        background: COLOR_SURFACE,
        borderRadius: 12,
        boxShadow: PANEL_SHADOW,
        display: "flex",
        flexDirection: "column",
        gap: 16,
      }}
    >
      {synthesizerBug && <SynthesizerBugBanner report={report} />}

      <HeaderRow report={report} allPassed={allPassed} />

      <PermitRow report={report} />

      <DenyMatrix results={report.deny_results} />

      <ReSimulateRow
        modified={modified}
        busy={busy}
        resimError={resimError}
        onReSimulate={onReSimulate}
      />
    </div>
  );
}

function HeaderRow({ report, allPassed }: { report: SimReport; allPassed: boolean }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        paddingBottom: 12,
        borderBottom: `1px solid ${COLOR_BORDER}`,
      }}
    >
      <h2
        style={{
          margin: 0,
          fontFamily: "'Bricolage Grotesque', sans-serif",
          fontSize: 20,
          fontWeight: 500,
          letterSpacing: "-0.01em",
        }}
      >
        Simulate
      </h2>
      <div
        data-testid="simulate-status"
        style={{ display: "flex", alignItems: "center", gap: 8 }}
      >
        <Dot passed={allPassed} />
        <span
          style={{
            fontFamily: MONO,
            fontSize: 12,
            color: allPassed ? COLOR_PASS : COLOR_FAIL,
            letterSpacing: "0.02em",
          }}
        >
          {allPassed ? "all passed" : `${report.passed}/${report.total_vectors} vectors passed`}
        </span>
      </div>
    </div>
  );
}

function PermitRow({ report }: { report: SimReport }) {
  const passed = report.permit.passed;
  return (
    <section
      data-testid="permit-row"
      aria-label="permit row"
      style={{
        display: "grid",
        gridTemplateColumns: "minmax(120px, 180px) 1fr",
        alignItems: "center",
        gap: 16,
        padding: "12px 14px",
        background: COLOR_SURFACE_ALT,
        border: `1px solid ${COLOR_BORDER}`,
        borderRadius: 10,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <Dot passed={passed} />
        <span
          style={{
            fontFamily: MONO,
            fontSize: 12,
            letterSpacing: "0.04em",
            color: COLOR_INK,
          }}
        >
          permit
        </span>
      </div>
      {passed ? (
        <span style={{ fontSize: 13.5, color: COLOR_MUTED, lineHeight: 1.5 }}>
          policy permits the recorded transaction (as expected)
        </span>
      ) : (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 8,
            alignItems: "flex-start",
          }}
        >
          <Chip kind="fail" label="E_SIM_PERMIT_DENIED" />
          <span
            data-testid="permit-error-text"
            style={{
              fontFamily: MONO,
              fontSize: 12,
              color: COLOR_FAIL,
              lineHeight: 1.5,
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
            }}
          >
            {report.permit.error ?? "(no error message)"}
          </span>
        </div>
      )}
    </section>
  );
}

function DenyMatrix({ results }: { results: DenyResult[] }) {
  if (results.length === 0) {
    return (
      <div
        data-testid="deny-empty"
        style={{
          fontSize: 13,
          color: COLOR_SUBTLE,
          padding: "8px 2px",
        }}
      >
        no deny vectors generated for this spec
      </div>
    );
  }
  return (
    <section
      aria-label="deny vector matrix"
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))",
        gap: 12,
      }}
    >
      {results.map((d, i) => (
        <DenyCard key={`${d.name}-${i}`} result={d} />
      ))}
    </section>
  );
}

function DenyCard({ result }: { result: DenyResult }) {
  const truncatedName =
    result.name.length > 50 ? `${result.name.slice(0, 49)}…` : result.name;
  const actual = result.actual_error_code === null ? "—" : String(result.actual_error_code);
  return (
    <article
      data-testid="deny-card"
      data-passed={result.passed ? "true" : "false"}
      style={{
        background: COLOR_SURFACE_ALT,
        border: `1px solid ${COLOR_BORDER}`,
        borderRadius: 10,
        padding: 12,
        display: "flex",
        flexDirection: "column",
        gap: 10,
      }}
    >
      <header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
        }}
      >
        <span
          title={result.name}
          style={{
            fontFamily: MONO,
            fontSize: 11.5,
            color: COLOR_INK,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            letterSpacing: "0.02em",
          }}
        >
          {truncatedName}
        </span>
        <Dot passed={result.passed} />
      </header>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr",
          gap: 8,
          fontFamily: MONO,
          fontSize: 11,
          color: COLOR_MUTED,
        }}
      >
        <span>
          expected:{" "}
          <span style={{ color: COLOR_INK }}>{result.expected_error_code}</span>
        </span>
        <span>
          actual: <span style={{ color: COLOR_INK }}>{actual}</span>
        </span>
      </div>
      {!result.passed && (
        <footer
          data-testid="deny-fail-footer"
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 4,
            paddingTop: 8,
            borderTop: `1px dashed ${COLOR_BORDER}`,
          }}
        >
          <span
            style={{
              color: COLOR_FAIL,
              fontSize: 12.5,
              lineHeight: 1.4,
            }}
          >
            policy failed to deny this vector
          </span>
          <span
            style={{
              fontFamily: MONO,
              fontSize: 11,
              color: COLOR_MUTED,
            }}
          >
            hint: {result.expected_error_code} → {hintForExpectedCode(result.expected_error_code)}
          </span>
        </footer>
      )}
    </article>
  );
}

function ReSimulateRow({
  modified,
  busy,
  resimError,
  onReSimulate,
}: {
  modified: boolean;
  busy: boolean;
  resimError: { code: string; detail: string } | null;
  onReSimulate: () => void;
}) {
  const disabled = !modified || busy;
  return (
    <div
      style={{
        position: "sticky",
        bottom: 0,
        background: COLOR_SURFACE,
        paddingTop: 12,
        borderTop: `1px solid ${COLOR_BORDER}`,
        display: "flex",
        flexDirection: "column",
        gap: 10,
      }}
    >
      {resimError !== null && (
        <div
          data-testid="resim-error-banner"
          role="alert"
          style={{
            background: "rgba(220,38,38,0.08)",
            border: `1px solid ${COLOR_FAIL}`,
            borderRadius: 8,
            padding: "10px 12px",
            display: "flex",
            flexDirection: "column",
            gap: 6,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Chip kind="fail" label={resimError.code} />
            <span
              style={{
                fontSize: 13,
                color: COLOR_FAIL,
                lineHeight: 1.45,
              }}
            >
              {describeError(resimError.code)}
            </span>
          </div>
          {resimError.detail && (
            <pre
              style={{
                margin: 0,
                fontFamily: MONO,
                fontSize: 11,
                color: COLOR_MUTED,
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
                lineHeight: 1.4,
              }}
            >
              {resimError.detail}
            </pre>
          )}
        </div>
      )}
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
            fontFamily: MONO,
            fontSize: 11,
            color: modified ? COLOR_INK : COLOR_SUBTLE,
            letterSpacing: "0.02em",
          }}
        >
          {modified ? "source diverges from spec" : "source matches spec"}
        </span>
        <button
          type="button"
          onClick={onReSimulate}
          disabled={disabled}
          aria-disabled={disabled}
          style={{
            border: `1px solid ${disabled ? COLOR_BORDER : COLOR_INK}`,
            background: disabled ? "rgba(28,28,33,0.06)" : COLOR_INK,
            color: disabled ? COLOR_SUBTLE : "#fbfbfb",
            fontFamily: MONO,
            fontSize: 12,
            letterSpacing: "0.04em",
            padding: "9px 14px",
            borderRadius: 8,
            cursor: disabled ? "not-allowed" : "pointer",
          }}
        >
          {busy ? "simulating…" : "re-simulate from source"}
        </button>
      </div>
    </div>
  );
}

function SynthesizerBugBanner({ report }: { report: SimReport }) {
  const rejectedRecorded = report.permit.passed === false;
  const phrase = rejectedRecorded
    ? "rejected the recorded tx"
    : "permitted something it shouldn't";
  return (
    <div
      data-testid="synthesizer-bug-banner"
      role="alert"
      style={{
        background: "rgba(220,38,38,0.08)",
        border: `1px solid ${COLOR_FAIL}`,
        borderRadius: 10,
        padding: "12px 14px",
        display: "flex",
        flexDirection: "column",
        gap: 6,
      }}
    >
      <span style={{ color: COLOR_FAIL, fontSize: 13.5, lineHeight: 1.5 }}>
        generated policy unexpectedly {phrase} — likely a synthesizer bug.{" "}
        <a
          href={BUG_REPORT_URL}
          target="_blank"
          rel="noopener noreferrer"
          style={{
            color: COLOR_FAIL,
            textDecoration: "underline",
            fontFamily: MONO,
            fontSize: 12.5,
          }}
        >
          Open a report?
        </a>
      </span>
    </div>
  );
}

function Dot({ passed }: { passed: boolean }) {
  return (
    <span
      data-testid="status-dot"
      data-passed={passed ? "true" : "false"}
      aria-hidden="true"
      style={{
        display: "inline-block",
        width: 10,
        height: 10,
        borderRadius: "50%",
        background: passed ? COLOR_PASS : COLOR_FAIL,
        boxShadow: `0 0 0 2px ${passed ? "rgba(22,163,74,0.18)" : "rgba(220,38,38,0.18)"}`,
        flexShrink: 0,
      }}
    />
  );
}

function Chip({ kind, label }: { kind: "fail"; label: string }) {
  const fg = kind === "fail" ? "#9c1f1f" : COLOR_INK;
  const bg = kind === "fail" ? "rgba(220,38,38,0.12)" : "rgba(28,28,33,0.06)";
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        background: bg,
        color: fg,
        fontFamily: MONO,
        fontSize: 11,
        padding: "4px 8px",
        borderRadius: 6,
        letterSpacing: "0.04em",
        flexShrink: 0,
      }}
    >
      {label}
    </span>
  );
}
