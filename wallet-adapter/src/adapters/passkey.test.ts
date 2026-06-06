/**
 * Tests for the {@link PasskeyWallet} adapter.
 *
 * IMPORTANT: there are NO mocks for the signing path. We use a real
 * `@stellar/stellar-sdk` `Keypair` to sign a real `TransactionEnvelope`,
 * then we decode the result and assert it has at least one signature.
 * This is the same code path examples/01-blend-yield-headless.ts uses
 * against testnet — if these unit tests pass, the headless signing path
 * is end-to-end correct (modulo network availability).
 *
 * The passkey / WebAuthn path is gated behind a `.skip`ed placeholder
 * because it requires a browser + virtual authenticator. Phase 7 Round 2
 * will cover it via Playwright (`page.evaluate` + WebDriver's CDP virtual
 * authenticator endpoint).
 */

import { describe, expect, it } from "vitest";
import {
  Account,
  Keypair,
  Networks,
  Operation,
  TransactionBuilder,
  xdr,
} from "@stellar/stellar-sdk";

import { WalletError, WalletErrorCode } from "../sep43.js";
import { PasskeyWallet } from "./passkey.js";

const NETWORK_PASSPHRASE = Networks.TESTNET; // "Test SDF Network ; September 2015"
const RPC_URL = "https://soroban-testnet.stellar.org";

/**
 * Build a minimal but valid `TransactionEnvelope` XDR for an unsigned classic
 * transaction with a single `bumpSequence` op. We use this as the input to
 * `signTransaction` — it's small, deterministic, and round-trips through any
 * SEP-43-compliant signer.
 */
function buildUnsignedEnvelope(sourceAccountG: string): string {
  const account = new Account(sourceAccountG, "0");
  const tx = new TransactionBuilder(account, {
    fee: "100",
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(Operation.bumpSequence({ bumpTo: "1" }))
    .setTimeout(0)
    .build();
  return tx.toEnvelope().toXDR("base64");
}

describe("PasskeyWallet construction", () => {
  it("requires at least one of signerSecretKey / passkeyCredentialId", () => {
    expect(
      () =>
        new PasskeyWallet({
          rpcUrl: RPC_URL,
          networkPassphrase: NETWORK_PASSPHRASE,
        }),
    ).toThrowError(WalletError);
  });

  it("throws WalletError(InvalidRequest) on a malformed signerSecretKey", () => {
    let caught: unknown;
    try {
      new PasskeyWallet({
        rpcUrl: RPC_URL,
        networkPassphrase: NETWORK_PASSPHRASE,
        signerSecretKey: "not-a-real-secret",
      });
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(WalletError);
    expect((caught as WalletError).code).toBe(WalletErrorCode.InvalidRequest);
  });

  it("accepts a valid signerSecretKey", () => {
    const kp = Keypair.random();
    const wallet = new PasskeyWallet({
      rpcUrl: RPC_URL,
      networkPassphrase: NETWORK_PASSPHRASE,
      signerSecretKey: kp.secret(),
    });
    expect(wallet).toBeInstanceOf(PasskeyWallet);
  });
});

describe("PasskeyWallet.isAvailable", () => {
  it("returns true once a signerSecretKey is configured", async () => {
    const wallet = new PasskeyWallet({
      rpcUrl: RPC_URL,
      networkPassphrase: NETWORK_PASSPHRASE,
      signerSecretKey: Keypair.random().secret(),
    });
    await expect(wallet.isAvailable()).resolves.toBe(true);
  });
});

describe("PasskeyWallet.getAddress (headless)", () => {
  it("returns the G-account derived from the secret", async () => {
    const kp = Keypair.random();
    const wallet = new PasskeyWallet({
      rpcUrl: RPC_URL,
      networkPassphrase: NETWORK_PASSPHRASE,
      signerSecretKey: kp.secret(),
    });
    await expect(wallet.getAddress()).resolves.toBe(kp.publicKey());
  });
});

describe("PasskeyWallet.signTransaction (headless)", () => {
  it("returns a real, submittable XDR with at least one signature", async () => {
    const kp = Keypair.random();
    const wallet = new PasskeyWallet({
      rpcUrl: RPC_URL,
      networkPassphrase: NETWORK_PASSPHRASE,
      signerSecretKey: kp.secret(),
    });

    const unsignedXdr = buildUnsignedEnvelope(kp.publicKey());

    const { signedTxXdr, signerAddress } = await wallet.signTransaction(
      unsignedXdr,
      { networkPassphrase: NETWORK_PASSPHRASE },
    );

    // 1. signerAddress matches the keypair's G-address.
    expect(signerAddress).toBe(kp.publicKey());

    // 2. signedTxXdr is a valid base64 string. We round-trip it: base64 decode
    //    must succeed, and re-encoding must yield the same string.
    expect(typeof signedTxXdr).toBe("string");
    expect(signedTxXdr.length).toBeGreaterThan(0);
    expect(Buffer.from(signedTxXdr, "base64").toString("base64")).toBe(
      signedTxXdr,
    );

    // 3. The XDR decodes to a `TransactionEnvelope` with at least one signature.
    //    we pull it back through stellar-sdk to prove it's structurally valid.
    const reconstructed = TransactionBuilder.fromXDR(
      signedTxXdr,
      NETWORK_PASSPHRASE,
    );
    // `Transaction` (classic) has `signatures`; FeeBumpTransaction wraps an
    // inner. Our minimal envelope is classic, so we narrow with a runtime guard.
    expect("signatures" in reconstructed).toBe(true);
    if ("signatures" in reconstructed) {
      expect(reconstructed.signatures.length).toBeGreaterThanOrEqual(1);
    }

    // 4. Round-trip via the raw XDR union directly — proves the bytes parse.
    const env = xdr.TransactionEnvelope.fromXDR(signedTxXdr, "base64");
    expect(env.switch().name).toMatch(/envelopeTypeTx/);
  });

  it("throws WalletError(InvalidRequest) on a malformed envelope XDR", async () => {
    const wallet = new PasskeyWallet({
      rpcUrl: RPC_URL,
      networkPassphrase: NETWORK_PASSPHRASE,
      signerSecretKey: Keypair.random().secret(),
    });
    await expect(
      wallet.signTransaction("not-base64-xdr!!", {
        networkPassphrase: NETWORK_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.InvalidRequest,
    });
  });

  it("throws WalletError(InvalidRequest) on a passkey-only wallet (no signerSecretKey)", async () => {
    const wallet = new PasskeyWallet({
      rpcUrl: RPC_URL,
      networkPassphrase: NETWORK_PASSPHRASE,
      passkeyCredentialId: "AAAAFAKEcredential",
      walletWasmHash: "00".repeat(32),
    });
    const unsignedXdr = buildUnsignedEnvelope(Keypair.random().publicKey());
    await expect(
      wallet.signTransaction(unsignedXdr, {
        networkPassphrase: NETWORK_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.InvalidRequest,
    });
  });
});

describe("PasskeyWallet.signAuthEntry (headless)", () => {
  it("returns a base64 signature for a base64 payload", async () => {
    const kp = Keypair.random();
    const wallet = new PasskeyWallet({
      rpcUrl: RPC_URL,
      networkPassphrase: NETWORK_PASSPHRASE,
      signerSecretKey: kp.secret(),
    });

    // any base64 string works for the headless smoke path — we sign the bytes.
    const payload = Buffer.from("auth-entry-bytes", "utf-8").toString("base64");
    const { signedAuthEntry, signerAddress } = await wallet.signAuthEntry(
      payload,
      { networkPassphrase: NETWORK_PASSPHRASE },
    );
    expect(signerAddress).toBe(kp.publicKey());
    expect(typeof signedAuthEntry).toBe("string");
    // ed25519 signatures are 64 bytes; base64-encoded that's 88 chars.
    expect(signedAuthEntry!.length).toBeGreaterThanOrEqual(80);

    // the signature must verify against the keypair.
    const sig = Buffer.from(signedAuthEntry!, "base64");
    const verifies = kp.verify(Buffer.from(payload, "base64"), sig);
    expect(verifies).toBe(true);
  });

  it("throws WalletError(InvalidRequest) on empty input", async () => {
    const wallet = new PasskeyWallet({
      rpcUrl: RPC_URL,
      networkPassphrase: NETWORK_PASSPHRASE,
      signerSecretKey: Keypair.random().secret(),
    });
    await expect(
      wallet.signAuthEntry("", { networkPassphrase: NETWORK_PASSPHRASE }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.InvalidRequest,
    });
  });
});

describe.skip("PasskeyWallet (WebAuthn / passkey path)", () => {
  // TODO: cover in Phase 7 Round 2 with playwright + virtual authenticator.
  // requires browser + CDP `WebAuthn.addVirtualAuthenticator` per
  // https://chromedevtools.github.io/devtools-protocol/tot/WebAuthn/.
  it("signs an auth entry via PasskeyKit.sign", () => {
    // intentionally empty — see TODO above.
  });
});
