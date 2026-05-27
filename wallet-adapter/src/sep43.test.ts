import { describe, expect, it } from "vitest";

import { WalletError, WalletErrorCode } from "./sep43.js";

describe("WalletError", () => {
  it("formats its message as [wallet:<code>] <detail>", () => {
    const err = new WalletError(WalletErrorCode.UserRejected, "user declined");
    expect(err.message).toBe("[wallet:-4] user declined");
  });

  it("exposes code and detail as readonly fields", () => {
    const err = new WalletError(
      WalletErrorCode.ExternalService,
      "rpc returned 500",
    );
    expect(err.code).toBe(WalletErrorCode.ExternalService);
    expect(err.code).toBe(-2);
    expect(err.detail).toBe("rpc returned 500");
  });

  it("is catchable as Error and as WalletError", () => {
    const err = new WalletError(WalletErrorCode.Internal, "boom");
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(WalletError);
    expect(err.name).toBe("WalletError");
  });

  it("maps each SEP-43 numeric code to the expected enum member", () => {
    expect(WalletErrorCode.Internal).toBe(-1);
    expect(WalletErrorCode.ExternalService).toBe(-2);
    expect(WalletErrorCode.InvalidRequest).toBe(-3);
    expect(WalletErrorCode.UserRejected).toBe(-4);
  });
});
