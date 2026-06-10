// /playground route shell. wave 1 scaffolding: 2-pane layout, tab state,
// snapshot-id surface from the URL. wave-2 sibling agents fill InputPanel
// + each tab panel; this file only owns the chrome.
//
// theme tokens come from spec §8 and match Synthesizer.tsx (inline styles,
// Hanken Grotesk body, Bricolage Grotesque display, JetBrains Mono labels,
// #1c1c20 ink, #fbfbfb/#fafafa surfaces, #e4e4e7 borders, panel shadow
// 0 12px 34px -20px rgba(22,24,21,0.35)). no Tailwind, no css modules.

import { useState } from "react";
import type { ReactNode } from "react";
import { useParams } from "react-router-dom";
import { SpecTab } from "./panels/SpecTab";
import { SourceTab } from "./panels/SourceTab";
import { SimulateTab } from "./panels/SimulateTab";
import { BundleTab } from "./panels/BundleTab";

type TabKey = "spec" | "source" | "simulate" | "bundle";

const TABS: Array<{ key: TabKey; label: string }> = [
  { key: "spec", label: "Spec" },
  { key: "source", label: "Source" },
  { key: "simulate", label: "Simulate" },
  { key: "bundle", label: "Bundle" },
];

export function PlaygroundPage() {
  const params = useParams<{ snapshotId?: string }>();
  const snapshotId = params.snapshotId ?? "";
  const [activeTab, setActiveTab] = useState<TabKey>("spec");

  return (
    <div
      style={{
        minHeight: "100vh",
        background: "#fafafa",
        fontFamily: "'Hanken Grotesk', sans-serif",
        color: "#1c1c20",
      }}
    >
      <Header snapshotId={snapshotId} />
      <div
        style={{
          maxWidth: 1400,
          margin: "0 auto",
          padding: "24px 28px 64px",
          display: "grid",
          gridTemplateColumns: "280px 1fr",
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
          <div style={{ color: "#a0a0a8" }}>InputPanel — coming in wave 2</div>
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
          <TabBar
            tabs={TABS}
            active={activeTab}
            onChange={setActiveTab}
          />
          <div role="tabpanel" aria-label={activeTab}>
            {activeTab === "spec" && <SpecTab />}
            {activeTab === "source" && <SourceTab />}
            {activeTab === "simulate" && <SimulateTab />}
            {activeTab === "bundle" && <BundleTab />}
          </div>
        </main>
      </div>
    </div>
  );
}

function Header({ snapshotId }: { snapshotId: string }) {
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
      <ShareBadge snapshotId={snapshotId} />
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
