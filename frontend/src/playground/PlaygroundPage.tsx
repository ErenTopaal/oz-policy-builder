// /playground route shell + controller.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { T } from "./theme";

type TabKey = "spec" | "source" | "simulate" | "bundle";

const TABS: Array<{ key: TabKey; label: string }> = [
  { key: "spec", label: "Spec" },
  { key: "source", label: "Source" },
  { key: "simulate", label: "Simulate" },
  { key: "bundle", label: "Bundle" },
];

type CompileError = { stderr: string; exit_code: number } | null;
type ResimError = { code: string; detail: string } | null;

interface PlaygroundPageProps {
  /**
   * Test seam. Production callers pass nothing → readConfig() + new McpClient().
   * Per feedback-no-mock-fallback, this is ONLY for orchestration tests.
   */
  clientFactory?: (cfg: McpConfig) => McpClient;
}

export function PlaygroundPage({ clientFactory }: PlaygroundPageProps = {}) {
  const params = useParams<{ snapshotId?: string }>();
  const routeSnapshotId = params.snapshotId ?? "";

  const [activeTab, setActiveTab] = useState<TabKey>("spec");
  const [phase, setPhase] = useState<SubmitPhase>("idle");
  const [busy, setBusy] = useState(false);
  const [compileError, setCompileError] = useState<CompileError>(null);
  const [resimError, setResimError] = useState<ResimError>(null);
  const [pageError, setPageError] = useState<{ code: string; detail: string } | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [ruleName, setRuleName] = useState<string>("");

  const cfg = useMemo(() => readConfig(), []);
  const { state, dispatch } = usePlaygroundState();
  const { presets } = usePresets();

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

  // Hydrate on :snapshotId mount. StrictMode double-invokes effects, so
  // we ref-guard to avoid double fetch.
  const hydratedFor = useRef<string | null>(null);
  useEffect(() => {
    if (!routeSnapshotId) return;
    if (hydratedFor.current === routeSnapshotId) return;
    hydratedFor.current = routeSnapshotId;

    let cancelled = false;
    (async () => {
      const snap = await loadSnapshot(routeSnapshotId);
      if (cancelled || !snap) return;
      dispatch({ type: "setRecording", recording: snap.recording });
      dispatch({ type: "setSpec", spec: snap.spec });
      dispatch({ type: "setReport", report: snap.report });
      if (snap.modified_lib_rs !== undefined && snap.modified_lib_rs !== null) {
        dispatch({ type: "setModifiedLibRs", modifiedLibRs: snap.modified_lib_rs });
      }
      dispatch({ type: "setSnapshotId", snapshotId: routeSnapshotId });
      if (!snap.spec_id) {
        // older snapshot records may not carry the spec_id explicitly —
        // we still have the embedded PolicySpec, so spec/simulate tabs
        // render; source/bundle just show their empty state.
        return;
      }
      try {
        const artifacts = await getClient().getPolicyArtifacts({ spec_id: snap.spec_id });
        if (!cancelled) dispatch({ type: "setArtifacts", artifacts });
      } catch (e) {
        if (e instanceof McpError && !cancelled) {
          // non-fatal — panels show empty state for source/bundle.
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [routeSnapshotId, loadSnapshot, dispatch, getClient]);

  useEffect(() => {
    if (!routeSnapshotId) return;
    if (snapshotError) {
      setPageError({ code: snapshotError.code, detail: snapshotError.detail });
    } else {
      setPageError(null);
    }
  }, [snapshotError, routeSnapshotId]);

  // toast auto-dismiss after 2.2s (matches design).
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 2200);
    return () => clearTimeout(t);
  }, [toast]);

  const onCancel = useCallback(() => {
    cancelRef.current?.abort();
    setPhase("idle");
    setBusy(false);
  }, []);

  const onReSimulate = useCallback(async () => {
    if (!state.recording || !state.spec) return;
    if (state.modifiedLibRs === null) return;

    setBusy(true);
    setCompileError(null);
    setResimError(null);

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
        setResimError(err);
      } else if (err.code === "E_CODEGEN_COMPILE_FAILED") {
        setCompileError({ stderr: err.detail, exit_code: 1 });
        setResimError(err);
      } else {
        setResimError(err);
      }
    } finally {
      setBusy(false);
    }
  }, [state.recording, state.spec, state.modifiedLibRs, dispatch, getClient]);

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

      const c = getClient();
      let recordingId: string | null = null;
      let specId: string | null = null;

      try {
        setPhase("recording");
        const rec = await c.recordTransaction({
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
        const synth = await c.synthesizePolicy({
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
        c.getPolicyArtifacts({ spec_id: specId! }),
        c.simulatePolicy({ spec_id: specId!, recording_id: recordingId! }),
      ]);

      if (artifactsRes.status === "fulfilled") {
        dispatch({ type: "setArtifacts", artifacts: artifactsRes.value });
      } else {
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
        setToast(`share link copied · /playground/s/${ref.snapshot_id}`);
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

  const onRevertSource = useCallback(
    () => dispatch({ type: "setModifiedLibRs", modifiedLibRs: null }),
    [dispatch],
  );

  // Full-page snapshot 404 block (design's pageErrorView).
  if (routeSnapshotId && pageError && pageError.code === "E_SNAPSHOT_NOT_FOUND") {
    return <ExpiredSnapshotBlock />;
  }

  const diverged = state.modifiedLibRs !== null;
  const visibleSnapshotId = state.snapshotId ?? (routeSnapshotId || null);

  return (
    <div
      style={{
        minHeight: "100vh",
        background: T.page,
        backgroundImage:
          "linear-gradient(rgba(255,255,255,0.03) 1px,transparent 1px),linear-gradient(90deg,rgba(255,255,255,0.03) 1px,transparent 1px)",
        backgroundSize: "42px 42px",
        color: T.ink,
        fontFamily: T.body,
      }}
    >
      <NavBar
        snapshotId={visibleSnapshotId}
        onShare={onShare}
        canShare={!!idsRef.current && !!state.latestReport && !busy}
      />
      <main
        style={{
          maxWidth: 1320,
          margin: "0 auto",
          padding: 24,
          display: "grid",
          gridTemplateColumns: "minmax(340px, 390px) 1fr",
          gap: 22,
          alignItems: "start",
        }}
      >
        <aside
          style={{
            position: "sticky",
            top: 88,
            background: T.surface,
            borderRadius: 16,
            padding: 24,
            display: "flex",
            flexDirection: "column",
            gap: 18,
            boxShadow: "0 16px 40px -22px rgba(0,0,0,0.55)",
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

        <section
          style={{
            minWidth: 0,
            display: "flex",
            flexDirection: "column",
            gap: 16,
          }}
        >
          <TabBar
            tabs={TABS}
            active={activeTab}
            onChange={setActiveTab}
            badges={{
              source:
                state.artifacts && state.artifacts.generated_sources.length === 0
                  ? "muted"
                  : null,
              simulate:
                state.latestReport &&
                (state.latestReport.permit.passed === false ||
                  state.latestReport.deny_results.some((d) => !d.passed))
                  ? "danger"
                  : null,
            }}
          />
          {pageError && pageError.code !== "E_SNAPSHOT_NOT_FOUND" && (
            <PageErrorBanner err={pageError} />
          )}
          <div role="tabpanel" aria-label={activeTab}>
            {activeTab === "spec" && (
              <SpecTab
                spec={state.spec}
                recording={state.recording}
                diverged={diverged}
                onRevert={onRevertSource}
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
        </section>
      </main>
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

function NavBar({
  snapshotId,
  onShare,
  canShare,
}: {
  snapshotId: string | null;
  onShare: () => void;
  canShare: boolean;
}) {
  return (
    <nav
      style={{
        position: "sticky",
        top: 0,
        zIndex: 50,
        backdropFilter: "blur(13px)",
        WebkitBackdropFilter: "blur(13px)",
        background: "rgba(20,20,23,0.8)",
        boxShadow: "0 1px 0 rgba(255,255,255,0.07)",
      }}
    >
      <div
        style={{
          maxWidth: 1320,
          margin: "0 auto",
          padding: "14px 24px",
          display: "flex",
          alignItems: "center",
          gap: 16,
          flexWrap: "wrap",
        }}
      >
        <Link
          to="/"
          style={{
            display: "flex",
            alignItems: "center",
            gap: 11,
            textDecoration: "none",
          }}
        >
          <span
            style={{
              width: 22,
              height: 22,
              borderRadius: 7,
              background: T.dark,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <span
              style={{
                width: 8,
                height: 8,
                borderRadius: "50%",
                background: T.page,
              }}
            />
          </span>
          <span
            style={{
              fontFamily: T.disp,
              fontSize: 16,
              color: T.ink,
              fontWeight: 600,
              letterSpacing: "-0.01em",
            }}
          >
            OZ Policy Builder
          </span>
        </Link>
        <span
          style={{
            fontFamily: T.mono,
            fontSize: 11,
            color: T.ink2,
            background: "rgba(255,255,255,0.08)",
            padding: "3px 9px",
            borderRadius: 7,
          }}
        >
          playground
        </span>
        <div style={{ flex: 1 }} />
        <div style={{ display: "flex", alignItems: "center", gap: 28, flexWrap: "wrap" }}>
          <Link
            to="/#how"
            style={{ textDecoration: "none", color: T.ink2, fontSize: 14.5, fontWeight: 500 }}
          >
            How it works
          </Link>
          <Link
            to="/playground"
            style={{ textDecoration: "none", color: T.ink2, fontSize: 14.5, fontWeight: 500 }}
          >
            Playground
          </Link>
          <Link
            to="/#quickstart"
            style={{ textDecoration: "none", color: T.ink2, fontSize: 14.5, fontWeight: 500 }}
          >
            Quick start
          </Link>
          <Link
            to="/#architecture"
            style={{ textDecoration: "none", color: T.ink2, fontSize: 14.5, fontWeight: 500 }}
          >
            Architecture
          </Link>
          <SnapshotPill snapshotId={snapshotId} />
          <button
            data-testid="share-button"
            onClick={onShare}
            disabled={!canShare}
            style={{
              background: canShare ? T.dark : T.stone,
              color: canShare ? T.darkInk : T.faint2,
              border: "none",
              fontFamily: T.mono,
              fontSize: 12.5,
              fontWeight: 600,
              padding: "9px 15px",
              borderRadius: 9,
              cursor: canShare ? "pointer" : "not-allowed",
            }}
          >
            share ↗
          </button>
          <a
            href="https://github.com/ErenTopaal/oz-policy-builder"
            target="_blank"
            rel="noopener noreferrer"
            style={{
              textDecoration: "none",
              color: T.page,
              fontSize: 13,
              fontFamily: T.mono,
              background: T.dark,
              padding: "9px 15px",
              borderRadius: 9,
            }}
          >
            GitHub ↗
          </a>
        </div>
      </div>
    </nav>
  );
}

function SnapshotPill({ snapshotId }: { snapshotId: string | null }) {
  const id = snapshotId;
  return (
    <div
      data-testid="share-badge"
      style={{
        display: "flex",
        alignItems: "center",
        gap: 9,
        background: T.surface,
        borderRadius: 9,
        padding: "8px 13px",
        boxShadow: "0 2px 8px -5px rgba(22,24,21,0.2)",
      }}
    >
      <span
        style={{
          width: 7,
          height: 7,
          borderRadius: "50%",
          background: id ? T.dark : "#c5c5ca",
        }}
      />
      <span
        style={{
          fontFamily: T.mono,
          fontSize: 11.5,
          color: id ? T.ink : T.faint2,
        }}
      >
        {id ? `share: ${id}` : "not shared yet"}
      </span>
    </div>
  );
}

function TabBar({
  tabs,
  active,
  onChange,
  badges,
}: {
  tabs: Array<{ key: TabKey; label: string }>;
  active: TabKey;
  onChange: (k: TabKey) => void;
  badges?: Partial<Record<TabKey, "danger" | "muted" | null>>;
}) {
  return (
    <div
      role="tablist"
      style={{
        display: "inline-flex",
        gap: 4,
        background: T.stone,
        padding: 5,
        borderRadius: 13,
        alignSelf: "flex-start",
        flexWrap: "wrap",
      }}
    >
      {tabs.map((tab) => {
        const isActive = tab.key === active;
        const badge = badges?.[tab.key];
        return (
          <button
            key={tab.key}
            role="tab"
            aria-selected={isActive}
            onClick={() => onChange(tab.key)}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 7,
              background: isActive ? T.surfaceHi : "transparent",
              color: isActive ? T.ink : T.ink2,
              border: "none",
              borderRadius: 9,
              padding: "10px 18px",
              cursor: "pointer",
              fontFamily: T.mono,
              fontSize: 13,
              fontWeight: isActive ? 600 : 500,
              boxShadow: isActive ? "0 2px 8px -4px rgba(0,0,0,0.4)" : "none",
              transition: "all .2s",
            }}
          >
            {tab.label}
            {badge && (
              <span
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: "50%",
                  background: badge === "danger" ? T.danger : T.faint2,
                }}
              />
            )}
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
        padding: "12px 14px",
        background: T.dangerBg,
        borderRadius: 12,
        fontFamily: T.mono,
        fontSize: 12,
        color: T.danger,
        boxShadow: `inset 0 0 0 1.5px ${T.danger}`,
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
        background: T.page,
        color: T.ink,
        fontFamily: T.body,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 28,
      }}
    >
      <div
        style={{
          maxWidth: 520,
          background: T.surface,
          borderRadius: 16,
          boxShadow: "0 16px 40px -22px rgba(0,0,0,0.6)",
          padding: 36,
          textAlign: "center",
        }}
      >
        <span
          style={{
            display: "inline-block",
            fontFamily: T.mono,
            fontSize: 11,
            color: T.darkInk,
            background: T.danger,
            padding: "4px 11px",
            borderRadius: 7,
            fontWeight: 600,
          }}
        >
          E_SNAPSHOT_NOT_FOUND
        </span>
        <h1
          style={{
            margin: "18px 0 0",
            fontFamily: T.disp,
            fontSize: 24,
            fontWeight: 600,
            color: T.ink,
          }}
        >
          this share link expired or was never created
        </h1>
        <p
          style={{
            margin: "9px auto 0",
            color: T.ink2,
            fontSize: 14.5,
            lineHeight: 1.55,
            maxWidth: "46ch",
          }}
        >
          Shared snapshots are retained for 30 days. Start a fresh synthesis to
          create a new one.
        </p>
        <Link
          to="/playground"
          style={{
            display: "inline-block",
            marginTop: 24,
            background: T.dark,
            color: T.darkInk,
            fontFamily: T.mono,
            fontSize: 13,
            fontWeight: 600,
            border: "none",
            borderRadius: 11,
            padding: "13px 22px",
            textDecoration: "none",
          }}
        >
          → back to playground
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
        bottom: 26,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 80,
        background: T.dark,
        color: T.darkInk,
        fontFamily: T.mono,
        fontSize: 12.5,
        padding: "13px 20px",
        borderRadius: 11,
        boxShadow: "0 16px 40px -16px rgba(0,0,0,0.5)",
        maxWidth: "90vw",
      }}
    >
      {text}
    </div>
  );
}
