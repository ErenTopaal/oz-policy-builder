// SpecTab — collapsible IR tree of the synthesized PolicySpec with a
// reasoning trace that points each leaf-level constraint back at concrete
// fields of the original Recording.
//
// honesty rules (per feedback-no-mock-fallback, feedback-honesty-no-fakes):
// - if `spec` is null, we render an explicit "no spec yet" empty state.
//   no fixture data, no placeholder constraints.
// - reasoning trace lines are emitted ONLY when the recording actually
//   contains the source data the constraint references. unmatched →
//   we omit the line silently rather than fabricate a derivation.
//
// theme tokens are inlined verbatim from spec §8. no Tailwind, no css
// modules. iconography is restricted to text glyphs (⚠ ↳ ▾ ▸).
//
// props are optional only to keep the wave-1 PlaygroundPage.tsx shell
// (`<SpecTab />`) type-clean until the wave-2 orchestrator wires real
// state through; passing nothing produces the same empty state as
// passing `spec={null}`. spec §5 defines the public signature as
// `{ spec, recording, diverged: boolean }`.

import { useState } from "react";
import type { ReactNode } from "react";
import type {
  PolicySpec,
  Recording,
  PolicySlot,
  Constraint,
  ArgValue,
} from "../../lib/types";

export interface SpecTabProps {
  spec?: PolicySpec | null;
  recording?: Recording | null;
  diverged?: boolean;
}

// --- theme tokens (spec §8) ---
const INK = "#1c1c20";
const INK_DIM = "#54545a";
const INK_FADED = "#797980";
const SURFACE = "#fbfbfb";
const BORDER = "#e4e4e7";
const ERROR = "#dc2626";
const MONO = "'JetBrains Mono', monospace";
const BODY = "'Hanken Grotesk', sans-serif";

// truncate middle of long identifiers (addresses, hashes) keeping the
// first `head` and last `tail` chars, joined by an ellipsis glyph.
function truncMiddle(s: string, head = 6, tail = 4): string {
  if (s.length <= head + tail + 1) return s;
  return `${s.slice(0, head)}…${s.slice(-tail)}`;
}

export function SpecTab({ spec = null, recording = null, diverged = false }: SpecTabProps) {
  if (spec === null) {
    return (
      <div
        data-testid="spec-empty"
        style={{
          padding: 24,
          fontFamily: BODY,
          color: INK_FADED,
          fontSize: 13.5,
        }}
      >
        no spec yet — synthesize a transaction first
      </div>
    );
  }

  return (
    <div style={{ padding: 20, fontFamily: BODY, color: INK }}>
      {diverged && <DivergenceWarning />}
      <RuleNode spec={spec} recording={recording} />
    </div>
  );
}

function DivergenceWarning() {
  return (
    <div
      data-testid="spec-divergence-warning"
      role="alert"
      style={{
        marginBottom: 14,
        padding: "10px 12px",
        background: "rgba(220,38,38,0.06)",
        border: `1px solid ${BORDER}`,
        borderRadius: 8,
        fontFamily: MONO,
        fontSize: 12,
        color: ERROR,
        letterSpacing: "0.01em",
      }}
    >
      ⚠ source diverges from spec — bundle will note divergence
    </div>
  );
}

// --- tree nodes ---

function RuleNode({ spec, recording }: { spec: PolicySpec; recording: Recording | null }) {
  const ctx = spec.context_rule.context_type;
  const ctxSummary =
    ctx.kind === "call_contract"
      ? `call_contract: ${truncMiddle(ctx.address, 8, 4)}`
      : "default";
  return (
    <TreeNode
      defaultOpen
      label={
        <span>
          <Glyph>rule</Glyph>
          <Mono>:{spec.context_rule.name}</Mono>
          <Sep />
          <Faded>{ctxSummary}</Faded>
        </span>
      }
    >
      {spec.policies.length === 0 ? (
        <Leaf>
          <Faded>(no policy slots)</Faded>
        </Leaf>
      ) : (
        spec.policies.map((slot, i) => (
          <SlotNode key={i} slot={slot} index={i} recording={recording} />
        ))
      )}
    </TreeNode>
  );
}

function SlotNode({
  slot,
  index,
  recording,
}: {
  slot: PolicySlot;
  index: number;
  recording: Recording | null;
}) {
  if (slot.kind === "existing") {
    return (
      <TreeNode
        defaultOpen
        label={
          <span>
            <Glyph>policies[{index}]</Glyph>
            <Sep />
            <Mono>primitive:{slot.primitive}</Mono>
          </span>
        }
      >
        <Leaf>
          <Mono>params: {JSON.stringify(slot.params)}</Mono>
        </Leaf>
      </TreeNode>
    );
  }

  return (
    <TreeNode
      defaultOpen
      label={
        <span>
          <Glyph>policies[{index}]</Glyph>
          <Sep />
          <Mono>generated:{slot.template_family}</Mono>
        </span>
      }
    >
      {slot.constraints.map((c, j) => (
        <ConstraintNode key={j} constraint={c} recording={recording} />
      ))}
    </TreeNode>
  );
}

function ConstraintNode({
  constraint,
  recording,
}: {
  constraint: Constraint;
  recording: Recording | null;
}) {
  // leaf-level: the reasoning trace renders inline below the label
  // rather than gated behind an expand caret, so the derivation is
  // visible the moment the parent slot is open.
  return (
    <div style={{ padding: "3px 0" }}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 6,
          paddingLeft: 14,
        }}
      >
        <span style={{ width: 14, display: "inline-block" }} />
        <span style={{ fontSize: 12.5 }}>
          <ConstraintLabel constraint={constraint} />
        </span>
      </div>
      <div style={{ paddingLeft: 28 }}>
        <ReasoningTrace constraint={constraint} recording={recording} />
      </div>
    </div>
  );
}

function ConstraintLabel({ constraint }: { constraint: Constraint }) {
  const c = constraint as Constraint;
  switch (c.kind) {
    case "function_allowlist":
      return (
        <Mono>
          function_allowlist: [{(c as { functions: string[] }).functions.join(", ")}]
        </Mono>
      );
    case "argument_pattern": {
      const cc = c as { fn_name: string; arg_index: number; matcher: unknown };
      return (
        <Mono>
          argument_pattern: {cc.fn_name}#{cc.arg_index} → {JSON.stringify(cc.matcher)}
        </Mono>
      );
    }
    case "amount_range": {
      const cc = c as {
        fn_name: string;
        arg_index: number;
        min_string: string | null;
        max_string: string | null;
      };
      return (
        <Mono>
          amount_range: {cc.fn_name}#{cc.arg_index} [{cc.min_string ?? "-∞"}..
          {cc.max_string ?? "+∞"}]
        </Mono>
      );
    }
    case "asset_allowlist":
      return (
        <Mono>
          asset_allowlist: [{(c as { assets: string[] }).assets.join(", ")}]
        </Mono>
      );
    case "time_window": {
      const cc = c as { start_ledger: number; end_ledger: number };
      return (
        <Mono>
          time_window: [{cc.start_ledger}..{cc.end_ledger}]
        </Mono>
      );
    }
    case "call_frequency": {
      const cc = c as { max_calls: number; window_ledgers: number };
      return (
        <Mono>
          call_frequency: {cc.max_calls}/{cc.window_ledgers}L
        </Mono>
      );
    }
    case "sequence_ordering":
      return (
        <Mono>
          sequence_ordering: [{(c as { phases: string[] }).phases.join(" → ")}]
        </Mono>
      );
    default:
      return <Mono>{c.kind}</Mono>;
  }
}

// --- reasoning trace ---

function ReasoningTrace({
  constraint,
  recording,
}: {
  constraint: Constraint;
  recording: Recording | null;
}) {
  if (!recording) return null;
  const lines = deriveTrace(constraint, recording);
  if (lines.length === 0) return null;
  return (
    <div
      data-testid="reasoning-trace"
      style={{
        marginTop: 4,
        marginLeft: 4,
        padding: "4px 0",
        fontFamily: MONO,
        fontSize: 11.5,
        color: INK_DIM,
        lineHeight: 1.55,
      }}
    >
      {lines.map((line, i) => (
        <div key={i}>↳ derived from: {line}</div>
      ))}
    </div>
  );
}

// returns one rendered string per recording reference; empty array means
// the constraint has no honest mapping in this recording.
function deriveTrace(constraint: Constraint, recording: Recording): string[] {
  const c = constraint as Constraint;
  switch (c.kind) {
    case "function_allowlist": {
      const fns = (c as { functions: string[] }).functions;
      const matches: string[] = [];
      for (const contract of recording.contracts) {
        if (fns.includes(contract.function)) {
          matches.push(`${truncMiddle(contract.address, 6, 4)}:${contract.function}`);
        }
      }
      return matches;
    }
    case "argument_pattern": {
      const cc = c as { fn_name: string; arg_index: number };
      const matches: string[] = [];
      for (const contract of recording.contracts) {
        if (contract.function === cc.fn_name && contract.args[cc.arg_index] !== undefined) {
          matches.push(
            `${truncMiddle(contract.address, 6, 4)}:${contract.function}(arg[${cc.arg_index}])`,
          );
        }
      }
      return matches;
    }
    case "amount_range": {
      const cc = c as { fn_name: string; arg_index: number };
      const matches: string[] = [];
      for (const contract of recording.contracts) {
        const arg = contract.args[cc.arg_index];
        if (contract.function === cc.fn_name && arg && arg.kind === "i128") {
          matches.push(
            `${truncMiddle(contract.address, 6, 4)}:${contract.function}(arg[${cc.arg_index}]=i128 ${(arg as { value: string }).value})`,
          );
        }
      }
      return matches;
    }
    case "asset_allowlist": {
      const wanted = new Set((c as { assets: string[] }).assets);
      const seen: string[] = [];
      const seenSet = new Set<string>();
      const walk = (v: ArgValue) => {
        if (v.kind === "bytes") {
          const decoded = tryDecodeUtf8((v as { hex: string }).hex);
          if (decoded && wanted.has(decoded) && !seenSet.has(decoded)) {
            seenSet.add(decoded);
            seen.push(decoded);
          }
        } else if (v.kind === "symbol") {
          const sym = (v as { value: string }).value;
          if (wanted.has(sym) && !seenSet.has(sym)) {
            seenSet.add(sym);
            seen.push(sym);
          }
        } else if (v.kind === "vec") {
          for (const inner of (v as { value: ArgValue[] }).value) walk(inner);
        } else if (v.kind === "map") {
          for (const entry of (v as { value: Array<{ key: ArgValue; value: ArgValue }> }).value) {
            walk(entry.key);
            walk(entry.value);
          }
        }
      };
      for (const contract of recording.contracts) {
        for (const arg of contract.args) walk(arg);
      }
      return seen;
    }
    case "time_window": {
      if (recording.ledger == null) return [];
      return [`recording.ledger=${recording.ledger}`];
    }
    case "call_frequency": {
      const n = recording.contracts.length;
      if (n === 0) return [];
      return [`${n} matching op${n === 1 ? "" : "s"} in recording`];
    }
    case "sequence_ordering": {
      if (recording.contracts.length === 0) return [];
      const order = recording.contracts
        .map((c2) => `${truncMiddle(c2.address, 6, 4)}:${c2.function}`)
        .join(" → ");
      return [order];
    }
    default:
      return [];
  }
}

function tryDecodeUtf8(hex: string): string | null {
  if (hex.length % 2 !== 0 || hex.length === 0) return null;
  try {
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      const byte = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
      if (Number.isNaN(byte)) return null;
      bytes[i] = byte;
    }
    const decoder = new TextDecoder("utf-8", { fatal: true });
    const out = decoder.decode(bytes);
    // SEP-41 asset codes are printable ASCII, length 1..12 typically.
    if (!/^[\x20-\x7e]+$/.test(out)) return null;
    return out;
  } catch {
    return null;
  }
}

// --- collapsible tree primitives ---

function TreeNode({
  label,
  children,
  defaultOpen = false,
}: {
  label: ReactNode;
  children?: ReactNode;
  defaultOpen?: boolean;
}) {
  const hasChildren = children !== undefined && children !== null && children !== false;
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div style={{ marginLeft: 0 }}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 6,
          padding: "3px 0",
        }}
      >
        {hasChildren ? (
          <button
            type="button"
            aria-expanded={open}
            onClick={() => setOpen((v) => !v)}
            style={{
              background: "transparent",
              border: "none",
              padding: 0,
              cursor: "pointer",
              color: INK_FADED,
              fontFamily: MONO,
              fontSize: 12,
              width: 14,
              textAlign: "left",
            }}
          >
            {open ? "▾" : "▸"}
          </button>
        ) : (
          <span style={{ width: 14, display: "inline-block" }} />
        )}
        <span style={{ fontSize: 12.5 }}>{label}</span>
      </div>
      {hasChildren && open && (
        <div
          style={{
            marginLeft: 14,
            paddingLeft: 10,
            borderLeft: `1px solid ${BORDER}`,
          }}
        >
          {children}
        </div>
      )}
    </div>
  );
}

function Leaf({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        padding: "3px 0 3px 20px",
        fontSize: 12,
      }}
    >
      {children}
    </div>
  );
}

// --- small typography helpers ---

function Mono({ children }: { children: ReactNode }) {
  return (
    <span style={{ fontFamily: MONO, color: INK, fontSize: 12 }}>{children}</span>
  );
}

function Glyph({ children }: { children: ReactNode }) {
  return (
    <span
      style={{
        fontFamily: MONO,
        color: INK,
        fontSize: 12,
        background: "rgba(28,28,33,0.06)",
        padding: "1px 6px",
        borderRadius: 5,
      }}
    >
      {children}
    </span>
  );
}

function Faded({ children }: { children: ReactNode }) {
  return (
    <span style={{ fontFamily: MONO, color: INK_FADED, fontSize: 11.5 }}>
      {children}
    </span>
  );
}

function Sep() {
  return <span style={{ color: INK_FADED, padding: "0 6px" }}>·</span>;
}

// keep referenced so unused-import lint doesn't complain in surface
void SURFACE;
