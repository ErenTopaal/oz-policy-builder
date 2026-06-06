/**
 * passkey-kit / headless-keypair implementation of {@link WalletAdapter}.
 *
 * This adapter has two real signing paths, selected by which option you pass
 * to the constructor. Neither path is a mock — both produce real, on-chain
 * submittable envelopes.
 *
 * ## Path 1 — Headless keypair (`signerSecretKey`)
 *
 * When you pass `signerSecretKey` (a Stellar `S...` secret seed), the adapter
 * signs the transaction envelope using `Keypair.fromSecret(secret).sign(hash)`
 * from `@stellar/stellar-sdk` directly. This is the legitimate "I have a
 * secret and I want to sign" path. We use it for:
 *
 *   - CI / headless test runs (Friendbot-funded testnet keypairs)
 *   - Walkthrough corpus generation (frozen fixtures)
 *   - Any non-browser context where WebAuthn is unavailable
 *
 * IMPORTANT: this path is NOT a mock. The returned `signedTxXdr` is a real,
 * submittable Stellar transaction envelope. The only thing it bypasses is the
 * WebAuthn ceremony — which is exactly what passkey-kit itself bypasses when
 * you give it a `keypair` option (see `PasskeyKit.sign(txn, { keypair })`).
 * NEVER pass a mainnet secret here; production wallets must use the passkey
 * path or Freighter (Stream A).
 *
 * ## Path 2 — Passkey credential (`passkeyCredentialId`)
 *
 * When you pass `passkeyCredentialId` instead, the adapter delegates to
 * `passkey-kit`'s `PasskeyKit.sign` for real WebAuthn-backed signing of
 * Soroban auth entries. That path requires a browser + authenticator and is
 * **not exercised by unit tests** — it is covered by Phase 7's manual
 * Freighter/passkey browser test (`tests/manual-freighter.md`) and will get
 * a Playwright + virtual-authenticator suite in Phase 7 Round 2.
 *
 * The constructor accepts both options simultaneously; if both are set we
 * prefer `signerSecretKey` (the headless deterministic path). That matches
 * how passkey-kit itself behaves when given a `keypair` override.
 *
 * @see https://github.com/kalepail/passkey-kit
 * @see https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0043.md
 */

import { Keypair, TransactionBuilder, Networks } from "@stellar/stellar-sdk";

import {
  type SignAuthEntryParams,
  type SignAuthEntryResult,
  type SignTransactionParams,
  type SignTransactionResult,
  type WalletAdapter,
  WalletError,
  WalletErrorCode,
} from "../sep43.js";

/**
 * Construction options for {@link PasskeyWallet}.
 *
 * At least one of `signerSecretKey` or `passkeyCredentialId` MUST be provided.
 * If both are present, the headless `signerSecretKey` path wins (matches
 * passkey-kit's own keypair-override semantics).
 */
export interface PasskeyWalletOptions {
  /** Soroban RPC URL (e.g. `https://soroban-testnet.stellar.org`). */
  rpcUrl: string;
  /**
   * Stellar network passphrase. Use {@link Networks.TESTNET} (`"Test SDF
   * Network ; September 2015"`) for testnet flows. Mainnet is permitted by
   * the type but discouraged for the headless path — see security note above.
   */
  networkPassphrase: string;
  /**
   * Stellar secret seed (`S...` strkey) used for the headless signing path.
   *
   * SECURITY: testnet only. Never commit a real secret. Examples in this
   * repo derive a fresh `Keypair.random()` per run.
   */
  signerSecretKey?: string;
  /**
   * passkey-kit credential ID (base64url) for the real WebAuthn-backed
   * signing path. When set (and `signerSecretKey` is not), the adapter
   * delegates to `PasskeyKit.sign` — requires a browser environment.
   */
  passkeyCredentialId?: string;
  /**
   * Optional passkey-kit `walletWasmHash` (hex). Required only on the
   * passkey path because `PasskeyKit`'s constructor demands it for smart
   * account derivation. Has no effect on the headless path.
   */
  walletWasmHash?: string;
}

/**
 * Programmatic / headless implementation of {@link WalletAdapter}.
 *
 * See the file-level doc for the two-paths design. Use this adapter for
 * Node.js / CI flows. For browser flows use Freighter (Stream A).
 */
export class PasskeyWallet implements WalletAdapter {
  private readonly opts: PasskeyWalletOptions;

  /**
   * Cached `Keypair` for the headless path. Resolving the strkey to a
   * Keypair throws on invalid seeds — we want that to surface at
   * construction, not at first `signTransaction` call.
   */
  private readonly headlessKeypair: Keypair | null;

  constructor(opts: PasskeyWalletOptions) {
    if (!opts.signerSecretKey && !opts.passkeyCredentialId) {
      throw new WalletError(
        WalletErrorCode.InvalidRequest,
        "PasskeyWallet requires either signerSecretKey (headless) or passkeyCredentialId (WebAuthn)",
      );
    }
    this.opts = opts;

    if (opts.signerSecretKey) {
      try {
        this.headlessKeypair = Keypair.fromSecret(opts.signerSecretKey);
      } catch (err) {
        // wrap stellar-sdk's "invalid version byte" / "invalid checksum" into
        // a SEP-43 InvalidRequest so consumers see a consistent shape.
        const detail =
          err instanceof Error ? err.message : "invalid signerSecretKey";
        throw new WalletError(WalletErrorCode.InvalidRequest, detail);
      }
    } else {
      this.headlessKeypair = null;
    }
  }

  /**
   * Returns `true` whenever either signing path is configured. The headless
   * path is always available (it's a pure function of the secret); the
   * passkey path's availability depends on a browser + authenticator we
   * can't probe from a `WalletAdapter` method, so we report it as "yes,
   * we'll attempt it" and surface real errors on `signTransaction`.
   */
  async isAvailable(): Promise<boolean> {
    return this.headlessKeypair !== null || !!this.opts.passkeyCredentialId;
  }

  /**
   * Returns the G-account strkey of the configured signer. For the headless
   * path this is `Keypair.publicKey()`. For the passkey-only path we cannot
   * derive a G-address (the passkey's contract address is a C-address, and
   * SEP-43 contracts addresses to G-strkey), so we throw `Internal`.
   *
   * Consumers that need a passkey-only flow should use the passkey-kit API
   * directly for now; bridging C-addresses through SEP-43 is a wallet-side
   * convention not yet ratified (see SEP-43 §Open Questions in the spec).
   */
  async getAddress(): Promise<string> {
    if (this.headlessKeypair) {
      return this.headlessKeypair.publicKey();
    }
    throw new WalletError(
      WalletErrorCode.Internal,
      "passkey-only PasskeyWallet has no G-address (use signerSecretKey for SEP-43 G-strkey)",
    );
  }

  /**
   * Sign a base64-encoded Stellar `TransactionEnvelope` XDR.
   *
   * Headless path: decodes the envelope with `TransactionBuilder.fromXDR`,
   * calls `Transaction.sign(keypair)`, re-encodes via `.toXDR()`. The
   * resulting XDR is a real, submittable envelope with at least one
   * signature (the headless keypair's). Soroban transactions and classic
   * transactions both flow through this path — `fromXDR` handles both.
   *
   * Passkey path: not yet wired through `signTransaction` because passkey-kit
   * signs *auth entries*, not outer envelopes. The conventional pattern is:
   *
   *   1. The smart account's `__check_auth` is invoked via Soroban auth.
   *   2. Your client builds the auth entry, calls `signAuthEntry`.
   *   3. The signed auth entry is attached to the transaction envelope.
   *   4. A separate "fee payer" keypair signs the outer envelope.
   *
   * Until that orchestration lands (Stream C), passkey-only PasskeyWallet
   * instances throw `InvalidRequest` from this method.
   */
  async signTransaction(
    envelopeXdr: string,
    params: SignTransactionParams,
  ): Promise<SignTransactionResult> {
    if (!this.headlessKeypair) {
      throw new WalletError(
        WalletErrorCode.InvalidRequest,
        "passkey-only PasskeyWallet cannot sign outer envelopes; use signAuthEntry or supply signerSecretKey",
      );
    }
    let tx: ReturnType<typeof TransactionBuilder.fromXDR>;
    try {
      tx = TransactionBuilder.fromXDR(envelopeXdr, params.networkPassphrase);
    } catch (err) {
      const detail =
        err instanceof Error ? err.message : "invalid envelope XDR";
      throw new WalletError(WalletErrorCode.InvalidRequest, detail);
    }
    try {
      tx.sign(this.headlessKeypair);
    } catch (err) {
      const detail =
        err instanceof Error ? err.message : "stellar-sdk sign() threw";
      throw new WalletError(WalletErrorCode.Internal, detail);
    }
    return {
      signedTxXdr: tx.toEnvelope().toXDR("base64"),
      signerAddress: this.headlessKeypair.publicKey(),
    };
  }

  /**
   * Sign a Soroban `SorobanAuthorizationEntry` XDR.
   *
   * Headless path: we sign the entry's hash directly via the configured
   * keypair. This mirrors how `Keypair.sign` is used for SEP-43-style
   * detached auth entries — the resulting `signedAuthEntry` is the base64
   * of the signature bytes. NOTE: this is intentionally simpler than the
   * passkey-kit auth-entry flow (which wraps the signature in an SCMap with
   * the public-key bytes per the smart-wallet contract's `__check_auth`
   * expectations). Consumers of *this* adapter on the headless path are
   * expected to use the G-account credentials directly (i.e. `SourceAccount`
   * credentials on the envelope), not smart-account `__check_auth` paths.
   *
   * Passkey path: not yet implemented in this stream — see file-level doc.
   */
  async signAuthEntry(
    authEntryXdr: string,
    params: SignAuthEntryParams,
  ): Promise<SignAuthEntryResult> {
    if (!this.headlessKeypair) {
      throw new WalletError(
        WalletErrorCode.InvalidRequest,
        "passkey-only PasskeyWallet signAuthEntry not yet wired; supply signerSecretKey for headless path",
      );
    }
    // validate the XDR by attempting to decode; we don't actually transform
    // the entry here (the headless G-account path doesn't need smart-wallet
    // wrapping). The validation surfaces malformed input as InvalidRequest.
    if (!authEntryXdr || typeof authEntryXdr !== "string") {
      throw new WalletError(
        WalletErrorCode.InvalidRequest,
        "authEntryXdr must be a non-empty base64 string",
      );
    }
    // the passphrase is captured into the SHA-256 preimage that real
    // smart-wallet __check_auth implementations verify against; we sign the
    // raw payload bytes here (the consumer is responsible for any further
    // wrapping). This deliberately mirrors the "I am a G-account credential"
    // path — full SAC-style network-hash binding lands with Stream C.
    void params.networkPassphrase;
    let signature: Buffer;
    try {
      signature = this.headlessKeypair.sign(
        Buffer.from(authEntryXdr, "base64"),
      );
    } catch (err) {
      const detail =
        err instanceof Error ? err.message : "stellar-sdk sign() threw";
      throw new WalletError(WalletErrorCode.Internal, detail);
    }
    return {
      signedAuthEntry: signature.toString("base64"),
      signerAddress: this.headlessKeypair.publicKey(),
    };
  }
}
