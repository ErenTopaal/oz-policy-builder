/**
 * Unit tests for the OZ SmartAccount AuthPayload encoder + auth-digest
 * computer + auth-entry builder. **No mocks for the SDK.** All XDR
 * encoding goes through the real `@stellar/stellar-sdk` `xdr.*` types so
 * round-trip identity is a meaningful invariant.
 *
 * What these tests pin (each test is a separate, named invariant):
 *
 *  1. `encodeAuthPayload` produces an `ScVal::Map` whose top-level keys
 *     are the two symbols `["context_rule_ids", "signers"]` in that
 *     order (the Soroban `#[contracttype]` struct ordering invariant).
 *  2. `computeAuthDigest` is 32 bytes, deterministic, sensitive to both
 *     inputs.
 *  3. `buildOzAuthEntry` produces a `SorobanAuthorizationEntry` whose
 *     `credentials` is `SorobanCredentials::Address` and whose
 *     `signature` ScVal is exactly the encoded AuthPayload.
 *  4. Round-trip: encode → `toXDR` → `fromXDR` → equality.
 *
 * Stream-B-frozen byte-anchor: a SHA-256 digest of one known-input encoded
 * AuthPayload is pinned here so a refactor that changes the wire format
 * fails loudly.
 */

import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";

import {
  Address,
  Keypair,
  Networks,
  xdr,
} from "@stellar/stellar-sdk";

import {
  buildOzAuthEntry,
  computeAuthDigest,
  computeSignaturePayload,
  encodeAuthPayload,
  encodeContextRuleIdsScVal,
  encodeSignerScVal,
  type OzSigner,
} from "./oz_smart_account_auth.js";

// ---------------------------------------------------------------------------
// Fixed test inputs. These are pinned so the SHA-256 anchor in this file
// stays stable across runs and the digest can be reproduced from the docs.
// ---------------------------------------------------------------------------

/** Phase 7 SA owner G-address (matches the frozen testnet fixture). */
const G_OWNER = "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ";

/** Phase 7 SA C-address (frozen testnet fixture). */
const C_SA = "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A";

/** A deterministic 64-byte "ed25519 signature" placeholder. */
const FAKE_SIG = new Uint8Array(64).map((_, i) => (i * 7) & 0xff);

function sha256Hex(b: Uint8Array | Buffer): string {
  return createHash("sha256").update(b).digest("hex");
}

// =========================================================================
// 1. encodeAuthPayload — ScVal::Map shape + key ordering
// =========================================================================

describe("encodeAuthPayload — ScVal::Map shape", () => {
  it("produces an ScVal::Map with sorted symbol keys [context_rule_ids, signers]", () => {
    const signers = new Map<OzSigner, Uint8Array>();
    signers.set({ kind: "delegated", address: G_OWNER }, FAKE_SIG);

    const sc = encodeAuthPayload({ signers, contextRuleIds: [0] });

    // Top-level discriminant must be scvMap.
    expect(sc.switch().name).toBe("scvMap");

    const entries = sc.map();
    expect(entries).not.toBeNull();
    const e = entries as xdr.ScMapEntry[];
    expect(e.length).toBe(2);

    // Keys must be symbols, in this exact order.
    expect(e[0]!.key().switch().name).toBe("scvSymbol");
    expect(e[0]!.key().sym().toString()).toBe("context_rule_ids");
    expect(e[1]!.key().switch().name).toBe("scvSymbol");
    expect(e[1]!.key().sym().toString()).toBe("signers");

    // The signers value must itself be an scvMap.
    expect(e[1]!.val().switch().name).toBe("scvMap");
    const signersInner = e[1]!.val().map() as xdr.ScMapEntry[];
    expect(signersInner.length).toBe(1);
    // The inner key is a Vec([Symbol("Delegated"), Address(G_OWNER)]).
    const innerKey = signersInner[0]!.key();
    expect(innerKey.switch().name).toBe("scvVec");
    const innerKeyVec = innerKey.vec() as xdr.ScVal[];
    expect(innerKeyVec[0]!.sym().toString()).toBe("Delegated");
    // The inner value is a Bytes equal to FAKE_SIG.
    expect(signersInner[0]!.val().switch().name).toBe("scvBytes");
    const bytes = signersInner[0]!.val().bytes();
    expect(Buffer.compare(bytes, Buffer.from(FAKE_SIG))).toBe(0);

    // The context_rule_ids value is an scvVec of scvU32.
    expect(e[0]!.val().switch().name).toBe("scvVec");
    const idVec = e[0]!.val().vec() as xdr.ScVal[];
    expect(idVec.length).toBe(1);
    expect(idVec[0]!.switch().name).toBe("scvU32");
    expect(idVec[0]!.u32()).toBe(0);
  });

  it("rejects context rule ids outside the u32 range", () => {
    expect(() =>
      encodeContextRuleIdsScVal([-1]),
    ).toThrow(/not a u32/);
    expect(() =>
      encodeContextRuleIdsScVal([0x1_0000_0000]),
    ).toThrow(/not a u32/);
  });
});

// =========================================================================
// 2. encodeSignerScVal — enum encoding
// =========================================================================

describe("encodeSignerScVal — #[contracttype] enum layout", () => {
  it("encodes Signer::Delegated(addr) as Vec([Symbol('Delegated'), Address])", () => {
    const sc = encodeSignerScVal({ kind: "delegated", address: G_OWNER });
    expect(sc.switch().name).toBe("scvVec");
    const v = sc.vec() as xdr.ScVal[];
    expect(v.length).toBe(2);
    expect(v[0]!.sym().toString()).toBe("Delegated");
    expect(v[1]!.switch().name).toBe("scvAddress");
    expect(Address.fromScVal(v[1]!).toString()).toBe(G_OWNER);
  });

  it("encodes Signer::External(verifier, bytes) as Vec([Symbol('External'), Address, Bytes])", () => {
    const sc = encodeSignerScVal({
      kind: "external_ed25519",
      verifier: C_SA,
      publicKeyHex: "00112233445566778899aabbccddeeff",
    });
    const v = sc.vec() as xdr.ScVal[];
    expect(v.length).toBe(3);
    expect(v[0]!.sym().toString()).toBe("External");
    expect(Address.fromScVal(v[1]!).toString()).toBe(C_SA);
    expect(v[2]!.switch().name).toBe("scvBytes");
    expect(v[2]!.bytes().toString("hex")).toBe("00112233445566778899aabbccddeeff");
  });
});

// =========================================================================
// 3. computeAuthDigest — sha256 properties
// =========================================================================

describe("computeAuthDigest — sha256(signature_payload || xdr(ids))", () => {
  it("returns a 32-byte deterministic digest for fixed inputs", () => {
    const sp = new Uint8Array(32).fill(0xab);
    const d1 = computeAuthDigest(sp, [0]);
    const d2 = computeAuthDigest(sp, [0]);
    expect(d1.length).toBe(32);
    expect(Buffer.compare(d1, d2)).toBe(0);
  });

  it("changes when context_rule_ids changes", () => {
    const sp = new Uint8Array(32).fill(0xcd);
    const d1 = computeAuthDigest(sp, [0]);
    const d2 = computeAuthDigest(sp, [1]);
    expect(Buffer.compare(d1, d2)).not.toBe(0);
  });

  it("changes when signature_payload changes", () => {
    const sp1 = new Uint8Array(32).fill(0x01);
    const sp2 = new Uint8Array(32).fill(0x02);
    expect(
      Buffer.compare(
        computeAuthDigest(sp1, [0]),
        computeAuthDigest(sp2, [0]),
      ),
    ).not.toBe(0);
  });

  it("rejects a signature_payload that isn't 32 bytes", () => {
    expect(() => computeAuthDigest(new Uint8Array(16), [0])).toThrow(
      /must be 32 bytes/,
    );
  });

  it("matches the spec formula sha256(signature_payload || xdr(ids))", () => {
    const sp = new Uint8Array(32).fill(0x42);
    const ids = [0, 1, 2];
    // Re-derive independently from the source formula to prove this
    // function is not just returning an opaque value.
    const expected = createHash("sha256")
      .update(Buffer.from(sp))
      .update(encodeContextRuleIdsScVal(ids).toXDR())
      .digest();
    const got = computeAuthDigest(sp, ids);
    expect(Buffer.compare(got, expected)).toBe(0);
  });
});

// =========================================================================
// 4. SHA-256 byte anchor — pins the wire format for the Phase 7 fixture
// =========================================================================

describe("AuthPayload byte anchor — Phase 7 fixture", () => {
  it("encodeAuthPayload with [G_OWNER → FAKE_SIG] + ids=[0] has a stable SHA-256", () => {
    const signers = new Map<OzSigner, Uint8Array>();
    signers.set({ kind: "delegated", address: G_OWNER }, FAKE_SIG);
    const sc = encodeAuthPayload({ signers, contextRuleIds: [0] });
    const xdrBytes = sc.toXDR();
    const digestHex = sha256Hex(xdrBytes);
    // This anchor is computed by `sha256(encodeAuthPayload(...).toXDR())`
    // for the inputs above. Any wire-format change makes this fail.
    // The literal value is verified by re-running the encoder against the
    // fixed inputs at Stream-B-freeze time.
    expect(digestHex).toMatch(/^[0-9a-f]{64}$/);
    // Pin the actual value — captured on 2026-05-16 from the encoder.
    expect(digestHex).toBe(
      "a30dec25dd420596b1541fa39ebf206057f1175c585d982aa536f12f77d7d53c",
    );
  });
});

// =========================================================================
// 5. buildOzAuthEntry — full SorobanAuthorizationEntry assembly
// =========================================================================

function makeRootInvocation(saAddress: string): xdr.SorobanAuthorizedInvocation {
  // SA.add_context_rule(...) — args content does not matter for the
  // shape-level assertions; we use a Symbol placeholder.
  const contractFn = new xdr.InvokeContractArgs({
    contractAddress: new Address(saAddress).toScAddress(),
    functionName: "add_context_rule",
    args: [xdr.ScVal.scvSymbol("placeholder")],
  });
  return new xdr.SorobanAuthorizedInvocation({
    function: xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(
      contractFn,
    ),
    subInvocations: [],
  });
}

describe("buildOzAuthEntry — full SorobanAuthorizationEntry", () => {
  it("produces credentials=SorobanCredentials::Address with signature=AuthPayload", async () => {
    const kp = Keypair.random();
    const inv = makeRootInvocation(C_SA);
    const entry = await buildOzAuthEntry({
      rootInvocation: inv,
      smartAccount: C_SA,
      signers: [
        {
          signer: { kind: "delegated", address: kp.publicKey() },
          keypair: kp,
        },
      ],
      contextRuleIds: [0],
      networkPassphrase: Networks.TESTNET,
      nonce: 12345n,
      signatureExpirationLedger: 1_000_000,
    });

    expect(entry).toBeInstanceOf(xdr.SorobanAuthorizationEntry);
    // Credentials = SorobanCredentials::Address.
    const creds = entry.credentials();
    expect(creds.switch().name).toBe("sorobanCredentialsAddress");
    const addrCreds = creds.address();
    // The SA's address is in the credentials.
    expect(Address.fromScAddress(addrCreds.address()).toString()).toBe(C_SA);
    expect(addrCreds.signatureExpirationLedger()).toBe(1_000_000);

    // The signature ScVal must be an AuthPayload (scvMap with the two
    // expected symbol keys).
    const sig = addrCreds.signature();
    expect(sig.switch().name).toBe("scvMap");
    const entries = sig.map() as xdr.ScMapEntry[];
    expect(entries.map((e) => e.key().sym().toString())).toEqual([
      "context_rule_ids",
      "signers",
    ]);
  });

  it("uses the signEd25519 callback when no keypair is supplied", async () => {
    const kp = Keypair.random();
    const inv = makeRootInvocation(C_SA);
    // Hand the encoder a stub signer; the produced signature bytes are
    // determined by `signEd25519`.
    const stubSig = Buffer.alloc(64, 0x33);
    let invocations = 0;
    const entry = await buildOzAuthEntry({
      rootInvocation: inv,
      smartAccount: C_SA,
      signers: [
        {
          signer: { kind: "delegated", address: kp.publicKey() },
          signEd25519: (digest) => {
            invocations += 1;
            expect(digest.length).toBe(32);
            return new Uint8Array(stubSig);
          },
        },
      ],
      contextRuleIds: [0],
      networkPassphrase: Networks.TESTNET,
      nonce: 1n,
      signatureExpirationLedger: 1_000,
    });
    expect(invocations).toBe(1);
    const sig = entry.credentials().address().signature();
    const signersMap = sig.map()![1]!.val().map() as xdr.ScMapEntry[];
    expect(Buffer.compare(signersMap[0]!.val().bytes(), stubSig)).toBe(0);
  });

  it("throws when neither keypair nor signEd25519 is provided", async () => {
    const inv = makeRootInvocation(C_SA);
    await expect(
      buildOzAuthEntry({
        rootInvocation: inv,
        smartAccount: C_SA,
        signers: [
          { signer: { kind: "delegated", address: G_OWNER } },
        ],
        contextRuleIds: [0],
        networkPassphrase: Networks.TESTNET,
        nonce: 1n,
        signatureExpirationLedger: 1_000,
      }),
    ).rejects.toThrow(/requires either signEd25519 or keypair/);
  });

  it("throws when the signer produces a non-64-byte signature", async () => {
    const inv = makeRootInvocation(C_SA);
    await expect(
      buildOzAuthEntry({
        rootInvocation: inv,
        smartAccount: C_SA,
        signers: [
          {
            signer: { kind: "delegated", address: G_OWNER },
            signEd25519: () => new Uint8Array(32),
          },
        ],
        contextRuleIds: [0],
        networkPassphrase: Networks.TESTNET,
        nonce: 1n,
        signatureExpirationLedger: 1_000,
      }),
    ).rejects.toThrow(/must be 64 bytes/);
  });

  it("computeSignaturePayload matches the standard envelope-type SHA-256", () => {
    const inv = makeRootInvocation(C_SA);
    const sp = computeSignaturePayload({
      networkPassphrase: Networks.TESTNET,
      nonce: 7n,
      signatureExpirationLedger: 999,
      invocation: inv,
    });
    expect(sp.length).toBe(32);
    // Re-derive independently from the source formula.
    const networkId = createHash("sha256")
      .update(Buffer.from(Networks.TESTNET, "utf8"))
      .digest();
    const preimage = xdr.HashIdPreimage.envelopeTypeSorobanAuthorization(
      new xdr.HashIdPreimageSorobanAuthorization({
        networkId,
        nonce: new xdr.Int64(7n),
        signatureExpirationLedger: 999,
        invocation: inv,
      }),
    );
    const expected = createHash("sha256").update(preimage.toXDR()).digest();
    expect(Buffer.compare(sp, expected)).toBe(0);
  });
});

// =========================================================================
// 6. Round-trip — toXDR → fromXDR → equality
// =========================================================================

describe("AuthPayload XDR round-trip", () => {
  it("encodeAuthPayload → toXDR → ScVal.fromXDR → byte-equal", () => {
    const signers = new Map<OzSigner, Uint8Array>();
    signers.set({ kind: "delegated", address: G_OWNER }, FAKE_SIG);
    const sc1 = encodeAuthPayload({ signers, contextRuleIds: [0, 5, 42] });
    const xdrBytes = sc1.toXDR();
    const sc2 = xdr.ScVal.fromXDR(xdrBytes);
    // Same wire bytes again on the second round-trip.
    expect(Buffer.compare(sc2.toXDR(), xdrBytes)).toBe(0);
  });

  it("buildOzAuthEntry → toXDR → SorobanAuthorizationEntry.fromXDR → byte-equal", async () => {
    const kp = Keypair.random();
    const inv = makeRootInvocation(C_SA);
    const e1 = await buildOzAuthEntry({
      rootInvocation: inv,
      smartAccount: C_SA,
      signers: [
        {
          signer: { kind: "delegated", address: kp.publicKey() },
          keypair: kp,
        },
      ],
      contextRuleIds: [0],
      networkPassphrase: Networks.TESTNET,
      nonce: 99n,
      signatureExpirationLedger: 500_000,
    });
    const bytes = e1.toXDR();
    const e2 = xdr.SorobanAuthorizationEntry.fromXDR(bytes);
    expect(Buffer.compare(e2.toXDR(), bytes)).toBe(0);
  });
});
