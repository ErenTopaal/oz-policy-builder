import { beforeEach, describe, expect, it, vi } from "vitest";

import { WalletError, WalletErrorCode } from "../sep43.js";

// mock the freighter-api module BEFORE importing the adapter so that the
// adapter's top-level imports bind to the mock. Vitest hoists `vi.mock`
// calls to the top of the file automatically.
vi.mock("@stellar/freighter-api", () => ({
  isConnected: vi.fn(),
  getAddress: vi.fn(),
  signTransaction: vi.fn(),
  signAuthEntry: vi.fn(),
}));

// these imports MUST come after `vi.mock` so the mock is bound.
import {
  getAddress,
  isConnected,
  signAuthEntry,
  signTransaction,
} from "@stellar/freighter-api";

import { FreighterWallet } from "./freighter.js";

const isConnectedMock = vi.mocked(isConnected);
const getAddressMock = vi.mocked(getAddress);
const signTransactionMock = vi.mocked(signTransaction);
const signAuthEntryMock = vi.mocked(signAuthEntry);

const NETWORK_PASSPHRASE = "Test SDF Network ; September 2015";
const G_ADDR = "GABCDEFGHIJKLMNOPQRSTUVWXYZ234567ABCDEFGHIJKLMNOPQRST";

beforeEach(() => {
  isConnectedMock.mockReset();
  getAddressMock.mockReset();
  signTransactionMock.mockReset();
  signAuthEntryMock.mockReset();
});

describe("FreighterWallet.isAvailable", () => {
  it("returns false when isConnected returns false", async () => {
    isConnectedMock.mockResolvedValue({ isConnected: false });
    const wallet = new FreighterWallet();
    await expect(wallet.isAvailable()).resolves.toBe(false);
  });

  it("returns true when isConnected returns true", async () => {
    isConnectedMock.mockResolvedValue({ isConnected: true });
    const wallet = new FreighterWallet();
    await expect(wallet.isAvailable()).resolves.toBe(true);
  });

  it("returns false when freighter-api returns an error (e.g. Node env)", async () => {
    isConnectedMock.mockResolvedValue({
      isConnected: false,
      error: { code: -1, message: "Node environment is not supported" },
    });
    const wallet = new FreighterWallet();
    await expect(wallet.isAvailable()).resolves.toBe(false);
  });

  it("returns false when freighter-api throws", async () => {
    isConnectedMock.mockRejectedValue(new Error("transport disconnected"));
    const wallet = new FreighterWallet();
    await expect(wallet.isAvailable()).resolves.toBe(false);
  });
});

describe("FreighterWallet.getAddress", () => {
  it("returns the StrKey on success", async () => {
    getAddressMock.mockResolvedValue({ address: G_ADDR });
    const wallet = new FreighterWallet();
    await expect(wallet.getAddress()).resolves.toBe(G_ADDR);
  });

  it("throws WalletError(UserRejected) when the user declines access", async () => {
    getAddressMock.mockResolvedValue({
      address: "",
      error: { code: -4, message: "User declined access" },
    });
    const wallet = new FreighterWallet();
    await expect(wallet.getAddress()).rejects.toMatchObject({
      code: WalletErrorCode.UserRejected,
      detail: "User declined access",
    });
  });

  it("throws WalletError(Internal) when freighter returns empty address with no error", async () => {
    getAddressMock.mockResolvedValue({ address: "" });
    const wallet = new FreighterWallet();
    await expect(wallet.getAddress()).rejects.toBeInstanceOf(WalletError);
    await expect(wallet.getAddress()).rejects.toMatchObject({
      code: WalletErrorCode.Internal,
    });
  });
});

describe("FreighterWallet.signTransaction", () => {
  const SIGNED_XDR = "AAAAA...signed";
  const UNSIGNED_XDR = "AAAAA...unsigned";

  it("returns signedTxXdr and signerAddress on success", async () => {
    signTransactionMock.mockResolvedValue({
      signedTxXdr: SIGNED_XDR,
      signerAddress: G_ADDR,
    });
    const wallet = new FreighterWallet();
    const result = await wallet.signTransaction(UNSIGNED_XDR, {
      networkPassphrase: NETWORK_PASSPHRASE,
    });
    expect(result).toEqual({
      signedTxXdr: SIGNED_XDR,
      signerAddress: G_ADDR,
    });
    expect(signTransactionMock).toHaveBeenCalledWith(UNSIGNED_XDR, {
      networkPassphrase: NETWORK_PASSPHRASE,
    });
  });

  it("forwards an explicit `address` option when provided", async () => {
    signTransactionMock.mockResolvedValue({
      signedTxXdr: SIGNED_XDR,
      signerAddress: G_ADDR,
    });
    const wallet = new FreighterWallet();
    await wallet.signTransaction(UNSIGNED_XDR, {
      networkPassphrase: NETWORK_PASSPHRASE,
      address: G_ADDR,
    });
    expect(signTransactionMock).toHaveBeenCalledWith(UNSIGNED_XDR, {
      networkPassphrase: NETWORK_PASSPHRASE,
      address: G_ADDR,
    });
  });

  it("throws WalletError(UserRejected) on user rejection (code -4)", async () => {
    signTransactionMock.mockResolvedValue({
      signedTxXdr: "",
      signerAddress: "",
      error: { code: -4, message: "The user rejected this request." },
    });
    const wallet = new FreighterWallet();
    await expect(
      wallet.signTransaction(UNSIGNED_XDR, {
        networkPassphrase: NETWORK_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.UserRejected,
      detail: "The user rejected this request.",
    });
  });

  it("throws WalletError(ExternalService) on unknown freighter error code", async () => {
    signTransactionMock.mockResolvedValue({
      signedTxXdr: "",
      signerAddress: "",
      // -99 is not a SEP-43 code — adapter must escalate to ExternalService.
      error: { code: -99, message: "weird wallet bug" },
    });
    const wallet = new FreighterWallet();
    await expect(
      wallet.signTransaction(UNSIGNED_XDR, {
        networkPassphrase: NETWORK_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.ExternalService,
      detail: "weird wallet bug",
    });
  });

  it("throws WalletError(ExternalService) when freighter-api itself throws", async () => {
    signTransactionMock.mockRejectedValue(new Error("transport gone"));
    const wallet = new FreighterWallet();
    await expect(
      wallet.signTransaction(UNSIGNED_XDR, {
        networkPassphrase: NETWORK_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.ExternalService,
      detail: "transport gone",
    });
  });

  it("appends `ext` details to the WalletError detail when present", async () => {
    signTransactionMock.mockResolvedValue({
      signedTxXdr: "",
      signerAddress: "",
      error: {
        code: -3,
        message: "Request is invalid.",
        ext: ["Invalid transaction XDR"],
      },
    });
    const wallet = new FreighterWallet();
    await expect(
      wallet.signTransaction(UNSIGNED_XDR, {
        networkPassphrase: NETWORK_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.InvalidRequest,
      detail: "Request is invalid. (Invalid transaction XDR)",
    });
  });
});

describe("FreighterWallet.signAuthEntry", () => {
  const SIGNED_ENTRY = "AAAA...signed-entry";
  const UNSIGNED_ENTRY = "AAAA...unsigned-entry";

  it("returns signedAuthEntry and signerAddress on success", async () => {
    signAuthEntryMock.mockResolvedValue({
      signedAuthEntry: SIGNED_ENTRY,
      signerAddress: G_ADDR,
    });
    const wallet = new FreighterWallet();
    const result = await wallet.signAuthEntry(UNSIGNED_ENTRY, {
      networkPassphrase: NETWORK_PASSPHRASE,
    });
    expect(result).toEqual({
      signedAuthEntry: SIGNED_ENTRY,
      signerAddress: G_ADDR,
    });
  });

  it("throws WalletError(UserRejected) on rejection", async () => {
    signAuthEntryMock.mockResolvedValue({
      signedAuthEntry: null,
      signerAddress: "",
      error: { code: -4, message: "User rejected." },
    });
    const wallet = new FreighterWallet();
    await expect(
      wallet.signAuthEntry(UNSIGNED_ENTRY, {
        networkPassphrase: NETWORK_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.UserRejected,
    });
  });

  it("throws WalletError(Internal) when signedAuthEntry is null with no error", async () => {
    signAuthEntryMock.mockResolvedValue({
      signedAuthEntry: null,
      signerAddress: G_ADDR,
    });
    const wallet = new FreighterWallet();
    await expect(
      wallet.signAuthEntry(UNSIGNED_ENTRY, {
        networkPassphrase: NETWORK_PASSPHRASE,
      }),
    ).rejects.toMatchObject({
      code: WalletErrorCode.Internal,
    });
  });
});
