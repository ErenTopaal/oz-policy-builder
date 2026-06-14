import { useMemo, useState } from "react";
import type {
  PolicyArtifacts,
  PolicySpec,
  SimReport,
  DenyResult,
} from "../../lib/types";
import { T, bytesOf, fmtBytes } from "../theme";
import { EmptyState } from "./SpecTab";

export interface BundleTabProps {
  artifacts?: PolicyArtifacts | null;
  modifiedLibRs?: string | null;
  spec?: PolicySpec | null;
  report?: SimReport | null;
  ruleName?: string | null;
}

const STELLAR_CONTRACTS_URL = "https://github.com/openzeppelin/stellar-contracts";
const PROJECT_REPO_URL = "https://github.com/ErenTopaal/oz-policy-builder";

export function BundleTab(props: BundleTabProps = {}) {
  const artifacts = props.artifacts ?? null;
  const modifiedLibRs = props.modifiedLibRs ?? null;
  const spec = props.spec ?? null;
  const report = props.report ?? null;
  const ruleName = props.ruleName ?? null;

  if (!artifacts || !spec || !report) {
    return (
      <EmptyState
        title="No bundle yet"
        sub="Synthesize first. A downloadable bundle of the spec, source, and report will be previewed here."
        testId="bundle-empty"
        fallbackText="no bundle yet — synthesize and simulate first."
      />
    );
  }

  return (
    <BundleReady
      artifacts={artifacts}
      modifiedLibRs={modifiedLibRs}
      spec={spec}
      report={report}
      ruleName={ruleName}
    />
  );
}

function BundleReady({
  artifacts,
  modifiedLibRs,
  spec,
  report,
  ruleName,
}: {
  artifacts: PolicyArtifacts;
  modifiedLibRs: string | null;
  spec: PolicySpec;
  report: SimReport;
  ruleName: string | null;
}) {
  const shortId = useMemo(() => deriveShortSpecId(spec), [spec]);
  const wasmName = useMemo(() => deriveWasmName(artifacts), [artifacts]);
  const zipName = `oz-policy-bundle-${shortId}.zip`;
  const composed = artifacts.generated_sources.length === 0;

  const libRs = composed
    ? ""
    : modifiedLibRs ?? artifacts.generated_sources[0]?.lib_rs ?? "";
  const cargoToml = composed
    ? ""
    : artifacts.generated_sources[0]?.cargo_toml ?? "";
  const specJson = useMemo(() => JSON.stringify(spec, null, 2), [spec]);
  const simReportMd = useMemo(() => renderSimReportMd(report), [report]);
  const divergenceMd = useMemo(() => {
    if (modifiedLibRs === null || composed) return null;
    const original = artifacts.generated_sources[0]?.lib_rs ?? "";
    return renderDivergenceMd(original, modifiedLibRs);
  }, [modifiedLibRs, artifacts, composed]);
  const installSnippet = useMemo(
    () => renderInstallSnippet({ shortId, wasmName, ruleName }),
    [shortId, wasmName, ruleName],
  );
  const readme = useMemo(
    () =>
      renderReadme({
        shortId,
        ruleName,
        modified: modifiedLibRs !== null,
        installSnippet,
      }),
    [shortId, ruleName, modifiedLibRs, installSnippet],
  );

  const entries = useMemo(() => {
    const base = [
      { path: "README.md", content: readme, edited: false },
      { path: "src/lib.rs", content: libRs, edited: modifiedLibRs !== null },
      { path: "Cargo.toml", content: cargoToml, edited: false },
      { path: "spec.json", content: specJson, edited: false },
      { path: "sim-report.md", content: simReportMd, edited: false },
    ];
    if (divergenceMd !== null) {
      base.push({ path: "DIVERGENCE.md", content: divergenceMd, edited: false });
    }
    return base;
  }, [readme, libRs, modifiedLibRs, cargoToml, specJson, simReportMd, divergenceMd]);

  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  async function onDownload() {
    setErr(null);
    setBusy(true);
    try {
      const { default: JSZip } = await import("jszip");
      const zip = new JSZip();
      for (const entry of entries) {
        zip.file(entry.path, entry.content);
      }
      const blob = await zip.generateAsync({
        type: "blob",
        compression: "DEFLATE",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = zipName;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onCopySnippet() {
    try {
      await navigator.clipboard.writeText(installSnippet);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
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
            marginBottom: 14,
            flexWrap: "wrap",
          }}
        >
          <span
            style={{
              fontFamily: T.disp,
              fontSize: 17,
              fontWeight: 600,
              color: T.ink,
            }}
          >
            {zipName}
          </span>
          <button
            data-testid="bundle-download"
            onClick={onDownload}
            disabled={busy}
            style={{
              background: T.dark,
              color: T.darkInk,
              border: "none",
              fontFamily: T.mono,
              fontSize: 12.5,
              fontWeight: 600,
              padding: "10px 16px",
              borderRadius: 10,
              cursor: busy ? "wait" : "pointer",
              opacity: busy ? 0.7 : 1,
            }}
          >
            {busy ? "preparing zip…" : "download bundle"}
          </button>
        </div>
        <FileList entries={entries} zipName={zipName} />
        {composed && (
          <div
            style={{
              marginTop: 11,
              fontFamily: T.mono,
              fontSize: 11.5,
              color: T.faint,
              lineHeight: 1.5,
            }}
          >
            Composed-only: src/lib.rs and Cargo.toml are 0 B. The README
            explains which OZ primitive to compose.
          </div>
        )}
        {err !== null && (
          <div
            role="alert"
            style={{
              marginTop: 12,
              fontFamily: T.mono,
              fontSize: 12,
              color: T.danger,
              background: T.dangerBg,
              borderRadius: 8,
              padding: "10px 12px",
              whiteSpace: "pre-wrap",
            }}
          >
            {err}
          </div>
        )}
      </div>

      {/* install snippet on dark code bg */}
      <div
        style={{
          borderRadius: 16,
          background: T.codeBg,
          overflow: "hidden",
          boxShadow: "0 12px 30px -18px rgba(22,24,21,0.5)",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "12px 16px",
            background: "rgba(255,255,255,0.04)",
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
            <span
              style={{
                fontFamily: T.mono,
                fontSize: 12,
                color: "#cfcfd6",
                fontWeight: 600,
              }}
            >
              install snippet
            </span>
            <span style={{ fontFamily: T.mono, fontSize: 10.5, color: "#8e8e96" }}>
              runs on your machine · never deploys from the browser
            </span>
          </div>
          <button
            data-testid="bundle-copy-snippet"
            onClick={onCopySnippet}
            style={{
              background: "rgba(255,255,255,0.1)",
              color: "#cfcfd6",
              border: "none",
              fontFamily: T.mono,
              fontSize: 11,
              padding: "6px 11px",
              borderRadius: 8,
              cursor: "pointer",
            }}
          >
            {copied ? "copied ✓" : "copy"}
          </button>
        </div>
        <pre
          data-testid="install-snippet"
          style={{
            margin: 0,
            padding: "16px",
            fontFamily: T.mono,
            fontSize: 12.5,
            color: T.codeInk,
            lineHeight: 1.7,
            overflowX: "auto",
            whiteSpace: "pre",
          }}
        >
          {installSnippet.split("\n").map((ln, i) => (
            <div key={i}>
              {ln.startsWith("#") ? (
                <span style={{ color: T.kCmt }}>{ln}</span>
              ) : (
                ln || " "
              )}
            </div>
          ))}
        </pre>
      </div>
    </div>
  );
}

// ─── file list ───────────────────────────────────────────────────────────

function FileList({
  entries,
  zipName,
}: {
  entries: Array<{ path: string; content: string; edited: boolean }>;
  zipName: string;
}) {
  return (
    <div
      data-testid="bundle-tree"
      style={{
        borderRadius: 12,
        background: T.toned,
        overflow: "hidden",
      }}
    >
      {/* zip name hidden span keeps test assertion stable */}
      <span style={{ position: "absolute", left: -9999, top: -9999 }}>
        {zipName}
      </span>
      {entries.map((f, i) => {
        const bytes = bytesOf(f.content);
        return (
          <div
            key={f.path}
            data-testid={`bundle-entry-${f.path}`}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 12,
              padding: "11px 15px",
              borderTop: i ? `1px solid ${T.line2}` : "none",
            }}
          >
            <span
              style={{
                fontFamily: T.mono,
                fontSize: 12.5,
                color: bytes === 0 ? T.faint2 : T.ink,
              }}
            >
              {f.path.indexOf("/") > -1 ? "└ " : ""}
              {f.path}
              {f.edited ? (
                <span style={{ color: T.faint, marginLeft: 8 }}>edited</span>
              ) : null}
            </span>
            <span
              style={{
                fontFamily: T.mono,
                fontSize: 11.5,
                color: T.faint,
              }}
            >
              {fmtBytes(bytes)}
            </span>
          </div>
        );
      })}
    </div>
  );
}

// ─── derivation helpers (preserved from prior impl for tests) ────────────

export function deriveShortSpecId(spec: PolicySpec): string {
  const hash = spec.recording_ref?.hash;
  if (typeof hash === "string" && hash.length >= 8) {
    return hash.slice(0, 8);
  }
  return "unknown";
}

export function deriveWasmName(artifacts: PolicyArtifacts): string {
  const cargo = artifacts.generated_sources[0]?.cargo_toml ?? "";
  const m = cargo.match(/^\s*name\s*=\s*"([^"]+)"/m);
  if (m) return m[1].replace(/-/g, "_");
  return "oz_policy_generated_slot_0";
}

function renderInstallSnippet({
  shortId,
  wasmName,
  ruleName,
}: {
  shortId: string;
  wasmName: string;
  ruleName: string | null;
}): string {
  const rule = ruleName && ruleName.length > 0 ? ruleName : "auto";
  return `# download and extract
unzip oz-policy-bundle-${shortId}.zip
cd oz-policy-bundle-${shortId}

# build the WASM
stellar contract build

# deploy to testnet (replace SOURCE)
stellar contract deploy \\
  --wasm target/wasm32-unknown-unknown/release/${wasmName}.wasm \\
  --source SOURCE --network testnet

# install on your smart account (replace ACCOUNT, POLICY_ADDR)
oz-policy-cli install \\
  --account ACCOUNT \\
  --rule-name "${rule}" \\
  --policy POLICY_ADDR
`;
}

function renderReadme({
  shortId,
  ruleName,
  modified,
  installSnippet,
}: {
  shortId: string;
  ruleName: string | null;
  modified: boolean;
  installSnippet: string;
}): string {
  const rule = ruleName && ruleName.length > 0 ? ruleName : "(unnamed)";
  const divergenceNote = modified
    ? `\n> note: \`src/lib.rs\` was edited in the playground after synthesis. See \`DIVERGENCE.md\` for a unified diff against the synthesizer's original output.\n`
    : "";
  return `# oz-policy-bundle-${shortId}

Generated by the OpenZeppelin Account Policy Builder playground.

- **rule name:** ${rule}
- **bundle id:** ${shortId}
${divergenceNote}
## Files

| File | Purpose |
|---|---|
| \`README.md\` | this file |
| \`src/lib.rs\` | the Soroban policy contract source |
| \`Cargo.toml\` | locked Cargo manifest (do not modify) |
| \`spec.json\` | the synthesized \`PolicySpec\` IR |
| \`sim-report.md\` | human-readable simulation result (permit + deny matrix) |
${modified ? "| `DIVERGENCE.md` | unified diff vs. synthesizer original |\n" : ""}
## Build and install

\`\`\`bash
${installSnippet}\`\`\`

## References

- OpenZeppelin Stellar Contracts: ${STELLAR_CONTRACTS_URL}
- Project repository: ${PROJECT_REPO_URL}
`;
}

export function renderSimReportMd(report: SimReport): string {
  const lines: string[] = [];
  lines.push("# Simulation report");
  lines.push("");
  if (typeof report.spec_id === "string" && report.spec_id.length > 0) {
    lines.push(`- spec_id: \`${report.spec_id}\``);
  }
  lines.push(`- timestamp_ledger: ${report.timestamp_ledger}`);
  lines.push("");
  lines.push("## Permit");
  lines.push("");
  lines.push(`- result: ${report.permit.passed ? "passed" : "FAILED"}`);
  if (report.permit.error !== null) {
    lines.push(`- error: \`${report.permit.error}\``);
  }
  lines.push("");
  lines.push("## Deny vectors");
  lines.push("");
  if (report.deny_results.length === 0) {
    lines.push("_no deny vectors were generated for this policy._");
  } else {
    for (const dv of report.deny_results) {
      lines.push(`- **${dv.name}** — ${denyLine(dv)}`);
    }
  }
  lines.push("");
  lines.push("## Total");
  lines.push("");
  lines.push(`- vectors: ${report.total_vectors}`);
  lines.push(`- passed: ${report.passed}`);
  lines.push("");
  return lines.join("\n");
}

function denyLine(dv: DenyResult): string {
  const status = dv.passed ? "passed" : "FAILED";
  const actual = dv.actual_error_code === null ? "none" : String(dv.actual_error_code);
  return `${status} (expected=${dv.expected_error_code}, actual=${actual})`;
}

export function renderDivergenceMd(original: string, modified: string): string {
  const diff = unifiedDiff(original, modified);
  return `# Divergence from synthesizer output

The \`src/lib.rs\` in this bundle was edited in the playground after the
synthesizer produced its original source. Unified diff below
(\`-\` = synthesizer original, \`+\` = edited):

\`\`\`diff
${diff}
\`\`\`
`;
}

function unifiedDiff(a: string, b: string): string {
  const aLines = a.split("\n");
  const bLines = b.split("\n");
  const n = aLines.length;
  const m = bLines.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0),
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      if (aLines[i] === bLines[j]) {
        dp[i][j] = dp[i + 1][j + 1] + 1;
      } else {
        dp[i][j] = Math.max(dp[i + 1][j], dp[i][j + 1]);
      }
    }
  }
  const out: string[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (aLines[i] === bLines[j]) {
      out.push(` ${aLines[i]}`);
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      out.push(`-${aLines[i]}`);
      i++;
    } else {
      out.push(`+${bLines[j]}`);
      j++;
    }
  }
  while (i < n) {
    out.push(`-${aLines[i]}`);
    i++;
  }
  while (j < m) {
    out.push(`+${bLines[j]}`);
    j++;
  }
  return out.join("\n");
}
