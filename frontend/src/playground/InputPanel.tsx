// /playground InputPanel. left rail of the 2-pane shell. emits a single
// SubmitIntent up to PlaygroundPage; this component owns no async work
// and no MCP calls — it's a pure form. theme tokens match Synthesizer.tsx
// per spec §8 (no Tailwind, no css modules; inline styles + JetBrains
// Mono for labels/inputs, Hanken Grotesk body).
//
// honesty rules carried in: presets that fail to fetch arrive here as
// `status: 'unavailable'` and render as disabled options with an explicit
// tooltip. there is no in-component fallback hash anywhere.

import type { Dispatch } from "react";
import { useMemo, useState } from "react";
import type { Action, PlaygroundState } from "./hooks/usePlaygroundState";
import type { PresetKey, Presets } from "./hooks/usePresets";
import type { Network, SynthesisMode, Tightness } from "../lib/types";
import { Field, FieldHeader, FieldLabel } from "../sections/fields";

// inline copy of Synthesizer.tsx's TIGHTNESS_HELP. duplicated rather than
// imported so this panel doesn't reach into the landing-page module.
const TIGHTNESS_HELP: Record<Tightness, string> = {
  exact: "constraints pin observed values exactly. no slack.",
  small_margin: "numeric ranges scale 1.1×. asset / function sets stay exact.",
  loose: "numeric ranges scale 2×. more agent flexibility, less tight bound.",
};

const PRESET_LABELS: Record<PresetKey, string> = {
  sample: "Current sample",
  blend: "Blend yield-claim",
  sep41: "SEP-41 transfer",
  soroswap: "Soroswap swap",
};

const PRESET_TIGHTNESS: Record<PresetKey, Tightness> = {
  sample: "exact",
  blend: "exact",
  sep41: "small_margin",
  soroswap: "loose",
};

const PRESET_ORDER: PresetKey[] = ["sample", "blend", "sep41", "soroswap"];

export type InputMode = "hash" | "envelope";

/** the phase string PlaygroundPage tracks for the submit button label. */
export type SubmitPhase = "idle" | "recording" | "synthesizing" | "simulating";

export interface SubmitIntent {
  inputMode: InputMode;
  hash?: string;
  envelope_xdr_base64?: string;
  network: Network;
  tightness: Tightness;
  mode: SynthesisMode;
  lifetime: number;
  ruleName?: string;
}

export interface InputPanelProps {
  state: PlaygroundState;
  dispatch: Dispatch<Action>;
  presets: Presets;
  /** PlaygroundPage's current phase, drives label + cancel affordance. */
  phase?: SubmitPhase;
  /** true while an async flow is in flight. disables form + flips submit to cancel. */
  busy: boolean;
  /** true when backend health check failed — disables submit honestly. */
  backendDown?: boolean;
  onSubmit: (intent: SubmitIntent) => void;
  onCancel: () => void;
}

const HEX64 = /^[0-9a-fA-F]{64}$/;
const BASE64_CHARS = /^[A-Za-z0-9+/=\s]+$/;

export function InputPanel({
  presets,
  phase = "idle",
  busy,
  backendDown = false,
  onSubmit,
  onCancel,
}: InputPanelProps) {
  const [inputMode, setInputMode] = useState<InputMode>("hash");
  const [hash, setHash] = useState("");
  const [xdr, setXdr] = useState("");
  const [presetKey, setPresetKey] = useState<PresetKey | "">("");
  const [network, setNetwork] = useState<Network>("testnet");
  const [tightness, setTightness] = useState<Tightness>("exact");
  const [mode, setMode] = useState<SynthesisMode>("auto");
  const [lifetime, setLifetime] = useState<number>(432000);
  const [ruleName, setRuleName] = useState("");

  const hashValid = useMemo(() => HEX64.test(hash.trim()), [hash]);
  const xdrTrimmed = xdr.trim();
  const xdrValid = xdrTrimmed.length > 80 && BASE64_CHARS.test(xdrTrimmed);

  const inputValid = inputMode === "hash" ? hashValid : xdrValid;
  const formDisabled = busy || backendDown;
  const submitDisabled = formDisabled || !inputValid;

  const submitLabel = (() => {
    if (backendDown) return "live mode unavailable";
    switch (phase) {
      case "recording":
        return "recording transaction…";
      case "synthesizing":
        return "synthesizing policy…";
      case "simulating":
        return "simulating permit + deny…";
      default:
        return "synthesize";
    }
  })();

  function handlePresetChange(e: React.ChangeEvent<HTMLSelectElement>) {
    const key = e.target.value as PresetKey | "";
    setPresetKey(key);
    if (!key) return;
    const entry = presets[key];
    if (entry.status === "unavailable" || !entry.hash) return;
    setInputMode("hash");
    setHash(entry.hash);
    setNetwork("testnet");
    setTightness(PRESET_TIGHTNESS[key]);
  }

  function handleSubmit() {
    if (submitDisabled) return;
    const intent: SubmitIntent = {
      inputMode,
      network,
      tightness,
      mode,
      lifetime,
      ruleName: ruleName.trim() || undefined,
    };
    if (inputMode === "hash") {
      intent.hash = hash.trim().toLowerCase();
    } else {
      intent.envelope_xdr_base64 = xdrTrimmed;
    }
    onSubmit(intent);
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 14,
      }}
    >
      <PanelTitle>input</PanelTitle>

      <Field>
        <FieldLabel>input mode</FieldLabel>
        <Segments
          options={[
            { value: "hash", label: "hash" },
            { value: "envelope", label: "envelope XDR" },
          ]}
          value={inputMode}
          onChange={(v) => setInputMode(v as InputMode)}
          disabled={formDisabled}
        />
      </Field>

      {inputMode === "hash" ? (
        <Field>
          <FieldHeader>
            <FieldLabel>transaction hash</FieldLabel>
            {hash.length > 0 && !hashValid && (
              <span
                style={{
                  fontFamily: "'JetBrains Mono', monospace",
                  fontSize: 10,
                  color: "#9c4a36",
                  letterSpacing: "0.04em",
                }}
              >
                need 64 hex chars
              </span>
            )}
          </FieldHeader>
          <input
            aria-label="transaction hash"
            value={hash}
            onChange={(e) => setHash(e.target.value)}
            disabled={formDisabled}
            placeholder="64-char hex transaction hash"
            style={{
              ...inputBig,
              borderBottom: hash.length > 0 && !hashValid ? "2px solid #c0533a" : "2px solid transparent",
            }}
          />
        </Field>
      ) : (
        <Field>
          <FieldHeader>
            <FieldLabel>envelope XDR</FieldLabel>
            {xdrTrimmed.length > 0 && !xdrValid && (
              <span
                style={{
                  fontFamily: "'JetBrains Mono', monospace",
                  fontSize: 10,
                  color: "#9c4a36",
                  letterSpacing: "0.04em",
                }}
              >
                invalid base64
              </span>
            )}
          </FieldHeader>
          <textarea
            aria-label="envelope XDR"
            value={xdr}
            onChange={(e) => setXdr(e.target.value)}
            disabled={formDisabled}
            placeholder="base64-encoded transaction envelope XDR"
            rows={5}
            style={{
              ...inputBig,
              fontFamily: "'JetBrains Mono', monospace",
              minHeight: 110,
              resize: "vertical",
              borderBottom:
                xdrTrimmed.length > 0 && !xdrValid ? "2px solid #c0533a" : "2px solid transparent",
            }}
          />
        </Field>
      )}

      <Field>
        <FieldLabel>preset</FieldLabel>
        <select
          aria-label="preset"
          value={presetKey}
          onChange={handlePresetChange}
          disabled={formDisabled}
          style={selectStyle}
        >
          <option value="">— choose preset —</option>
          {PRESET_ORDER.map((k) => {
            const entry = presets[k];
            const base = PRESET_LABELS[k];
            const label =
              entry.status === "stale"
                ? `${base} (stale)`
                : entry.status === "unavailable"
                ? `${base} (unavailable)`
                : base;
            const disabled = entry.status === "unavailable";
            return (
              <option
                key={k}
                value={k}
                disabled={disabled}
                title={disabled ? "preset unavailable — last refresh failed" : undefined}
              >
                {label}
              </option>
            );
          })}
        </select>
      </Field>

      <Field>
        <FieldLabel>network</FieldLabel>
        <Segments
          options={[
            { value: "testnet", label: "testnet" },
            { value: "mainnet", label: "mainnet" },
          ]}
          value={network}
          onChange={(v) => setNetwork(v as Network)}
          disabled={formDisabled}
        />
      </Field>

      <div style={{ height: 1, background: "rgba(28,28,33,0.08)" }} />

      <Field>
        <FieldLabel>
          tightness
          <span style={{ color: "#a0a0a6", textTransform: "none", letterSpacing: 0 }}>
            {" "}
            · how tightly to bound
          </span>
        </FieldLabel>
        <Segments
          options={[
            { value: "exact", label: "exact" },
            { value: "small_margin", label: "small margin" },
            { value: "loose", label: "loose" },
          ]}
          value={tightness}
          onChange={(v) => setTightness(v as Tightness)}
          disabled={formDisabled}
        />
        <span
          style={{
            fontSize: 11.5,
            color: "#797980",
            lineHeight: 1.45,
            fontFamily: "'JetBrains Mono', monospace",
          }}
        >
          {TIGHTNESS_HELP[tightness]}
        </span>
      </Field>

      <Field>
        <FieldLabel>synthesis mode</FieldLabel>
        <Segments
          options={[
            { value: "auto", label: "auto" },
            { value: "compose_only", label: "compose only" },
            { value: "codegen_only", label: "codegen only" },
          ]}
          value={mode}
          onChange={(v) => setMode(v as SynthesisMode)}
          disabled={formDisabled}
        />
      </Field>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
        <label style={{ display: "flex", flexDirection: "column", gap: 7 }}>
          <FieldLabel>lifetime · ledgers</FieldLabel>
          <input
            type="number"
            aria-label="lifetime ledgers"
            value={lifetime}
            onChange={(e) => setLifetime(Number(e.target.value) || 0)}
            disabled={formDisabled}
            style={inputSmall}
          />
        </label>
        <label style={{ display: "flex", flexDirection: "column", gap: 7 }}>
          <FieldLabel>
            rule name<span style={{ color: "#99999e", textTransform: "none" }}> · opt</span>
          </FieldLabel>
          <input
            aria-label="rule name"
            value={ruleName}
            onChange={(e) => setRuleName(e.target.value)}
            disabled={formDisabled}
            placeholder="auto"
            style={inputSmall}
          />
        </label>
      </div>

      {busy ? (
        <button onClick={onCancel} style={submitBtn}>
          cancel
        </button>
      ) : (
        <button
          onClick={handleSubmit}
          disabled={submitDisabled}
          style={{
            ...submitBtn,
            opacity: submitDisabled ? 0.5 : 1,
            cursor: submitDisabled ? "not-allowed" : "pointer",
          }}
        >
          {submitLabel}
        </button>
      )}
    </div>
  );
}

// ─── sub-components ────────────────────────────────────────────────────────────

function PanelTitle({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        fontFamily: "'JetBrains Mono', monospace",
        fontSize: 11,
        letterSpacing: "0.08em",
        textTransform: "uppercase",
        color: "#797980",
      }}
    >
      {children}
    </span>
  );
}

function Segments<T extends string>({
  options,
  value,
  onChange,
  disabled,
}: {
  options: Array<{ value: T; label: string }>;
  value: T;
  onChange: (v: T) => void;
  disabled?: boolean;
}) {
  return (
    <div
      role="group"
      style={{
        display: "flex",
        gap: 6,
        background: "rgba(28,28,33,0.06)",
        padding: 4,
        borderRadius: 10,
      }}
    >
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            onClick={() => onChange(o.value)}
            disabled={disabled}
            aria-pressed={active}
            style={{
              flex: 1,
              border: "none",
              background: active ? "#1c1c20" : "transparent",
              color: active ? "#f4f4f5" : "#54545a",
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 11.5,
              padding: "9px 8px",
              borderRadius: 7,
              cursor: disabled ? "not-allowed" : "pointer",
              opacity: disabled ? 0.55 : 1,
              transition: "background 0.15s, color 0.15s",
              letterSpacing: "0.02em",
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

// ─── style objects (mirrors Synthesizer.tsx) ───────────────────────────────────

const inputBig: React.CSSProperties = {
  background: "#ebebec",
  border: "none",
  borderRadius: 11,
  padding: 12,
  color: "#1d1d1e",
  fontFamily: "'JetBrains Mono', monospace",
  fontSize: 12.5,
  outline: "none",
  width: "100%",
  boxSizing: "border-box",
};

const inputSmall: React.CSSProperties = {
  background: "#ebebec",
  border: "none",
  borderRadius: 10,
  padding: "10px 11px",
  color: "#1d1d1e",
  fontFamily: "'JetBrains Mono', monospace",
  fontSize: 12,
  outline: "none",
  width: "100%",
  boxSizing: "border-box",
};

const selectStyle: React.CSSProperties = {
  background: "#ebebec",
  border: "none",
  borderRadius: 10,
  padding: "11px 12px",
  color: "#1d1d1e",
  fontFamily: "'JetBrains Mono', monospace",
  fontSize: 12.5,
  outline: "none",
  width: "100%",
  appearance: "none",
  cursor: "pointer",
};

const submitBtn: React.CSSProperties = {
  marginTop: 2,
  width: "100%",
  background: "#1c1c20",
  color: "#f4f4f5",
  fontFamily: "'JetBrains Mono', monospace",
  fontWeight: 600,
  fontSize: 13.5,
  border: "none",
  borderRadius: 11,
  padding: 14,
  letterSpacing: "0.02em",
  cursor: "pointer",
};
