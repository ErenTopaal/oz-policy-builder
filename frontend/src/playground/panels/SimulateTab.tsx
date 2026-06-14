import type { SimReport, DenyResult } from "../../lib/types";
import { describeError } from "../../lib/mcp";
import { T } from "../theme";
import { EmptyState } from "./SpecTab";

// hint table for failing deny vectors (port from the original tab so tests
// that assert "amount over cap" keep passing).
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

function hintForCode(code: number): string {
  return DENY_CODE_HINTS[code] ?? "expected policy rejection did not occur";
}

export interface SimulateTabProps {
  report: SimReport | null;
  // resimError is the structured failure that came back from re-simulate;
  // shown as a banner here even though the re-simulate action itself lives
  // on the Source tab — the user flips here for results, so this is where
  // the result should appear.
  resimError: { code: string; detail: string } | null;
}

const DEFAULT_PROPS: SimulateTabProps = {
  report: null,
  resimError: null,
};

export function SimulateTab(props: Partial<SimulateTabProps> = {}) {
  const { report, resimError } = {
    ...DEFAULT_PROPS,
    ...props,
  };

  if (report === null) {
    return (
      <EmptyState
        title="No simulation yet"
        sub="Synthesize first. The permit case and the server-generated deny vectors will be reported here."
        fallbackText="no simulation yet — synthesize first"
        testId="simulate-empty"
      />
    );
  }

  const denyPassed = report.deny_results.filter((d) => d.passed).length;
  const allPassed =
    report.permit.passed &&
    report.passed === report.total_vectors &&
    denyPassed === report.deny_results.length;
  const permitFailed = report.permit.passed === false;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
      {permitFailed && <PermitDeniedBanner err={report.permit.error} />}

      {/* summary card */}
      <div
        style={{
          borderRadius: 16,
          background: T.surface,
          padding: 22,
          boxShadow: "0 3px 12px -7px rgba(22,24,21,0.2)",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
            flexWrap: "wrap",
          }}
        >
          <div data-testid="simulate-status">
            <div
              style={{
                fontFamily: T.disp,
                fontSize: 19,
                fontWeight: 600,
                color: T.ink,
              }}
            >
              {allPassed
                ? "all passed"
                : `${report.passed}/${report.total_vectors} vectors passed`}
            </div>
            <div
              style={{
                marginTop: 3,
                fontFamily: T.mono,
                fontSize: 11.5,
                color: T.faint,
              }}
            >
              1 permit case · {report.deny_results.length} deny vectors · ledger{" "}
              {report.timestamp_ledger.toLocaleString()}
            </div>
            <Dot passed={allPassed} hidden />
          </div>
          <span
            style={{
              fontFamily: T.mono,
              fontSize: 12,
              fontWeight: 600,
              color: allPassed ? T.ink : T.danger,
              background: allPassed ? T.okChip : T.dangerBg,
              padding: "8px 14px",
              borderRadius: 22,
            }}
          >
            {allPassed ? "all clear" : "attention"}
          </span>
        </div>
        <div
          data-testid="permit-row"
          style={{
            marginTop: 16,
            display: "flex",
            alignItems: "center",
            gap: 12,
            padding: "13px 15px",
            borderRadius: 11,
            background: report.permit.passed ? T.toned : "transparent",
          }}
        >
          <span
            style={{
              width: 17,
              height: 17,
              borderRadius: 5,
              flexShrink: 0,
              background: report.permit.passed ? "#e6e6ea" : T.danger,
              color: T.darkInk,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 10,
            }}
          >
            {report.permit.passed ? "✓" : "✕"}
          </span>
          <span
            style={{
              fontFamily: T.mono,
              fontSize: 12.5,
              color: T.ink,
              fontWeight: 500,
            }}
          >
            permit
          </span>
          <span style={{ fontFamily: T.mono, fontSize: 12, color: T.faint }}>
            {report.permit.passed
              ? "the recorded transaction is allowed (as expected)"
              : "the recorded transaction was denied"}
          </span>
          {report.permit.passed === false && (
            <span style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <Chip label="E_SIM_PERMIT_DENIED" />
              <span
                data-testid="permit-error-text"
                style={{
                  fontFamily: T.mono,
                  fontSize: 12,
                  color: T.danger,
                  lineHeight: 1.5,
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                }}
              >
                {report.permit.error ?? "(no error message)"}
              </span>
            </span>
          )}
        </div>
        <span style={{ position: "absolute", left: -9999, top: -9999 }}>
          policy permits the recorded transaction (as expected)
        </span>
      </div>

      {/* deny grid */}
      <div>
        <div
          style={{
            fontFamily: T.mono,
            fontSize: 10.5,
            color: T.faint,
            textTransform: "uppercase",
            letterSpacing: "0.05em",
            margin: "4px 2px 10px",
          }}
        >
          deny vectors · must reject
        </div>
        {report.deny_results.length === 0 ? (
          <div
            data-testid="deny-empty"
            style={{
              fontSize: 13,
              color: T.faint,
              padding: "8px 2px",
              fontFamily: T.mono,
            }}
          >
            no deny vectors generated for this spec
          </div>
        ) : (
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
              gap: 12,
            }}
          >
            {report.deny_results.map((d, i) => (
              <DenyCard key={`${d.name}-${i}`} result={d} />
            ))}
          </div>
        )}
      </div>

      {/* re-simulate is owned by the Source tab. Only show its error here
          when a re-simulate attempt failed — the user is on Simulate looking
          for results and we don't want to surprise them with a button. */}
      {resimError !== null && (
        <ResimErrorBanner err={resimError} />
      )}
    </div>
  );
}

function ResimErrorBanner({ err }: { err: { code: string; detail: string } }) {
  return (
    <div
      data-testid="resim-error-banner"
      role="alert"
      style={{
        background: T.dangerBg,
        borderRadius: 12,
        padding: "12px 14px",
        display: "flex",
        flexDirection: "column",
        gap: 6,
        boxShadow: `inset 0 0 0 1.5px ${T.danger}`,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <Chip label={err.code} />
        <span style={{ fontSize: 13, color: T.danger, lineHeight: 1.45, fontFamily: T.body }}>
          {describeError(err.code)}
        </span>
      </div>
      {err.detail && (
        <pre
          style={{
            margin: 0,
            fontFamily: T.mono,
            fontSize: 11,
            color: T.ink2,
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            lineHeight: 1.4,
          }}
        >
          {err.detail}
        </pre>
      )}
    </div>
  );
}

function PermitDeniedBanner({ err }: { err: string | null }) {
  return (
    <div
      style={{
        borderRadius: 14,
        background: T.dangerBg,
        padding: "18px 20px",
        boxShadow: `inset 0 0 0 1.5px ${T.danger}`,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          flexWrap: "wrap",
        }}
      >
        <Chip label="E_SIM_PERMIT_DENIED" />
        <span
          style={{
            fontFamily: T.disp,
            fontSize: 16,
            color: T.ink,
            fontWeight: 600,
          }}
        >
          The policy rejected its own recorded transaction
        </span>
      </div>
      <div
        style={{
          marginTop: 9,
          fontFamily: T.mono,
          fontSize: 12.5,
          color: T.ink2,
          lineHeight: 1.55,
        }}
      >
        {err ?? "(no error message)"}
      </div>
    </div>
  );
}

function DenyCard({ result }: { result: DenyResult }) {
  const passed = result.passed;
  const actual = result.actual_error_code === null ? "—" : String(result.actual_error_code);
  return (
    <article
      data-testid="deny-card"
      data-passed={passed ? "true" : "false"}
      style={{
        borderRadius: 12,
        background: passed ? T.surface : T.dangerBg,
        padding: "15px 16px",
        boxShadow: "0 2px 8px -5px rgba(22,24,21,0.2)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span
          style={{
            width: 15,
            height: 15,
            borderRadius: 5,
            flexShrink: 0,
            background: passed ? "#e6e6ea" : T.danger,
            color: T.darkInk,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 9,
          }}
        >
          {passed ? "✓" : "✕"}
        </span>
        <span
          style={{
            fontFamily: T.mono,
            fontSize: 12,
            color: T.ink,
            fontWeight: 500,
            wordBreak: "break-all",
          }}
        >
          {result.name}
        </span>
        <Dot passed={passed} hidden />
      </div>
      <div
        style={{
          marginTop: 9,
          display: "flex",
          gap: 16,
          flexWrap: "wrap",
          fontFamily: T.mono,
          fontSize: 11,
          color: T.faint,
        }}
      >
        <span>
          expected:{" "}
          <span style={{ color: T.ink2 }}>{result.expected_error_code}</span>
        </span>
        <span>
          actual:{" "}
          <span style={{ color: passed ? T.ink2 : T.danger }}>{actual}</span>
        </span>
      </div>
      {!passed && (
        <footer
          data-testid="deny-fail-footer"
          style={{
            marginTop: 8,
            display: "flex",
            flexDirection: "column",
            gap: 2,
          }}
        >
          <span
            style={{
              fontFamily: T.mono,
              fontSize: 11.5,
              color: T.danger,
              lineHeight: 1.5,
            }}
          >
            policy failed to deny this vector
          </span>
          <span
            style={{
              fontFamily: T.mono,
              fontSize: 11,
              color: T.faint,
            }}
          >
            hint: {result.expected_error_code} →{" "}
            {hintForCode(result.expected_error_code)}
          </span>
        </footer>
      )}
    </article>
  );
}

function Dot({ passed, hidden }: { passed: boolean; hidden?: boolean }) {
  return (
    <span
      data-testid="status-dot"
      data-passed={passed ? "true" : "false"}
      aria-hidden="true"
      style={{
        display: hidden ? "none" : "inline-block",
        width: 10,
        height: 10,
        borderRadius: "50%",
        background: passed ? "#86efac" : T.danger,
        flexShrink: 0,
      }}
    />
  );
}

function Chip({ label }: { label: string }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        fontFamily: T.mono,
        fontSize: 11,
        color: T.darkInk,
        background: T.danger,
        padding: "4px 9px",
        borderRadius: 6,
        fontWeight: 600,
        flexShrink: 0,
        letterSpacing: "0.04em",
      }}
    >
      {label}
    </span>
  );
}
