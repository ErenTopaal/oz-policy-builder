/**
 * SEP-43 (Draft v1.2.1) — Standard Web Wallet API Interface
 * Source: https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0043.md
 *
 * Shared types Stream A authors. Streams B (passkey-kit adapter) and C
 * (install/verify orchestration) import from here — they do NOT redefine
 * these shapes.
 *
 * Design notes:
 * - We intentionally restrict `submit` to `false` (never auto-submit). Our
 *   install/verify pipeline owns submission so that simulate -> sign -> submit
 *   is an explicit, auditable sequence.
 * - `WalletErrorCode` numerics MUST match the SEP-43 spec (-1..-4) so that
 *   adapters can pass through the wallet's native `code` field unchanged.
 */

/** Options accepted by {@link WalletAdapter.signTransaction}. */
export interface SignTransactionParams {
  /** Stellar network passphrase the wallet must sign against. */
  networkPassphrase: string;
  /** Optional G-address (StrKey) when the wallet holds multiple accounts. */
  address?: string;
  /**
   * SEP-43 allows wallets to submit on the client's behalf. We forbid that —
   * submission is owned by `wallet-adapter/src/install.ts` (Stream C).
   */
  submit?: false;
}

/** Result of {@link WalletAdapter.signTransaction}. */
export interface SignTransactionResult {
  /** Base64-encoded signed TransactionEnvelope XDR. */
  signedTxXdr: string;
  /** G-address (StrKey) of the signer. */
  signerAddress: string;
}

/** Options accepted by {@link WalletAdapter.signAuthEntry}. */
export interface SignAuthEntryParams {
  /** Stellar network passphrase the wallet must sign against. */
  networkPassphrase: string;
  /** Optional G-address (StrKey) when the wallet holds multiple accounts. */
  address?: string;
}

/** Result of {@link WalletAdapter.signAuthEntry}. */
export interface SignAuthEntryResult {
  /** Base64-encoded signed Soroban auth entry hash. */
  signedAuthEntry: string;
  /** G-address (StrKey) of the signer. */
  signerAddress: string;
}

/**
 * Common adapter interface that every wallet implementation in this package
 * MUST satisfy. Higher layers (`install.ts`, `verify.ts`) program against
 * this contract — never against the underlying SDK directly.
 */
export interface WalletAdapter {
  /** Returns `true` if the wallet is reachable and ready to sign. */
  isAvailable(): Promise<boolean>;
  /** Returns the active G-address (StrKey) the wallet will sign with. */
  getAddress(): Promise<string>;
  /** Signs a base64 TransactionEnvelope XDR. */
  signTransaction(
    envelopeXdr: string,
    params: SignTransactionParams,
  ): Promise<SignTransactionResult>;
  /** Signs a base64 Soroban auth-entry preimage XDR. */
  signAuthEntry(
    authEntryXdr: string,
    params: SignAuthEntryParams,
  ): Promise<SignAuthEntryResult>;
}

/**
 * Canonical SEP-43 error codes (Draft v1.2.1, §"Errors").
 *
 * - `Internal` (-1): wallet-internal failure, e.g. a JS runtime error inside
 *   the extension.
 * - `ExternalService` (-2): an upstream service (Horizon, RPC, etc.) returned
 *   an error to the wallet.
 * - `InvalidRequest` (-3): client passed malformed input, e.g. invalid XDR.
 * - `UserRejected` (-4): the user explicitly declined. Clients SHOULD NOT
 *   retry without user action.
 */
export enum WalletErrorCode {
  Internal = -1,
  ExternalService = -2,
  InvalidRequest = -3,
  UserRejected = -4,
}

/**
 * Thrown by adapters when a wallet operation fails. Adapters MUST map the
 * underlying wallet's native error onto one of {@link WalletErrorCode} so
 * that consumers can handle errors uniformly.
 */
export class WalletError extends Error {
  /**
   * @param code SEP-43 numeric code (one of {@link WalletErrorCode}).
   * @param detail Human-readable detail from the underlying wallet.
   */
  constructor(
    public readonly code: WalletErrorCode,
    public readonly detail: string,
  ) {
    super(`[wallet:${code}] ${detail}`);
    this.name = "WalletError";
    // Restore prototype chain when transpiled down — needed for `instanceof`
    // to work across module boundaries in some bundlers.
    Object.setPrototypeOf(this, WalletError.prototype);
  }
}
