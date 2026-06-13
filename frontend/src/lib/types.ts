// types mirroring the rust crates' public surface. used by the mcp client + the
// synthesizer widget to render results.

export type Network = "testnet" | "mainnet";
export type Tightness = "exact" | "small_margin" | "loose";
export type SynthesisMode = "auto" | "compose_only" | "codegen_only";

// --- Recording (oz_policy_core::recording::Recording) ---

export type ArgValue =
  | { kind: "address"; value: string }
  | { kind: "i128"; value: string } // string for json precision
  | { kind: "u32"; value: number }
  | { kind: "u64"; value: string }
  | { kind: "bytes"; hex: string }
  | { kind: "symbol"; value: string }
  | { kind: "vec"; value: ArgValue[] }
  | { kind: "map"; value: Array<{ key: ArgValue; value: ArgValue }> }
  | { kind: "bool"; value: boolean }
  | { kind: "void" }
  | { kind: string; [key: string]: unknown };

export interface ContractRecord {
  address: string;
  function: string;
  args: ArgValue[];
}

export interface Recording {
  schema: string;
  network_passphrase: string;
  ingest: { kind: "hash"; hash: string } | { kind: "simulation"; envelope_xdr_sha256: string };
  ledger?: number | null;
  contracts: ContractRecord[];
  auth_tree: { roots: unknown[] };
  state_changes: unknown[];
  events: unknown[];
}

// --- PolicySpec (oz_policy_core::spec::PolicySpec) ---

export interface PolicySpec {
  schema: string;
  synthesis_mode: SynthesisMode;
  context_rule: {
    name: string;
    context_type:
      | { kind: "default" }
      | { kind: "call_contract"; address: string };
    valid_until: number | null;
  };
  signers: unknown[];
  policies: PolicySlot[];
  lifetime_ledgers: number | null;
  recording_ref: { hash: string | null; schema: string };
}

export type PolicySlot =
  | {
      kind: "existing";
      primitive: "simple_threshold" | "weighted_threshold" | "spending_limit";
      params: Record<string, unknown>;
    }
  | {
      kind: "generated";
      template_family: TemplateFamily;
      constraints: Constraint[];
    };

export type TemplateFamily =
  | "function_allowlist"
  | "argument_pattern"
  | "amount_range"
  | "asset_allowlist"
  | "time_window"
  | "call_frequency"
  | "sequence_ordering";

export type Constraint =
  | { kind: "function_allowlist"; functions: string[] }
  | { kind: "argument_pattern"; fn_name: string; arg_index: number; matcher: unknown }
  | { kind: "amount_range"; fn_name: string; arg_index: number; min_string: string | null; max_string: string | null }
  | { kind: "asset_allowlist"; assets: string[] }
  | { kind: "time_window"; start_ledger: number; end_ledger: number }
  | { kind: "call_frequency"; max_calls: number; window_ledgers: number }
  | { kind: "sequence_ordering"; phases: string[] }
  | { kind: string; [key: string]: unknown };

// --- SimReport (oz_policy_simhost::run::SimReport) ---

export interface SimReport {
  spec_id?: string;
  permit: { passed: boolean; error: string | null };
  deny_results: DenyResult[];
  total_vectors: number;
  passed: number;
  timestamp_ledger: number;
}

export interface DenyResult {
  name: string;
  passed: boolean;
  expected_error_code: number;
  actual_error_code: number | null;
}

// --- mcp tool input / output envelopes ---

export interface RecordTransactionInput {
  network: Network;
  rpc_url?: string;
  hash?: string;
  envelope_xdr_base64?: string;
  instruction_leeway?: number;
}

export interface RecordTransactionOutput {
  recording_id: string;
  recording: Recording;
  retention_warning?: string;
}

export interface SynthesizePolicyInput {
  recording_id: string;
  tightness: Tightness;
  lifetime_ledgers?: number;
  delegated_signer?: string;
  mode: SynthesisMode;
  rule_name?: string;
}

export interface SynthesizePolicyOutput {
  spec_id: string;
  spec: PolicySpec;
  generated_count: number;
  composed_count: number;
}

export interface SimulatePolicyInput {
  spec_id: string;
  recording_id: string;
  extra_deny_vectors?: unknown[];
}

// SimulatePolicyOutput is SimReport directly.

// --- /playground tools (spec §3.4 + §5) ---

export interface GetPolicyArtifactsInput {
  spec_id: string;
}

export interface GeneratedSource {
  slot_index: number;
  cargo_toml: string;
  lib_rs: string;
}

export interface PolicyArtifacts {
  spec_id: string;
  generated_sources: GeneratedSource[];
  composed_count: number;
  generated_count: number;
  wasm_sha256: string;
  optimized_wasm_sha256: string;
}

export interface SimulateCustomSourceInput {
  recording_id: string;
  spec_id: string;
  modified_lib_rs: string;
}

// SimulateCustomSourceOutput is SimReport.

export interface CreateSnapshotInput {
  recording_id: string;
  spec_id: string;
  modified_lib_rs?: string;
  report: SimReport;
}

export interface SnapshotRef {
  snapshot_id: string;
  expires_at: string;
}

export interface Snapshot {
  recording_id: string;
  spec_id: string;
  // backend SnapshotRecord embeds Recording + PolicySpec by value so shared
  // snapshot URLs render full state (SpecTab reasoning trace etc.) even
  // after the in-memory recorder cache has GC'd the original recording.
  recording: Recording;
  spec: PolicySpec;
  modified_lib_rs?: string;
  report: SimReport;
}

// --- typed mcp errors. matches `oz_policy_core::errors::Error::code()`. ---

export type McpErrorCode =
  | "E_RECORDER_HASH_NOT_FOUND"
  | "E_RECORDER_SIM_FAILED"
  | "E_RECORDER_XDR_DECODE_FAILED"
  | "E_SYNTH_NOT_EXPRESSIBLE"
  | "E_CODEGEN_COMPILE_FAILED"
  | "E_SIM_PERMIT_DENIED"
  | "E_SIM_DENY_PASSED"
  | "E_VERIFY_DRIFT"
  | "E_WALLET_REJECTED"
  | "E_INSTALL_PREFLIGHT_FAILED";

export class McpError extends Error {
  code: McpErrorCode | string;
  detail: string;
  jsonRpcCode: number;
  constructor(code: McpErrorCode | string, detail: string, jsonRpcCode: number) {
    super(`[${code}] ${detail}`);
    this.name = "McpError";
    this.code = code;
    this.detail = detail;
    this.jsonRpcCode = jsonRpcCode;
  }
}
