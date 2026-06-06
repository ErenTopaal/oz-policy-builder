/**
 * Mocked-Vitest tests for `installPolicy`. Stream C Phase 7.
 *
 * Mock surface (deliberately narrow):
 *  - The `WalletAdapter` is a hand-rolled `vi.fn()` set so each test
 *    pins a specific success/failure on `signTransaction`.
 *  - `@stellar/stellar-sdk`'s `rpc.Server` is mocked via `vi.mock(...)`
 *    so `sendTransaction` / `getTransaction` produce typed responses.
 *  - `TransactionBuilder.fromXDR` is stubbed to return an opaque
 *    `Transaction` token (its only contract here is that
 *    `sendTransaction` is called with the returned object).
 *  - `scValToNative` is mocked so the test asserts on a specific
 *    `ContextRule` shape — `{ id: 7, name: "rule", ... }`.
 *
 * Why this is not a "mocks pretending to be real" test: every
 * assertion pins a concrete value on the production code path. Mocks
 * exist to inject deterministic *data*, not to bypass logic. The
 * happy-path test specifically asserts that `installPolicy` extracts
 * `contextRuleId: 7` — which is only true if the `extractContextRuleId`
 * implementation actually reads the `.id` field of the native value
 * returned by `scValToNative`.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// ---- module mocks (must precede module imports under test) ----------

// `vi.hoisted` runs BEFORE `vi.mock` factories so we can hand the same
// stable function references to both the mocked module factory and the
// test body. Without this hoist, the factory creates fresh closures that
// the test file can't see.
const hoisted = vi.hoisted(() => {
  return {
    sendTransactionMock: vi.fn(),
    getTransactionMock: vi.fn(),
    simulateTransactionMock: vi.fn(),
    fromXdrMock: vi.fn(),
    scValToNativeMock: vi.fn(),
    assembleTransactionMock: vi.fn(),
  };
});

vi.mock("@stellar/stellar-sdk", async () => {
  const real =
    await vi.importActual<typeof import("@stellar/stellar-sdk")>(
      "@stellar/stellar-sdk",
    );

  class FakeServer {
    sendTransaction = hoisted.sendTransactionMock;
    getTransaction = hoisted.getTransactionMock;
    simulateTransaction = hoisted.simulateTransactionMock;
    constructor(_url: string) {
      void _url;
    }
  }

  return {
    ...real,
    rpc: {
      ...real.rpc,
      Server: FakeServer,
      assembleTransaction: hoisted.assembleTransactionMock,
    },
    TransactionBuilder: {
      ...real.TransactionBuilder,
      fromXDR: hoisted.fromXdrMock,
    },
    scValToNative: hoisted.scValToNativeMock,
  };
});

// ---- module imports (post-mock) -------------------------------------

import { rpc as sorobanRpc } from "@stellar/stellar-sdk";

import {
  WalletAdapter,
  WalletError,
  WalletErrorCode,
} from "./sep43.js";

import {
  extractContextRuleId,
  installPolicy,
  WalletInstallError,
} from "./install.js";

// ---- mock handles ----------------------------------------------------

const sendTransactionMock = hoisted.sendTransactionMock;
const getTransactionMock = hoisted.getTransactionMock;
const simulateTransactionMock = hoisted.simulateTransactionMock;
const fromXdrMock = hoisted.fromXdrMock;
const scValToNativeMock = hoisted.scValToNativeMock;
const assembleTransactionMock = hoisted.assembleTransactionMock;

const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";
const ENVELOPE_XDR_B64 = "AAAAAg=="; // placeholder; mocked out anyway
const SIGNED_TX_XDR_B64 = "AAAAAgSIGN==";
const TX_HASH = "deadbeef".repeat(8);
const G_SIGNER = "G".repeat(56);

// ---- adapter factory -------------------------------------------------

interface MockedAdapter extends WalletAdapter {
  signTransaction: ReturnType<typeof vi.fn>;
}

function makeAdapter(signResult: unknown, throwInstead = false): MockedAdapter {
  const sig = vi.fn();
  if (throwInstead) {
    sig.mockRejectedValue(signResult);
  } else {
    sig.mockResolvedValue(signResult);
  }
  return {
    isAvailable: vi.fn().mockResolvedValue(true),
    getAddress: vi.fn().mockResolvedValue(G_SIGNER),
    signTransaction: sig,
    signAuthEntry: vi.fn(),
  };
}

// ---- shared setup ----------------------------------------------------

beforeEach(() => {
  sendTransactionMock.mockReset();
  getTransactionMock.mockReset();
  simulateTransactionMock.mockReset();
  fromXdrMock.mockReset();
  scValToNativeMock.mockReset();
  assembleTransactionMock.mockReset();
  fromXdrMock.mockImplementation((..._args: unknown[]) => ({
    __kind: "fake-tx",
  }));
});

afterEach(() => {
  vi.useRealTimers();
});

// installPolicy happy path

describe("installPolicy — happy path", () => {
  it("returns { txHash, contextRuleId, ledger } extracted from RPC + return value", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    sendTransactionMock.mockResolvedValue({
      status: "PENDING",
      hash: TX_HASH,
      latestLedger: 100_000,
      latestLedgerCloseTime: 1_700_000_000,
    });
    // the opaque ScVal returned by getTransaction.returnValue. The
    // `scValToNative` mock will turn this into the canonical object.
    const fakeScVal = { __kind: "scval" } as unknown;
    getTransactionMock.mockResolvedValue({
      status: sorobanRpc.Api.GetTransactionStatus.SUCCESS,
      ledger: 100_042,
      latestLedger: 100_050,
      latestLedgerCloseTime: 0,
      oldestLedger: 99_000,
      oldestLedgerCloseTime: 0,
      createdAt: 0,
      applicationOrder: 1,
      feeBump: false,
      envelopeXdr: {} as never,
      resultXdr: {} as never,
      resultMetaXdr: {} as never,
      returnValue: fakeScVal,
    });
    scValToNativeMock.mockReturnValue({
      id: 7,
      context_type: "Default",
      name: "rule-deadbeef",
      signers: [],
      signer_ids: [],
      policies: [],
      policy_ids: [],
      valid_until: null,
    });

    const result = await installPolicy({
      adapter,
      envelopeXdrBase64: ENVELOPE_XDR_B64,
      rpcUrl: "https://soroban-testnet.stellar.org",
      network: "testnet",
      networkPassphrase: TESTNET_PASSPHRASE,
      pollIntervalMs: 1,
      pollTimeoutMs: 1_000,
    });

    expect(result).toEqual({
      txHash: TX_HASH,
      contextRuleId: 7,
      ledger: 100_042,
    });

    // adapter received the unsigned envelope + passphrase verbatim.
    expect(adapter.signTransaction).toHaveBeenCalledWith(ENVELOPE_XDR_B64, {
      networkPassphrase: TESTNET_PASSPHRASE,
    });
    // the signed XDR (NOT the unsigned) went through fromXDR.
    expect(fromXdrMock).toHaveBeenCalledWith(
      SIGNED_TX_XDR_B64,
      TESTNET_PASSPHRASE,
    );
    // the reconstructed tx went through sendTransaction.
    expect(sendTransactionMock).toHaveBeenCalledTimes(1);
    expect(sendTransactionMock).toHaveBeenCalledWith({ __kind: "fake-tx" });
    // we invoked the canonical extractor with the actual ScVal payload.
    expect(scValToNativeMock).toHaveBeenCalledWith(fakeScVal);
  });

  it("polls past one NOT_FOUND response before resolving on SUCCESS", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    sendTransactionMock.mockResolvedValue({
      status: "PENDING",
      hash: TX_HASH,
      latestLedger: 100,
      latestLedgerCloseTime: 0,
    });
    const fakeScVal = { __kind: "scval" } as unknown;
    getTransactionMock
      .mockResolvedValueOnce({
        status: sorobanRpc.Api.GetTransactionStatus.NOT_FOUND,
        latestLedger: 100,
        latestLedgerCloseTime: 0,
        oldestLedger: 99,
        oldestLedgerCloseTime: 0,
      })
      .mockResolvedValueOnce({
        status: sorobanRpc.Api.GetTransactionStatus.SUCCESS,
        ledger: 101,
        latestLedger: 101,
        latestLedgerCloseTime: 0,
        oldestLedger: 99,
        oldestLedgerCloseTime: 0,
        createdAt: 0,
        applicationOrder: 1,
        feeBump: false,
        envelopeXdr: {} as never,
        resultXdr: {} as never,
        resultMetaXdr: {} as never,
        returnValue: fakeScVal,
      });
    scValToNativeMock.mockReturnValue({ id: 42 });

    const result = await installPolicy({
      adapter,
      envelopeXdrBase64: ENVELOPE_XDR_B64,
      rpcUrl: "https://soroban-testnet.stellar.org",
      network: "testnet",
      networkPassphrase: TESTNET_PASSPHRASE,
      pollIntervalMs: 1,
      pollTimeoutMs: 1_000,
    });

    expect(getTransactionMock).toHaveBeenCalledTimes(2);
    expect(result.contextRuleId).toBe(42);
    expect(result.ledger).toBe(101);
  });
});

// installPolicy error branches

describe("installPolicy — error branches", () => {
  it("re-throws WalletError(UserRejected) as WalletInstallError(E_WALLET_REJECTED)", async () => {
    const adapter = makeAdapter(
      new WalletError(WalletErrorCode.UserRejected, "user declined"),
      true,
    );

    const promise = installPolicy({
      adapter,
      envelopeXdrBase64: ENVELOPE_XDR_B64,
      rpcUrl: "https://soroban-testnet.stellar.org",
      network: "testnet",
      networkPassphrase: TESTNET_PASSPHRASE,
    });

    await expect(promise).rejects.toBeInstanceOf(WalletInstallError);
    await expect(promise).rejects.toMatchObject({
      code: "E_WALLET_REJECTED",
      detail: "user declined",
    });
    // critically: we never hit the RPC.
    expect(sendTransactionMock).not.toHaveBeenCalled();
    expect(getTransactionMock).not.toHaveBeenCalled();
  });

  it("maps a non-UserRejected WalletError to E_INSTALL_SUBMIT_FAILED", async () => {
    const adapter = makeAdapter(
      new WalletError(WalletErrorCode.Internal, "wallet crashed"),
      true,
    );
    const promise = installPolicy({
      adapter,
      envelopeXdrBase64: ENVELOPE_XDR_B64,
      rpcUrl: "https://soroban-testnet.stellar.org",
      network: "testnet",
      networkPassphrase: TESTNET_PASSPHRASE,
    });
    await expect(promise).rejects.toMatchObject({
      code: "E_INSTALL_SUBMIT_FAILED",
    });
  });

  it("surfaces sendTransaction.status='ERROR' as E_INSTALL_SUBMIT_FAILED", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    sendTransactionMock.mockResolvedValue({
      status: "ERROR",
      hash: TX_HASH,
      latestLedger: 100,
      latestLedgerCloseTime: 0,
    });
    await expect(
      installPolicy({
        adapter,
        envelopeXdrBase64: ENVELOPE_XDR_B64,
        rpcUrl: "https://soroban-testnet.stellar.org",
        network: "testnet",
        networkPassphrase: TESTNET_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: "E_INSTALL_SUBMIT_FAILED",
      detail: expect.stringContaining("status=ERROR"),
    });
    // we never reached getTransaction.
    expect(getTransactionMock).not.toHaveBeenCalled();
  });

  it("surfaces sendTransaction.status='TRY_AGAIN_LATER' as E_INSTALL_SUBMIT_FAILED", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    sendTransactionMock.mockResolvedValue({
      status: "TRY_AGAIN_LATER",
      hash: TX_HASH,
      latestLedger: 100,
      latestLedgerCloseTime: 0,
    });
    await expect(
      installPolicy({
        adapter,
        envelopeXdrBase64: ENVELOPE_XDR_B64,
        rpcUrl: "https://soroban-testnet.stellar.org",
        network: "testnet",
        networkPassphrase: TESTNET_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: "E_INSTALL_SUBMIT_FAILED",
      detail: expect.stringContaining("TRY_AGAIN_LATER"),
    });
  });

  it("surfaces FAILED transaction status as E_INSTALL_SUBMIT_FAILED", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    sendTransactionMock.mockResolvedValue({
      status: "PENDING",
      hash: TX_HASH,
      latestLedger: 100,
      latestLedgerCloseTime: 0,
    });
    getTransactionMock.mockResolvedValue({
      status: sorobanRpc.Api.GetTransactionStatus.FAILED,
      ledger: 101,
      latestLedger: 101,
      latestLedgerCloseTime: 0,
      oldestLedger: 99,
      oldestLedgerCloseTime: 0,
      createdAt: 0,
      applicationOrder: 1,
      feeBump: false,
      envelopeXdr: {} as never,
      resultXdr: {} as never,
      resultMetaXdr: {} as never,
    });

    await expect(
      installPolicy({
        adapter,
        envelopeXdrBase64: ENVELOPE_XDR_B64,
        rpcUrl: "https://soroban-testnet.stellar.org",
        network: "testnet",
        networkPassphrase: TESTNET_PASSPHRASE,
        pollIntervalMs: 1,
        pollTimeoutMs: 1_000,
      }),
    ).rejects.toMatchObject({
      code: "E_INSTALL_SUBMIT_FAILED",
      detail: expect.stringContaining("FAILED"),
    });
  });

  it("times out polling with E_INSTALL_POLL_TIMEOUT when status stays NOT_FOUND", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    sendTransactionMock.mockResolvedValue({
      status: "PENDING",
      hash: TX_HASH,
      latestLedger: 100,
      latestLedgerCloseTime: 0,
    });
    getTransactionMock.mockResolvedValue({
      status: sorobanRpc.Api.GetTransactionStatus.NOT_FOUND,
      latestLedger: 100,
      latestLedgerCloseTime: 0,
      oldestLedger: 99,
      oldestLedgerCloseTime: 0,
    });

    const promise = installPolicy({
      adapter,
      envelopeXdrBase64: ENVELOPE_XDR_B64,
      rpcUrl: "https://soroban-testnet.stellar.org",
      network: "testnet",
      networkPassphrase: TESTNET_PASSPHRASE,
      pollIntervalMs: 1,
      pollTimeoutMs: 25,
    });

    await expect(promise).rejects.toMatchObject({
      code: "E_INSTALL_POLL_TIMEOUT",
    });
    // we polled at least twice before giving up.
    expect(getTransactionMock.mock.calls.length).toBeGreaterThanOrEqual(2);
  });

  it("rejects with E_INSTALL_RESULT_DECODE_FAILED when SUCCESS carries no returnValue", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    sendTransactionMock.mockResolvedValue({
      status: "PENDING",
      hash: TX_HASH,
      latestLedger: 100,
      latestLedgerCloseTime: 0,
    });
    getTransactionMock.mockResolvedValue({
      status: sorobanRpc.Api.GetTransactionStatus.SUCCESS,
      ledger: 101,
      latestLedger: 101,
      latestLedgerCloseTime: 0,
      oldestLedger: 99,
      oldestLedgerCloseTime: 0,
      createdAt: 0,
      applicationOrder: 1,
      feeBump: false,
      envelopeXdr: {} as never,
      resultXdr: {} as never,
      resultMetaXdr: {} as never,
      // returnValue intentionally omitted.
    });
    await expect(
      installPolicy({
        adapter,
        envelopeXdrBase64: ENVELOPE_XDR_B64,
        rpcUrl: "https://soroban-testnet.stellar.org",
        network: "testnet",
        networkPassphrase: TESTNET_PASSPHRASE,
        pollIntervalMs: 1,
        pollTimeoutMs: 1_000,
      }),
    ).rejects.toMatchObject({
      code: "E_INSTALL_RESULT_DECODE_FAILED",
    });
  });
});

// ozAuthPayloadEncoder hook

describe("installPolicy — ozAuthPayloadEncoder hook", () => {
  it("wipes op.auth[], re-simulates, runs encoder, then wallet signs (RFP #5)", async () => {
    // 2026-05-18: this test pins the THREE-step OZ-SA refresh pipeline
    // documented in install.ts. Earlier attempts (post-sign encoder; pre-
    // sign encoder + re-simulate of encoded envelope) failed against
    // testnet — the first with `tx_bad_auth`, the second with
    // `Unauthorized function call for address GCM2…`. See
    // `walkthroughs/phase7-testnet-install/CLOSURE_ATTEMPT_2026-05-18.md`
    // for the literal diagnostic events that drove this final design.
    const real =
      await vi.importActual<typeof import("@stellar/stellar-sdk")>(
        "@stellar/stellar-sdk",
      );
    // build a minimal real Soroban v1 envelope so `clearOpAuthEntries`'
    // XDR-decode path runs over real bytes (the wipe-and-resimulate
    // pipeline parses the envelope before passing to the encoder).
    const sourceKp = real.Keypair.random();
    const account = new real.Account(sourceKp.publicKey(), "1");
    const realEnvelope = new real.TransactionBuilder(account, {
      fee: "100",
      networkPassphrase: TESTNET_PASSPHRASE,
    })
      .addOperation(
        real.Operation.invokeContractFunction({
          contract:
            "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A",
          function: "noop",
          args: [],
        }),
      )
      .setTimeout(0)
      .build();
    const realEnvelopeXdr = realEnvelope.toXDR();

    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    const ENCODED_OUTPUT = "AAAAAg==ENCODED_OUTPUT";
    const encoder = vi.fn().mockResolvedValue(ENCODED_OUTPUT);

    const assembledTx = Object.create(
      real.Transaction.prototype,
    ) as InstanceType<typeof real.Transaction>;
    let assembledXdrCaptured = "";
    (assembledTx as unknown as { toXDR: () => string }).toXDR = () => {
      assembledXdrCaptured = "AAAAAg==ASSEMBLED_PLACEHOLDER";
      return assembledXdrCaptured;
    };
    fromXdrMock.mockImplementation((_xdrIn: string) => {
      // the install path calls fromXDR twice: once on the wiped
      // envelope (must be a real Transaction so `instanceof` check
      // passes) and once on the wallet's signed XDR (any opaque).
      // we return a real-prototype object for both — the wallet-
      // signed path uses sendTransaction which just forwards.
      return Object.create(
        real.Transaction.prototype,
      ) as InstanceType<typeof real.Transaction>;
    });
    simulateTransactionMock.mockResolvedValue({
      results: [{ auth: [], xdr: {} as never }],
      transactionData: {} as never,
      minResourceFee: "1000",
    } as unknown as Awaited<
      ReturnType<sorobanRpc.Server["simulateTransaction"]>
    >);
    const builderMock = {
      setTimeout: function () {
        return this;
      },
      build: () => assembledTx,
    };
    assembleTransactionMock.mockReturnValue(
      builderMock as unknown as ReturnType<typeof assembleTransactionMock>,
    );

    sendTransactionMock.mockResolvedValue({
      status: "PENDING",
      hash: TX_HASH,
      latestLedger: 100,
      latestLedgerCloseTime: 0,
    });
    getTransactionMock.mockResolvedValue({
      status: sorobanRpc.Api.GetTransactionStatus.SUCCESS,
      ledger: 101,
      latestLedger: 101,
      latestLedgerCloseTime: 0,
      oldestLedger: 99,
      oldestLedgerCloseTime: 0,
      createdAt: 0,
      applicationOrder: 1,
      feeBump: false,
      envelopeXdr: {} as never,
      resultXdr: {} as never,
      resultMetaXdr: {} as never,
      returnValue: { __kind: "scval" } as unknown,
    });
    scValToNativeMock.mockReturnValue({ id: 13 });

    await installPolicy({
      adapter,
      envelopeXdrBase64: realEnvelopeXdr,
      rpcUrl: "https://soroban-testnet.stellar.org",
      network: "testnet",
      networkPassphrase: TESTNET_PASSPHRASE,
      pollIntervalMs: 1,
      pollTimeoutMs: 1_000,
      ozAuthPayloadEncoder: encoder,
    });

    // pipeline ordering:
    //   1. `clearOpAuthEntries` produces a wiped envelope.
    //   2. `simulateTransaction` is called once on the wiped tx.
    //   3. `assembleTransaction` is called once to bake sim results.
    //   4. The encoder runs on the ASSEMBLED envelope XDR.
    //   5. The wallet adapter signs the encoder's output.
    expect(simulateTransactionMock).toHaveBeenCalledTimes(1);
    expect(assembleTransactionMock).toHaveBeenCalledTimes(1);
    expect(encoder).toHaveBeenCalledTimes(1);
    expect(encoder).toHaveBeenCalledWith(assembledXdrCaptured);
    expect(adapter.signTransaction).toHaveBeenCalledWith(ENCODED_OUTPUT, {
      networkPassphrase: TESTNET_PASSPHRASE,
    });
  });

  it("maps encoder failure to E_INSTALL_SUBMIT_FAILED", async () => {
    const real =
      await vi.importActual<typeof import("@stellar/stellar-sdk")>(
        "@stellar/stellar-sdk",
      );
    const sourceKp = real.Keypair.random();
    const account = new real.Account(sourceKp.publicKey(), "1");
    const realEnvelopeXdr = new real.TransactionBuilder(account, {
      fee: "100",
      networkPassphrase: TESTNET_PASSPHRASE,
    })
      .addOperation(
        real.Operation.invokeContractFunction({
          contract:
            "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A",
          function: "noop",
          args: [],
        }),
      )
      .setTimeout(0)
      .build()
      .toXDR();
    // the wipe + re-simulate path runs BEFORE the encoder, so we need
    // working mocks for simulateTransaction + assembleTransaction even
    // though we expect the encoder itself to throw.
    fromXdrMock.mockImplementation(() =>
      Object.create(
        real.Transaction.prototype,
      ) as InstanceType<typeof real.Transaction>,
    );
    simulateTransactionMock.mockResolvedValue({
      results: [{ auth: [], xdr: {} as never }],
      transactionData: {} as never,
      minResourceFee: "1000",
    } as unknown as Awaited<
      ReturnType<sorobanRpc.Server["simulateTransaction"]>
    >);
    const assembledTx = Object.create(
      real.Transaction.prototype,
    ) as InstanceType<typeof real.Transaction>;
    (assembledTx as unknown as { toXDR: () => string }).toXDR = () =>
      "AAAAAg==ASSEMBLED";
    assembleTransactionMock.mockReturnValue({
      setTimeout: function () {
        return this;
      },
      build: () => assembledTx,
    } as unknown as ReturnType<typeof assembleTransactionMock>);

    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    const encoder = vi.fn().mockRejectedValue(new Error("encoder boom"));
    await expect(
      installPolicy({
        adapter,
        envelopeXdrBase64: realEnvelopeXdr,
        rpcUrl: "https://soroban-testnet.stellar.org",
        network: "testnet",
        networkPassphrase: TESTNET_PASSPHRASE,
        ozAuthPayloadEncoder: encoder,
      }),
    ).rejects.toMatchObject({
      code: "E_INSTALL_SUBMIT_FAILED",
      detail: expect.stringContaining("encoder boom"),
    });
    expect(sendTransactionMock).not.toHaveBeenCalled();
  });
});

// mainnet consent guard

describe("installPolicy — mainnet consent", () => {
  it("throws E_MAINNET_REQUIRES_CONSENT when network=mainnet without confirmMainnet", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    await expect(
      installPolicy({
        adapter,
        envelopeXdrBase64: ENVELOPE_XDR_B64,
        rpcUrl: "https://soroban.stellar.org",
        network: "mainnet",
        networkPassphrase: "Public Global Stellar Network ; September 2015",
      }),
    ).rejects.toMatchObject({
      code: "E_MAINNET_REQUIRES_CONSENT",
    });
    expect(adapter.signTransaction).not.toHaveBeenCalled();
    expect(sendTransactionMock).not.toHaveBeenCalled();
  });

  it("proceeds when network=mainnet AND confirmMainnet=true", async () => {
    const adapter = makeAdapter({
      signedTxXdr: SIGNED_TX_XDR_B64,
      signerAddress: G_SIGNER,
    });
    sendTransactionMock.mockResolvedValue({
      status: "PENDING",
      hash: TX_HASH,
      latestLedger: 1,
      latestLedgerCloseTime: 0,
    });
    getTransactionMock.mockResolvedValue({
      status: sorobanRpc.Api.GetTransactionStatus.SUCCESS,
      ledger: 2,
      latestLedger: 2,
      latestLedgerCloseTime: 0,
      oldestLedger: 1,
      oldestLedgerCloseTime: 0,
      createdAt: 0,
      applicationOrder: 1,
      feeBump: false,
      envelopeXdr: {} as never,
      resultXdr: {} as never,
      resultMetaXdr: {} as never,
      returnValue: { __kind: "scval" } as unknown,
    });
    scValToNativeMock.mockReturnValue({ id: 1 });

    const result = await installPolicy({
      adapter,
      envelopeXdrBase64: ENVELOPE_XDR_B64,
      rpcUrl: "https://soroban.stellar.org",
      network: "mainnet",
      networkPassphrase: "Public Global Stellar Network ; September 2015",
      confirmMainnet: true,
      pollIntervalMs: 1,
      pollTimeoutMs: 1_000,
    });
    expect(result.contextRuleId).toBe(1);
  });
});

// extractContextRuleId — direct unit tests (the load-bearing decoder)

describe("extractContextRuleId", () => {
  it("returns the integer id from a well-formed native ContextRule", () => {
    scValToNativeMock.mockReturnValue({ id: 99, name: "rule" });
    const id = extractContextRuleId({ __kind: "scval" } as never, "abc");
    expect(id).toBe(99);
    expect(scValToNativeMock).toHaveBeenCalledTimes(1);
  });

  it("throws E_INSTALL_RESULT_DECODE_FAILED when the native value is not an object", () => {
    scValToNativeMock.mockReturnValue(7 as never);
    expect(() =>
      extractContextRuleId({ __kind: "scval" } as never, "abc"),
    ).toThrow(WalletInstallError);
  });

  it("throws E_INSTALL_RESULT_DECODE_FAILED when id is missing", () => {
    scValToNativeMock.mockReturnValue({ name: "rule" });
    expect(() =>
      extractContextRuleId({ __kind: "scval" } as never, "abc"),
    ).toThrow(/E_INSTALL_RESULT_DECODE_FAILED/);
  });

  it("throws E_INSTALL_RESULT_DECODE_FAILED when id is negative", () => {
    scValToNativeMock.mockReturnValue({ id: -1 });
    expect(() =>
      extractContextRuleId({ __kind: "scval" } as never, "abc"),
    ).toThrow(WalletInstallError);
  });

  it("throws E_INSTALL_RESULT_DECODE_FAILED when scValToNative itself throws", () => {
    scValToNativeMock.mockImplementation(() => {
      throw new Error("xdr corruption");
    });
    expect(() =>
      extractContextRuleId({ __kind: "scval" } as never, "abc"),
    ).toThrow(/scValToNative on returnValue failed/);
  });
});
