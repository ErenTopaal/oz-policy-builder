/**
 * Example 03 — Soroswap bounded trading, headless install flow.
 *
 * UNWIRED. The Soroswap walkthrough corpus is now frozen at
 * `walkthroughs/03-soroswap-bounded/` (source tx `7475b169…`, spec, WASM,
 * sim-report, and envelope-XDR all present — see that dir's README). But
 * THIS SCRIPT BODY has not yet been wired to consume the corpus and run
 * the install. The corresponding wired siblings are
 * `01-blend-yield-headless.ts` and `02-sep41-subscription-headless.ts`.
 *
 * Wiring TODO (mirror those siblings):
 *   - Friendbot a fresh ed25519 keypair on testnet.
 *   - Read `walkthroughs/03-soroswap-bounded/expected-spec-auto.json`.
 *   - Build the install envelope via `oz-policy-cli prepare-install`.
 *   - Sign with `PasskeyWallet` (headless-keypair mode).
 *   - Submit + poll; surface tx hash + `context_rule_id`.
 *
 * Until wired, this script emits a single placeholder JSON report and
 * exits non-zero so CI can flag the gap.
 *
 * Run:  `pnpm tsx examples/03-soroswap-bounded-headless.ts`
 */

interface PlaceholderReport {
  walkthrough: string;
  network: "testnet";
  status: "placeholder";
  error: string;
}

const report: PlaceholderReport = {
  walkthrough: "03-soroswap-bounded",
  network: "testnet",
  status: "placeholder",
  error:
    "Soroswap walkthrough corpus not yet frozen (Phase 8 dependency). " +
    "TODO: implement once `walkthroughs/03-soroswap-bounded/recording.json` " +
    "and `expected-spec-auto.json` land. See plan.md Phase 8.",
};

process.stdout.write(JSON.stringify(report, null, 2) + "\n");
process.exit(1);
