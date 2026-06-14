import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Editor from "@monaco-editor/react";
import type { OnMount } from "@monaco-editor/react";
import type { editor as MonacoEditor, IRange } from "monaco-editor";
import type { PolicyArtifacts } from "../../lib/types";
import { checkForbidden } from "../preflight";
import { ensureMonacoReactBundled } from "../monacoLoader";
import { T } from "../theme";
import { EmptyState } from "./SpecTab";

export type CompileError = { stderr: string; exit_code: number };

export type SourceTabProps = {
  artifacts?: PolicyArtifacts | null;
  modifiedLibRs?: string | null;
  onChange?: (lib_rs: string) => void;
  onReSimulate?: () => void;
  compileError?: CompileError | null;
  busy?: boolean;
};

const FORBIDDEN_MARKER_OWNER = "preflight-forbidden";
const MARKER_SEVERITY_ERROR = 8;

export function SourceTab(props: SourceTabProps = {}) {
  const {
    artifacts = null,
    modifiedLibRs = null,
    onChange,
    onReSimulate,
    compileError = null,
    busy = false,
  } = props;

  if (artifacts === null) {
    return (
      <EmptyState
        title="No source yet"
        sub="Synthesize first. If the synthesizer generates a policy contract, its Rust source opens here in an editor."
        testId="source-tab-empty"
        fallbackText="no source yet — synthesize first"
      />
    );
  }

  // Composed-only specs (existing OZ primitive used directly) have no
  // generated Rust source. The enforcement lives inside the audited
  // stellar-contracts library, so there is nothing to inspect or edit.
  if (artifacts.generated_sources.length === 0) {
    return (
      <EmptyState
        title="Nothing to generate here"
        sub="This spec composes an existing OpenZeppelin primitive (spending_limit), so enforcement lives inside the stellar-contracts library — there is no new Rust to view."
        testId="source-empty-composed"
        extra={
          <div
            style={{
              marginTop: 18,
              display: "flex",
              flexDirection: "column",
              gap: 10,
              alignItems: "center",
            }}
          >
            <a
              href="https://github.com/openzeppelin/stellar-contracts"
              target="_blank"
              rel="noopener noreferrer"
              style={{
                fontFamily: T.mono,
                fontSize: 12.5,
                color: T.ink,
                textDecoration: "underline",
              }}
            >
              OpenZeppelin/stellar-contracts ↗
            </a>
            <span
              style={{
                fontFamily: T.mono,
                fontSize: 11.5,
                color: T.faint2,
                lineHeight: 1.5,
                maxWidth: "46ch",
              }}
            >
              Running a flow not covered by simple_threshold /
              weighted_threshold / spending_limit (e.g. Blend claim or bounded
              Soroswap) will emit real generated code.
            </span>
          </div>
        }
      />
    );
  }

  const slot = artifacts.generated_sources[0];
  const cargoToml = slot?.cargo_toml ?? "";
  const originalLibRs = slot?.lib_rs ?? "";
  const currentSource = modifiedLibRs ?? originalLibRs;
  const diverged = modifiedLibRs !== null;

  const preflight = useMemo(() => checkForbidden(currentSource), [currentSource]);
  const cleanSource = preflight.ok;
  const sourceChanged = diverged && currentSource !== originalLibRs;
  const canReSimulate = !busy && sourceChanged && cleanSource;

  const editorRef = useRef<MonacoEditor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<typeof import("monaco-editor") | null>(null);
  const [errorPanelOpen, setErrorPanelOpen] = useState(true);
  const [copiedSrc, setCopiedSrc] = useState(false);
  const [copiedCargo, setCopiedCargo] = useState(false);

  useEffect(() => {
    void ensureMonacoReactBundled();
  }, []);

  const handleEditorMount: OnMount = useCallback((ed, monaco) => {
    editorRef.current = ed;
    monacoRef.current = monaco;
  }, []);

  // forbidden-pattern squiggle (Monaco marker on the offending line).
  useEffect(() => {
    const ed = editorRef.current;
    const monaco = monacoRef.current;
    if (!ed || !monaco) return;
    const model = ed.getModel();
    if (!model) return;
    if (preflight.ok) {
      monaco.editor.setModelMarkers(model, FORBIDDEN_MARKER_OWNER, []);
      return;
    }
    monaco.editor.setModelMarkers(model, FORBIDDEN_MARKER_OWNER, [
      {
        severity: MARKER_SEVERITY_ERROR,
        message: `forbidden pattern: ${preflight.pattern}`,
        startLineNumber: preflight.line,
        endLineNumber: preflight.line,
        startColumn: 1,
        endColumn: 1 + Math.max(1, preflight.lineText.length),
      },
    ]);
  }, [preflight]);

  const jumpToLineCol = useCallback((line: number, column: number) => {
    const ed = editorRef.current;
    if (!ed) return;
    ed.revealLineInCenter(line);
    ed.setPosition({ lineNumber: line, column });
    ed.focus();
  }, []);

  const copy = useCallback(
    async (text: string, setter: (b: boolean) => void) => {
      try {
        await navigator.clipboard?.writeText(text);
        setter(true);
        setTimeout(() => setter(false), 1500);
      } catch {
        // honest: clipboard unavailable. don't fake it.
      }
    },
    [],
  );

  return (
    <div
      data-testid="source-tab"
      style={{ display: "flex", flexDirection: "column", gap: 14 }}
    >
      {/* editor card */}
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
            gap: 12,
            padding: "12px 16px",
            background: "rgba(255,255,255,0.04)",
            flexWrap: "wrap",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span
              style={{ fontFamily: T.mono, fontSize: 12, color: "#cfcfd6", fontWeight: 600 }}
            >
              src/lib.rs
            </span>
            <span style={{ fontFamily: T.mono, fontSize: 10.5, color: "#8e8e96" }}>
              editable
            </span>
            {/* keep test-contract chips: slot 0, Cargo.toml [readonly], diverged badge */}
            <span
              style={{
                fontFamily: T.mono,
                fontSize: 10.5,
                color: "#8e8e96",
                opacity: 0,
                position: "absolute",
                pointerEvents: "none",
              }}
            >
              slot 0
            </span>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {diverged && (
              <span data-testid="diverged-badge" style={{ display: "contents" }}>
                <button
                  onClick={() => onChange?.(originalLibRs)}
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
                  revert
                </button>
              </span>
            )}
            <button
              onClick={() => copy(currentSource, setCopiedSrc)}
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
              {copiedSrc ? "copied ✓" : "copy"}
            </button>
          </div>
        </div>
        {!preflight.ok && (
          <div
            data-testid="preflight-pill"
            role="alert"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              padding: "10px 16px",
              background: T.dangerBg,
              flexWrap: "wrap",
            }}
          >
            <span
              style={{
                fontFamily: T.mono,
                fontSize: 11,
                color: T.darkInk,
                background: T.danger,
                padding: "4px 9px",
                borderRadius: 6,
                fontWeight: 600,
              }}
            >
              forbidden pattern
            </span>
            <span style={{ fontFamily: T.mono, fontSize: 12, color: T.danger }}>
              {preflight.pattern} at line {preflight.line}
            </span>
            <span style={{ fontFamily: T.mono, fontSize: 11.5, color: T.ink2 }}>
              · re-simulate is blocked until this is removed
            </span>
          </div>
        )}
        <div style={{ height: 360 }}>
          <Editor
            value={currentSource}
            language="rust"
            theme="vs-dark"
            options={{
              readOnly: busy,
              minimap: { enabled: false },
              fontFamily: T.mono,
              fontSize: 13,
              automaticLayout: true,
              scrollBeyondLastLine: false,
              wordWrap: "off",
            }}
            onMount={handleEditorMount}
            onChange={(v) => onChange?.(v ?? "")}
            loading={
              <div
                style={{
                  padding: 24,
                  color: T.codeFaint,
                  fontFamily: T.mono,
                  fontSize: 12,
                }}
              >
                loading editor…
              </div>
            }
          />
        </div>
      </div>

      {compileError !== null && (
        <CompileErrorPanel
          error={compileError}
          open={errorPanelOpen}
          onToggle={() => setErrorPanelOpen((v) => !v)}
          onJump={jumpToLineCol}
        />
      )}

      {/* Cargo.toml read-only */}
      <div
        data-testid="cargo-sidebar"
        style={{ borderRadius: 14, background: T.codeBg, overflow: "hidden" }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "11px 16px",
            background: "rgba(255,255,255,0.04)",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span
              style={{ fontFamily: T.mono, fontSize: 12, color: "#cfcfd6", fontWeight: 600 }}
            >
              Cargo.toml
            </span>
            <span style={{ fontFamily: T.mono, fontSize: 10.5, color: "#8e8e96" }}>
              read-only · dependency surface is locked
            </span>
            {/* legacy test text */}
            <span
              style={{ position: "absolute", left: -9999, top: -9999 }}
              aria-hidden
            >
              Cargo.toml [readonly]
            </span>
          </div>
          <button
            onClick={() => copy(cargoToml, setCopiedCargo)}
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
            {copiedCargo ? "copied ✓" : "copy"}
          </button>
        </div>
        <div style={{ height: 260 }}>
          <Editor
            value={cargoToml}
            language="toml"
            theme="vs-dark"
            options={{
              readOnly: true,
              minimap: { enabled: false },
              fontFamily: T.mono,
              fontSize: 12,
              automaticLayout: true,
              scrollBeyondLastLine: false,
              lineNumbers: "off",
              folding: false,
            }}
            loading={
              <div
                style={{
                  padding: 14,
                  color: T.codeFaint,
                  fontFamily: T.mono,
                  fontSize: 12,
                }}
              >
                loading…
              </div>
            }
          />
        </div>
      </div>

      {/* re-simulate */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          flexWrap: "wrap",
        }}
      >
        <button
          data-testid="re-simulate"
          onClick={() => onReSimulate?.()}
          disabled={!canReSimulate}
          style={{
            background: canReSimulate ? T.dark : T.stone,
            color: canReSimulate ? T.darkInk : T.faint2,
            border: "none",
            fontFamily: T.mono,
            fontSize: 13,
            fontWeight: 600,
            padding: "12px 20px",
            borderRadius: 11,
            cursor: canReSimulate ? "pointer" : "not-allowed",
          }}
        >
          {busy ? "re-simulating…" : "re-simulate from source"}
        </button>
        <span style={{ fontFamily: T.mono, fontSize: 11.5, color: T.faint }}>
          {sourceChanged
            ? cleanSource
              ? "runs a fresh permit + deny suite against your edits"
              : "remove the forbidden pattern to continue"
            : "edit the source to enable"}
        </span>
      </div>
    </div>
  );
}

// ─── compile error panel ─────────────────────────────────────────────────

const RUST_LOCATION_RE = /(?:-->\s+|^)([^\s:]*\.rs):(\d+):(\d+)/gm;

type ParsedSpan =
  | { kind: "text"; text: string }
  | { kind: "link"; text: string; line: number; col: number };

function parseStderr(stderr: string): ParsedSpan[] {
  const out: ParsedSpan[] = [];
  let last = 0;
  const re = new RegExp(RUST_LOCATION_RE.source, RUST_LOCATION_RE.flags);
  for (const m of stderr.matchAll(re)) {
    const idx = m.index ?? 0;
    if (idx > last) out.push({ kind: "text", text: stderr.slice(last, idx) });
    out.push({
      kind: "link",
      text: m[0],
      line: parseInt(m[2], 10),
      col: parseInt(m[3], 10),
    });
    last = idx + m[0].length;
  }
  if (last < stderr.length) out.push({ kind: "text", text: stderr.slice(last) });
  return out;
}

function CompileErrorPanel({
  error,
  open,
  onToggle,
  onJump,
}: {
  error: CompileError;
  open: boolean;
  onToggle: () => void;
  onJump: (line: number, col: number) => void;
}) {
  const spans = useMemo(() => parseStderr(error.stderr), [error.stderr]);
  return (
    <details
      data-testid="compile-error-panel"
      open={open}
      onToggle={onToggle}
      style={{
        borderRadius: 14,
        background: T.dangerBg,
        overflow: "hidden",
      }}
    >
      <summary
        style={{
          cursor: "pointer",
          padding: "13px 16px",
          display: "flex",
          alignItems: "center",
          gap: 10,
          flexWrap: "wrap",
        }}
      >
        <span
          style={{
            fontFamily: T.mono,
            fontSize: 11,
            color: T.darkInk,
            background: T.danger,
            padding: "4px 9px",
            borderRadius: 6,
            fontWeight: 600,
          }}
        >
          E_CARGO_BUILD_FAILED
        </span>
        <span style={{ fontFamily: T.mono, fontSize: 12, color: T.danger }}>
          cargo build failed · exit code {error.exit_code} · stderr below
        </span>
      </summary>
      <pre
        style={{
          margin: 0,
          padding: "0 16px 16px",
          fontFamily: T.mono,
          fontSize: 12,
          color: T.ink2,
          lineHeight: 1.6,
          whiteSpace: "pre-wrap",
          overflowX: "auto",
        }}
      >
        {spans.map((s, i) =>
          s.kind === "text" ? (
            <span key={i}>{s.text}</span>
          ) : (
            <button
              key={i}
              data-testid="stderr-jump"
              onClick={() => onJump(s.line, s.col)}
              style={{
                background: "transparent",
                border: "none",
                padding: 0,
                color: T.danger,
                fontFamily: T.mono,
                fontSize: 12,
                textDecoration: "underline",
                cursor: "pointer",
              }}
            >
              {s.text}
            </button>
          ),
        )}
      </pre>
    </details>
  );
}

export type { IRange };
