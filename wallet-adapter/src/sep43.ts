/**
 * SEP-43 web-wallet api types.
 * https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0043.md
 * we restrict `submit` to `false` — install/verify owns submission so the
 * simulate → sign → submit sequence is auditable.
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

/** sep-43 error codes (must stay -1..-4 to round-trip native wallet codes). */
export enum WalletErrorCode {
  Internal = -1,
  ExternalService = -2,
  InvalidRequest = -3,
  UserRejected = -4,
}

/** thrown when a wallet op fails. adapters map native codes onto WalletErrorCode. */
export class WalletError extends Error {
  constructor(
    public readonly code: WalletErrorCode,
    public readonly detail: string,
  ) {
    super(`[wallet:${code}] ${detail}`);
    this.name = "WalletError";
    // restore prototype chain so `instanceof` works across bundle boundaries.
    Object.setPrototypeOf(this, WalletError.prototype);
  }
}
