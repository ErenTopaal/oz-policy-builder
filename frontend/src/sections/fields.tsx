// shared field primitives extracted from Synthesizer.tsx so they can be
// reused by the /playground route's InputPanel without duplicating styles.
// behaviour is preserved verbatim from the original Synthesizer definitions.

import type { ReactNode } from "react";

export function Field({ children }: { children: ReactNode }) {
  return <div style={{ display: "flex", flexDirection: "column", gap: 9 }}>{children}</div>;
}

export function FieldHeader({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 10,
      }}
    >
      {children}
    </div>
  );
}

export function FieldLabel({ children }: { children: ReactNode }) {
  return (
    <span
      style={{
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 10.5,
        letterSpacing: "0.05em",
        color: "#797980",
        textTransform: "uppercase",
      }}
    >
      {children}
    </span>
  );
}
