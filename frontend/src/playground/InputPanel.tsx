import type { Dispatch } from "react";
import { useMemo, useState } from "react";
import type { Action, PlaygroundState } from "./hooks/usePlaygroundState";
import type { PresetKey, Presets } from "./hooks/usePresets";
import type { Network, SynthesisMode, Tightness } from "../lib/types";
import { T } from "./theme";

const TIGHTNESS_HELP: Record<Tightness, string> = {
  exact: "permit only the exact values observed",
  small_margin: "allow modest headroom above observed amounts",
  loose: "permit the function family with relaxed bounds",
};

const MODE_HELP: Record<SynthesisMode, string> = {
  auto: "auto · the synthesizer decides whether to compose an existing primitive or generate a new contract.",
  compose_only: "compose · reuse an existing OZ primitive only; never generate a new contract.",
  codegen_only: "codegen · always generate a fresh policy contract, even if a primitive would fit.",
};

const PRESET_LABELS: Record<PresetKey, string> = {
  sample: "sample · invoke_host_function",
  blend: "blend · claim",
  sep41: "sep41 · transfer",
  soroswap: "soroswap · swap",
};

const PRESET_TIGHTNESS: Record<PresetKey, Tightness> = {
  sample: "exact",
  blend: "exact",
  sep41: "small_margin",
  soroswap: "loose",
};

const PRESET_ORDER: PresetKey[] = ["sample", "blend", "sep41", "soroswap"];

export type InputMode = "hash" | "envelope";

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
  phase?: SubmitPhase;
  busy: boolean;
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
  const [presetOpen, setPresetOpen] = useState(false);
  const [pickedPreset, setPickedPreset] = useState<PresetKey | null>(null);
  const [network, setNetwork] = useState<Network>("testnet");
  const [tightness, setTightness] = useState<Tightness>("exact");
  const [mode, setMode] = useState<SynthesisMode>("auto");
  const [lifetime, setLifetime] = useState<number>(432000);
  const [ruleName, setRuleName] = useState("");

  const hashTrimmed = hash.trim();
  const hashValid = useMemo(() => HEX64.test(hashTrimmed), [hashTrimmed]);
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

  function pickPreset(k: PresetKey) {
    const entry = presets[k];
    if (entry.status === "unavailable" || !entry.hash) return;
    setPickedPreset(k);
    setInputMode("hash");
    setHash(entry.hash);
    setNetwork("testnet");
    setTightness(PRESET_TIGHTNESS[k]);
    setPresetOpen(false);
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
      intent.hash = hashTrimmed.toLowerCase();
    } else {
      intent.envelope_xdr_base64 = xdrTrimmed;
    }
    onSubmit(intent);
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18 }}>
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span
          style={{
            fontFamily: T.disp,
            fontSize: 20,
            fontWeight: 600,
            letterSpacing: "-0.015em",
            color: T.ink,
          }}
        >
          Synthesize a policy
        </span>
        <span style={{ fontSize: 13, color: T.faint, lineHeight: 1.5 }}>
          Turn a real Stellar transaction into a minimum-rights policy.
        </span>
      </div>

      <FieldGroup label="input">
        <Segments
          options={[
            { value: "hash", label: "hash" },
            { value: "envelope", label: "envelope XDR" },
          ]}
          value={inputMode}
          onChange={(v) => setInputMode(v as InputMode)}
          disabled={formDisabled}
        />
        <HelpText>
          Start from a transaction already on chain (hash), or paste one you
          built locally but haven't submitted (envelope XDR).
        </HelpText>
      </FieldGroup>

      <PresetRow
        presets={presets}
        open={presetOpen}
        onToggle={() => setPresetOpen((v) => !v)}
        onPick={pickPreset}
        picked={pickedPreset}
        disabled={formDisabled}
      />

      {inputMode === "hash" ? (
        <div style={{ display: "flex", flexDirection: "column", gap: 7 }}>
          <LabelMono>transaction hash</LabelMono>
          <HelpText>
            The 64-character hex id of the transaction whose permissions you
            want to capture.
          </HelpText>
          <input
            aria-label="transaction hash"
            value={hash}
            onChange={(e) => setHash(e.target.value)}
            disabled={formDisabled}
            placeholder="64-char hex"
            style={{
              ...textInputStyle,
              boxShadow:
                hash.length > 0 && !hashValid
                  ? `inset 0 0 0 1.5px ${T.danger}`
                  : "none",
            }}
          />
          {hash.length > 0 && !hashValid && (
            <span style={{ fontFamily: T.mono, fontSize: 11, color: T.danger }}>
              must be 64 hex characters
            </span>
          )}
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 7 }}>
          <LabelMono>envelope xdr · base64</LabelMono>
          <HelpText>
            Paste a base64 TransactionEnvelope you built locally but have not
            submitted yet.
          </HelpText>
          <textarea
            aria-label="envelope XDR"
            value={xdr}
            onChange={(e) => setXdr(e.target.value)}
            disabled={formDisabled}
            spellCheck={false}
            placeholder="AAAAA… base64-encoded TransactionEnvelope"
            style={{
              ...textInputStyle,
              minHeight: 92,
              resize: "vertical",
              lineHeight: 1.5,
            }}
          />
          {xdrTrimmed.length > 0 && !xdrValid && (
            <span style={{ fontFamily: T.mono, fontSize: 11, color: T.danger }}>
              invalid base64 envelope (need &gt; 80 chars)
            </span>
          )}
        </div>
      )}

      <FieldGroup label="network">
        <Segments
          options={[
            { value: "testnet", label: "testnet" },
            { value: "mainnet", label: "mainnet" },
          ]}
          value={network}
          onChange={(v) => setNetwork(v as Network)}
          disabled={formDisabled}
        />
        <HelpText>
          Which Soroban RPC the recorder queries. Match the network the
          transaction ran on.
        </HelpText>
      </FieldGroup>

      <div style={{ height: 1, background: T.line }} />

      <FieldGroup label="tightness">
        <Segments
          options={[
            { value: "exact", label: "exact" },
            { value: "small_margin", label: "margin" },
            { value: "loose", label: "loose" },
          ]}
          value={tightness}
          onChange={(v) => setTightness(v as Tightness)}
          disabled={formDisabled}
        />
        <HelpText>
          How tightly numeric constraints hug the observed values.{" "}
          <span style={{ color: "#cfcfd6" }}>{TIGHTNESS_HELP[tightness]}</span>
        </HelpText>
      </FieldGroup>

      <FieldGroup label="synthesis mode">
        <Segments
          options={[
            { value: "auto", label: "auto" },
            { value: "compose_only", label: "compose" },
            { value: "codegen_only", label: "codegen" },
          ]}
          value={mode}
          onChange={(v) => setMode(v as SynthesisMode)}
          disabled={formDisabled}
        />
        <HelpText>{MODE_HELP[mode]}</HelpText>
      </FieldGroup>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 14 }}>
        <label style={{ display: "flex", flexDirection: "column", gap: 7 }}>
          <LabelMono>lifetime · ledgers</LabelMono>
          <input
            type="number"
            aria-label="lifetime ledgers"
            value={lifetime}
            onChange={(e) => setLifetime(Number(e.target.value) || 0)}
            disabled={formDisabled}
            style={smallInputStyle}
          />
          <span
            style={{ fontFamily: T.mono, fontSize: 10.5, color: T.faint, lineHeight: 1.4 }}
          >
            how long the rule stays valid · 432000 ≈ 30 days
          </span>
        </label>
        <label style={{ display: "flex", flexDirection: "column", gap: 7 }}>
          <LabelMono>
            rule name<span style={{ color: "#a0a0a6", textTransform: "none" }}> · opt</span>
          </LabelMono>
          <input
            aria-label="rule name"
            value={ruleName}
            onChange={(e) => setRuleName(e.target.value)}
            disabled={formDisabled}
            placeholder="auto"
            style={smallInputStyle}
          />
          <span
            style={{ fontFamily: T.mono, fontSize: 10.5, color: T.faint, lineHeight: 1.4 }}
          >
            optional label · blank lets the server pick
          </span>
        </label>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {busy ? (
          <>
            <button
              data-testid="synth-busy-label"
              disabled
              style={{
                ...primaryBtn,
                background: T.stone,
                color: T.faint2,
                cursor: "default",
              }}
            >
              <span
                aria-hidden="true"
                style={{
                  width: 14,
                  height: 14,
                  borderRadius: "50%",
                  border: "2px solid rgba(255,255,255,0.18)",
                  borderTopColor: T.ink,
                  display: "inline-block",
                  marginRight: 10,
                  verticalAlign: "middle",
                }}
              />
              {submitLabel}
            </button>
            <button
              onClick={onCancel}
              style={{
                alignSelf: "center",
                background: "transparent",
                border: "none",
                color: T.ink2,
                fontFamily: T.mono,
                fontSize: 12,
                cursor: "pointer",
                textDecoration: "underline",
              }}
            >
              ✕ cancel
            </button>
          </>
        ) : (
          <button
            onClick={handleSubmit}
            disabled={submitDisabled}
            style={{
              ...primaryBtn,
              background: submitDisabled ? T.stone : T.dark,
              color: submitDisabled ? T.faint2 : T.darkInk,
              cursor: submitDisabled ? "default" : "pointer",
            }}
          >
            {submitLabel}
          </button>
        )}
      </div>
    </div>
  );
}

// ─── preset row (custom collapsible dropdown) ────────────────────────────

function PresetRow({
  presets,
  open,
  onToggle,
  onPick,
  picked,
  disabled,
}: {
  presets: Presets;
  open: boolean;
  onToggle: () => void;
  onPick: (k: PresetKey) => void;
  picked: PresetKey | null;
  disabled: boolean;
}) {
  const triggerLabel = picked ? PRESET_LABELS[picked] : "choose a sample transaction";
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 9,
        position: "relative",
      }}
    >
      <LabelMono>preset · refreshed hourly</LabelMono>
      <HelpText>
        New to this? Pick a ready-made sample transaction to fill the form,
        then hit synthesize.
      </HelpText>
      <button
        data-testid="preset-trigger"
        aria-label="preset"
        aria-expanded={open}
        onClick={onToggle}
        disabled={disabled}
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
          background: T.stone,
          border: "none",
          borderRadius: 11,
          padding: "12px 13px",
          cursor: disabled ? "not-allowed" : "pointer",
          fontFamily: T.mono,
          fontSize: 12.5,
          color: T.ink,
          width: "100%",
          opacity: disabled ? 0.6 : 1,
        }}
      >
        <span>{triggerLabel}</span>
        <span
          style={{
            color: T.faint,
            transform: open ? "rotate(180deg)" : "none",
            transition: "transform .2s",
          }}
        >
          ▾
        </span>
      </button>
      {open && (
        <div
          data-testid="preset-panel"
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            right: 0,
            marginTop: 6,
            zIndex: 20,
            background: T.surface,
            borderRadius: 12,
            padding: 6,
            boxShadow: "0 16px 40px -16px rgba(22,24,21,0.45)",
          }}
        >
          {PRESET_ORDER.map((k) => {
            const entry = presets[k];
            const isUnavailable = entry.status === "unavailable";
            const reason = isUnavailable
              ? "no recent testnet activity on this contract"
              : "";
            return (
              <button
                key={k}
                data-testid={`preset-row-${k}`}
                disabled={isUnavailable}
                title={reason}
                onClick={() => onPick(k)}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  gap: 10,
                  width: "100%",
                  background: "transparent",
                  border: "none",
                  borderRadius: 9,
                  padding: "11px 12px",
                  cursor: isUnavailable ? "not-allowed" : "pointer",
                  textAlign: "left",
                  opacity: isUnavailable ? 0.55 : 1,
                }}
              >
                <span style={{ fontFamily: T.mono, fontSize: 12, color: T.ink }}>
                  {PRESET_LABELS[k]}
                </span>
                <span
                  data-testid={`preset-chip-${k}`}
                  style={{
                    fontFamily: T.mono,
                    fontSize: 10,
                    padding: "2px 7px",
                    borderRadius: 20,
                    background:
                      entry.status === "fresh"
                        ? T.okChip
                        : entry.status === "stale"
                        ? "rgba(255,255,255,0.1)"
                        : T.dangerBg,
                    color: entry.status === "unavailable" ? T.danger : T.ink2,
                  }}
                >
                  {entry.status}
                </span>
              </button>
            );
          })}
          <div
            style={{
              fontFamily: T.mono,
              fontSize: 10.5,
              color: T.faint2,
              lineHeight: 1.5,
              padding: "6px 8px 2px",
            }}
          >
            unavailable = no recent testnet activity on that contract, not a
            refresh failure
          </div>
        </div>
      )}
    </div>
  );
}

// ─── small primitives ────────────────────────────────────────────────────

function FieldGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 9 }}>
      <LabelMono>{label}</LabelMono>
      {children}
    </div>
  );
}

function LabelMono({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        fontFamily: T.mono,
        fontSize: 10.5,
        letterSpacing: "0.05em",
        color: T.faint,
        textTransform: "uppercase",
      }}
    >
      {children}
    </span>
  );
}

function HelpText({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        fontFamily: T.mono,
        fontSize: 11,
        color: T.faint,
        lineHeight: 1.45,
      }}
    >
      {children}
    </span>
  );
}

function Segments<TV extends string>({
  options,
  value,
  onChange,
  disabled,
}: {
  options: Array<{ value: TV; label: string }>;
  value: TV;
  onChange: (v: TV) => void;
  disabled?: boolean;
}) {
  return (
    <div
      role="group"
      style={{
        display: "flex",
        gap: 4,
        background: T.stone,
        borderRadius: 11,
        padding: 4,
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
              background: active ? T.dark : "transparent",
              color: active ? T.darkInk : T.ink2,
              border: "none",
              borderRadius: 8,
              padding: "9px 6px",
              cursor: disabled ? "not-allowed" : "pointer",
              fontFamily: T.mono,
              fontSize: 12,
              fontWeight: active ? 600 : 500,
              transition: "background .2s, color .2s",
              whiteSpace: "nowrap",
              opacity: disabled ? 0.5 : 1,
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

const textInputStyle: React.CSSProperties = {
  background: T.stone,
  border: "none",
  borderRadius: 11,
  padding: 13,
  color: T.ink,
  fontFamily: T.mono,
  fontSize: 12.5,
  outline: "none",
  width: "100%",
  boxSizing: "border-box",
};

const smallInputStyle: React.CSSProperties = {
  background: T.stone,
  border: "none",
  borderRadius: 10,
  padding: "11px 12px",
  color: T.ink,
  fontFamily: T.mono,
  fontSize: 12.5,
  outline: "none",
  width: "100%",
  boxSizing: "border-box",
};

const primaryBtn: React.CSSProperties = {
  width: "100%",
  fontFamily: T.mono,
  fontWeight: 600,
  fontSize: 14,
  border: "none",
  borderRadius: 12,
  padding: 15,
  letterSpacing: "0.02em",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  gap: 10,
};
