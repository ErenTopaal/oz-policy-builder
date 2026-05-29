/**
 * OpenZeppelin SmartAccount `AuthPayload` ScVal encoder + auth-digest
 * computer + full `SorobanAuthorizationEntry` builder.
 *
 * **Phase 8 Stream B.** Closes the Phase 7 Round 2 BLOCKER documented in
 * `walkthroughs/phase7-testnet-install/BLOCKER.md`:
 *
 *   The OZ MinimalSmartAccount's `__check_auth` reads its second positional
 *   argument as `AuthPayload { signers: Map<Signer, Bytes>, context_rule_ids:
 *   Vec<u32> }`. The `record_signature_payload` simulator mode emits `Void`
 *   in that slot — which traps the SA with `UnreachableCodeReached`. This
 *   module is the client-side post-processor that converts a Void-signature
 *   auth entry into a properly encoded `AuthPayload` ScVal, plus computes
 *   the post-PR-#655 auth digest each signer must actually sign.
 *
 * ## Shapes (verified against source — see `docs/oz-internal-shapes.md` §10
 * which transcribes `OpenZeppelin/stellar-contracts@v0.7.1`)
 *
 * ```rust
 * #[contracttype]
 * pub struct AuthPayload {
 *     pub signers: Map<Signer, Bytes>,
 *     pub context_rule_ids: Vec<u32>,
 * }
 *
 * #[contracttype]
 * pub enum Signer {
 *     Delegated(Address),
 *     External(Address, Bytes),
 * }
 * ```
 *
 * Soroban encodes `#[contracttype]` structs as `ScVal::Map([{Symbol(field),
 * <value>}, ...])` with entries **sorted ascending by field name** (the
 * host's `ScMap` invariant). For `AuthPayload`, that yields
 * `["context_rule_ids", "signers"]` in that order.
 *
 * Soroban encodes `#[contracttype]` enums as `ScVal::Vec([Symbol(variant),
 * <field0>, <field1>, ...])`. For `Signer::Delegated(addr)`, that's
 * `Vec([Symbol("Delegated"), Address(addr)])`.
 *
 * ## Auth digest (post-PR-#655)
 *
 * From `packages/accounts/src/smart_account/storage.rs:493-495` (verified
 * verbatim in `docs/oz-internal-shapes.md` §10):
 *
 * ```rust
 * let mut preimage = signature_payload.to_bytes().to_bytes();
 * preimage.append(&signatures.context_rule_ids.clone().to_xdr(e));
 * let auth_digest = e.crypto().sha256(&preimage);
 * ```
 *
 * - `signature_payload` is the standard 32-byte Soroban auth signature
 *   payload, i.e. `sha256(HashIdPreimageSorobanAuthorization.to_xdr())`
 *   (see `stellar-xdr` `EnvelopeType::SorobanAuthorization`).
 * - `context_rule_ids.clone().to_xdr(e)` is the XDR encoding of
 *   `ScVal::Vec(Some([ScVal::U32(id), ...]))`.
 * - `auth_digest = sha256(signature_payload || xdr(context_rule_ids_scval))`.
 *
 * Signers sign `auth_digest`, NOT the raw `signature_payload`.
 */

import { createHash } from "node:crypto";

import {
  Address,
  Keypair,
  xdr,
} from "@stellar/stellar-sdk";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/**
 * Logical representation of the `Signer` enum from
 * `packages/accounts/src/smart_account/storage.rs` (v0.7.1).
 *
 * - `delegated` → `Signer::Delegated(Address)` — built-in ed25519 verifier
 *   keyed by a G-address (or a contract C-address).
 * - `external_ed25519` / `external_webauthn` → `Signer::External(verifier,
 *   public_key_bytes)`. The verifier is whichever contract address the OZ
 *   project deploys for that verifier kind. `publicKeyHex` is the key bytes
 *   that the verifier compares against.
 */
export type OzSigner =
  | { kind: "delegated"; address: string }
  | { kind: "external_ed25519"; verifier: string; publicKeyHex: string }
  | { kind: "external_webauthn"; verifier: string; publicKeyHex: string };

/** Logical OZ `AuthPayload` — signer → signature bytes, plus selected rule ids. */
export interface OzAuthPayload {
  signers: Map<OzSigner, Uint8Array>;
  contextRuleIds: number[];
}

// ---------------------------------------------------------------------------
// Encoding helpers
// ---------------------------------------------------------------------------

/** Coerce `Uint8Array | Buffer` to a Node `Buffer` (stellar-sdk requires Buffer). */
function toBuf(b: Uint8Array | Buffer): Buffer {
  return Buffer.isBuffer(b) ? b : Buffer.from(b);
}

/**
 * Encode an {@link OzSigner} as a Soroban `ScVal::Vec` matching the
 * `#[contracttype]` enum layout — `Vec([Symbol(variant), <fields...>])`.
 *
 * Verified against `Signer` definition at `storage.rs:93-102` (see
 * `docs/oz-internal-shapes.md` §10).
 */
export function encodeSignerScVal(signer: OzSigner): xdr.ScVal {
  switch (signer.kind) {
    case "delegated":
      return xdr.ScVal.scvVec([
        xdr.ScVal.scvSymbol("Delegated"),
        new Address(signer.address).toScVal(),
      ]);
    case "external_ed25519":
    case "external_webauthn":
      return xdr.ScVal.scvVec([
        xdr.ScVal.scvSymbol("External"),
        new Address(signer.verifier).toScVal(),
        xdr.ScVal.scvBytes(Buffer.from(signer.publicKeyHex, "hex")),
      ]);
  }
}

/**
 * Encode `Vec<u32>` as an `ScVal::Vec` of `ScVal::U32`. Used both for the
 * `context_rule_ids` field of the `AuthPayload` and (separately) for the
 * `to_xdr()` slab that feeds the auth-digest preimage.
 */
export function encodeContextRuleIdsScVal(ids: number[]): xdr.ScVal {
  return xdr.ScVal.scvVec(
    ids.map((id) => {
      if (!Number.isInteger(id) || id < 0 || id > 0xffff_ffff) {
        throw new Error(
          `encodeContextRuleIdsScVal: id ${id} is not a u32`,
        );
      }
      return xdr.ScVal.scvU32(id);
    }),
  );
}

/**
 * Encode the OZ `AuthPayload` as an `ScVal::Map` with entries SORTED
 * ascending by field-name symbol (the Soroban `ScMap` invariant). For
 * `AuthPayload` that means the entries are emitted in this order:
 *
 *   1. `context_rule_ids: Vec<u32>`
 *   2. `signers: Map<Signer, Bytes>`
 *
 * The inner `signers` map is itself an `ScVal::Map` whose entries are
 * `{ key: Signer ScVal, val: Bytes }`. Soroban requires the inner map to
 * also be sorted by key (the host enforces this on read), so we sort the
 * `signers` entries by their key ScVal's serialized XDR before emitting.
 * This matches the canonical ordering the OZ contract produces when it
 * `clone()`s an in-host `Map<Signer, Bytes>` and serialises it — fields
 * inserted in any order end up sorted in the wire encoding.
 */
export function encodeAuthPayload(payload: OzAuthPayload): xdr.ScVal {
  // ---- inner signers map ------------------------------------------------
  const signerEntries = Array.from(payload.signers.entries()).map(
    ([signer, sig]) => {
      return new xdr.ScMapEntry({
        key: encodeSignerScVal(signer),
        val: xdr.ScVal.scvBytes(toBuf(sig)),
      });
    },
  );
  // Sort the inner Map<Signer, Bytes> by the XDR-serialised key bytes —
  // this matches the host's `ScMap` ordering invariant.
  signerEntries.sort((a, b) =>
    Buffer.compare(a.key().toXDR(), b.key().toXDR()),
  );
  const signersScVal = xdr.ScVal.scvMap(signerEntries);

  // ---- outer struct map (sorted by field name) --------------------------
  // The `#[contracttype]` struct layout is keyed by field name, sorted
  // ascending lexicographically. For AuthPayload: "context_rule_ids" <
  // "signers".
  const outerEntries: xdr.ScMapEntry[] = [
    new xdr.ScMapEntry({
      key: xdr.ScVal.scvSymbol("context_rule_ids"),
      val: encodeContextRuleIdsScVal(payload.contextRuleIds),
    }),
    new xdr.ScMapEntry({
      key: xdr.ScVal.scvSymbol("signers"),
      val: signersScVal,
    }),
  ];

  return xdr.ScVal.scvMap(outerEntries);
}

// ---------------------------------------------------------------------------
// Auth digest
// ---------------------------------------------------------------------------

/** SHA-256 over the concatenation of its arguments (Node `crypto`). */
function sha256(...chunks: Array<Uint8Array | Buffer>): Buffer {
  const h = createHash("sha256");
  for (const c of chunks) {
    h.update(toBuf(c));
  }
  return h.digest();
}

/**
 * Compute the OZ-SA auth digest:
 *
 *   auth_digest = sha256(signature_payload || xdr(context_rule_ids))
 *
 * Where:
 *  - `signature_payload` is the 32-byte Soroban auth signature payload
 *    (i.e. `sha256(HashIdPreimageSorobanAuthorization.to_xdr())`), supplied
 *    by the caller. It is NOT recomputed here because the caller already
 *    has the four ingredients (network id, nonce, expiration ledger,
 *    invocation) and recomputing would duplicate `buildOzAuthEntry`'s
 *    work.
 *  - `xdr(context_rule_ids)` is the XDR encoding of the `Vec<u32>` as a
 *    Soroban `ScVal::Vec(Some([U32(id), ...]))`. This matches the OZ
 *    source: `signatures.context_rule_ids.clone().to_xdr(e)` — soroban
 *    `Vec<T>::to_xdr` produces `ScVal::Vec(...).to_xdr()`.
 *
 * Returns the 32-byte digest. Signers must sign THIS, not the raw
 * `signature_payload`.
 */
export function computeAuthDigest(
  signaturePayload: Uint8Array,
  contextRuleIds: number[],
): Uint8Array {
  if (signaturePayload.length !== 32) {
    throw new Error(
      `computeAuthDigest: signaturePayload must be 32 bytes (sha256 output), got ${signaturePayload.length}`,
    );
  }
  const idsXdr = encodeContextRuleIdsScVal(contextRuleIds).toXDR();
  return sha256(signaturePayload, idsXdr);
}

/**
 * Compute the standard Soroban auth signature payload —
 * `sha256(HashIdPreimageSorobanAuthorization.toXDR())` — for one auth
 * entry. This is the input to {@link computeAuthDigest}.
 */
export function computeSignaturePayload(params: {
  networkPassphrase: string;
  nonce: bigint;
  signatureExpirationLedger: number;
  invocation: xdr.SorobanAuthorizedInvocation;
}): Buffer {
  const networkId = sha256(Buffer.from(params.networkPassphrase, "utf8"));
  const preimage = xdr.HashIdPreimage.envelopeTypeSorobanAuthorization(
    new xdr.HashIdPreimageSorobanAuthorization({
      networkId,
      nonce: new xdr.Int64(params.nonce),
      signatureExpirationLedger: params.signatureExpirationLedger,
      invocation: params.invocation,
    }),
  );
  return sha256(preimage.toXDR());
}

// ---------------------------------------------------------------------------
// Full auth-entry builder
// ---------------------------------------------------------------------------

/** Either a real {@link Keypair} or an opaque ed25519 signer function. */
export interface OzSignerWithKey {
  signer: OzSigner;
  /**
   * Ed25519 signature producer. Required when `signer.kind === "delegated"`
   * (the OZ SA's built-in verifier is ed25519). Receives the 32-byte
   * auth digest and returns the 64-byte raw ed25519 signature.
   */
  signEd25519?: (digest: Uint8Array) => Uint8Array;
  /**
   * Alternative: provide a {@link Keypair} and let us call `keypair.sign`
   * directly. Convenience over `signEd25519`.
   */
  keypair?: Keypair;
}

export interface BuildOzAuthEntryParams {
  /** The root invocation being authorised (e.g., `SA.add_context_rule(...)`). */
  rootInvocation: xdr.SorobanAuthorizedInvocation;
  /** SA C-address — the contract whose `__check_auth` is invoked. */
  smartAccount: string;
  /**
   * Signers — one entry per OZ `Signer` that authorises under the selected
   * context rule(s). Each must produce a signature over the computed auth
   * digest.
   */
  signers: OzSignerWithKey[];
  /** OZ `ContextRule.id` selection — e.g. `[0]` for the bootstrap rule. */
  contextRuleIds: number[];
  /** Network passphrase (e.g. `Networks.TESTNET`). */
  networkPassphrase: string;
  /** Nonce assigned by simulation. */
  nonce: bigint;
  /** Ledger at which the signature expires (also from simulation). */
  signatureExpirationLedger: number;
}

/**
 * Convenience: build an `ozAuthPayloadEncoder` callback suitable for
 * `installPolicy({ ozAuthPayloadEncoder })`. The returned function:
 *
 *   1. Decodes the signed envelope.
 *   2. Locates every `SorobanAuthorizationEntry` whose `credentials` are
 *      `SorobanCredentials::Address(<smartAccount>)`.
 *   3. For each such entry, reuses the entry's existing `nonce` and
 *      `signatureExpirationLedger` (set by the simulator) plus its
 *      `rootInvocation` (the actual call being authorised), runs
 *      {@link buildOzAuthEntry} to produce a properly encoded
 *      `AuthPayload` ScVal, and replaces the entry's `signature`.
 *   4. Re-encodes the envelope.
 *
 * Limitations (documented loudly so a future caller hits them at the
 * doc, not at runtime):
 *
 *   * Only rewrites entries whose credential address matches
 *     `smartAccount`. Nested entries (e.g. the `Signer::Delegated`
 *     `require_auth_for_args` sub-invocation) are NOT rewritten; the
 *     outer wallet-signed `SorobanCredentials::SourceAccount` path
 *     handles them via the normal envelope-signature flow already in
 *     `installPolicy`.
 *   * Assumes the entry's `rootInvocation` already correctly describes
 *     the SA call (i.e. the simulator's output is trusted). This
 *     matches the Phase 2 envelope-builder contract: simulation fills
 *     the invocation; the wallet provides the signature.
 *   * Uses the simulator-assigned nonce verbatim — does not re-derive
 *     it. Replacing the nonce post-simulation would invalidate the
 *     resource footprint.
 */
export function makeOzSmartAccountAuthEncoder(args: {
  /** SA C-address whose entries should be rewritten. */
  smartAccount: string;
  /** Signer set authorising under the selected rule(s). */
  signers: OzSignerWithKey[];
  /** ContextRule id(s) on the SA being used to authorise. */
  contextRuleIds: number[];
  /** Network passphrase the envelope was signed against. */
  networkPassphrase: string;
}): (signedEnvelopeXdrBase64: string) => Promise<string> {
  return async (signedEnvelopeXdrBase64: string): Promise<string> => {
    const env = xdr.TransactionEnvelope.fromXDR(
      signedEnvelopeXdrBase64,
      "base64",
    );
    // Only Soroban v1 envelopes carry InvokeHostFunctionOp + auth.
    if (env.switch() !== xdr.EnvelopeType.envelopeTypeTx()) {
      throw new Error(
        `makeOzSmartAccountAuthEncoder: expected envelopeTypeTx, got ${env.switch().name}`,
      );
    }
    const v1 = env.v1();
    const tx = v1.tx();
    const ops = tx.operations();
    let rewroteAny = false;
    for (const op of ops) {
      const body = op.body();
      if (body.switch() !== xdr.OperationType.invokeHostFunction()) continue;
      const ihf = body.invokeHostFunctionOp();
      const auths = ihf.auth();
      const targetAddr = new Address(args.smartAccount).toScAddress();
      const targetXdr = targetAddr.toXDR();
      const newAuths: xdr.SorobanAuthorizationEntry[] = [];
      for (const a of auths) {
        const c = a.credentials();
        if (c.switch().name !== "sorobanCredentialsAddress") {
          newAuths.push(a);
          continue;
        }
        const addrCreds = c.address();
        if (
          Buffer.compare(addrCreds.address().toXDR(), targetXdr) !== 0
        ) {
          newAuths.push(a);
          continue;
        }
        // Match — rewrite.
        const replaced = await buildOzAuthEntry({
          rootInvocation: a.rootInvocation(),
          smartAccount: args.smartAccount,
          signers: args.signers,
          contextRuleIds: args.contextRuleIds,
          networkPassphrase: args.networkPassphrase,
          nonce: addrCreds.nonce().toBigInt(),
          signatureExpirationLedger: addrCreds.signatureExpirationLedger(),
        });
        newAuths.push(replaced);
        rewroteAny = true;
      }
      ihf.auth(newAuths);
    }
    if (!rewroteAny) {
      throw new Error(
        `makeOzSmartAccountAuthEncoder: no SorobanCredentials::Address ` +
          `entry targeted smartAccount=${args.smartAccount} in the signed envelope`,
      );
    }
    return env.toXDR("base64");
  };
}

/**
 * Build a complete `SorobanAuthorizationEntry` whose
 * `credentials = SorobanAddressCredentials { address: SA, ..., signature:
 * encodeAuthPayload({signers, context_rule_ids}) }` and whose
 * `rootInvocation` is the supplied invocation.
 *
 * Each signer signs the auth digest (post-PR-#655 formula). The resulting
 * raw signature bytes go into the `Map<Signer, Bytes>` entry for that
 * signer.
 *
 * Note on nested auth entries: a `Signer::Delegated(addr)` causes the SA
 * to call `addr.require_auth_for_args((auth_digest,))` inside
 * `__check_auth`. The host already discovers that require_auth via
 * simulation and emits a corresponding auth tree entry for `addr` —
 * callers that need to sign that nested entry should do so via the
 * standard outer-envelope signing path (the simulator includes the
 * nested entry in the envelope's `op.auth[]` list). This builder only
 * constructs the OUTER SA-targeted entry.
 */
export async function buildOzAuthEntry(
  params: BuildOzAuthEntryParams,
): Promise<xdr.SorobanAuthorizationEntry> {
  // 1. Compute the auth digest each signer signs.
  const signaturePayload = computeSignaturePayload({
    networkPassphrase: params.networkPassphrase,
    nonce: params.nonce,
    signatureExpirationLedger: params.signatureExpirationLedger,
    invocation: params.rootInvocation,
  });
  const authDigest = computeAuthDigest(
    signaturePayload,
    params.contextRuleIds,
  );

  // 2. Build the signers map.
  const signersMap = new Map<OzSigner, Uint8Array>();
  for (const entry of params.signers) {
    let sig: Uint8Array;
    if (entry.signEd25519) {
      sig = entry.signEd25519(authDigest);
    } else if (entry.keypair) {
      sig = entry.keypair.sign(Buffer.from(authDigest));
    } else {
      throw new Error(
        `buildOzAuthEntry: signer ${JSON.stringify(entry.signer)} ` +
          `requires either signEd25519 or keypair`,
      );
    }
    if (sig.length !== 64) {
      throw new Error(
        `buildOzAuthEntry: signature for ${JSON.stringify(entry.signer)} ` +
          `must be 64 bytes (ed25519), got ${sig.length}`,
      );
    }
    signersMap.set(entry.signer, sig);
  }

  // 3. Encode the AuthPayload ScVal.
  const authPayloadScVal = encodeAuthPayload({
    signers: signersMap,
    contextRuleIds: params.contextRuleIds,
  });

  // 4. Wrap into SorobanAddressCredentials + SorobanCredentials::Address.
  const addrCreds = new xdr.SorobanAddressCredentials({
    address: new Address(params.smartAccount).toScAddress(),
    nonce: new xdr.Int64(params.nonce),
    signatureExpirationLedger: params.signatureExpirationLedger,
    signature: authPayloadScVal,
  });
  const credentials = xdr.SorobanCredentials.sorobanCredentialsAddress(addrCreds);

  // 5. Assemble the entry.
  return new xdr.SorobanAuthorizationEntry({
    credentials,
    rootInvocation: params.rootInvocation,
  });
}
