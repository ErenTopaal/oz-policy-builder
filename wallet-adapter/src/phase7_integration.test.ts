/**
 * Phase 7 Round 2 — testnet end-to-end install integration test.
 *
 * **Gated by `INTEGRATION=1`** so default `pnpm test` runs do not hit the
 * network. Run with:
 *
 *   PHASE7_SA_OWNER_SECRET=$(stellar keys show sa-owner-p7r2 --network testnet) \
 *   INTEGRATION=1 pnpm test phase7_integration
 *
 * ## What this test does (real, no mocks)
 *
 * 1. Loads the frozen deployment addresses from
 *    `walkthroughs/phase7-testnet-install/deployed-addresses.json`.
 * 2. Calls the `prepare-install` CLI binary (built once via `cargo build`
 *    before the test runs) against the testnet RPC — this is the real
 *    Phase 2 envelope-builder pipeline; the simulator's resource footprint,
 *    nonce, and read/write entries all come from a live RPC call.
 * 3. Asserts the envelope is well-formed base64 XDR that round-trips through
 *    `TransactionBuilder.fromXDR`.
 * 4. Signs the outer envelope via the headless `PasskeyWallet` path (the
 *    `signerSecretKey` flow, using the SA owner's testnet keypair) AND
 *    rewrites the OZ-SA auth entry's `signature` slot to carry a properly
 *    encoded `AuthPayload` ScVal (via `makeOzSmartAccountAuthEncoder` from
 *    `oz_smart_account_auth.ts` — Phase 8 Stream B). This is the
 *    BLOCKER fix that closes RFP deliverable #5.
 * 5. Submits the signed envelope to testnet RPC (real
 *    `sendTransaction` + `getTransaction` poll).
 * 6. Asserts the transaction lands `SUCCESS`, captures the resulting
 *    `context_rule_id`, and calls `verifyInstall` against it via the
 *    real MCP subprocess. The MCP server now does a real on-chain
 *    readback (RFP deliverable #5, 2026-05-18) and `matches: true`
 *    is the closure assertion.
 *
 * ## Why this is not a mock
 *
 * Every external surface is hit for real:
 *   * `prepare-install` performs a live `simulateTransaction` RPC call.
 *   * `Keypair.sign` produces a real ed25519 signature.
 *   * `sendTransaction` posts the envelope to a public testnet RPC.
 *   * `getTransaction` polls until SUCCESS or FAILED — no fake clock.
 *   * `verifyInstall` spawns the real `oz-policy-mcp` server binary,
 *     which itself runs `simulateTransaction` against testnet to read
 *     the on-chain `ContextRule`.
 *
 * The literal output of every step is captured into the test's failure
 * message so a re-run on a fresh machine reproduces (or diagnoses) the
 * outcome.
 */

import { describe, expect, it } from "vitest";
import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import {
  Keypair,
  Networks,
  TransactionBuilder,
} from "@stellar/stellar-sdk";

import { PasskeyWallet } from "./adapters/passkey.js";
import { installPolicy, WalletInstallError } from "./install.js";
import { makeOzSmartAccountAuthEncoder } from "./oz_smart_account_auth.js";
import { verifyInstall } from "./verify.js";

const execFileAsync = promisify(execFile);

// repository layout helpers.

const HERE = dirname(fileURLToPath(import.meta.url));
// src/phase7_integration.test.ts → wallet-adapter/ → repo root
const REPO_ROOT = resolve(HERE, "..", "..");
const CLI_BIN = join(REPO_ROOT, "target", "debug", "oz-policy-cli");
const MCP_BIN = join(REPO_ROOT, "target", "debug", "oz-policy-mcp");
const FIXTURE_PATH = join(
  REPO_ROOT,
  "walkthroughs",
  "phase7-testnet-install",
  "deployed-addresses.json",
);
const SPEC_PATH = join(
  REPO_ROOT,
  "walkthroughs",
  "phase7-testnet-install",
  "spec.json",
);

// frozen testnet endpoint. Hard-coded so the test cannot accidentally hit
// mainnet (the `network` discriminant in installPolicy also gates that).

const TESTNET_RPC = "https://soroban-testnet.stellar.org";
const TESTNET_PASSPHRASE = Networks.TESTNET; // "Test SDF Network ; September 2015"

// SA owner secret seed (testnet only). Pulled from env so the literal
// secret is NOT committed. The matching G-address
// (GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ) is in the
// fixture; the seed is held in the developer's local stellar keys store.
// `INTEGRATION=1` runs in CI MUST supply this via the `PHASE7_SA_OWNER_SECRET`
// env var (or the test will skip-with-failure to surface the missing input).

const SA_OWNER_SECRET_ENV = "PHASE7_SA_OWNER_SECRET";

// fixture loader. Strict — any missing field surfaces immediately so the
// test never silently runs against the wrong addresses.

interface PhaseFixture {
  smart_account: string;
  policy_function_allowlist: string;
  sa_owner_pubkey: string;
  bootstrap_context_rule_id: number;
  network: string;
  network_passphrase: string;
  rpc_url: string;
}

async function loadFixture(): Promise<PhaseFixture> {
  const raw = await readFile(FIXTURE_PATH, "utf-8");
  const parsed: unknown = JSON.parse(raw);
  if (parsed === null || typeof parsed !== "object") {
    throw new Error(`fixture is not an object: ${typeof parsed}`);
  }
  const required = [
    "smart_account",
    "policy_function_allowlist",
    "sa_owner_pubkey",
    "bootstrap_context_rule_id",
    "network",
    "network_passphrase",
    "rpc_url",
  ] as const;
  for (const k of required) {
    if (!(k in (parsed as Record<string, unknown>))) {
      throw new Error(`fixture missing required field: ${k}`);
    }
  }
  const fx = parsed as Record<string, unknown>;
  return {
    smart_account: String(fx.smart_account),
    policy_function_allowlist: String(fx.policy_function_allowlist),
    sa_owner_pubkey: String(fx.sa_owner_pubkey),
    bootstrap_context_rule_id: Number(fx.bootstrap_context_rule_id),
    network: String(fx.network),
    network_passphrase: String(fx.network_passphrase),
    rpc_url: String(fx.rpc_url),
  };
}

// drive the prepare-install CLI to produce a real install envelope. We
// spawn the binary rather than re-implement the Phase 2 logic in JS so
// the test exercises the same code path users hit.

interface PreparedEnvelope {
  envelope_xdr_base64: string;
  min_resource_fee: number;
  host_function_count: number;
}

async function prepareInstallEnvelope(fx: PhaseFixture): Promise<PreparedEnvelope> {
  const { stdout, stderr } = await execFileAsync(
    CLI_BIN,
    [
      "prepare-install",
      SPEC_PATH,
      "--smart-account",
      fx.smart_account,
      "--source",
      fx.sa_owner_pubkey,
      "--rpc",
      fx.rpc_url,
      "--network",
      fx.network_passphrase,
      "--account-revision",
      "post-pr-655",
    ],
    {
      // 60 s is conservative; the simulator usually returns within 5 s.
      timeout: 60_000,
      // inherit cwd from process; CLI_BIN is absolute.
    },
  );
  void stderr; // CLI logs go to stderr; we don't assert on them.
  const parsed = JSON.parse(stdout) as PreparedEnvelope;
  if (typeof parsed.envelope_xdr_base64 !== "string") {
    throw new Error(`prepare-install missing envelope_xdr_base64; stdout=${stdout}`);
  }
  return parsed;
}

// the actual integration test.

describe.skipIf(process.env.INTEGRATION !== "1")(
  "Phase 7 Round 2 — testnet end-to-end install",
  () => {
    it(
      "builds a real envelope, signs it, attempts submission, and reads back via verifyInstall",
      async () => {
        // ---------- 0. Pre-flight: the secret seed must be supplied ----------
        const ownerSecret = process.env[SA_OWNER_SECRET_ENV];
        if (!ownerSecret) {
          throw new Error(
            `INTEGRATION=1 requires the ${SA_OWNER_SECRET_ENV} env var ` +
              `(testnet S… seed of GCM2…). The seed is not committed; ` +
              `see walkthroughs/phase7-testnet-install/README.md.`,
          );
        }

        // ---------- 1. Load the frozen fixture --------------------------------
        const fx = await loadFixture();
        expect(fx.network).toBe("testnet");
        expect(fx.network_passphrase).toBe(TESTNET_PASSPHRASE);
        expect(fx.rpc_url).toBe(TESTNET_RPC);
        // pin the addresses so a corpus drift is caught before any RPC call.
        expect(fx.smart_account).toBe(
          "CAQGYWVEZIE6ZZBVDIVUYTH4BBC5UVQMUOPAKYKDU2POXISSNFKCBN3A",
        );
        expect(fx.policy_function_allowlist).toBe(
          "CDBE67MNNVIOAD5RSKO6IECOGIVK45L3NRP4PS2DMCI3GPDYOLY7CWAR",
        );
        expect(fx.sa_owner_pubkey).toBe(
          "GCM2CB7P7ZL4QCCI62WIOCLFW2LT5AP7HPUQY7J6JQQUQT4XXZZNWHLJ",
        );
        expect(fx.bootstrap_context_rule_id).toBe(0);

        // ---------- 2. Build the envelope (real testnet simulate) ------------
        const env = await prepareInstallEnvelope(fx);
        expect(typeof env.envelope_xdr_base64).toBe("string");
        expect(env.envelope_xdr_base64.length).toBeGreaterThan(500);
        expect(env.host_function_count).toBe(1);
        expect(env.min_resource_fee).toBeGreaterThan(0);

        // sanity: the envelope must round-trip through TransactionBuilder.
        // failures here would mean the CLI's emitted XDR is corrupt — not
        // an on-chain problem but a Phase-2 regression.
        const rehydrated = TransactionBuilder.fromXDR(
          env.envelope_xdr_base64,
          fx.network_passphrase,
        );
        expect(rehydrated).toBeDefined();

        // ---------- 3. Wallet adapter — sign the outer envelope --------------
        const wallet = new PasskeyWallet({
          rpcUrl: fx.rpc_url,
          networkPassphrase: fx.network_passphrase,
          signerSecretKey: ownerSecret,
        });
        expect(await wallet.getAddress()).toBe(fx.sa_owner_pubkey);

        // ---------- 4. Submit (real RPC) -------------------------------------
        // phase 8 Stream B: wire the OZ-SA AuthPayload encoder so the
        // signed envelope's `SorobanAuthorizationEntry` targeting the SA
        // carries a properly encoded AuthPayload (rather than the
        // simulator's Void placeholder that traps __check_auth). The
        // SA's bootstrap rule (id 0) authorises via Signer::Delegated(
        // GCM2…) — same keypair as the outer envelope signer.
        const ozKp = Keypair.fromSecret(ownerSecret);
        const ozEncoder = makeOzSmartAccountAuthEncoder({
          smartAccount: fx.smart_account,
          contextRuleIds: [fx.bootstrap_context_rule_id],
          networkPassphrase: fx.network_passphrase,
          signers: [
            {
              signer: { kind: "delegated", address: ozKp.publicKey() },
              keypair: ozKp,
            },
          ],
        });

        let installResult: {
          txHash: string;
          contextRuleId: number;
          ledger: number;
        };
        try {
          installResult = await installPolicy({
            adapter: wallet,
            envelopeXdrBase64: env.envelope_xdr_base64,
            rpcUrl: fx.rpc_url,
            network: "testnet",
            networkPassphrase: fx.network_passphrase,
            pollIntervalMs: 1_000,
            pollTimeoutMs: 90_000,
            ozAuthPayloadEncoder: ozEncoder,
          });
        } catch (err) {
          // any failure is now a real regression: the BLOCKER is closed
          // (RFP deliverable #5, 2026-05-18). Surface the failure shape
          // verbatim so we can diagnose what regressed.
          if (err instanceof WalletInstallError) {
            throw new Error(
              `installPolicy failed unexpectedly — the RFP-deliverable-5 ` +
                `closure assertion requires status=SUCCESS. ` +
                `code=${err.code} detail=${err.detail}`,
            );
          }
          throw new Error(
            `installPolicy threw an unexpected (non-WalletInstallError) ` +
              `failure: ${
                err instanceof Error ? (err.stack ?? err.message) : String(err)
              }`,
          );
        }

        // ---------- 5. SUCCESS assertions ------------------------------------
        expect(installResult.txHash).toMatch(/^[0-9a-f]{64}$/);
        expect(Number.isInteger(installResult.contextRuleId)).toBe(true);
        // the bootstrap rule is id 0; this install MUST mint a new rule
        // (id ≥ 1). Asserting strictly > 0 catches any drift where the
        // installer accidentally rebinds the bootstrap rule.
        expect(installResult.contextRuleId).toBeGreaterThan(0);
        expect(installResult.ledger).toBeGreaterThan(0);
        // eslint-disable-next-line no-console
        console.log(
          "[Phase 7 SUCCESS] installPolicy:",
          JSON.stringify(installResult, null, 2),
        );

        // ---------- 6. verifyInstall via real MCP on-chain readback ---------
        // load the canonical PolicySpec so verifyInstall can diff each
        // field. This is the same spec the prepare-install CLI consumed.
        const specRaw = await readFile(SPEC_PATH, "utf-8");
        const expectedSpec: unknown = JSON.parse(specRaw);

        const report = await verifyInstall({
          smartAccount: fx.smart_account,
          contextRuleId: installResult.contextRuleId,
          network: "testnet",
          rpcUrl: fx.rpc_url,
          expectedSpec,
          sourceAccount: fx.sa_owner_pubkey,
          mcpServerCmd: [MCP_BIN, "--stdio"],
          timeoutMs: 60_000,
        });
        // eslint-disable-next-line no-console
        console.log(
          "[Phase 7 SUCCESS] verifyInstall report:",
          JSON.stringify(report, null, 2),
        );

        // the RFP-deliverable-5 closure assertion: the on-chain rule
        // must match the spec field-for-field.
        expect(report.matches).toBe(true);
        expect(report.drift).toEqual([]);
      },
      // 5-minute timeout: build (≤2 s) + simulate (≤10 s) + sign (≤100 ms)
      // + submit (≤2 s) + poll (≤90 s) + verifyInstall (≤60 s, twice in
      // happy path) + slack. The MCP subprocess startup is cargo-built
      // before the suite via the verification gate.
      5 * 60 * 1000,
    );
  },
);
