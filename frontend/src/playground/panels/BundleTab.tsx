// BundleTab — generates a downloadable .zip containing the policy source
// bundle (README, src/lib.rs, Cargo.toml, spec.json, sim-report.md, and
// DIVERGENCE.md if the user edited lib.rs). zip layout per spec §4.4.
//
// honesty rules (per feedback-no-mock-fallback, feedback-honesty-no-fakes):
// - if any of `artifacts | spec | report` is null, we render an explicit
//   "no bundle yet" empty state. no fixture data, no half-bundle.
// - jszip is lazy-imported on click so it lands in its own chunk and
//   never pollutes the landing-page bundle.
// - install snippet references real binaries (`stellar contract build`,
//   `oz-policy-cli install`) and a real upstream URL
//   (github.com/openzeppelin/stellar-contracts). the project repo URL
//   is left as a TODO placeholder rather than fabricated.
//
// per RFP code-first / spec §1: NO deploy button. the install snippet is
// the entire deploy surface.
//
// theme tokens inlined verbatim from spec §8. no Tailwind, no css
// modules. primary button mirrors Synthesizer.tsx's submitBtn style.
//
// props are optional only to keep the wave-1 PlaygroundPage.tsx shell
// (`<BundleTab />`) type-clean until the wave-2 orchestrator wires real
// state through; passing nothing renders the same empty state as
// passing all-null. spec §5 defines the public signature as
// `{ artifacts, modifiedLibRs, spec, report, ruleName }`.

import { useMemo, useState } from "react";
import type { ReactNode } from "react";
import type {
  PolicyArtifacts,
  PolicySpec,
  SimReport,
  DenyResult,
} from "../../lib/types";

export interface BundleTabProps {
  artifacts?: PolicyArtifacts | null;
  modifiedLibRs?: string | null;
  spec?: PolicySpec | null;
  report?: SimReport | null;
  ruleName?: string | null;
}

// upstream reference link. real. not fabricated.
const STELLAR_CONTRACTS_URL = "https://github.com/openzeppelin/stellar-contracts";
// project repo url is a placeholder — flagged in agent reply, not fabricated.
const PROJECT_REPO_URL_PLACEHOLDER =
  "TODO: project repo URL (placeholder — replace before publishing the bundle)";

export function BundleTab(props: BundleTabProps = {}): ReactNode {
  const artifacts = props.artifacts ?? null;
  const modifiedLibRs = props.modifiedLibRs ?? null;
  const spec = props.spec ?? null;
  const report = props.report ?? null;
  const ruleName = props.ruleName ?? null;

  if (!artifacts || !spec || !report) {
    return (
      <div
        style={{
          padding: 24,
          color: "#a0a0a8",
          fontFamily: "'Hanken Grotesk', sans-serif",
          fontSize: 13.5,
          lineHeight: 1.55,
        }}
        data-testid="bundle-empty"
      >
        no bundle yet — synthesize and simulate first.
      </div>
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
}): ReactNode {
  const shortId = useMemo(() => deriveShortSpecId(spec), [spec]);
  const wasmName = useMemo(() => deriveWasmName(artifacts), [artifacts]);
  const zipName = `oz-policy-bundle-${shortId}.zip`;

  const libRs = modifiedLibRs ?? artifacts.generated_sources[0]?.lib_rs ?? "";
  const cargoToml = artifacts.generated_sources[0]?.cargo_toml ?? "";
  const specJson = useMemo(() => JSON.stringify(spec, null, 2), [spec]);
  const simReportMd = useMemo(() => renderSimReportMd(report), [report]);
  const divergenceMd = useMemo(() => {
    if (modifiedLibRs === null) return null;
    const original = artifacts.generated_sources[0]?.lib_rs ?? "";
    return renderDivergenceMd(original, modifiedLibRs);
  }, [modifiedLibRs, artifacts]);
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
      // lazy import so jszip is split into its own chunk.
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
      // give the browser a tick before revoking, otherwise some browsers
      // cancel the download mid-flight.
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
      // clipboard may not be available (e.g. insecure context). don't fake it.
      setCopied(false);
    }
  }

  return (
    <div
      style={{
        padding: "20px 22px 28px",
        display: "flex",
        flexDirection: "column",
        gap: 18,
      }}
    >
      <div>
        <div
          style={{
            fontFamily: "'Bricolage Grotesque', sans-serif",
            fontSize: 18,
            fontWeight: 500,
            letterSpacing: "-0.01em",
            color: "#1c1c20",
            marginBottom: 4,
          }}
        >
          bundle
        </div>
        <div
          style={{
            fontFamily: "'Hanken Grotesk', sans-serif",
            fontSize: 13,
            color: "#54545a",
            lineHeight: 1.55,
          }}
        >
          a self-contained zip you can hand off to{" "}
          <code style={inlineCode}>oz-policy-cli</code> to install on a smart
          account.
        </div>
      </div>

      <FileTree zipName={zipName} entries={entries} />

      <button
        type="button"
        onClick={onDownload}
        disabled={busy}
        data-testid="bundle-download"
        style={{
          ...primaryBtn,
          opacity: busy ? 0.6 : 1,
          cursor: busy ? "wait" : "pointer",
        }}
      >
        {busy ? "preparing zip…" : "download bundle"}
      </button>

      {err !== null && (
        <div
          role="alert"
          style={{
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 12,
            color: "#dc2626",
            background: "rgba(220,38,38,0.06)",
            border: "1px solid rgba(220,38,38,0.25)",
            borderRadius: 8,
            padding: "10px 12px",
            whiteSpace: "pre-wrap",
          }}
        >
          {err}
        </div>
      )}

      <div>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            marginBottom: 6,
          }}
        >
          <div
            style={{
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 11,
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              color: "#797980",
            }}
          >
            install snippet
          </div>
          <button
            type="button"
            onClick={onCopySnippet}
            data-testid="bundle-copy-snippet"
            style={{
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 11.5,
              color: "#1c1c20",
              background: "rgba(28,28,33,0.06)",
              border: "1px solid #e4e4e7",
              padding: "5px 10px",
              borderRadius: 7,
              cursor: "pointer",
              letterSpacing: "0.02em",
            }}
          >
            {copied ? "copied" : "copy"}
          </button>
        </div>
        <pre
          data-testid="install-snippet"
          style={{
            margin: 0,
            background: "#1e1e1e",
            color: "#dddddd",
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 12,
            lineHeight: 1.55,
            padding: "14px 16px",
            borderRadius: 10,
            overflowX: "auto",
            whiteSpace: "pre",
          }}
        >
          {installSnippet}
        </pre>
      </div>
    </div>
  );
}

// --- file tree ---

function FileTree({
  zipName,
  entries,
}: {
  zipName: string;
  entries: Array<{ path: string; content: string; edited: boolean }>;
}): ReactNode {
  return (
    <div
      data-testid="bundle-tree"
      style={{
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 12.5,
        lineHeight: 1.7,
        background: "#fafafa",
        border: "1px solid #e4e4e7",
        borderRadius: 8,
        padding: "12px 14px",
        color: "#1c1c20",
        whiteSpace: "pre",
        overflowX: "auto",
      }}
    >
      <div>{zipName}</div>
      {entries.map((entry, i) => {
        const isLast = i === entries.length - 1;
        const branch = isLast ? "└── " : "├── ";
        const size = formatSize(byteLength(entry.content));
        return (
          <div key={entry.path} data-testid={`bundle-entry-${entry.path}`}>
            <span>{branch}</span>
            <span>{entry.path}</span>
            <span style={{ color: "#797980" }}>
              {pad(entry.path, 36)} ({size}
              {entry.edited ? ", edited" : ""})
            </span>
          </div>
        );
      })}
    </div>
  );
}

function pad(path: string, target: number): string {
  const spaces = Math.max(2, target - path.length);
  return " ".repeat(spaces);
}

function byteLength(s: string): number {
  // approximate utf-8 byte length without needing TextEncoder in older envs.
  if (typeof TextEncoder !== "undefined") {
    return new TextEncoder().encode(s).length;
  }
  return s.length;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  return `${mb.toFixed(2)} MB`;
}

// --- derivation helpers ---

export function deriveShortSpecId(spec: PolicySpec): string {
  const hash = spec.recording_ref?.hash;
  if (typeof hash === "string" && hash.length >= 8) {
    return hash.slice(0, 8);
  }
  return "unknown";
}

export function deriveWasmName(artifacts: PolicyArtifacts): string {
  const cargo = artifacts.generated_sources[0]?.cargo_toml ?? "";
  // parse top-level `name = "..."` from [package]. simple regex; the cargo
  // template is generated server-side and we only need one line.
  const m = cargo.match(/^\s*name\s*=\s*"([^"]+)"/m);
  if (m) return m[1].replace(/-/g, "_");
  // fallback matches crates/oz-policy-codegen/src/render.rs::generated_cargo_toml.
  return "oz_policy_generated_slot_0";
}

// --- README + install snippet rendering ---

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

# install on your smart account (replace ACCOUNT, POLICY_ADDR, CONTEXT_RULE_ID)
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
- Project repository: ${PROJECT_REPO_URL_PLACEHOLDER}
`;
}

// --- sim report md rendering ---

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

// --- divergence md rendering (tiny unified diff) ---

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

// minimal unified diff. line-oriented, no hunk headers (the file is short
// enough to read whole). keeps the implementation under 100 LOC per spec.
function unifiedDiff(a: string, b: string): string {
  const aLines = a.split("\n");
  const bLines = b.split("\n");
  // LCS table for line-level diff.
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

// --- styles ---

const inlineCode: React.CSSProperties = {
  fontFamily: "'JetBrains Mono', monospace",
  fontSize: 12,
  background: "rgba(28,28,33,0.06)",
  border: "1px solid #e4e4e7",
  padding: "1px 6px",
  borderRadius: 5,
};

const primaryBtn: React.CSSProperties = {
  width: "100%",
  background: "#1c1c20",
  color: "#fbfbfb",
  fontFamily: "'JetBrains Mono', monospace",
  fontWeight: 600,
  fontSize: 14,
  border: "none",
  borderRadius: 11,
  padding: 15,
  letterSpacing: "0.02em",
};
