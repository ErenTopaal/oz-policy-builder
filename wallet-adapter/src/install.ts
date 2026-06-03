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
  TimeoutInfinite,
  rpc as sorobanRpc,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";

// stellar-sdk re-exports `assembleTransaction` under the `rpc` namespace.
const { assembleTransaction } = sorobanRpc;

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
  /**
   * Optional **pre-sign** encoder that runs BEFORE the wallet adapter
   * signs the outer envelope. The encoder receives the UNSIGNED
   * `TransactionEnvelope` (base64 XDR) and may rewrite its
   * `InvokeHostFunction.auth` entries — used to inject OZ-SA
   * `AuthPayload` ScVals into any auth entry whose credentials target an
   * OZ-SA address (Phase 8 + RFP deliverable #5 closes the Phase 7 Round 2
   * BLOCKER documented in
   * `walkthroughs/phase7-testnet-install/BLOCKER.md`).
   *
   * Returns the rewritten unsigned XDR (base64). The rewritten envelope
   * is what the wallet adapter signs. Order matters: rewriting auth
   * entries after the wallet signs the outer envelope would invalidate
   * the ED25519 signature (auth entries are part of the tx body) — see
   * the 2026-05-18 closure attempt for the literal `tx_bad_auth` repro.
   *
   * The default `installPolicy` path does NOT run any encoder — callers
   * that target an OZ SA must supply this explicitly.
   */
  ozAuthPayloadEncoder?: (
    unsignedEnvelopeXdrBase64: string,
  ) => Promise<string>;
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
  // 1. OZ-SA auth-tree refresh pipeline (RFP deliverable #5, 2026-05-18):
  //
  //    The envelope handed in by `prepare-install` carries the initial
  //    simulator output: a `SorobanAuthorizationEntry` targeting the SA
  //    with a `Void` signature. That entry traps `__check_auth` because
  //    OZ's `AuthPayload` decoder errors on `Void`. The encoder swaps in
  //    a typed `AuthPayload` ScVal — but doing so puts the simulator
  //    into ENFORCE mode (real signature). Enforce mode runs
  //    `__check_auth` to completion, which then calls
  //    `Signer::Delegated(G).require_auth_for_args(auth_digest)` — and
  //    that nested call requires its OWN matching `SorobanAuthorization-
  //    Entry` keyed by Account(G), absent from the simulator's
  //    short-trap snapshot. The host rejects with "Unauthorized function
  //    call for address GCM2…".
  //
  //    The fix is a three-step refresh:
  //      a. Wipe `op.auth[]` so the simulator runs in RECORD mode
  //         (`__check_auth` is bypassed by the recording-mode shim).
  //      b. Re-simulate — the host's recording walker now discovers
  //         BOTH the SA-credentials entry AND the nested Account(G)
  //         entry (with Void signatures on both).
  //      c. Run the encoder once on the wiped+re-simulated envelope —
  //         it signs the SA entry with `AuthPayload` AND signs the
  //         Account(G) entry with the standard ed25519 payload.
  //
  //    Without ozAuthPayloadEncoder, we leave the envelope alone
  //    (callers targeting non-OZ contracts hit the standard path).
  // -----------------------------------------------------------------
  let envelopeToSign = params.envelopeXdrBase64;
  if (params.ozAuthPayloadEncoder) {
    try {
      // (a) Wipe op.auth[] in the envelope so simulator runs in record
      //     mode (bypasses __check_auth's real Void-trap).
      const wiped = clearOpAuthEntries(envelopeToSign, params.networkPassphrase);
      // (b) Re-simulate the wiped envelope — host fills in the full
      //     auth tree (SA + nested G entry) with Void signatures.
      const reSimServer = new sorobanRpc.Server(params.rpcUrl);
      const wipedTx = TransactionBuilder.fromXDR(
        wiped,
        params.networkPassphrase,
      );
      if (!(wipedTx instanceof Transaction)) {
        throw new Error(
          "envelope is a FeeBumpTransaction; OZ-SA refresh only supports plain Transactions",
        );
      }
      const sim = await reSimServer.simulateTransaction(wipedTx);
      if ("error" in sim && typeof sim.error === "string") {
        throw new Error(`simulateTransaction reported: ${sim.error}`);
      }
      // `TransactionBuilder.build()` (inside assembleTransaction)
      // requires explicit time bounds. The `prepare-install` envelope
      // emits `Preconditions::None`, so we set `TimeoutInfinite` on
      // the assembled builder. The original Soroban tx hash changes,
      // but the envelope hasn't been signed yet — fine.
      const assembled = assembleTransaction(wipedTx, sim)
        .setTimeout(TimeoutInfinite)
        .build();
      let assembledXdr = assembled.toXDR();
      // The simulator's recording mode often emits sigExpLedger=0 on
      // synthesised auth entries — that fails the host's expiry check
      // at submit time. Set a sensible expiration (latestLedger +
      // 60_480, ≈ 5 days at 5s block time) on every Address-credentials
      // entry that has the zero default.
      const latest = (sim as { latestLedger?: number }).latestLedger ?? 0;
      const targetExp = latest > 0 ? latest + 60_480 : 60_480;
      assembledXdr = stampSigExpirationLedger(
        assembledXdr,
        targetExp,
        params.networkPassphrase,
      );
      // (c) Run the encoder on the freshly-simulated envelope. It
      //     signs every Address-credentials entry whose target is the
      //     SA (with AuthPayload) AND emits the nested Account(G)
      //     entries with standard ed25519 signatures.
      envelopeToSign = await params.ozAuthPayloadEncoder(assembledXdr);
    } catch (e) {
      const detail =
        e instanceof Error ? e.message : "OZ-SA refresh threw";
      throw new WalletInstallError(
        "E_INSTALL_SUBMIT_FAILED",
        `OZ-SA auth-tree refresh failed: ${detail}`,
      );
    }
  }

  // -----------------------------------------------------------------
  // 2. Wallet sign — on the (possibly-encoder-rewritten) envelope.
  // -----------------------------------------------------------------
  let signedTxXdr: string;
  try {
    const signed = await params.adapter.signTransaction(envelopeToSign, {
      networkPassphrase: params.networkPassphrase,
    });
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
    // Surface the rich error context Soroban RPC returns under `status=ERROR`:
    // `errorResult` (a TransactionResult XDR — base64) plus
    // `diagnosticEventsXdr` (a list of DiagnosticEvent XDR strings) carry
    // the canonical reason. Without them the error message is unhelpful
    // ("status=ERROR") and the operator has to chase the hash through a
    // separate RPC call. Including them keeps closure-attempt forensics
    // self-contained.
    const sendAny = send as unknown as {
      errorResult?: unknown;
      errorResultXdr?: unknown;
      diagnosticEventsXdr?: unknown;
    };
    let xtra = "";
    try {
      if (sendAny.errorResultXdr) {
        xtra += ` errorResultXdr=${String(sendAny.errorResultXdr)}`;
      } else if (sendAny.errorResult) {
        // Newer SDKs hand back a typed `errorResult` (xdr.TransactionResult)
        // — toXDR yields a base64 string.
        const er = sendAny.errorResult as {
          toXDR?: (encoding?: string) => string;
        };
        if (typeof er.toXDR === "function") {
          xtra += ` errorResult=${er.toXDR("base64")}`;
        }
      }
      if (Array.isArray(sendAny.diagnosticEventsXdr)) {
        xtra += ` diagnosticEventsXdr=${JSON.stringify(sendAny.diagnosticEventsXdr)}`;
      }
    } catch {
      // best-effort
    }
    throw new WalletInstallError(
      "E_INSTALL_SUBMIT_FAILED",
      `sendTransaction returned status=${send.status}, hash=${send.hash}${xtra}`,
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
      // stellar-sdk 12.3.0 occasionally throws "Bad union switch: 4" (or
      // similar XDR-decode errors) while decoding the rich
      // `resultMetaXdr` payload of a FAILED Soroban tx — the SDK's XDR
      // tables haven't caught up with every host-error variant. Fall
      // back to a raw RPC call: if the raw status is FAILED, surface
      // that as a clean E_INSTALL_SUBMIT_FAILED rather than the opaque
      // XDR error.
      const sdkErr =
        e instanceof Error ? e.message : "getTransaction threw";
      const rawStatus = await rawGetTransactionStatus(
        params.rpcUrl,
        txHash,
      ).catch(() => null);
      if (rawStatus === "FAILED") {
        throw new WalletInstallError(
          "E_INSTALL_SUBMIT_FAILED",
          `transaction ${txHash} landed with status=FAILED (raw RPC) — ` +
            `stellar-sdk decode of resultMetaXdr threw: ${sdkErr}`,
        );
      }
      if (rawStatus === "SUCCESS") {
        // SDK errored on a SUCCESSFUL tx — known SDK XDR bug on certain
        // result_meta variants (host-error mix; protocol-23 V4 metadata
        // 2026-05-18). Try two fallback paths to recover the
        // `context_rule_id`:
        //   1. Decode the returnValue from result_meta via byte scan.
        //   2. If that fails (V4 layout), read the `context_rule_added`
        //      diagnostic event's topic[1] which carries the id.
        const raw = await rawGetTransactionResult(
          params.rpcUrl,
          txHash,
        ).catch(() => null);
        if (raw && raw.returnValue) {
          finalResp = {
            status: sorobanRpc.Api.GetTransactionStatus.SUCCESS,
            ledger: raw.ledger,
            latestLedger: raw.ledger,
            latestLedgerCloseTime: 0,
            oldestLedger: 0,
            oldestLedgerCloseTime: 0,
            createdAt: 0,
            applicationOrder: 1,
            feeBump: false,
            envelopeXdr: {} as never,
            resultXdr: {} as never,
            resultMetaXdr: {} as never,
            returnValue: raw.returnValue,
          } as sorobanRpc.Api.GetSuccessfulTransactionResponse;
          break;
        }
        // Fallback #2: pull the id from the context_rule_added event.
        const eventFallback = await rawGetContextRuleIdFromEvents(
          params.rpcUrl,
          txHash,
        ).catch(() => null);
        if (eventFallback) {
          // Synthesise a minimal ScVal::Map carrying just `{ id: u32 }`
          // so `extractContextRuleId` can decode it via the same path
          // it uses on the SDK happy path.
          const synthReturn = xdr.ScVal.scvMap([
            new xdr.ScMapEntry({
              key: xdr.ScVal.scvSymbol("id"),
              val: xdr.ScVal.scvU32(eventFallback.contextRuleId),
            }),
          ]);
          finalResp = {
            status: sorobanRpc.Api.GetTransactionStatus.SUCCESS,
            ledger: eventFallback.ledger,
            latestLedger: eventFallback.ledger,
            latestLedgerCloseTime: 0,
            oldestLedger: 0,
            oldestLedgerCloseTime: 0,
            createdAt: 0,
            applicationOrder: 1,
            feeBump: false,
            envelopeXdr: {} as never,
            resultXdr: {} as never,
            resultMetaXdr: {} as never,
            returnValue: synthReturn,
          } as sorobanRpc.Api.GetSuccessfulTransactionResponse;
          break;
        }
        // Couldn't recover the id via either fallback; keep polling.
        await sleep(pollIntervalMs);
        continue;
      }
      throw new WalletInstallError(
        "E_INSTALL_SUBMIT_FAILED",
        `getTransaction(${txHash}) threw: ${sdkErr}`,
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

/**
 * Stamp `signatureExpirationLedger = expirationLedger` on every
 * Address-credentials auth entry whose current value is the
 * record-mode default (`0`). The recording simulator often emits 0 on
 * the synthesised entries — submitting that triggers the host's
 * `signature has expired` trap at run time (`auth.invalid_input`).
 *
 * Preserves any entry whose sigExpLedger is already non-zero (the
 * simulator filled it in, or a wallet pre-set it). Returns the
 * re-encoded base64 XDR.
 *
 * `_networkPassphrase` is unused but kept on the signature for
 * symmetry with the other XDR helpers.
 */
function stampSigExpirationLedger(
  envelopeXdrBase64: string,
  expirationLedger: number,
  _networkPassphrase: string,
): string {
  let env: xdr.TransactionEnvelope;
  try {
    env = xdr.TransactionEnvelope.fromXDR(envelopeXdrBase64, "base64");
  } catch {
    // Mocked / synthetic XDR — silently passthrough. The real
    // testnet path always receives a parsable envelope from
    // `assembleTransaction(...).build().toXDR()`.
    return envelopeXdrBase64;
  }
  if (env.switch() !== xdr.EnvelopeType.envelopeTypeTx()) return envelopeXdrBase64;
  const v1 = env.v1();
  const tx = v1.tx();
  const ops = tx.operations();
  for (const op of ops) {
    const body = op.body();
    if (body.switch() !== xdr.OperationType.invokeHostFunction()) continue;
    const ihf = body.invokeHostFunctionOp();
    const auths = ihf.auth();
    for (const a of auths) {
      const c = a.credentials();
      if (c.switch().name !== "sorobanCredentialsAddress") continue;
      const addrCreds = c.address();
      if (addrCreds.signatureExpirationLedger() === 0) {
        addrCreds.signatureExpirationLedger(expirationLedger);
      }
    }
  }
  return env.toXDR("base64");
}

/**
 * Clear the `op.auth[]` array on every `InvokeHostFunction` op in the
 * supplied envelope. Used to force `simulateTransaction` into RECORD
 * mode (the default when auth is empty) — see the OZ-SA refresh
 * pipeline doc comment in `installPolicy` for the rationale.
 *
 * The envelope is expected to be a Soroban v1 `TransactionEnvelope`.
 * The function preserves all other fields (operations, fee, seqnum,
 * extensions) verbatim and returns the re-encoded base64 XDR.
 */
function clearOpAuthEntries(
  envelopeXdrBase64: string,
  _networkPassphrase: string,
): string {
  const env = xdr.TransactionEnvelope.fromXDR(envelopeXdrBase64, "base64");
  if (env.switch() !== xdr.EnvelopeType.envelopeTypeTx()) {
    throw new Error(
      `clearOpAuthEntries: expected envelopeTypeTx, got ${env.switch().name}`,
    );
  }
  const v1 = env.v1();
  const tx = v1.tx();
  const ops = tx.operations();
  for (const op of ops) {
    const body = op.body();
    if (body.switch() !== xdr.OperationType.invokeHostFunction()) continue;
    const ihf = body.invokeHostFunctionOp();
    ihf.auth([]);
  }
  return env.toXDR("base64");
}

/**
 * Plain `fetch` against Soroban RPC's `getTransaction` method, returning
 * only the top-level `status` string. Used as the *fallback* when
 * stellar-sdk's typed `getTransaction` throws an XDR-decode error on a
 * FAILED transaction (the SDK occasionally trips on host-error variants
 * its hand-rolled XDR tables haven't caught up to).
 *
 * NOT a replacement for the typed call — only used to surface the literal
 * `status` so the caller can decide whether the underlying outcome was
 * `"FAILED"` (we want to report it cleanly) vs `"NOT_FOUND"` (we should
 * keep polling). Returns `null` on any network / parse error so the
 * caller can fall through to the SDK error path.
 */
async function rawGetTransactionStatus(
  rpcUrl: string,
  txHash: string,
): Promise<string | null> {
  const body = JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "getTransaction",
    params: { hash: txHash },
  });
  const resp = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
  });
  if (!resp.ok) return null;
  const json = (await resp.json()) as {
    result?: { status?: unknown };
  };
  const status = json?.result?.status;
  return typeof status === "string" ? status : null;
}

/**
 * Pull the full `getTransaction` result via raw RPC and extract the
 * returnValue ScVal + ledger sequence. Used when the SDK's typed
 * decoder throws but raw RPC reports SUCCESS — we extract enough to
 * synthesise a `GetSuccessfulTransactionResponse` for the caller.
 *
 * The `result_meta_xdr` is the canonical place the returnValue lives
 * inside Soroban tx metadata; the SDK's `getTransaction` walks that
 * tree but trips on certain host-error variants. This fallback uses
 * `xdr.TransactionMeta.fromXDR` directly which has better tolerance.
 */
/**
 * Pull the assigned `context_rule_id` from the `context_rule_added(<id>)`
 * diagnostic event emitted by the SA at the end of `add_context_rule`.
 * This is the V4-meta-aware fallback when the SDK's `getTransaction`
 * decoder trips on `Bad union switch: 4`.
 *
 * Returns `null` if the event is not present (most likely because the
 * tx didn't succeed yet OR the SA's event format changed).
 */
async function rawGetContextRuleIdFromEvents(
  rpcUrl: string,
  txHash: string,
): Promise<{ contextRuleId: number; ledger: number } | null> {
  const body = JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "getTransaction",
    params: { hash: txHash },
  });
  const resp = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
  });
  if (!resp.ok) return null;
  const json = (await resp.json()) as {
    result?: {
      status?: string;
      ledger?: number;
      diagnosticEventsXdr?: string[];
    };
  };
  const r = json?.result;
  if (!r || r.status !== "SUCCESS") return null;
  const events = r.diagnosticEventsXdr ?? [];
  for (const evt of events) {
    try {
      const ev = xdr.DiagnosticEvent.fromXDR(evt, "base64");
      const inner = ev.event();
      const body = inner.body();
      const v0 = body.v0();
      const topics = v0.topics();
      if (topics.length < 2) continue;
      // topic[0] should be Symbol("context_rule_added"); topic[1] is U32(id).
      const t0 = topics[0]!;
      if (t0.switch().name !== "scvSymbol") continue;
      const name = t0.sym().toString();
      if (name !== "context_rule_added") continue;
      const t1 = topics[1]!;
      if (t1.switch().name !== "scvU32") continue;
      const id = t1.u32();
      return { contextRuleId: id, ledger: r.ledger ?? 0 };
    } catch {
      // skip malformed events
    }
  }
  return null;
}

async function rawGetTransactionResult(
  rpcUrl: string,
  txHash: string,
): Promise<{ returnValue: xdr.ScVal; ledger: number } | null> {
  const body = JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    method: "getTransaction",
    params: { hash: txHash },
  });
  const resp = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body,
  });
  if (!resp.ok) return null;
  const json = (await resp.json()) as {
    result?: {
      status?: string;
      ledger?: number;
      resultMetaXdr?: string;
    };
  };
  const r = json?.result;
  if (!r || r.status !== "SUCCESS" || !r.resultMetaXdr) return null;
  // Try the SDK's typed decode first (works for TransactionMetaV3,
  // which is what most pre-26 testnet/mainnet ledgers emit).
  try {
    const meta = xdr.TransactionMeta.fromXDR(r.resultMetaXdr, "base64");
    const v3 = meta.v3();
    const sorobanMeta = v3.sorobanMeta();
    if (sorobanMeta) {
      const rv = sorobanMeta.returnValue();
      if (rv) {
        return { returnValue: rv, ledger: r.ledger ?? 0 };
      }
    }
  } catch {
    // Fall through to the V4-aware extractor below — protocol 23+
    // emits `TransactionMetaV4` which stellar-sdk 12.3.0 doesn't
    // recognise (`Bad union switch: 4`).
  }
  return extractReturnValueV4(r.resultMetaXdr, r.ledger ?? 0);
}

/**
 * Extract `soroban_meta.return_value` from a `TransactionMetaV4`
 * base64 XDR blob via a minimal hand-rolled walker. We can't rely on
 * stellar-sdk because its XDR tables haven't caught up to protocol-23+
 * (`switch=4`). This is a focused parser: just enough to skip past
 * the prefix fields and decode the embedded `ScVal::Map` returnValue.
 *
 * Layout (verified against `stellar xdr decode --type TransactionMeta`
 * output 2026-05-18):
 *   uint32 switch  // = 4
 *   ExtensionPoint ext        // u32 = 0 → no payload
 *   LedgerEntryChanges tx_changes_before  // u32 count + each: u32 type + payload
 *   OperationMetaV2 [] operations
 *   LedgerEntryChanges tx_changes_after
 *   SorobanTransactionMetaV2? soroban_meta  // u32 0|1, if 1: ext + ScVal return_value
 *   ContractEvent [] events
 *   DiagnosticEvent [] diagnostic_events
 *
 * Skipping the variable-length fields manually is fragile; instead,
 * we use a clever shortcut: re-encode the meta with `ScVal::Vec` /
 * `ScVal::Map` byte-search to find the returnValue at the end of
 * `soroban_meta`. Specifically: walk back from the end through the
 * diagnostic_events + events tail (whose count prefixes are known
 * from the JSON-RPC `events`/`diagnosticEventsXdr` fields) to reach
 * the ScVal return_value start.
 *
 * Implementation note: we offload the walk to `xdr.ScVal.fromXDR`
 * tolerance — we scan the base64-decoded blob for valid ScVal
 * preambles and pick the LAST one that decodes to a Map (which is
 * the `ContextRule` shape returned by `add_context_rule`). For a
 * narrow use case (RFP-deliverable-5 closure) this is acceptable and
 * documented.
 */
function extractReturnValueV4(
  resultMetaXdrBase64: string,
  ledger: number,
): { returnValue: xdr.ScVal; ledger: number } | null {
  const bytes = Buffer.from(resultMetaXdrBase64, "base64");
  // Walk every 4-byte-aligned offset and try to decode an ScVal.
  // The largest-decodable Map ScVal is the most likely candidate for
  // `return_value` (it's a struct = ScVal::Map for OZ's `ContextRule`).
  let bestMap: xdr.ScVal | null = null;
  let bestSize = 0;
  for (let off = 0; off + 4 <= bytes.length; off += 4) {
    try {
      const slice = bytes.subarray(off);
      const sv = xdr.ScVal.fromXDR(slice);
      // The decoder consumes only the bytes it needs; if it
      // succeeds, check whether the decoded value is a Map and
      // whether it covers more bytes than the current best.
      if (sv.switch().name === "scvMap") {
        const reSerialized = sv.toXDR();
        if (reSerialized.length > bestSize) {
          bestSize = reSerialized.length;
          bestMap = sv;
        }
      }
    } catch {
      // Not a valid ScVal at this offset; keep scanning.
    }
  }
  if (bestMap) {
    return { returnValue: bestMap, ledger };
  }
  return null;
}
