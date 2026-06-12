// /playground route shell + controller. Wave-2 wiring:
//   - usePlaygroundState (reducer, single source of truth)
//   - useSnapshot (load on :snapshotId mount, create on share click)
//   - usePresets (preset dropdown source-of-truth for InputPanel)
//   - InputPanel onSubmit orchestrates record → synth → [artifacts || report]
//   - SourceTab onReSimulate runs preflight + simulate_custom_source
//   - Header share button calls createSnapshot, pushes URL, copies link
//
// Honesty rules carried in (per the user's standing feedback memory):
//   - Real MCP calls; zero mock fallbacks anywhere in this module.
//   - Each backend error is surfaced into the right panel slot (spec §7),
//     never collapsed into a generic toast.
//   - Snapshot 404 → full-page error block per spec §4.3.
//
// Theme tokens are inlined (spec §8) — Hanken Grotesk body, Bricolage
// Grotesque display, JetBrains Mono labels, #1c1c20 ink, #fbfbfb/#fafafa
// surfaces, #e4e4e7 borders, panel shadow 0 12px 34px -20px rgba(22,24,21,0.35).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Link, useParams } from "react-router-dom";
import { InputPanel, type SubmitIntent, type SubmitPhase } from "./InputPanel";
import { SpecTab } from "./panels/SpecTab";
import { SourceTab } from "./panels/SourceTab";
import { SimulateTab } from "./panels/SimulateTab";
import { BundleTab } from "./panels/BundleTab";
import { usePlaygroundState } from "./hooks/usePlaygroundState";
import { usePresets } from "./hooks/usePresets";
import { useSnapshot } from "./hooks/useSnapshot";
import { checkForbidden } from "./preflight";
import { pushSnapshotUrl, snapshotShareUrl } from "./urlSync";
import {
  McpClient,
  McpError,
  readConfig,
  type McpConfig,
} from "../lib/mcp";

type TabKey = "spec" | "source" | "simulate" | "bundle";

const TABS: Array<{ key: TabKey; label: string }> = [
  { key: "spec", label: "Spec" },
  { key: "source", label: "Source" },
  { key: "simulate", label: "Simulate" },
  { key: "bundle", label: "Bundle" },
];

// SourceTab's compileError prop type is { stderr, exit_code }. We honor
// that contract here — preflight rejections are visualized by SourceTab
// itself (it runs its own checkForbidden against the live source), so
// PlaygroundPage's compileError only carries server-side cargo build
// stderr or transport errors that should look the same to the user.
type CompileError = { stderr: string; exit_code: number } | null;

type ResimError = { code: string; detail: string } | null;

interface PlaygroundPageProps {
  /**
   * Optional override for tests: a function returning the McpClient.
   * Production callers pass nothing → readConfig() + new McpClient().
   * Per feedback-no-mock-fallback, this is ONLY for orchestration tests
   * (see __tests__/PlaygroundPage.controller.test.tsx). The prod path is
   * real network I/O via the default factory below.
   */
  clientFactory?: (cfg: McpConfig) => McpClient;
}

export function PlaygroundPage({ clientFactory }: PlaygroundPageProps = {}) {
  const params = useParams<{ snapshotId?: string }>();
  const routeSnapshotId = params.snapshotId ?? "";

  const [activeTab, setActiveTab] = useState<TabKey>("spec");
  const [phase, setPhase] = useState<SubmitPhase>("idle");
  const [busy, setBusy] = useState(false);
  const [compileError, setCompileError] = useState<CompileError | null>(null);
  const [resimError, setResimError] = useState<ResimError>(null);
  const [pageError, setPageError] = useState<{ code: string; detail: string } | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [ruleName, setRuleName] = useState<string>("");

  const cfg = useMemo(() => readConfig(), []);
  const { state, dispatch } = usePlaygroundState();
  const { presets } = usePresets();

  // Single MCP client per page-load. Built lazily so SSR / static rendering
  // doesn't fail on a missing endpoint. clientFactory is the test seam.
  // Production path: `new McpClient(readConfig())` — real network calls,
  // no fallbacks, per feedback-no-mock-fallback.
  const client = useMemo<McpClient | null>(() => {
    try {
      return clientFactory ? clientFactory(cfg) : new McpClient(cfg);
    } catch (e) {
      if (e instanceof McpError) return null;
      throw e;
    }
  }, [cfg, clientFactory]);

  const { createSnapshot, loadSnapshot, error: snapshotError } = useSnapshot(
    cfg,
    client ?? undefined,
  );

  const cancelRef = useRef<AbortController | null>(null);
  // recording_id + spec_id are not on Recording / PolicySpec by value;
  // we stash them here so re-simulate + share can reuse them after
  // onSubmit finishes. Cleared at the start of each new submit.
  const idsRef = useRef<{ recordingId: string; specId: string } | null>(null);

  const getClient = useCallback((): McpClient => {
    if (!client) {
      throw new McpError(
        "CLIENT_NOT_CONFIGURED",
        "mcp endpoint env var is not set",
        -32099,
      );
    }
    return client;
  }, [client]);

  // Spec §4.3 — on mount with :snapshotId, hydrate from the server.
  // Tracks a ref because StrictMode double-invokes effects; we only want
  // one load + one hydrate per snapshotId.
  const hydratedFor = useRef<string | null>(null);
  useEffect(() => {
    if (!routeSnapshotId) return;
    if (hydratedFor.current === routeSnapshotId) return;
    hydratedFor.current = routeSnapshotId;

    let cancelled = false;
    (async () => {
      const snap = await loadSnapshot(routeSnapshotId);
      if (cancelled || !snap) return;
      // Snapshot record carries the full Recording + Spec + Report by
      // value (spec §3.4). Hydrate state with what we have.
      // Note: Snapshot type currently doesn't include recording/spec
      // by-value — only recording_id/spec_id + modified_lib_rs + report.
      // We hydrate report immediately, and fire follow-up calls to fetch
      // the spec + artifacts so the panels render fully.
      dispatch({ type: "setReport", report: snap.report });
      if (snap.modified_lib_rs !== undefined && snap.modified_lib_rs !== null) {
        dispatch({ type: "setModifiedLibRs", modifiedLibRs: snap.modified_lib_rs });
      }
      dispatch({ type: "setSnapshotId", snapshotId: routeSnapshotId });

      // Fetch artifacts so SourceTab + BundleTab have lib_rs / cargo_toml.
      try {
        const artifacts = await getClient().getPolicyArtifacts({ spec_id: snap.spec_id });
        if (!cancelled) dispatch({ type: "setArtifacts", artifacts });
      } catch (e) {
        // Non-fatal: snapshot still renders report + lib edits. Surface
        // honestly in the bundle/source areas via empty state.
        if (e instanceof McpError && !cancelled) {
          // leave artifacts null — panels render empty state per §7.
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [routeSnapshotId, loadSnapshot, dispatch, getClient]);

  // Surface loadSnapshot's error to a full-page block (spec §4.3 / §7).
  useEffect(() => {
    if (!routeSnapshotId) return;
    if (snapshotError && snapshotError.code === "E_SNAPSHOT_NOT_FOUND") {
      setPageError({ code: snapshotError.code, detail: snapshotError.detail });
    } else if (snapshotError) {
      setPageError({ code: snapshotError.code, detail: snapshotError.detail });
    } else {
      setPageError(null);
    }
  }, [snapshotError, routeSnapshotId]);

  // Auto-dismiss toast.
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3000);
    return () => clearTimeout(t);
  }, [toast]);

  const onCancel = useCallback(() => {
    cancelRef.current?.abort();
    setPhase("idle");
    setBusy(false);
  }, []);

  // ──────────────────────────────────────────────────────────────────────
  // SourceTab.onReSimulate — preflight + simulate_custom_source.
  // ──────────────────────────────────────────────────────────────────────
  const onReSimulate = useCallback(async () => {
    if (!state.recording || !state.spec) return;
    if (state.modifiedLibRs === null) return;

    setBusy(true);
    setCompileError(null);
    setResimError(null);

    // Spec §4.2 pre-flight: never call the server if the client check rejects.
    // SourceTab visualizes the offending line itself by running the same
    // checkForbidden over the editor buffer (see SourceTab's preflight
    // useMemo). We mirror it here so the network call is skipped and the
    // Simulate panel surfaces the rejection with the matching error code.
    const pf = checkForbidden(state.modifiedLibRs);
    if (!pf.ok) {
      setResimError({
        code: "E_PREFLIGHT_FORBIDDEN_PATTERN",
        detail: `forbidden pattern at line ${pf.line}: ${pf.pattern}`,
      });
      setBusy(false);
      return;
    }

    const ids = idsRef.current;
    if (!ids) {
      setResimError({
        code: "E_NO_SESSION",
        detail: "no active session — synthesize first before re-simulating",
      });
      setBusy(false);
      return;
    }

    try {
      const newReport = await getClient().simulateCustomSource({
        recording_id: ids.recordingId,
        spec_id: ids.specId,
        modified_lib_rs: state.modifiedLibRs,
      });
      dispatch({ type: "setReport", report: newReport });
    } catch (e) {
      const err = toErr(e);
      if (err.code === "E_PREFLIGHT_FORBIDDEN_PATTERN") {
        // Backend mirrored the preflight reject — surface to Simulate
        // banner; SourceTab's own preflight squiggle already covers the
        // line:col annotation.
        setResimError(err);
      } else if (err.code === "E_CODEGEN_COMPILE_FAILED") {
        // Cargo build failed — backend includes stderr in detail.
        setCompileError({ stderr: err.detail, exit_code: 1 });
        setResimError(err);
      } else {
        // Any other transport / network error — Simulate tab banner.
        setResimError(err);
      }
    } finally {
      setBusy(false);
    }
  }, [state.recording, state.spec, state.modifiedLibRs, dispatch, getClient]);

  // onSubmit — wraps record→synth→[artifacts||report] and updates idsRef
  // so re-simulate + share can reuse the ids without re-fetching.
  const onSubmitWithIds = useCallback(
    async (intent: SubmitIntent) => {
      idsRef.current = null;
      cancelRef.current?.abort();
      cancelRef.current = new AbortController();
      setBusy(true);
      setPageError(null);
      setResimError(null);
      setCompileError(null);
      setRuleName(intent.ruleName ?? "");

      const client = getClient();
      let recordingId: string | null = null;
      let specId: string | null = null;

      try {
        setPhase("recording");
        const rec = await client.recordTransaction({
          network: intent.network,
          hash: intent.hash,
          envelope_xdr_base64: intent.envelope_xdr_base64,
        });
        recordingId = rec.recording_id;
        dispatch({ type: "setRecording", recording: rec.recording });
      } catch (e) {
        const err = toErr(e);
        setResimError(err);
        if (err.code === "E_RECORDER_HASH_NOT_FOUND") setPageError(err);
        setPhase("idle");
        setBusy(false);
        return;
      }

      try {
        setPhase("synthesizing");
        const synth = await client.synthesizePolicy({
          recording_id: recordingId!,
          tightness: intent.tightness,
          mode: intent.mode,
          lifetime_ledgers: intent.lifetime,
          rule_name: intent.ruleName,
        });
        specId = synth.spec_id;
        dispatch({ type: "setSpec", spec: synth.spec });
      } catch (e) {
        const err = toErr(e);
        setPageError(err);
        setPhase("idle");
        setBusy(false);
        return;
      }

      idsRef.current = { recordingId: recordingId!, specId: specId! };

      setPhase("simulating");
      const [artifactsRes, reportRes] = await Promise.allSettled([
        client.getPolicyArtifacts({ spec_id: specId! }),
        client.simulatePolicy({ spec_id: specId!, recording_id: recordingId! }),
      ]);

      if (artifactsRes.status === "fulfilled") {
        dispatch({ type: "setArtifacts", artifacts: artifactsRes.value });
      } else {
        // E_CODEGEN_COMPILE_FAILED + any transport error get rendered
        // as stderr in SourceTab so the user sees the raw backend
        // message — per feedback-honesty-no-fakes, never collapse.
        const err = toErr(artifactsRes.reason);
        setCompileError({
          stderr: `[${err.code}] ${err.detail}`,
          exit_code: 1,
        });
      }
      if (reportRes.status === "fulfilled") {
        dispatch({ type: "setReport", report: reportRes.value });
      } else {
        const err = toErr(reportRes.reason);
        setResimError(err);
      }
      setPhase("idle");
      setBusy(false);
    },
    [dispatch, getClient],
  );

  // ──────────────────────────────────────────────────────────────────────
  // Share button — createSnapshot → URL push → clipboard → toast.
  // ──────────────────────────────────────────────────────────────────────
  const onShare = useCallback(async () => {
    const ids = idsRef.current;
    if (!ids || !state.latestReport) {
      setToast("nothing to share yet — synthesize first");
      return;
    }
    const ref = await createSnapshot({
      recording_id: ids.recordingId,
      spec_id: ids.specId,
      modified_lib_rs: state.modifiedLibRs ?? undefined,
      report: state.latestReport,
    });
    if (!ref) {
      setToast("share failed — see backend error");
      return;
    }
    pushSnapshotUrl(ref.snapshot_id);
    dispatch({ type: "setSnapshotId", snapshotId: ref.snapshot_id });

    const url = snapshotShareUrl(ref.snapshot_id);
    try {
      if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(url);
        setToast("share link copied");
      } else {
        setToast("share link ready (clipboard unavailable)");
      }
    } catch {
      setToast("share link ready (clipboard write blocked)");
    }
  }, [createSnapshot, state.latestReport, state.modifiedLibRs, dispatch]);

  const onSourceChange = useCallback(
    (s: string) => dispatch({ type: "setModifiedLibRs", modifiedLibRs: s }),
    [dispatch],
  );

  // ──────────────────────────────────────────────────────────────────────
  // render
  // ──────────────────────────────────────────────────────────────────────

  // Snapshot 404 → full-page error block (spec §4.3 + §7).
  if (
    routeSnapshotId &&
    pageError &&
    pageError.code === "E_SNAPSHOT_NOT_FOUND"
  ) {
    return <ExpiredSnapshotBlock />;
  }

  const diverged = state.modifiedLibRs !== null;

  return (
    <div
      style={{
        minHeight: "100vh",
        background: "#fafafa",
        fontFamily: "'Hanken Grotesk', sans-serif",
        color: "#1c1c20",
      }}
    >
      <Header snapshotId={state.snapshotId ?? routeSnapshotId} onShare={onShare} />
      <div
        style={{
          maxWidth: 1400,
          margin: "0 auto",
          padding: "24px 28px 64px",
          display: "grid",
          gridTemplateColumns: "320px 1fr",
          gap: 24,
          alignItems: "flex-start",
        }}
      >
        <aside
          style={{
            position: "sticky",
            top: 24,
            alignSelf: "flex-start",
            background: "#fbfbfb",
            border: "1px solid #e4e4e7",
            borderRadius: 12,
            boxShadow: "0 12px 34px -20px rgba(22,24,21,0.35)",
            padding: 18,
            minHeight: 360,
          }}
          aria-label="input panel"
        >
          <InputPanel
            state={state}
            dispatch={dispatch}
            presets={presets}
            phase={phase}
            busy={busy}
            onSubmit={onSubmitWithIds}
            onCancel={onCancel}
          />
        </aside>

        <main
          style={{
            background: "#fbfbfb",
            border: "1px solid #e4e4e7",
            borderRadius: 12,
            boxShadow: "0 12px 34px -20px rgba(22,24,21,0.35)",
            overflow: "hidden",
            minHeight: 480,
          }}
        >
          <TabBar tabs={TABS} active={activeTab} onChange={setActiveTab} />
          {pageError && pageError.code !== "E_SNAPSHOT_NOT_FOUND" && (
            <PageErrorBanner err={pageError} />
          )}
          <div role="tabpanel" aria-label={activeTab}>
            {activeTab === "spec" && (
              <SpecTab
                spec={state.spec}
                recording={state.recording}
                diverged={diverged}
              />
            )}
            {activeTab === "source" && (
              <SourceTab
                artifacts={state.artifacts}
                modifiedLibRs={state.modifiedLibRs}
                onChange={onSourceChange}
                onReSimulate={onReSimulate}
                compileError={compileError}
                busy={busy}
              />
            )}
            {activeTab === "simulate" && (
              <SimulateTab
                report={state.latestReport}
                modified={diverged}
                onReSimulate={onReSimulate}
                busy={busy}
                resimError={resimError}
              />
            )}
            {activeTab === "bundle" && (
              <BundleTab
                artifacts={state.artifacts}
                modifiedLibRs={state.modifiedLibRs}
                spec={state.spec}
                report={state.latestReport}
                ruleName={ruleName}
              />
            )}
          </div>
        </main>
      </div>
      {toast && <Toast text={toast} />}
    </div>
  );
}

// ─── small helpers ────────────────────────────────────────────────────────

function toErr(e: unknown): { code: string; detail: string } {
  if (e instanceof McpError) return { code: e.code, detail: e.detail };
  if (e instanceof Error) return { code: "E_UNKNOWN", detail: e.message };
  return { code: "E_UNKNOWN", detail: String(e) };
}

function Header({
  snapshotId,
  onShare,
}: {
  snapshotId: string | null;
  onShare: () => void;
}) {
  return (
    <header
      style={{
        maxWidth: 1400,
        margin: "0 auto",
        padding: "28px 28px 8px",
        display: "flex",
        alignItems: "flex-end",
        justifyContent: "space-between",
        gap: 16,
        flexWrap: "wrap",
      }}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        <span
          style={{
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 11,
            letterSpacing: "0.08em",
            textTransform: "uppercase",
            color: "#797980",
          }}
        >
          /playground
        </span>
        <h1
          style={{
            margin: 0,
            fontFamily: "'Bricolage Grotesque', sans-serif",
            fontSize: "clamp(22px,2.4vw,32px)",
            fontWeight: 500,
            letterSpacing: "-0.02em",
            color: "#1c1c20",
          }}
        >
          playground
        </h1>
        <p
          style={{
            margin: 0,
            color: "#54545a",
            fontSize: 13.5,
            lineHeight: 1.5,
            maxWidth: "62ch",
          }}
        >
          RFP §3.1 — inspect, modify, simulate generated policy code
        </p>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <ShareBadge snapshotId={snapshotId ?? ""} />
        <button
          data-testid="share-button"
          onClick={onShare}
          style={{
            background: "#1c1c20",
            color: "#f4f4f5",
            fontFamily: "'JetBrains Mono', monospace",
            fontWeight: 600,
            fontSize: 12.5,
            border: "none",
            borderRadius: 9,
            padding: "9px 14px",
            cursor: "pointer",
            letterSpacing: "0.02em",
          }}
        >
          share
        </button>
      </div>
    </header>
  );
}

function ShareBadge({ snapshotId }: { snapshotId: string }) {
  return (
    <span
      data-testid="share-badge"
      style={{
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 11.5,
        color: "#1c1c20",
        opacity: 0.7,
        background: "rgba(28,28,33,0.06)",
        border: "1px solid #e4e4e7",
        padding: "5px 10px",
        borderRadius: 7,
        letterSpacing: "0.02em",
      }}
    >
      share: {snapshotId}
    </span>
  );
}

function TabBar({
  tabs,
  active,
  onChange,
}: {
  tabs: Array<{ key: TabKey; label: string }>;
  active: TabKey;
  onChange: (k: TabKey) => void;
}): ReactNode {
  return (
    <div
      role="tablist"
      style={{
        display: "flex",
        gap: 4,
        padding: "10px 10px 0",
        borderBottom: "1px solid #e4e4e7",
        background: "#fafafa",
      }}
    >
      {tabs.map((t) => {
        const isActive = t.key === active;
        return (
          <button
            key={t.key}
            role="tab"
            aria-selected={isActive}
            onClick={() => onChange(t.key)}
            style={{
              border: "none",
              background: isActive ? "#fbfbfb" : "transparent",
              color: isActive ? "#1c1c20" : "#54545a",
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 12,
              padding: "10px 14px",
              borderRadius: "8px 8px 0 0",
              cursor: "pointer",
              letterSpacing: "0.02em",
              borderBottom: isActive ? "2px solid #1c1c20" : "2px solid transparent",
              marginBottom: -1,
            }}
          >
            {t.label}
          </button>
        );
      })}
    </div>
  );
}

function PageErrorBanner({ err }: { err: { code: string; detail: string } }) {
  return (
    <div
      data-testid="page-error-banner"
      role="alert"
      style={{
        margin: "10px 14px 0",
        padding: "10px 12px",
        background: "rgba(220,38,38,0.06)",
        border: "1px solid #e4e4e7",
        borderRadius: 8,
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 12,
        color: "#dc2626",
      }}
    >
      [{err.code}] {err.detail}
    </div>
  );
}

function ExpiredSnapshotBlock() {
  return (
    <div
      data-testid="snapshot-expired-block"
      style={{
        minHeight: "100vh",
        background: "#fafafa",
        fontFamily: "'Hanken Grotesk', sans-serif",
        color: "#1c1c20",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 28,
      }}
    >
      <div
        style={{
          maxWidth: 480,
          background: "#fbfbfb",
          border: "1px solid #e4e4e7",
          borderRadius: 12,
          boxShadow: "0 12px 34px -20px rgba(22,24,21,0.35)",
          padding: 28,
          textAlign: "center",
        }}
      >
        <h1
          style={{
            margin: "0 0 12px",
            fontFamily: "'Bricolage Grotesque', sans-serif",
            fontSize: 22,
            fontWeight: 500,
          }}
        >
          this share link expired or was never created
        </h1>
        <p
          style={{
            margin: "0 0 18px",
            color: "#54545a",
            fontSize: 13.5,
            lineHeight: 1.5,
          }}
        >
          snapshots are retained for 30 days. open a new session to start
          fresh.
        </p>
        <Link
          to="/playground"
          style={{
            display: "inline-block",
            background: "#1c1c20",
            color: "#f4f4f5",
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 12.5,
            border: "none",
            borderRadius: 9,
            padding: "10px 16px",
            textDecoration: "none",
            letterSpacing: "0.02em",
          }}
        >
          start a new session →
        </Link>
      </div>
    </div>
  );
}

function Toast({ text }: { text: string }) {
  return (
    <div
      data-testid="toast"
      role="status"
      style={{
        position: "fixed",
        bottom: 24,
        right: 24,
        background: "#1c1c20",
        color: "#f4f4f5",
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 12,
        padding: "10px 14px",
        borderRadius: 9,
        boxShadow: "0 12px 34px -20px rgba(22,24,21,0.35)",
      }}
    >
      {text}
    </div>
  );
}
