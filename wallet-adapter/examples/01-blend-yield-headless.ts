/**
 * Example 01 — Blend yield-claim, headless install flow.
 *
 * Mirrors `walkthroughs/01-blend-yield/`. Demonstrates the full headless
 * pipeline against Stellar testnet:
 *
 *   1. Generate a fresh testnet keypair via `Keypair.random()`.
 *   2. Fund it via Friendbot (`https://friendbot.stellar.org/?addr=...`).
 *   3. Shell out to `oz-policy-cli prepare-install` to build the install
 *      envelope for the Blend `PolicySpec`. This will (currently) fail with
 *      `E_INSTALL_PREFLIGHT_FAILED('primitive_address_unknown ...')` because
 *      the per-network deployment registry of OZ primitive contracts hasn't
 *      shipped yet (see `crates/oz-policy-installer/src/registry.rs` and
 *      `plan.md` Phase 7's dependency notes).
 *   4. If `prepare-install` succeeded, sign with `PasskeyWallet` and submit.
 *   5. Print a single JSON report to stdout.
 *
 * This example is **NETWORK-DEPENDENT**. It hits Friendbot and the public
 * testnet RPC. It will fail with `network_error` if either is unreachable.
 *
 * Run:  `pnpm tsx examples/01-blend-yield-headless.ts`
 *
 * SECURITY: Testnet only. The script generates a fresh keypair per run; the
 * secret never leaves the process memory.
 */

import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { readFile, writeFile, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Keypair, Networks } from "@stellar/stellar-sdk";

import { PasskeyWallet } from "../src/adapters/passkey.js";

const execFileAsync = promisify(execFile);

interface Report {
  walkthrough: string;
  network: "testnet";
  status: "submitted" | "preflight_failed" | "rejected" | "network_error";
  txHash?: string;
  contextRuleId?: number;
  error?: string;
  signerAddress?: string;
}

const RPC_URL = "https://soroban-testnet.stellar.org";
const NETWORK_PASSPHRASE = Networks.TESTNET; // "Test SDF Network ; September 2015"
const FRIENDBOT_URL = "https://friendbot.stellar.org";

// Phase 1 frozen Blend recording lives here. We derive the spec by re-running
// synthesize against the frozen recording (deterministic). The spec target
// contract C-address is the Blend TestnetV2 pool from the walkthrough README.
const BLEND_RECORDING_PATH = "walkthroughs/01-blend-yield/expected-recording.json";
const BLEND_POOL_CADDRESS = "CCEBVDYM32YNYCVNRXQKDFFPISJJCV557CDZEIRBEE4NCV4KHPQ44HGF";

// Path to the workspace-root binary. We assume the binary has been built
// (`cargo build -p oz-policy-cli`). The example surfaces the build state
// honestly — if the binary doesn't exist we report `network_error` (the
// shorthand status bucket for any "couldn't even get to RPC" outcome).
const CLI_BIN = "../target/debug/oz-policy-cli";

async function fundViaFriendbot(address: string): Promise<void> {
  const url = `${FRIENDBOT_URL}/?addr=${encodeURIComponent(address)}`;
  const res = await fetch(url);
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(
      `friendbot non-2xx: ${res.status} ${res.statusText} — ${body.slice(0, 200)}`,
    );
  }
}

async function synthesize(workdir: string, recordingPath: string): Promise<string> {
  // Run `oz-policy-cli synthesize <recording>` to produce a PolicySpec JSON.
  const { stdout } = await execFileAsync(CLI_BIN, [
    "synthesize",
    "--mode",
    "auto",
    recordingPath,
  ]);
  const specPath = join(workdir, "spec.json");
  await writeFile(specPath, stdout, "utf-8");
  return specPath;
}

async function prepareInstall(
  specPath: string,
  smartAccount: string,
  sourceG: string,
): Promise<{ envelopeXdr: string }> {
  // `prepare-install` returns an `EnvelopeArtifact` JSON; we only need
  // the envelope XDR field. If preflight fails (e.g. primitive registry
  // miss) it exits non-zero and we surface the stderr verbatim.
  const { stdout } = await execFileAsync(CLI_BIN, [
    "prepare-install",
    specPath,
    "--smart-account",
    smartAccount,
    "--source",
    sourceG,
    "--rpc",
    RPC_URL,
    "--network",
    NETWORK_PASSPHRASE,
    "--account-revision",
    "post-pr-655",
  ]);
  const artifact = JSON.parse(stdout);
  // Field name is whatever `EnvelopeArtifact` serializes to; we accept
  // either `envelope_xdr` (snake) or `envelopeXdr` (camel) for resilience.
  const envelopeXdr =
    artifact.envelope_xdr ?? artifact.envelopeXdr ?? artifact.envelope;
  if (typeof envelopeXdr !== "string") {
    throw new Error(
      `prepare-install output missing envelope_xdr; got keys=${Object.keys(artifact).join(",")}`,
    );
  }
  return { envelopeXdr };
}

async function submitTransaction(signedXdr: string): Promise<{ hash: string; status: string }> {
  // We use the Soroban RPC `sendTransaction` method directly so we don't
  // need to spin up a `Server` instance from stellar-sdk. The JSON-RPC
  // payload is part of the public Soroban RPC contract:
  // https://developers.stellar.org/network/soroban-rpc/methods/sendTransaction
  const body = {
    jsonrpc: "2.0",
    id: 1,
    method: "sendTransaction",
    params: { transaction: signedXdr },
  };
  const res = await fetch(RPC_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(`sendTransaction non-2xx: ${res.status} ${res.statusText}`);
  }
  const json = (await res.json()) as {
    result?: { hash?: string; status?: string; errorResult?: unknown };
    error?: { message?: string };
  };
  if (json.error) {
    throw new Error(`sendTransaction error: ${json.error.message ?? "unknown"}`);
  }
  if (!json.result?.hash) {
    throw new Error(`sendTransaction missing hash: ${JSON.stringify(json.result)}`);
  }
  return { hash: json.result.hash, status: json.result.status ?? "UNKNOWN" };
}

async function main(): Promise<Report> {
  const kp = Keypair.random();
  const sourceG = kp.publicKey();

  // 1. Fund.
  try {
    await fundViaFriendbot(sourceG);
  } catch (err) {
    return {
      walkthrough: "01-blend-yield",
      network: "testnet",
      status: "network_error",
      error: `friendbot: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  }

  // 2. Synthesize spec from the frozen recording.
  const workdir = await mkdtemp(join(tmpdir(), "oz-example-blend-"));
  let specPath: string;
  try {
    // Resolve relative to the wallet-adapter directory (the script's cwd is
    // workspace-root if invoked as `pnpm tsx wallet-adapter/examples/...`).
    const recordingPath = await fileExists(BLEND_RECORDING_PATH)
      ? BLEND_RECORDING_PATH
      : `../${BLEND_RECORDING_PATH}`;
    specPath = await synthesize(workdir, recordingPath);
  } catch (err) {
    return {
      walkthrough: "01-blend-yield",
      network: "testnet",
      status: "preflight_failed",
      error: `synthesize: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  }

  // 3. prepare-install. This is where E_INSTALL_PREFLIGHT_FAILED surfaces
  // until the deployment registry lands. We capture the stderr verbatim.
  let envelopeXdr: string;
  try {
    ({ envelopeXdr } = await prepareInstall(specPath, BLEND_POOL_CADDRESS, sourceG));
  } catch (err) {
    return {
      walkthrough: "01-blend-yield",
      network: "testnet",
      status: "preflight_failed",
      error: `prepare-install: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  } finally {
    await rm(workdir, { recursive: true, force: true });
  }

  // 4. Sign.
  const wallet = new PasskeyWallet({
    rpcUrl: RPC_URL,
    networkPassphrase: NETWORK_PASSPHRASE,
    signerSecretKey: kp.secret(),
  });
  let signedXdr: string;
  try {
    const result = await wallet.signTransaction(envelopeXdr, {
      networkPassphrase: NETWORK_PASSPHRASE,
    });
    signedXdr = result.signedTxXdr;
  } catch (err) {
    return {
      walkthrough: "01-blend-yield",
      network: "testnet",
      status: "rejected",
      error: `sign: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  }

  // 5. Submit.
  try {
    const { hash, status } = await submitTransaction(signedXdr);
    return {
      walkthrough: "01-blend-yield",
      network: "testnet",
      status: status === "ERROR" ? "rejected" : "submitted",
      txHash: hash,
      signerAddress: sourceG,
      ...(status === "ERROR" ? { error: `RPC returned status=ERROR` } : {}),
    };
  } catch (err) {
    return {
      walkthrough: "01-blend-yield",
      network: "testnet",
      status: "network_error",
      error: `submit: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  }
}

async function fileExists(p: string): Promise<boolean> {
  try {
    await readFile(p);
    return true;
  } catch {
    return false;
  }
}

main().then(
  (report) => {
    process.stdout.write(JSON.stringify(report, null, 2) + "\n");
    process.exit(report.status === "submitted" ? 0 : 1);
  },
  (err) => {
    const report: Report = {
      walkthrough: "01-blend-yield",
      network: "testnet",
      status: "network_error",
      error: `unhandled: ${(err as Error).message}`,
    };
    process.stdout.write(JSON.stringify(report, null, 2) + "\n");
    process.exit(2);
  },
);
