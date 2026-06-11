// SourceTab — the edit half of the /playground edit-loop (spec §4.2). The
// user lands on this tab to inspect the generated Rust, optionally edit
// `lib.rs`, then click `re-simulate` to push the edit through the same
// sandbox + simhost pipeline. Cargo.toml is locked (spec §6.2) so it lives
// in a read-only sidebar.
//
// design tokens come from spec §8 — same palette as Synthesizer.tsx
// (#1c1c20 ink, #fbfbfb/#fafafa surfaces, #e4e4e7 borders, JetBrains Mono
// labels, panel shadow `0 12px 34px -20px rgba(22,24,21,0.35)`). No new
// tokens, no Tailwind, no CSS modules — inline styles only.
//
// every prop is optional so that the PlaygroundPage shell (which is owned
// by a different agent and still passes no props) keeps compiling. With
// no props the tab renders the empty state ("no source yet — synthesize
// first"), which is what the shell currently wants.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Editor from "@monaco-editor/react";
import type { OnMount } from "@monaco-editor/react";
import type { editor as MonacoEditor, IRange } from "monaco-editor";
import type { PolicyArtifacts } from "../../lib/types";
import { checkForbidden } from "../preflight";
import { ensureMonacoReactBundled } from "../monacoLoader";

export type CompileError = { stderr: string; exit_code: number };

export type SourceTabProps = {
  artifacts?: PolicyArtifacts | null;
  /** null when the user has not diverged from the synthesized source. */
  modifiedLibRs?: string | null;
  onChange?: (lib_rs: string) => void;
  onReSimulate?: () => void;
  compileError?: CompileError | null;
  busy?: boolean;
};

// ----- style tokens --------------------------------------------------------
// inlined rather than imported so the file is self-contained for review.

const TOKEN = {
  ink: "#1c1c20",
  surfaceA: "#fbfbfb",
  surfaceB: "#fafafa",
  border: "#e4e4e7",
  muted: "#54545a",
  faint: "#797980",
  ghost: "#a0a0a8",
  error: "#dc2626",
  errorBg: "rgba(220,38,38,0.08)",
  warnBg: "rgba(28,28,33,0.06)",
  shadow: "0 12px 34px -20px rgba(22,24,21,0.35)",
  monoFont: "'JetBrains Mono', monospace",
  bodyFont: "'Hanken Grotesk', sans-serif",
} as const;

const FORBIDDEN_MARKER_OWNER = "preflight-forbidden";

// `monaco.MarkerSeverity.Error` — we hard-code the numeric value (`8`)
// rather than reach into the lazy-loaded monaco namespace at module
// scope. Stable since Monaco 0.10.x.
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

  // empty state — no artifacts yet, so nothing to show. honest empty marker.
  if (artifacts === null) {
    return (
      <div
        data-testid="source-tab-empty"
        style={{
          padding: 36,
          color: TOKEN.ghost,
          fontFamily: TOKEN.bodyFont,
          fontSize: 14,
        }}
      >
        no source yet — synthesize first
      </div>
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
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [errorPanelOpen, setErrorPanelOpen] = useState(true);

  // route `@monaco-editor/react` away from its default CDN loader and point
  // it at our locally bundled `monaco-editor`. This is what causes Vite to
  // emit Monaco as a separate dynamic chunk (`dist/assets/monaco-*.js`)
  // rather than skipping it entirely (the react wrapper would otherwise
  // fetch Monaco from JSDelivr at runtime, which we explicitly disallow
  // — see monacoLoader.ts docs).
  useEffect(() => {
    void ensureMonacoReactBundled();
  }, []);

  const handleEditorMount: OnMount = useCallback((ed, monaco) => {
    editorRef.current = ed;
    monacoRef.current = monaco;
  }, []);

  // apply / clear the red squiggle for the preflight hit. Monaco markers
  // are owner-scoped, so we only ever touch our own.
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

  return (
    <div
      data-testid="source-tab"
      style={{
        display: "grid",
        gridTemplateColumns: sidebarOpen ? "1fr 320px" : "1fr auto",
        gap: 0,
        background: TOKEN.surfaceA,
        fontFamily: TOKEN.bodyFont,
        color: TOKEN.ink,
      }}
    >
      {/* main editor column */}
      <section style={{ display: "flex", flexDirection: "column" }}>
        <HeaderRow diverged={diverged} />
        <div style={{ borderTop: `1px solid ${TOKEN.border}`, height: 420 }}>
          <Editor
            value={currentSource}
            language="rust"
            theme="vs-dark"
            options={{
              readOnly: busy,
              minimap: { enabled: false },
              fontFamily: TOKEN.monoFont,
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
                  color: TOKEN.ghost,
                  fontFamily: TOKEN.monoFont,
                  fontSize: 12,
                }}
              >
                loading editor…
              </div>
            }
          />
        </div>
        <ActionRow
          canReSimulate={canReSimulate}
          busy={busy}
          sourceChanged={sourceChanged}
          preflight={preflight}
          onReSimulate={() => onReSimulate?.()}
        />
        {compileError !== null && (
          <CompileErrorPanel
            error={compileError}
            open={errorPanelOpen}
            onToggle={() => setErrorPanelOpen((v) => !v)}
            onJump={jumpToLineCol}
          />
        )}
      </section>

      {/* read-only Cargo.toml sidebar */}
      <CargoSidebar
        open={sidebarOpen}
        onToggle={() => setSidebarOpen((v) => !v)}
        cargoToml={cargoToml}
      />
    </div>
  );
}

// ----- header row ----------------------------------------------------------

function HeaderRow({ diverged }: { diverged: boolean }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "10px 14px",
        background: TOKEN.surfaceB,
        flexWrap: "wrap",
      }}
    >
      <Chip>slot 0</Chip>
      <Chip muted>Cargo.toml [readonly]</Chip>
      <span
        style={{
          fontFamily: TOKEN.monoFont,
          fontSize: 12,
          color: TOKEN.muted,
          letterSpacing: "0.02em",
        }}
      >
        src/lib.rs
      </span>
      {diverged && (
        <span
          data-testid="diverged-badge"
          style={{
            fontFamily: TOKEN.monoFont,
            fontSize: 11,
            color: TOKEN.error,
            background: TOKEN.errorBg,
            border: `1px solid ${TOKEN.error}`,
            padding: "3px 8px",
            borderRadius: 6,
            letterSpacing: "0.02em",
          }}
        >
          diverged from spec
        </span>
      )}
    </div>
  );
}

function Chip({
  children,
  muted = false,
}: {
  children: React.ReactNode;
  muted?: boolean;
}) {
  return (
    <span
      style={{
        fontFamily: TOKEN.monoFont,
        fontSize: 11,
        color: TOKEN.ink,
        opacity: muted ? 0.75 : 1,
        background: TOKEN.warnBg,
        border: `1px solid ${TOKEN.border}`,
        padding: "3px 8px",
        borderRadius: 6,
        letterSpacing: "0.02em",
      }}
    >
      {children}
    </span>
  );
}

// ----- action row ----------------------------------------------------------

function ActionRow({
  canReSimulate,
  busy,
  sourceChanged,
  preflight,
  onReSimulate,
}: {
  canReSimulate: boolean;
  busy: boolean;
  sourceChanged: boolean;
  preflight: ReturnType<typeof checkForbidden>;
  onReSimulate: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        gap: 12,
        padding: "12px 14px",
        borderTop: `1px solid ${TOKEN.border}`,
        background: TOKEN.surfaceA,
        flexWrap: "wrap",
      }}
    >
      <button
        type="button"
        data-testid="re-simulate"
        disabled={!canReSimulate}
        onClick={onReSimulate}
        style={{
          background: canReSimulate ? TOKEN.ink : TOKEN.warnBg,
          color: canReSimulate ? "#ffffff" : TOKEN.muted,
          border: `1px solid ${canReSimulate ? TOKEN.ink : TOKEN.border}`,
          borderRadius: 8,
          padding: "8px 16px",
          fontFamily: TOKEN.monoFont,
          fontSize: 12,
          letterSpacing: "0.04em",
          cursor: canReSimulate ? "pointer" : "not-allowed",
        }}
      >
        {busy ? "simulating…" : "re-simulate"}
      </button>

      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        {!preflight.ok && (
          <span
            data-testid="preflight-pill"
            role="alert"
            style={{
              fontFamily: TOKEN.monoFont,
              fontSize: 11.5,
              color: TOKEN.error,
              background: TOKEN.errorBg,
              border: `1px solid ${TOKEN.error}`,
              padding: "4px 10px",
              borderRadius: 6,
            }}
          >
            forbidden pattern: {preflight.pattern} at line {preflight.line}
          </span>
        )}
        {preflight.ok && !sourceChanged && (
          <span
            style={{
              fontFamily: TOKEN.monoFont,
              fontSize: 11,
              color: TOKEN.faint,
            }}
          >
            no changes to simulate
          </span>
        )}
        {preflight.ok && sourceChanged && (
          <span
            style={{
              fontFamily: TOKEN.monoFont,
              fontSize: 11,
              color: TOKEN.muted,
            }}
          >
            preflight ok
          </span>
        )}
      </div>
    </div>
  );
}

// ----- compile error panel -------------------------------------------------

// Rust's standard rustc output looks like:
//   error[E0425]: cannot find value `x` in this scope
//    --> src/lib.rs:12:9
// we surface every (line, col) hit as a click-jump into Monaco. The regex
// is intentionally generous — Rust's `--> path:line:col` shape is stable
// across cargo / rustc, but we don't anchor on the leading arrow because
// `cargo build --message-format=short` drops it.
const RUST_LOCATION_RE = /(?:-->\s+|^)([^\s:]*\.rs):(\d+):(\d+)/gm;

type ParsedSpan = { kind: "text"; text: string } | {
  kind: "link";
  text: string;
  line: number;
  col: number;
};

function parseStderr(stderr: string): ParsedSpan[] {
  const out: ParsedSpan[] = [];
  let last = 0;
  // Reset regex state — using `matchAll` is cleaner but exhausts a global
  // regex's lastIndex across calls otherwise.
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
    <div
      data-testid="compile-error-panel"
      style={{
        borderTop: `1px solid ${TOKEN.border}`,
        background: TOKEN.errorBg,
      }}
    >
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        style={{
          width: "100%",
          textAlign: "left",
          background: "transparent",
          border: "none",
          padding: "10px 14px",
          fontFamily: TOKEN.monoFont,
          fontSize: 12,
          color: TOKEN.error,
          cursor: "pointer",
          letterSpacing: "0.02em",
        }}
      >
        {open ? "▾" : "▸"} cargo build failed — exit code {error.exit_code}
      </button>
      {open && (
        <pre
          style={{
            margin: 0,
            padding: "10px 14px 14px",
            fontFamily: TOKEN.monoFont,
            fontSize: 12,
            color: TOKEN.ink,
            background: TOKEN.surfaceA,
            whiteSpace: "pre-wrap",
            wordBreak: "break-word",
            maxHeight: 320,
            overflow: "auto",
          }}
        >
          {spans.map((s, i) =>
            s.kind === "text" ? (
              <span key={i}>{s.text}</span>
            ) : (
              <button
                key={i}
                type="button"
                data-testid="stderr-jump"
                onClick={() => onJump(s.line, s.col)}
                style={{
                  background: "transparent",
                  border: "none",
                  padding: 0,
                  color: TOKEN.error,
                  fontFamily: TOKEN.monoFont,
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
      )}
    </div>
  );
}

// ----- cargo sidebar -------------------------------------------------------

function CargoSidebar({
  open,
  onToggle,
  cargoToml,
}: {
  open: boolean;
  onToggle: () => void;
  cargoToml: string;
}) {
  return (
    <aside
      data-testid="cargo-sidebar"
      style={{
        borderLeft: `1px solid ${TOKEN.border}`,
        display: "flex",
        flexDirection: "column",
        background: TOKEN.surfaceB,
        minWidth: open ? 320 : 28,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "10px 12px",
          borderBottom: `1px solid ${TOKEN.border}`,
        }}
      >
        {open && (
          <span
            style={{
              fontFamily: TOKEN.monoFont,
              fontSize: 11.5,
              color: TOKEN.muted,
              letterSpacing: "0.04em",
            }}
          >
            Cargo.toml
          </span>
        )}
        <button
          type="button"
          aria-label={open ? "collapse cargo sidebar" : "expand cargo sidebar"}
          onClick={onToggle}
          style={{
            background: "transparent",
            border: "none",
            cursor: "pointer",
            fontFamily: TOKEN.monoFont,
            fontSize: 13,
            color: TOKEN.muted,
            padding: 0,
          }}
        >
          {open ? "▸" : "◂"}
        </button>
      </div>
      {open && (
        <div style={{ flex: 1, minHeight: 420 }}>
          <Editor
            value={cargoToml}
            language="toml"
            theme="vs-dark"
            options={{
              readOnly: true,
              minimap: { enabled: false },
              fontFamily: TOKEN.monoFont,
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
                  color: TOKEN.ghost,
                  fontFamily: TOKEN.monoFont,
                  fontSize: 12,
                }}
              >
                loading…
              </div>
            }
          />
        </div>
      )}
    </aside>
  );
}

// silence the unused import warning when this file is consumed without
// the editor ever mounting (e.g. in jsdom tests via the vi.mock shim).
export type { IRange };
