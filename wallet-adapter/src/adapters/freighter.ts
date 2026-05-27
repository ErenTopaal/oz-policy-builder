/**
 * Freighter (browser extension) implementation of {@link WalletAdapter}.
 *
 * Wraps `@stellar/freighter-api` v6.0.1. Verified shape (from the package's
 * own `.d.ts` and source under `node_modules/@stellar/freighter-api/src/`):
 *
 * ```ts
 * isConnected(): Promise<{ isConnected: boolean; error?: FreighterApiError }>
 * getAddress(): Promise<{ address: string; error?: FreighterApiError }>
 * signTransaction(xdr, opts?): Promise<{ signedTxXdr: string; signerAddress: string; error?: FreighterApiError }>
 * signAuthEntry(xdr, opts?): Promise<{ signedAuthEntry: string | null; signerAddress: string; error?: FreighterApiError }>
 *
 * interface FreighterApiError { code: number; message: string; ext?: string[] }
 * ```
 *
 * The freighter-api package returns errors as a field on a resolved promise
 * (it does NOT reject). We must inspect `error` on every call and translate
 * to {@link WalletError} so consumers can `try/catch` uniformly.
 *
 * Error mapping is by the SEP-43 numeric `code` (canonical), not by string
 * matching the message — Freighter passes the extension's `code` through and
 * `code` is the only stable contract.
 */

import {
  getAddress as freighterGetAddress,
  isConnected as freighterIsConnected,
  signAuthEntry as freighterSignAuthEntry,
  signTransaction as freighterSignTransaction,
} from "@stellar/freighter-api";

import {
  type SignAuthEntryParams,
  type SignAuthEntryResult,
  type SignTransactionParams,
  type SignTransactionResult,
  type WalletAdapter,
  WalletError,
  WalletErrorCode,
} from "../sep43.js";

/** Shape of the `error` field returned by freighter-api 6.0.1. */
interface FreighterApiErrorLike {
  code: number;
  message: string;
  ext?: string[];
}

/**
 * Map a freighter-api error onto a {@link WalletError}.
 *
 * If the wallet returns a recognized SEP-43 code (-1..-4) we pass it through
 * verbatim. Any other code is escalated to {@link WalletErrorCode.ExternalService}
 * — that bucket explicitly covers "we got a structured error but didn't
 * recognize it" per SEP-43's §Errors section.
 */
function toWalletError(err: FreighterApiErrorLike): WalletError {
  const detail = err.ext?.length
    ? `${err.message} (${err.ext.join("; ")})`
    : err.message;

  switch (err.code) {
    case WalletErrorCode.Internal:
    case WalletErrorCode.ExternalService:
    case WalletErrorCode.InvalidRequest:
    case WalletErrorCode.UserRejected:
      return new WalletError(err.code, detail);
    default:
      return new WalletError(WalletErrorCode.ExternalService, detail);
  }
}

/**
 * Wrap a thrown (non-resolved-error) failure — e.g. the freighter-api package
 * itself crashes, or a transport rejection. This path is rare but possible
 * (e.g. content-script disconnected mid-call).
 */
function fromThrown(thrown: unknown): WalletError {
  if (thrown instanceof WalletError) {
    return thrown;
  }
  const detail =
    thrown instanceof Error
      ? thrown.message
      : typeof thrown === "string"
        ? thrown
        : "unknown freighter-api failure";
  return new WalletError(WalletErrorCode.ExternalService, detail);
}

/** Freighter (browser extension) adapter. */
export class FreighterWallet implements WalletAdapter {
  async isAvailable(): Promise<boolean> {
    try {
      const { isConnected, error } = await freighterIsConnected();
      // freighter-api returns `error` (with code -1) in Node-like environments
      // where `window` is absent. Treat any error as "not available" — do NOT
      // throw. Availability is a probe, not a sign operation.
      if (error) {
        return false;
      }
      return isConnected === true;
    } catch {
      return false;
    }
  }

  async getAddress(): Promise<string> {
    let resp: { address: string; error?: FreighterApiErrorLike };
    try {
      resp = await freighterGetAddress();
    } catch (thrown) {
      throw fromThrown(thrown);
    }
    if (resp.error) {
      throw toWalletError(resp.error);
    }
    if (!resp.address) {
      throw new WalletError(
        WalletErrorCode.Internal,
        "freighter returned empty address",
      );
    }
    return resp.address;
  }

  async signTransaction(
    envelopeXdr: string,
    params: SignTransactionParams,
  ): Promise<SignTransactionResult> {
    let resp: {
      signedTxXdr: string;
      signerAddress: string;
      error?: FreighterApiErrorLike;
    };
    try {
      resp = await freighterSignTransaction(envelopeXdr, {
        networkPassphrase: params.networkPassphrase,
        ...(params.address !== undefined ? { address: params.address } : {}),
      });
    } catch (thrown) {
      throw fromThrown(thrown);
    }
    if (resp.error) {
      throw toWalletError(resp.error);
    }
    return {
      signedTxXdr: resp.signedTxXdr,
      signerAddress: resp.signerAddress,
    };
  }

  async signAuthEntry(
    authEntryXdr: string,
    params: SignAuthEntryParams,
  ): Promise<SignAuthEntryResult> {
    let resp: {
      signedAuthEntry: string | null;
      signerAddress: string;
      error?: FreighterApiErrorLike;
    };
    try {
      resp = await freighterSignAuthEntry(authEntryXdr, {
        networkPassphrase: params.networkPassphrase,
        ...(params.address !== undefined ? { address: params.address } : {}),
      });
    } catch (thrown) {
      throw fromThrown(thrown);
    }
    if (resp.error) {
      throw toWalletError(resp.error);
    }
    if (resp.signedAuthEntry === null) {
      // freighter-api types this field as `string | null`; null with no
      // accompanying error is unexpected. Treat as internal wallet bug.
      throw new WalletError(
        WalletErrorCode.Internal,
        "freighter returned null signedAuthEntry with no error",
      );
    }
    return {
      signedAuthEntry: resp.signedAuthEntry,
      signerAddress: resp.signerAddress,
    };
  }
}
