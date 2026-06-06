/**
 * Example 02 — SEP-41 USDC subscription, headless install flow.
 *
 * Mirrors `walkthroughs/02-sep41-subscription/`. Same pipeline as
 * `01-blend-yield-headless.ts`:
 *
 *   1. Fresh testnet `Keypair.random()` → Friendbot funding.
 *   2. `oz-policy-cli synthesize` against the frozen SEP-41 recording to
 *      derive the PolicySpec (track-a `spending_limit` composition).
 *   3. `oz-policy-cli prepare-install` to build the install envelope.
 *      Until the OZ primitive deployment registry lands, this returns
 *      `E_INSTALL_PREFLIGHT_FAILED('primitive_address_unknown spending_limit ...')`.
 *   4. Sign with `PasskeyWallet` (headless signer-secret path).
 *   5. Submit to Soroban RPC.
 *   6. Print a single JSON report to stdout.
 *
 * NETWORK-DEPENDENT. Real testnet RPC and real Friendbot.
 *
 * Run:  `pnpm tsx examples/02-sep41-subscription-headless.ts`
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
const NETWORK_PASSPHRASE = Networks.TESTNET;
const FRIENDBOT_URL = "https://friendbot.stellar.org";

// frozen sep-41 walkthrough.
const SEP41_RECORDING_PATH = "walkthroughs/02-sep41-subscription/recording.json";
const SEP41_USDC_SAC = "CDG7N5LG7TAWOHZH27TW6XN3WBA66TA5TUXYJP6552KVPZ3CTWABHKIH";

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
  const { stdout } = await execFileAsync(CLI_BIN, [
    "synthesize",
    "--mode",
    "compose-only",
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

async function fileExists(p: string): Promise<boolean> {
  try {
    await readFile(p);
    return true;
  } catch {
    return false;
  }
}

async function main(): Promise<Report> {
  const kp = Keypair.random();
  const sourceG = kp.publicKey();

  try {
    await fundViaFriendbot(sourceG);
  } catch (err) {
    return {
      walkthrough: "02-sep41-subscription",
      network: "testnet",
      status: "network_error",
      error: `friendbot: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  }

  const workdir = await mkdtemp(join(tmpdir(), "oz-example-sep41-"));
  let specPath: string;
  try {
    const recordingPath = (await fileExists(SEP41_RECORDING_PATH))
      ? SEP41_RECORDING_PATH
      : `../${SEP41_RECORDING_PATH}`;
    specPath = await synthesize(workdir, recordingPath);
  } catch (err) {
    return {
      walkthrough: "02-sep41-subscription",
      network: "testnet",
      status: "preflight_failed",
      error: `synthesize: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  }

  let envelopeXdr: string;
  try {
    ({ envelopeXdr } = await prepareInstall(specPath, SEP41_USDC_SAC, sourceG));
  } catch (err) {
    return {
      walkthrough: "02-sep41-subscription",
      network: "testnet",
      status: "preflight_failed",
      error: `prepare-install: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  } finally {
    await rm(workdir, { recursive: true, force: true });
  }

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
      walkthrough: "02-sep41-subscription",
      network: "testnet",
      status: "rejected",
      error: `sign: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  }

  try {
    const { hash, status } = await submitTransaction(signedXdr);
    return {
      walkthrough: "02-sep41-subscription",
      network: "testnet",
      status: status === "ERROR" ? "rejected" : "submitted",
      txHash: hash,
      signerAddress: sourceG,
      ...(status === "ERROR" ? { error: "RPC returned status=ERROR" } : {}),
    };
  } catch (err) {
    return {
      walkthrough: "02-sep41-subscription",
      network: "testnet",
      status: "network_error",
      error: `submit: ${(err as Error).message}`,
      signerAddress: sourceG,
    };
  }
}

main().then(
  (report) => {
    process.stdout.write(JSON.stringify(report, null, 2) + "\n");
    process.exit(report.status === "submitted" ? 0 : 1);
  },
  (err) => {
    const report: Report = {
      walkthrough: "02-sep41-subscription",
      network: "testnet",
      status: "network_error",
      error: `unhandled: ${(err as Error).message}`,
    };
    process.stdout.write(JSON.stringify(report, null, 2) + "\n");
    process.exit(2);
  },
);
