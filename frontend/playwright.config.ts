// Playwright config for /playground end-to-end against LIVE production.
//
// honesty rules carried in:
//   - no localhost spawn / no dev server start. baseURL points at the real
//     production deployment so any failure here is a real production
//     regression we want to surface, not a fixture artifact.
//   - retries: 1 — covers a single transient network blip on a 30-day
//     snapshot store. anything that fails both attempts is a real bug.
//   - video: retain-on-failure so we can rewatch a failed Monaco edit or
//     a missed deny card without re-running.

import { defineConfig } from "@playwright/test";

const baseURL = process.env.E2E_BASE_URL ?? "https://policy.erentopal.xyz";

export default defineConfig({
  testDir: "./e2e",
  timeout: 6 * 60 * 1000, // 6 min per test — Test 3 waits ~5 min for first sandbox compile
  expect: { timeout: 15_000 },
  retries: 1,
  workers: 1, // share the same fresh preset across tests; avoid burst-fetching
  reporter: [["list"]],
  use: {
    baseURL,
    video: "retain-on-failure",
    trace: "retain-on-failure",
    actionTimeout: 30_000,
    navigationTimeout: 60_000,
    headless: true,
    permissions: ["clipboard-read", "clipboard-write"],
  },
  projects: [
    {
      name: "chromium",
      use: { browserName: "chromium" },
    },
  ],
});
