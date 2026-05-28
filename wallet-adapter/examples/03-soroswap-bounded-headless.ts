/**
 * Example 03 — Soroswap bounded trading, headless install flow.
 *
 * PLACEHOLDER. The Soroswap walkthrough corpus (`walkthroughs/03-soroswap-bounded/`)
 * does not yet exist — see `plan.md` Phase 8 "Three end-to-end walkthroughs".
 * Specifically:
 *
 *   - A frozen testnet `source.json` (recorded `swap_exact_tokens_for_tokens`
 *     call against the Soroswap router) must be captured.
 *   - The `expected-spec-auto.json` must be derived deterministically.
 *   - Track-B codegen must produce a `bounded_trading` policy WASM.
 *
 * Once Phase 8 freezes the Soroswap corpus, this script will mirror
 * `01-blend-yield-headless.ts` / `02-sep41-subscription-headless.ts`
 * verbatim — same Friendbot → synthesize → prepare-install → sign → submit
 * pipeline, with `--mode auto` because the bounded-trading constraint
 * doesn't compose onto an existing OZ primitive in v1.
 *
 * Until then, running this script emits a single placeholder JSON report
 * and exits non-zero so CI can flag any premature inclusion.
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
