/**
 * `installPolicy` — sign + submit + poll + extract `context_rule_id`.
 *
 * Phase 7 Stream C. Drives the high-level "install" pipeline once the
 * Phase 2 `oz-policy-installer` (or the MCP `export_policy` tool) has
 * produced a base64 `TransactionEnvelope` XDR.
 *
 * Flow:
 *   1. `adapter.signTransaction(envelopeXdrBase64, { networkPassphrase })`
 *      — collects a user-authorized signed envelope. NEVER auto-signed.
 *   2. `rpc.Server.sendTransaction(signedTx)` — pushes the envelope to a
 *      Soroban RPC. The result carries the canonical tx hash.
 *   3. `rpc.Server.getTransaction(hash)` polled at 1 s intervals up to 60 s.
 *   4. On SUCCESS: extract the `context_rule_id` from the Soroban return
 *      value (`add_context_rule` returns a `ContextRule` struct with an
 *      `id: u32` field — see `docs/oz-internal-shapes.md` §6.2 and the
 *      installer's `envelope.rs` module doc).
 *   5. Return `{ txHash, contextRuleId, ledger }`.
 *
 * Hard invariant (declared at the type and at runtime): mainnet submission
 * requires an explicit `confirmMainnet: true` on the input. Calling
 * `installPolicy` against a `mainnet` envelope without the flag throws
 * `WalletInstallError(code: 'E_MAINNET_REQUIRES_CONSENT')` before any
 * wallet/RPC interaction. This is a deliberate footgun guard — see
 * `plan.md` § Cross-Phase Invariants ("No auto-deployment, ever").
 */

import {
  Transaction,
  FeeBumpTransaction,
  TransactionBuilder,
  rpc as sorobanRpc,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";

import { WalletAdapter, WalletError, WalletErrorCode } from "./sep43.js";

/** Input parameters for {@link installPolicy}. */
export interface InstallPolicyParams {
  /** Wallet adapter (Freighter, passkey-kit, etc.) — see SEP-43 surface. */
  adapter: WalletAdapter;
  /** Base64 `TransactionEnvelope` XDR produced by `prepare_install`. */
  envelopeXdrBase64: string;
  /** Soroban RPC URL. */
  rpcUrl: string;
  /** Stellar network discriminant. */
  network: "testnet" | "mainnet";
  /** Network passphrase the wallet must sign against. */
  networkPassphrase: string;
  /**
   * Explicit consent flag required for `network === "mainnet"`. The
   * function throws `WalletInstallError(E_MAINNET_REQUIRES_CONSENT)` when
   * `network === "mainnet"` and this is anything other than `true`.
   */
  confirmMainnet?: boolean;
  /**
   * Override the poll interval (ms). Defaults to 1000 ms. Reduced in
   * tests so the 60-s timeout doesn't burn a CI minute. Capped at the
   * total timeout below.
   */
  pollIntervalMs?: number;
  /**
   * Override the total timeout (ms). Defaults to 60_000 ms. Reduced in
   * tests so the timeout branch is reachable in <100 ms.
   */
  pollTimeoutMs?: number;
}

/** Successful result of {@link installPolicy}. */
export interface InstallPolicyResult {
  /** Transaction hash (hex, lowercase, 64 chars). */
  txHash: string;
  /** `context_rule_id` assigned by `add_context_rule` on chain. */
  contextRuleId: number;
  /** Ledger sequence the transaction landed in. */
  ledger: number;
}

/** Canonical error codes emitted by {@link installPolicy}. */
export type WalletInstallErrorCode =
  | "E_WALLET_REJECTED"
  | "E_INSTALL_SUBMIT_FAILED"
  | "E_INSTALL_POLL_TIMEOUT"
  | "E_INSTALL_RESULT_DECODE_FAILED"
  | "E_MAINNET_REQUIRES_CONSENT";

/**
 * Typed error thrown by {@link installPolicy}. Every failure path the
 * caller might want to branch on is encoded as a string `code` (cf. the
 * Rust crate's `Error::*` variants — same naming convention).
 */
export class WalletInstallError extends Error {
  constructor(
    public readonly code: WalletInstallErrorCode,
    public readonly detail: string,
  ) {
    super(`[${code}] ${detail}`);
    this.name = "WalletInstallError";
    Object.setPrototypeOf(this, WalletInstallError.prototype);
  }
}

/** Default poll interval (1 second). */
const DEFAULT_POLL_INTERVAL_MS = 1_000;

/** Default total poll timeout (60 seconds). */
const DEFAULT_POLL_TIMEOUT_MS = 60_000;

/**
 * Sign the install envelope via the wallet adapter, submit to Soroban
 * RPC, poll until SUCCESS or FAILED, extract `context_rule_id` from the
 * returned `ContextRule` struct, and return the resolved IDs.
 *
 * @see InstallPolicyParams
 * @see InstallPolicyResult
 * @see WalletInstallError
 */
export async function installPolicy(
  params: InstallPolicyParams,
): Promise<InstallPolicyResult> {
  // -----------------------------------------------------------------
  // Mainnet consent gate. Runs BEFORE any wallet/RPC interaction so a
  // forgotten flag is loud and free.
  // -----------------------------------------------------------------
  if (params.network === "mainnet" && params.confirmMainnet !== true) {
    throw new WalletInstallError(
      "E_MAINNET_REQUIRES_CONSENT",
      "mainnet submission requires confirmMainnet: true on InstallPolicyParams",
    );
  }

  // -----------------------------------------------------------------
  // 1. Wallet sign.
  // -----------------------------------------------------------------
  let signedTxXdr: string;
  try {
    const signed = await params.adapter.signTransaction(
      params.envelopeXdrBase64,
      { networkPassphrase: params.networkPassphrase },
    );
    signedTxXdr = signed.signedTxXdr;
  } catch (thrown) {
    if (
      thrown instanceof WalletError &&
      thrown.code === WalletErrorCode.UserRejected
    ) {
      throw new WalletInstallError("E_WALLET_REJECTED", thrown.detail);
    }
    if (thrown instanceof WalletError) {
      throw new WalletInstallError(
        "E_INSTALL_SUBMIT_FAILED",
        `wallet error before submit: ${thrown.detail}`,
      );
    }
    const detail =
      thrown instanceof Error ? thrown.message : "unknown signing failure";
    throw new WalletInstallError("E_INSTALL_SUBMIT_FAILED", detail);
  }

  // Re-hydrate the signed XDR into a Transaction so we can hand it to
  // `sendTransaction`. The SDK requires a typed `Transaction` /
  // `FeeBumpTransaction` here — passing the raw base64 would force the
  // SDK to guess, and `sendTransaction` does not accept strings.
  let signedTx: Transaction | FeeBumpTransaction;
  try {
    signedTx = TransactionBuilder.fromXDR(
      signedTxXdr,
      params.networkPassphrase,
    );
  } catch (e) {
    const detail = e instanceof Error ? e.message : "tx decode failed";
    throw new WalletInstallError(
      "E_INSTALL_SUBMIT_FAILED",
      `signed envelope did not round-trip through TransactionBuilder.fromXDR: ${detail}`,
    );
  }

  // -----------------------------------------------------------------
  // 2. Submit to Soroban RPC.
  // -----------------------------------------------------------------
  const server = new sorobanRpc.Server(params.rpcUrl);
  let send: sorobanRpc.Api.SendTransactionResponse;
  try {
    send = await server.sendTransaction(signedTx);
  } catch (e) {
    const detail = e instanceof Error ? e.message : "sendTransaction threw";
    throw new WalletInstallError("E_INSTALL_SUBMIT_FAILED", detail);
  }

  if (send.status === "ERROR" || send.status === "TRY_AGAIN_LATER") {
    throw new WalletInstallError(
      "E_INSTALL_SUBMIT_FAILED",
      `sendTransaction returned status=${send.status}, hash=${send.hash}`,
    );
  }
  // PENDING and DUPLICATE both produce a valid hash we can poll.
  const txHash = send.hash;

  // -----------------------------------------------------------------
  // 3. Poll until SUCCESS / FAILED / timeout.
  // -----------------------------------------------------------------
  const pollIntervalMs = params.pollIntervalMs ?? DEFAULT_POLL_INTERVAL_MS;
  const pollTimeoutMs = params.pollTimeoutMs ?? DEFAULT_POLL_TIMEOUT_MS;
  const startedAt = Date.now();

  let finalResp:
    | sorobanRpc.Api.GetSuccessfulTransactionResponse
    | sorobanRpc.Api.GetFailedTransactionResponse
    | undefined;

  while (Date.now() - startedAt < pollTimeoutMs) {
    let resp: sorobanRpc.Api.GetTransactionResponse;
    try {
      resp = await server.getTransaction(txHash);
    } catch (e) {
      const detail = e instanceof Error ? e.message : "getTransaction threw";
      throw new WalletInstallError(
        "E_INSTALL_SUBMIT_FAILED",
        `getTransaction(${txHash}) threw: ${detail}`,
      );
    }

    if (resp.status === sorobanRpc.Api.GetTransactionStatus.SUCCESS) {
      finalResp = resp;
      break;
    }
    if (resp.status === sorobanRpc.Api.GetTransactionStatus.FAILED) {
      finalResp = resp;
      break;
    }
    // NOT_FOUND — still propagating through the RPC cluster. Keep polling.
    await sleep(pollIntervalMs);
  }

  if (!finalResp) {
    throw new WalletInstallError(
      "E_INSTALL_POLL_TIMEOUT",
      `getTransaction(${txHash}) did not reach SUCCESS or FAILED within ${pollTimeoutMs} ms`,
    );
  }

  if (finalResp.status === sorobanRpc.Api.GetTransactionStatus.FAILED) {
    throw new WalletInstallError(
      "E_INSTALL_SUBMIT_FAILED",
      `transaction ${txHash} landed in ledger ${finalResp.ledger} with status=FAILED`,
    );
  }

  // -----------------------------------------------------------------
  // 4. Extract context_rule_id from the host-fn return value.
  //
  //   `add_context_rule(...) -> ContextRule`
  //   ContextRule = #[contracttype] struct { id: u32, ... }
  //
  // Soroban encodes `#[contracttype]` structs as `ScVal::Map([{Symbol(
  // field), <value>}, ...])`. The SDK helper `scValToNative` walks that
  // map and produces an object with snake_case keys, so we can read
  // `.id` directly.
  // -----------------------------------------------------------------
  const success = finalResp;
  const returnValue: xdr.ScVal | undefined = success.returnValue;
  if (!returnValue) {
    throw new WalletInstallError(
      "E_INSTALL_RESULT_DECODE_FAILED",
      `transaction ${txHash} succeeded but carried no returnValue; cannot read context_rule_id`,
    );
  }

  const contextRuleId = extractContextRuleId(returnValue, txHash);

  return {
    txHash,
    contextRuleId,
    ledger: success.ledger,
  };
}

/**
 * Decode `add_context_rule`'s `ContextRule` return value and return the
 * `id` field (`u32`).
 *
 * Exported for unit tests; production callers go through
 * {@link installPolicy}.
 */
export function extractContextRuleId(
  returnValue: xdr.ScVal,
  txHashForDiag: string,
): number {
  let native: unknown;
  try {
    native = scValToNative(returnValue);
  } catch (e) {
    const detail = e instanceof Error ? e.message : "scValToNative threw";
    throw new WalletInstallError(
      "E_INSTALL_RESULT_DECODE_FAILED",
      `tx ${txHashForDiag}: scValToNative on returnValue failed: ${detail}`,
    );
  }

  if (native === null || typeof native !== "object" || Array.isArray(native)) {
    throw new WalletInstallError(
      "E_INSTALL_RESULT_DECODE_FAILED",
      `tx ${txHashForDiag}: expected ContextRule struct (object) from returnValue, got ${typeof native}`,
    );
  }

  const id = (native as { id?: unknown }).id;
  if (typeof id !== "number" || !Number.isInteger(id) || id < 0) {
    throw new WalletInstallError(
      "E_INSTALL_RESULT_DECODE_FAILED",
      `tx ${txHashForDiag}: ContextRule.id field missing or not a non-negative integer; got ${JSON.stringify(id)}`,
    );
  }

  return id;
}

/** Awaits `ms` milliseconds. Test-overridable indirectly via fake timers. */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
